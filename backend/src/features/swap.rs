use super::{
    portfolio::format_amount,
    tokens::{TokenClient, TokenInfo},
    transactions::{TransferError, parse_amount},
};
use reqwest::{Client, RequestBuilder};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

const ORDER_URL: &str = "https://api.jup.ag/swap/v2/order";
fn default_slippage() -> u16 {
    50
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwapInput {
    /// Token symbol/name or exact mainnet mint. SOL denotes native SOL pricing.
    pub input_asset: String,
    pub output_asset: String,
    /// Exact human-readable quantity of the input token (not base units).
    pub amount: String,
    /// 50 bps = 0.5%. Default 50; valid range 0..=10000.
    #[serde(default = "default_slippage")]
    pub slippage_bps: u16,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteStep {
    swap_info: SwapInfo,
    percent: Option<u16>,
    bps: Option<u16>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwapInfo {
    label: Option<String>,
    input_mint: String,
    output_mint: String,
    in_amount: String,
    out_amount: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformFee {
    amount: Option<String>,
    fee_bps: u16,
    fee_mint: Option<String>,
}

// Deliberately omit transaction, requestId, and signing/execution metadata.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    input_mint: String,
    output_mint: String,
    in_amount: String,
    out_amount: String,
    other_amount_threshold: Option<String>,
    slippage_bps: Option<u16>,
    price_impact: Option<f64>,
    in_usd_value: Option<f64>,
    out_usd_value: Option<f64>,
    router: String,
    swap_mode: String,
    #[serde(default)]
    route_plan: Vec<RouteStep>,
    fee_bps: Option<u16>,
    fee_mint: Option<String>,
    platform_fee: Option<PlatformFee>,
}

#[derive(Serialize)]
pub struct QuotedToken {
    mint: String,
    symbol: String,
    decimals: u8,
    amount: String,
    raw_amount: String,
}
#[derive(Serialize)]
pub struct SwapQuote {
    mode: &'static str,
    executable: bool,
    quote_network: &'static str,
    connected_network: String,
    quoted_at_unix: u64,
    input: QuotedToken,
    estimated_output: QuotedToken,
    minimum_output_amount: Option<String>,
    /// Output tokens per one input token, using quoted amounts.
    exchange_rate: Option<String>,
    requested_slippage_bps: u16,
    slippage_bps: Option<u16>,
    slippage_percent: Option<String>,
    price_impact_percent: Option<f64>,
    estimated_input_usd: Option<f64>,
    estimated_output_usd: Option<f64>,
    router: String,
    route_plan: Vec<RouteStep>,
    fee_bps: Option<u16>,
    fee_mint: Option<String>,
    platform_fee: Option<PlatformFee>,
    note: &'static str,
}

fn invalid() -> TransferError {
    TransferError::bad(
        "invalid_swap_quote",
        "Jupiter returned an incomplete or inconsistent quote. Request a fresh quote.",
    )
}

fn request(
    client: &Client,
    input: &TokenInfo,
    output: &TokenInfo,
    units: u64,
    slippage: u16,
) -> RequestBuilder {
    client.get(ORDER_URL).query(&[
        ("inputMint", input.id.clone()),
        ("outputMint", output.id.clone()),
        ("amount", units.to_string()),
        ("slippageBps", slippage.to_string()),
        ("swapMode", "ExactIn".into()),
    ])
}

pub(super) async fn quote(
    client: &Client,
    tokens: &TokenClient,
    input: SwapInput,
    network: &str,
) -> Result<SwapQuote, TransferError> {
    if input.slippage_bps > 10000 {
        return Err(TransferError::bad(
            "invalid_slippage",
            "Slippage must be 0..=10000 basis points; 50 means 0.5%.",
        ));
    }
    let sell = tokens.quote_token(&input.input_asset).await?;
    let buy = tokens.quote_token(&input.output_asset).await?;
    if sell.id == buy.id {
        return Err(TransferError::bad(
            "same_asset",
            "Choose two different token mints.",
        ));
    }
    for token in [&sell, &buy] {
        if token.decimals > 19 || Pubkey::from_str(&token.id).is_err() {
            return Err(TransferError::bad(
                "invalid_token_metadata",
                "Jupiter token metadata has an invalid mint or unsupported decimals.",
            ));
        }
    }
    let units = parse_amount(&input.amount, sell.decimals)?;
    let response = request(client, &sell, &buy, units, input.slippage_bps)
        .send()
        .await
        .map_err(|_| {
            TransferError::bad(
                "swap_quote_unavailable",
                "Jupiter quote request timed out or failed. Retry shortly.",
            )
        })?;
    if !response.status().is_success() {
        let (code, message) = match response.status().as_u16() {
            401 | 403 => (
                "jupiter_access_denied",
                "Check JUPITER_API_KEY and its Swap API permissions.",
            ),
            429 => (
                "jupiter_rate_limited",
                "Jupiter rate limit reached. Retry shortly.",
            ),
            400 | 404 => (
                "swap_route_unavailable",
                "No usable quote for this token pair/amount. Try another amount or token.",
            ),
            _ => (
                "swap_quote_unavailable",
                "Jupiter could not return a swap quote. Retry shortly.",
            ),
        };
        return Err(TransferError::bad(code, message));
    }
    let order: Order = response.json().await.map_err(|_| invalid())?;
    preview(order, sell, buy, units, input.slippage_bps, network)
}

fn preview(
    order: Order,
    sell: TokenInfo,
    buy: TokenInfo,
    units: u64,
    requested_slippage: u16,
    network: &str,
) -> Result<SwapQuote, TransferError> {
    let out = order
        .out_amount
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    if order.input_mint != sell.id
        || order.output_mint != buy.id
        || order.in_amount != units.to_string()
        || order.swap_mode != "ExactIn"
        || order.slippage_bps.is_some_and(|bps| bps > 10000)
    {
        return Err(invalid());
    }
    let minimum = order
        .other_amount_threshold
        .as_deref()
        .map(|raw| {
            raw.parse::<u64>()
                .ok()
                .filter(|value| *value <= out)
                .map(|value| format_amount(value, buy.decimals))
                .ok_or_else(invalid)
        })
        .transpose()?;
    let amount_in = format_amount(units, sell.decimals);
    let amount_out = format_amount(out, buy.decimals);
    let rate = Decimal::from_str(&amount_out)
        .ok()
        .zip(Decimal::from_str(&amount_in).ok())
        .and_then(|(out, input)| out.checked_div(input))
        .map(|value| value.normalize().to_string());
    Ok(SwapQuote {
        mode: "quote_only",
        executable: false,
        quote_network: "solana_mainnet",
        connected_network: network.into(),
        quoted_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid())?
            .as_secs(),
        input: QuotedToken {
            mint: sell.id,
            symbol: sell.symbol,
            decimals: sell.decimals,
            amount: amount_in,
            raw_amount: order.in_amount,
        },
        estimated_output: QuotedToken {
            mint: buy.id,
            symbol: buy.symbol,
            decimals: buy.decimals,
            amount: amount_out,
            raw_amount: order.out_amount,
        },
        minimum_output_amount: minimum,
        exchange_rate: rate,
        requested_slippage_bps: requested_slippage,
        slippage_bps: order.slippage_bps,
        slippage_percent: order.slippage_bps.map(|bps| {
            (Decimal::from(bps) / Decimal::from(100))
                .normalize()
                .to_string()
        }),
        price_impact_percent: order.price_impact,
        estimated_input_usd: order.in_usd_value,
        estimated_output_usd: order.out_usd_value,
        router: order.router,
        route_plan: order.route_plan,
        fee_bps: order.fee_bps,
        fee_mint: order.fee_mint,
        platform_fee: order.platform_fee,
        note: "Informational mainnet market quote only. Surfpool/test tokens cannot fund this mainnet route. Wallet balances and gas/rent affordability were not checked. Estimates can change; minimum output is only the provider's quoted threshold, not a guaranteed receipt. Fees are shown as returned by Jupiter. Swap execution is not implemented: no transaction was built, simulated, signed, or submitted, and no balances changed.",
    })
}
