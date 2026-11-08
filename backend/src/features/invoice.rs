
use super::{
    portfolio::{PortfolioResponse, fetch_portfolio, format_amount},
    transactions::{self, BillPaymentContext, TransferError, TransferInput, TransferView},
};
use crate::{
    auth::{AuthenticatedUser, random_token},
    state::AppState,
};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const RETENTION_TTL: Duration = Duration::from_secs(900);
const QUOTE_TTL: u64 = 120;
const CAPACITY: usize = 1000;
type Result<T> = std::result::Result<T, TransferError>;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuoteInput {
    /// Optional merchant label read by the client; display only.
    pub merchant: Option<String>,
    /// Exact total explicitly confirmed/corrected by the user after reading the extraction.
    pub confirmed_total: String,
    /// USD or SOL. Never assume a bare $ means USD; ask the user.
    pub confirmed_currency: String,
    /// Set true only after the user confirmed total and currency in conversation.
    pub confirmed_by_user: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PaymentOption {
    /// SOL means native SOL; otherwise this is the actual SPL mint address.
    pub asset: String,
    pub symbol: Option<String>,
    pub available_amount: String,
    pub required_amount: Option<String>,
    pub usd_price: Option<String>,
    pub affordable_before_recipient_costs: bool,
    pub unavailable_reason: Option<&'static str>,
}

#[derive(Clone, Serialize)]
pub struct BillQuote {
    pub quote_id: String,
    pub merchant: Option<String>,
    pub confirmed_total: String,
    pub currency: String,
    pub estimated_bill_usd: Option<String>,
    pub quoted_at_unix: u64,
    pub expires_at_unix: u64,
    pub network: String,
    pub options: Vec<PaymentOption>,
    pub note: &'static str,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareBillInput {
    pub quote_id: String,
    /// Recipient WALLET explicitly supplied/confirmed by the user. Never copy an OCR address silently.
    pub recipient: String,
    /// User-selected asset from quote options: SOL or the exact mint address.
    pub asset: String,
}

struct Prepared {
    recipient: String,
    asset: String,
    transfer_id: String,
}
struct StoredQuote {
    wallet: Pubkey,
    quote: BillQuote,
    prepared: Option<Prepared>,
}
#[derive(Default)]
struct Records {
    quotes: HashMap<String, (Instant, Arc<Mutex<StoredQuote>>)>,
}
impl Records {
    fn clean(&mut self) {
        let now = Instant::now();
        self.quotes.retain(|_, (expires, _)| *expires > now);
    }
}

#[derive(Clone, Default)]
pub struct BillService {
    records: Arc<Mutex<Records>>,
}

fn bad(code: &'static str, message: &str) -> TransferError {
    TransferError::bad(code, message)
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| bad("clock_error", "Server clock unavailable."))
}
fn scope(user: &AuthenticatedUser, name: &str) -> Result<()> {
    user.require_scope(name).map_err(|_| {
        bad(
            "forbidden",
            "Reconnect with the required portfolio:read/transactions:write permissions.",
        )
    })
}

impl BillService {
    pub async fn quote(
        &self,
        state: &AppState,
        user: &AuthenticatedUser,
        input: QuoteInput,
    ) -> Result<BillQuote> {
        scope(user, "portfolio:read")?;
        if !input.confirmed_by_user {
            return Err(bad(
                "confirmation_required",
                "Ask the user to confirm the bill total and currency before quoting.",
            ));
        }
        let currency = input.confirmed_currency.trim().to_uppercase();
        let total = bill_total(&input.confirmed_total, &currency)?;
        let merchant = input
            .merchant
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if merchant.as_ref().is_some_and(|value| value.len() > 200) {
            return Err(bad(
                "invalid_merchant",
                "Merchant label must be at most 200 bytes.",
            ));
        }
        let portfolio = fetch_portfolio(
            state._rpc.as_ref(),
            &user.wallet,
            &state.network,
            &state.prices,
        )
        .await
        .map_err(|_| {
            bad(
                "rpc_unavailable",
                "Could not read the connected wallet balances. Retry shortly.",
            )
        })?;
        let (mut options, usd) = payment_options(&portfolio, total, &currency)?;
        let mints: Vec<_> = options
            .iter()
            .filter(|option| option.asset != "SOL")
            .map(|option| option.asset.clone())
            .collect();
        let symbols = state.tokens.symbols(&mints).await;
        for option in &mut options {
            if option.asset != "SOL" {
                option.symbol = symbols.get(&option.asset).cloned();
            }
        }
        let time = now()?;
        let quote = BillQuote {
            quote_id: random_token(),
            merchant,
            confirmed_total: total.normalize().to_string(),
            currency,
            estimated_bill_usd: usd.map(|v| v.normalize().to_string()),
            quoted_at_unix: time,
            expires_at_unix: time + QUOTE_TTL,
            network: state.network.clone(),
            options,
            note: "USD amounts are estimates using Jupiter mainnet market quotes. Surfpool balances are test assets. Affordability includes the transfer amount and a minimum SOL fee check; recipient-account rent and actual fees are checked during preparation. Ask the user to select an asset and confirm the recipient wallet. No transaction has been prepared.",
        };
        let mut records = self.records.lock().await;
        records.clean();
        if records.quotes.len() >= CAPACITY {
            return Err(bad(
                "capacity_reached",
                "Too many active quotes. Retry later.",
            ));
        }
        records.quotes.insert(
            quote.quote_id.clone(),
            (
                Instant::now() + RETENTION_TTL,
                Arc::new(Mutex::new(StoredQuote {
                    wallet: user.wallet,
                    quote: quote.clone(),
                    prepared: None,
                })),
            ),
        );
        Ok(quote)
    }

    pub async fn prepare(
        &self,
        state: &AppState,
        user: &AuthenticatedUser,
        input: PrepareBillInput,
    ) -> Result<TransferView> {
        scope(user, "transactions:write")?;
        let entry = {
            let mut records = self.records.lock().await;
            records.clean();
            records.quotes.get(&input.quote_id).map(|(_,entry)|entry.clone())
            .ok_or_else(||bad("quote_expired","Quote expired or was not found. Request a fresh quote and let the user review it."))?
        };
        let mut entry = entry.lock().await;
        if entry.wallet != user.wallet {
            return Err(bad("quote_expired", "Quote not found for this wallet."));
        }
        let recipient = input.recipient.trim();
        let asset = input.asset.trim();
        if let Some(prepared) = &entry.prepared {
            if prepared.recipient != recipient || prepared.asset != asset {
                return Err(bad(
                    "quote_already_used",
                    "This quote already prepared a different transfer. Check its status; do not create a duplicate payment.",
                ));
            }
            return transactions::status(state, user, &prepared.transfer_id).await;
        }
        if entry.quote.expires_at_unix <= now()? {
            return Err(bad(
                "quote_expired",
                "Prices expired. Request a fresh quote and have the user review the new amounts.",
            ));
        }
        let option = entry
            .quote
            .options
            .iter()
            .find(|option| option.asset == asset)
            .ok_or_else(|| {
                bad(
                    "invalid_asset",
                    "Select an exact asset from the quote options.",
                )
            })?;
        if !option.affordable_before_recipient_costs {
            return Err(bad(
                "insufficient_funds",
                "That option has insufficient funds or no usable conversion price. Select another quoted asset.",
            ));
        }
        let amount = option.required_amount.clone().ok_or_else(|| {
            bad(
                "price_unavailable",
                "This asset has no usable conversion price.",
            )
        })?;
        let context = BillPaymentContext {
            merchant: entry.quote.merchant.clone(),
            confirmed_total: entry.quote.confirmed_total.clone(),
            bill_currency: entry.quote.currency.clone(),
            token_usd_price: option.usd_price.clone(),
            estimated_bill_usd: entry.quote.estimated_bill_usd.clone(),
            quoted_at_unix: entry.quote.quoted_at_unix,
        };
        let transfer = transactions::prepare_with_bill(
            state,
            user,
            TransferInput {
                recipient: recipient.into(),
                asset: asset.into(),
                amount,
            },
            Some(context),
        )
        .await?;
        entry.prepared = Some(Prepared {
            recipient: recipient.into(),
            asset: asset.into(),
            transfer_id: transfer.transfer_id.clone(),
        });
        Ok(transfer)
    }
}

fn bill_total(value: &str, currency: &str) -> Result<Decimal> {
    let scale = match currency {
        "USD" => 2,
        "SOL" => 9,
        _ => {
            return Err(bad(
                "unsupported_currency",
                "Confirm USD or SOL. Other currencies need an explicit user-agreed conversion to USD; never assume $ means USD.",
            ));
        }
    };
    if value.is_empty()
        || value.len() > 40
        || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        || value.starts_with('.')
        || value.ends_with('.')
    {
        return Err(bad(
            "invalid_total",
            "Use a positive plain decimal total without separators or exponent notation.",
        ));
    }
    let total =
        Decimal::from_str(value).map_err(|_| bad("invalid_total", "Invalid decimal total."))?;
    if total <= Decimal::ZERO
        || total.scale() > scale
        || total > Decimal::from(1_000_000_000_000u64)
    {
        return Err(bad(
            "invalid_total",
            "Total must be positive, at most 1 trillion, and use at most 2 USD or 9 SOL decimal places.",
        ));
    }
    Ok(total)
}

fn price(value: Option<f64>) -> Option<Decimal> {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .and_then(|value| {
            let text = value.to_string();
            Decimal::from_str(&text)
                .or_else(|_| Decimal::from_scientific(&text))
                .ok()
        })
        .filter(|value| *value > Decimal::ZERO)
}

fn required_units(total: Decimal, unit_price: Decimal, decimals: u8) -> Option<u64> {
    let factor = 10u64.checked_pow(decimals as u32)?;
    total
        .checked_mul(Decimal::from(factor))?
        .checked_div(unit_price)?
        .ceil()
        .to_u64()
        .filter(|value| *value > 0)
}

struct Holding {
    decimals: u8,
    balances: Vec<u64>,
    price: Option<Decimal>,
}

fn payment_options(
    portfolio: &PortfolioResponse,
    total: Decimal,
    currency: &str,
) -> Result<(Vec<PaymentOption>, Option<Decimal>)> {
    let sol_price = price(portfolio.native_sol.valuation.usd_price);
    let usd = if currency == "USD" {
        Some(total)
    } else {
        sol_price.and_then(|price| total.checked_mul(price))
    };
    let sol_balance = portfolio
        .native_sol
        .lamports
        .parse::<u64>()
        .map_err(|_| bad("invalid_balance", "RPC returned invalid SOL balance."))?;
    let mut holdings = BTreeMap::from([(
        "SOL".to_owned(),
        Holding {
            decimals: 9,
            balances: vec![sol_balance],
            price: sol_price,
        },
    )]);
    for token in &portfolio.token_accounts {
        if token.token_program != spl_token::ID.to_string() || token.state != "initialized" {
            continue;
        }
        let balance = token
            .raw_amount
            .parse::<u64>()
            .map_err(|_| bad("invalid_balance", "RPC returned invalid token balance."))?;
        if balance == 0 {
            continue;
        }
        let holding = holdings
            .entry(token.mint.clone())
            .or_insert_with(|| Holding {
                decimals: token.decimals,
                balances: Vec::new(),
                price: price(token.valuation.usd_price),
            });
        if holding.decimals != token.decimals {
            return Err(bad(
                "invalid_balance",
                "RPC returned inconsistent token decimals.",
            ));
        }
        holding.balances.push(balance);
    }
    let options = holdings
        .into_iter()
        .map(|(asset, mut holding)| {
            holding.balances.sort_unstable_by(|a, b| b.cmp(a));
            let available = holding
                .balances
                .into_iter()
                .take(8)
                .fold(0u64, u64::saturating_add);
            let units = if currency == "SOL" && asset == "SOL" {
                transactions::parse_amount(&total.normalize().to_string(), 9).ok()
            } else {
                usd.zip(holding.price)
                    .and_then(|(usd, price)| required_units(usd, price, holding.decimals))
            };
            let affordable = units.is_some_and(|units| {
                if asset == "SOL" {
                    units
                        .checked_add(5000)
                        .is_some_and(|required| required <= available)
                } else {
                    units <= available && sol_balance >= 5000
                }
            });
            PaymentOption {
                symbol: (asset == "SOL").then(|| "SOL".into()),
                asset,
                available_amount: format_amount(available, holding.decimals),
                required_amount: units.map(|units| format_amount(units, holding.decimals)),
                usd_price: holding.price.map(|v| v.normalize().to_string()),
                affordable_before_recipient_costs: affordable,
                unavailable_reason: if units.is_none() {
                    Some("Price/conversion unavailable or amount exceeds supported precision.")
                } else if !affordable {
                    Some("Insufficient spendable balance or SOL for network fees.")
                } else {
                    None
                },
            }
        })
        .collect();
    Ok((options, usd))
}

