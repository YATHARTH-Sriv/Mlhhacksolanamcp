use super::price::{PriceClient, PriceSnapshot, Quote, SOL_MINT};
use crate::{auth::AuthenticatedUser, state::AppState};
use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use solana_account_decoder_client_types::UiAccountData;
use solana_rpc_client::api::{request::TokenAccountsFilter, response::RpcKeyedAccount};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use spl_token::ID as TOKEN_PROGRAM_ID;
use spl_token_2022::ID as TOKEN_2022_PROGRAM_ID;

#[derive(Debug, Serialize, Default)]
pub struct Valuation {
    pub(crate) usd_price: Option<f64>,
    value_usd: Option<f64>,
    price_block_id: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct PricingSummary {
    source: &'static str,
    price_network: &'static str,
    status: &'static str,
    priced_value_usd: Option<f64>,
    unpriced_holdings: usize,
    fetched_at_unix: Option<u64>,
    warning: Option<&'static str>,
}

fn valuation(amount: &str, quote: Option<&Quote>) -> Valuation {
    let value = if amount == "0" {
        Some(0.0)
    } else {
        quote
            .and_then(|quote| {
                amount
                    .parse::<f64>()
                    .ok()
                    .map(|amount| amount * quote.usd_price)
            })
            .filter(|value| value.is_finite())
    };
    Valuation {
        usd_price: quote.map(|q| q.usd_price),
        value_usd: value,
        price_block_id: quote.and_then(|q| q.block_id),
    }
}

#[derive(Debug, Serialize)]
pub struct NativeBalance {
    pub(crate) lamports: String,
    amount: String,
    decimals: u8,
    #[serde(flatten)]
    pub(crate) valuation: Valuation,
}

#[derive(Debug, Serialize)]
pub struct TokenBalance {
    token_account: String,
    pub(crate) mint: String,
    pub(crate) token_program: String,
    pub(crate) raw_amount: String,
    pub(crate) decimals: u8,
    amount: String,
    pub(crate) state: String,
    #[serde(flatten)]
    pub(crate) valuation: Valuation,
}

#[derive(Debug, Serialize)]
pub struct PortfolioResponse {
    wallet_address: String,
    network: String,
    pub(crate) native_sol: NativeBalance,
    pub(crate) token_accounts: Vec<TokenBalance>,
    pricing: PricingSummary,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    error: &'static str,
    message: &'static str,
}

#[derive(Debug, Deserialize)]
struct ParsedTokenAccount {
    #[serde(rename = "type")]
    account_type: String,
    info: TokenAccountInfo,
}

#[derive(Debug, Deserialize)]
struct TokenAccountInfo {
    mint: String,
    state: String,

    #[serde(rename = "tokenAmount")]
    token_amount: ParsedTokenAmount,
}

#[derive(Debug, Deserialize)]
struct ParsedTokenAmount {
    amount: String,
    decimals: u8,
}

pub(crate) type ApiError = (StatusCode, Json<ErrorResponse>);

fn rpc_error(message: &'static str) -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: "rpc_error",
            message,
        }),
    )
}

fn invalid_data_error() -> ApiError {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: "invalid_rpc_data",
            message: "The RPC returned unsupported or invalid token account data.",
        }),
    )
}

pub(crate) fn format_amount(raw: u64, decimals: u8) -> String {
    if decimals == 0 {
        return raw.to_string();
    }

    let digits = format!("{:0>width$}", raw, width = decimals as usize + 1,);

    let split = digits.len() - decimals as usize;
    let whole = &digits[..split];
    let fractional = digits[split..].trim_end_matches('0');

    if fractional.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{fractional}")
    }
}

fn parse_token_accounts(
    accounts: Vec<RpcKeyedAccount>,
    token_program: &str,
) -> Result<Vec<TokenBalance>, ApiError> {
    accounts
        .into_iter()
        .map(|account| {
            let UiAccountData::Json(data) = account.account.data else {
                return Err(invalid_data_error());
            };

            let parsed: ParsedTokenAccount =
                serde_json::from_value(data.parsed).map_err(|_| invalid_data_error())?;

            if parsed.account_type != "account" {
                return Err(invalid_data_error());
            }

            let info = parsed.info;
            let raw = info
                .token_amount
                .amount
                .parse::<u64>()
                .map_err(|_| invalid_data_error())?;

            Ok(TokenBalance {
                token_account: account.pubkey,
                mint: info.mint,
                token_program: token_program.to_string(),
                raw_amount: raw.to_string(),
                decimals: info.token_amount.decimals,
                amount: format_amount(raw, info.token_amount.decimals),
                state: info.state,
                valuation: Valuation::default(),
            })
        })
        .collect()
}

pub async fn load_portfolio(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<PortfolioResponse>, Response> {
    user.require_scope("portfolio:read")
        .map_err(IntoResponse::into_response)?;
    fetch_portfolio(
        state._rpc.as_ref(),
        &user.wallet,
        &state.network,
        &state.prices,
    )
    .await
    .map(Json)
    .map_err(IntoResponse::into_response)
}

pub(crate) async fn fetch_portfolio(
    rpc: &RpcClient,
    wallet: &Pubkey,
    network: &str,
    prices: &PriceClient,
) -> Result<PortfolioResponse, ApiError> {
    let (lamports, legacy_accounts, token_2022_accounts) = tokio::try_join!(
        rpc.get_balance(wallet),
        rpc.get_token_accounts_by_owner(wallet, TokenAccountsFilter::ProgramId(TOKEN_PROGRAM_ID),),
        rpc.get_token_accounts_by_owner(
            wallet,
            TokenAccountsFilter::ProgramId(TOKEN_2022_PROGRAM_ID),
        ),
    )
    .map_err(|_| rpc_error("Unable to fetch wallet balances from the RPC."))?;

    let mut token_accounts = parse_token_accounts(legacy_accounts, &TOKEN_PROGRAM_ID.to_string())?;

    token_accounts.extend(parse_token_accounts(
        token_2022_accounts,
        &TOKEN_2022_PROGRAM_ID.to_string(),
    )?);

    // Stable ordering makes responses easier to inspect and display.
    token_accounts.sort_by(|a, b| {
        a.mint
            .cmp(&b.mint)
            .then_with(|| a.token_account.cmp(&b.token_account))
    });

    let snapshot = prices
        .fetch(
            std::iter::once(SOL_MINT.to_owned())
                .chain(token_accounts.iter().map(|token| token.mint.clone())),
        )
        .await;
    let mut portfolio = PortfolioResponse {
        wallet_address: wallet.to_string(),
        network: network.to_owned(),
        native_sol: NativeBalance {
            lamports: lamports.to_string(),
            amount: format_amount(lamports, 9),
            decimals: 9,
            valuation: Valuation::default(),
        },
        token_accounts,
        pricing: PricingSummary {
            source: "jupiter_price_v3",
            price_network: "mainnet",
            status: "unavailable",
            priced_value_usd: None,
            unpriced_holdings: 0,
            fetched_at_unix: None,
            warning: None,
        },
    };
    portfolio.apply_prices(snapshot);
    Ok(portfolio)
}

impl PortfolioResponse {
    fn apply_prices(&mut self, snapshot: PriceSnapshot) {
        self.native_sol.valuation =
            valuation(&self.native_sol.amount, snapshot.quotes.get(SOL_MINT));
        for token in &mut self.token_accounts {
            token.valuation = valuation(&token.amount, snapshot.quotes.get(&token.mint));
        }
        let valuations: Vec<_> = std::iter::once(&self.native_sol.valuation)
            .chain(self.token_accounts.iter().map(|t| &t.valuation))
            .collect();
        let missing = valuations.iter().filter(|v| v.value_usd.is_none()).count();
        let total: f64 = valuations.iter().filter_map(|v| v.value_usd).sum();
        self.pricing = PricingSummary {
            source: "jupiter_price_v3",
            price_network: "mainnet",
            status: if missing == valuations.len() {
                "unavailable"
            } else if missing > 0 || snapshot.warning.is_some() {
                "partial"
            } else {
                "complete"
            },
            priced_value_usd: (missing < valuations.len() && total.is_finite()).then_some(total),
            unpriced_holdings: missing,
            fetched_at_unix: snapshot.fetched_at_unix,
            warning: snapshot.warning,
        };
    }
}
