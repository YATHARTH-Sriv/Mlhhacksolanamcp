use super::tokens::TokenInfo;
use crate::{
    auth::{AuthenticatedUser, random_token},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use bincode::Options;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use solana_instruction::Instruction;
use solana_message::Message;
use solana_rpc_client::api::{config::RpcSimulateTransactionConfig, request::TokenAccountsFilter};
use solana_sdk::{program_pack::Pack, pubkey::Pubkey};
use solana_transaction::Transaction;
use spl_token::state::{Account as TokenAccount, AccountState, Mint};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

#[derive(Debug, Serialize)]
pub struct TransferError {
    pub error: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<TokenInfo>>,
}
impl TransferError {
    pub fn bad(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error: code,
            message: message.into(),
            candidates: None,
        }
    }
}
impl IntoResponse for TransferError {
    fn into_response(self) -> Response {
        let status = match self.error {
            "not_found" => StatusCode::NOT_FOUND,
            "rpc_unavailable" | "token_lookup_unavailable" => StatusCode::BAD_GATEWAY,
            "forbidden" => StatusCode::FORBIDDEN,
            _ => StatusCode::BAD_REQUEST,
        };
        let mut response = (status, Json(self)).into_response();
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        response
    }
}
fn rpc_error(_: impl std::fmt::Display) -> TransferError {
    TransferError::bad(
        "rpc_unavailable",
        "The configured Surfpool RPC is unavailable. Retry shortly.",
    )
}

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferInput {
    /// Recipient WALLET address, not a token account.
    pub recipient: String,
    /// SOL for native SOL, or exact token symbol/name or mint address. Wrapped SOL requires its mint.
    pub asset: String,
    /// Exact decimal string, such as "100" or "0.01". Never a JSON number.
    pub amount: String,
}

#[derive(Clone, Serialize)]
pub struct BillPaymentContext {
    pub merchant: Option<String>,
    pub confirmed_total: String,
    pub bill_currency: String,
    pub token_usd_price: Option<String>,
    pub estimated_bill_usd: Option<String>,
    pub quoted_at_unix: u64,
}

#[derive(Clone, Serialize)]
pub struct TransferView {
    pub transfer_id: String,
    pub sender: String,
    pub recipient: String,
    pub asset: String,
    pub mint: Option<String>,
    pub amount: String,
    pub raw_amount: String,
    pub decimals: u8,
    pub network: String,
    pub rpc_genesis_hash: String,
    pub fee_lamports: String,
    pub account_creation_lamports: String,
    pub creates_recipient_token_account: bool,
    pub simulation: &'static str,
    pub signing_url: String,
    pub expires_at_unix: u64,
    pub status: &'static str,
    pub signature: Option<String>,
    pub message: Option<String>,
    // Returned only to the browser approval endpoint, never needed by the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bill_payment: Option<BillPaymentContext>,
}

struct Pending {
    tx: Transaction,
    view: TransferView,
    last_valid_height: u64,
    retained_until: Instant,
}
#[derive(Clone, Default)]
pub struct TransferStore(Arc<Mutex<HashMap<String, Arc<Mutex<Pending>>>>>);

pub fn parse_amount(value: &str, decimals: u8) -> Result<u64, TransferError> {
    let invalid = || {
        TransferError::bad(
            "invalid_amount",
            "Amount must be positive plain decimal text, fit token precision, and fit a u64 base-unit amount.",
        )
    };
    if value.is_empty() || value.len() > 80 {
        return Err(invalid());
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap();
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > decimals as usize
    {
        return Err(invalid());
    }
    let factor = 10u64.checked_pow(decimals as u32).ok_or_else(invalid)?;
    let integer = whole
        .parse::<u64>()
        .map_err(|_| invalid())?
        .checked_mul(factor)
        .ok_or_else(invalid)?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u64>()
            .map_err(|_| invalid())?
            .checked_mul(
                10u64
                    .checked_pow(decimals as u32 - fraction.len() as u32)
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?
    };
    integer
        .checked_add(fractional)
        .filter(|amount| *amount > 0)
        .ok_or_else(invalid)
}


fn instruction(ix: spl_token::solana_program::instruction::Instruction) -> Instruction {
    Instruction {
        program_id: Pubkey::new_from_array(ix.program_id.to_bytes()),
        data: ix.data,
        accounts: ix
            .accounts
            .into_iter()
            .map(|a| solana_instruction::AccountMeta {
                pubkey: Pubkey::new_from_array(a.pubkey.to_bytes()),
                is_signer: a.is_signer,
                is_writable: a.is_writable,
            })
            .collect(),
    }
}

fn require_local(state: &AppState) -> Result<(), TransferError> {
    let rpc = url::Url::parse(&state._rpc.url()).map_err(rpc_error)?;
    if state.network != "surfpool"
        || !matches!(rpc.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Err(TransferError::bad(
            "unsupported_network",
            "This transfer checkpoint is enabled only for Surfpool with a local RPC URL.",
        ));
    }
    Ok(())
}

pub async fn prepare(
    state: &AppState,
    user: &AuthenticatedUser,
    input: TransferInput,
) -> Result<TransferView, TransferError> {
    prepare_with_bill(state, user, input, None).await
}

pub(crate) async fn prepare_with_bill(
    state: &AppState,
    user: &AuthenticatedUser,
    input: TransferInput,
    bill_payment: Option<BillPaymentContext>,
) -> Result<TransferView, TransferError> {
    user.require_scope("transactions:write").map_err(|_| {
        TransferError::bad("forbidden", "Reconnect with transactions:write permission.")
    })?;
    require_local(state)?;
    let sender = user.wallet;
    let recipient = Pubkey::from_str(input.recipient.trim()).map_err(|_| {
        TransferError::bad(
            "invalid_recipient",
            "Recipient must be a valid Solana wallet address.",
        )
    })?;
    if !recipient.is_on_curve() || recipient == sender {
        return Err(TransferError::bad(
            "invalid_recipient",
            "Use a different on-curve recipient wallet address.",
        ));
    }
    let rpc = state._rpc.as_ref();
    let sender_account = rpc.get_account(&sender).await.map_err(rpc_error)?;
    let system = Pubkey::from_str("11111111111111111111111111111111").unwrap();
    if sender_account.owner != system
        || !sender_account.data.is_empty()
        || sender_account.executable
    {
        return Err(TransferError::bad(
            "invalid_sender",
            "The fee payer must be a standard system wallet.",
        ));
    }
    let recipient_account = rpc
        .get_account_with_commitment(&recipient, rpc.commitment())
        .await
        .map_err(rpc_error)?
        .value;
    if recipient_account
        .as_ref()
        .is_some_and(|a| a.owner != system || a.executable || !a.data.is_empty())
    {
        return Err(TransferError::bad(
            "invalid_recipient",
            "Provide a wallet address, not a mint, token account, or program account.",
        ));
    }
    // Fix the fee instructions before review so Phantom does not inject them
    // during signing and change the exact message the backend verifies.
    let mut instructions = transfer_budget();
    let mut rent = 0u64;
    let native = input.asset.trim().eq_ignore_ascii_case("SOL");
    let (asset, mint, decimals, amount) = if native {
        let amount = parse_amount(&input.amount, 9)?;
        instructions.push(instruction(solana_system_interface::instruction::transfer(
            &sender, &recipient, amount,
        )));
        ("SOL".to_owned(), None, 9, amount)
    } else {
        let metadata = state.tokens.resolve(&input.asset).await?;
        let mint = Pubkey::from_str(&metadata.id).map_err(|_| {
            TransferError::bad("invalid_mint", "Jupiter returned an invalid mint address.")
        })?;
        let mint_account = rpc.get_account(&mint).await.map_err(rpc_error)?;
        if mint_account.owner.to_bytes() != spl_token::ID.to_bytes() {
            return Err(TransferError::bad(
                "unsupported_token_program",
                "This checkpoint supports standard SPL tokens. Token-2022 extensions are not supported for transfers.",
            ));
        }
        let mint_data = Mint::unpack(&mint_account.data).map_err(|_| {
            TransferError::bad(
                "invalid_mint",
                "The address is not an initialized SPL mint.",
            )
        })?;
        let decimals = mint_data.decimals;
        let amount = parse_amount(&input.amount, decimals)?;
        let old_sender = sender.to_bytes().into();
        let old_recipient = recipient.to_bytes().into();
        let old_mint = mint.to_bytes().into();
        let destination =
            spl_associated_token_account_interface::address::get_associated_token_address(
                &old_recipient,
                &old_mint,
            );
        let destination_key = Pubkey::new_from_array(destination.to_bytes());
        let destination_account = rpc
            .get_account_with_commitment(&destination_key, rpc.commitment())
            .await
            .map_err(rpc_error)?
            .value;
        if let Some(account) = destination_account {
            let data = TokenAccount::unpack(&account.data).map_err(|_| {
                TransferError::bad(
                    "invalid_token_account",
                    "Recipient token account is invalid.",
                )
            })?;
            if account.owner.to_bytes() != spl_token::ID.to_bytes()
                || data.mint.to_bytes() != mint.to_bytes()
                || data.owner.to_bytes() != recipient.to_bytes()
                || data.state != AccountState::Initialized
            {
                return Err(TransferError::bad(
                    "invalid_token_account",
                    "Recipient token account has the wrong mint, owner, program, or is frozen.",
                ));
            }
        } else {
            rent = rpc
                .get_minimum_balance_for_rent_exemption(TokenAccount::LEN)
                .await
                .map_err(rpc_error)?;
        }
        instructions.push(instruction(spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&old_sender, &old_recipient, &old_mint, &spl_token::ID)));
        let accounts = rpc
            .get_token_accounts_by_owner(&sender, TokenAccountsFilter::Mint(mint))
            .await
            .map_err(rpc_error)?;
        let addresses: Vec<Pubkey> = accounts
            .iter()
            .map(|a| Pubkey::from_str(&a.pubkey))
            .collect::<Result<_, _>>()
            .map_err(rpc_error)?;
        if addresses.len() > 100 {
            return Err(TransferError::bad(
                "too_many_accounts",
                "This mint has too many source accounts for this simple transfer. Consolidate funds first.",
            ));
        }
        let raw_accounts = rpc
            .get_multiple_accounts(&addresses)
            .await
            .map_err(rpc_error)?;
        let mut sources = Vec::new();
        for (key, account) in addresses.into_iter().zip(raw_accounts) {
            let Some(account) = account else {
                continue;
            };
            if account.owner.to_bytes() != spl_token::ID.to_bytes() {
                continue;
            }
            let Ok(data) = TokenAccount::unpack(&account.data) else {
                continue;
            };
            if data.owner.to_bytes() == sender.to_bytes()
                && data.mint.to_bytes() == mint.to_bytes()
                && data.state == AccountState::Initialized
                && data.amount > 0
            {
                sources.push((key, data.amount));
            }
        }
        sources.sort_by(|a, b| b.1.cmp(&a.1));
        let mut remaining = amount;
        for (key, balance) in sources.into_iter().take(8) {
            let take = balance.min(remaining);
            instructions.push(instruction(
                spl_token::instruction::transfer_checked(
                    &spl_token::ID,
                    &key.to_bytes().into(),
                    &old_mint,
                    &destination,
                    &old_sender,
                    &[],
                    take,
                    decimals,
                )
                .map_err(rpc_error)?,
            ));
            remaining -= take;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return Err(TransferError::bad(
                "insufficient_token_balance",
                "Insufficient unfrozen token balance in up to eight source accounts.",
            ));
        }
        (metadata.symbol, Some(mint.to_string()), decimals, amount)
    };
    let (blockhash, last_valid_height) = rpc
        .get_latest_blockhash_with_commitment(rpc.commitment())
        .await
        .map_err(rpc_error)?;
    let message = Message::new_with_blockhash(&instructions, Some(&sender), &blockhash);
    let fee = rpc.get_fee_for_message(&message).await.map_err(rpc_error)?;
    let required = fee
        .checked_add(rent)
        .and_then(|v| v.checked_add(if native { amount } else { 0 }))
        .ok_or_else(|| {
            TransferError::bad("invalid_amount", "Transfer amount plus costs overflows.")
        })?;
    if sender_account.lamports < required {
        return Err(TransferError::bad(
            "insufficient_sol",
            "Insufficient SOL for the transfer, network fee, and recipient token-account rent.",
        ));
    }
    let tx = Transaction::new_unsigned(message);
    if bincode::serialized_size(&tx).map_err(rpc_error)? > 1232 {
        return Err(TransferError::bad(
            "transaction_too_large",
            "Too many source accounts for one transaction. Consolidate funds first.",
        ));
    }
    let simulation = rpc
        .simulate_transaction_with_config(
            &tx,
            RpcSimulateTransactionConfig {
                sig_verify: false,
                commitment: Some(rpc.commitment()),
                ..Default::default()
            },
        )
        .await
        .map_err(rpc_error)?;
    if let Some(error) = simulation.value.err {
        return Err(TransferError::bad(
            "simulation_failed",
            format!("Transaction simulation failed: {error:?}"),
        ));
    }
    let id = random_token();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(rpc_error)?
        .as_secs();
    let view = TransferView {
        transfer_id: id.clone(),
        sender: sender.to_string(),
        recipient: recipient.to_string(),
        asset,
        mint,
        amount: input.amount,
        raw_amount: amount.to_string(),
        decimals,
        network: state.network.clone(),
        rpc_genesis_hash: rpc.get_genesis_hash().await.map_err(rpc_error)?.to_string(),
        fee_lamports: fee.to_string(),
        account_creation_lamports: rent.to_string(),
        creates_recipient_token_account: rent > 0,
        simulation: "passed",
        signing_url: format!(
            "{}/transfers/approve/{id}",
            state.auth.config.frontend_origin
        ),
        expires_at_unix: now + 120,
        status: "awaiting_signature",
        signature: None,
        message: None,
        transaction_base64: None,
        bill_payment,
    };
    let mut store = state.transfers.0.lock().await;
    // Retain recent statuses for two hours. Avoid holding the global store lock during RPC calls.
    if store.len() >= 1000 {
        let mut expired = Vec::new();
        for (id, pending) in store.iter() {
            if let Ok(pending) = pending.try_lock() {
                if pending.retained_until < Instant::now() {
                    expired.push(id.clone());
                }
            }
        }
        for id in expired {
            store.remove(&id);
        }
        if store.len() >= 1000 {
            return Err(TransferError::bad(
                "capacity_reached",
                "Too many pending transfers. Try again later.",
            ));
        }
    }
    store.insert(
        id,
        Arc::new(Mutex::new(Pending {
            tx,
            view: view.clone(),
            last_valid_height,
            retained_until: Instant::now() + Duration::from_secs(7200),
        })),
    );
    Ok(view)
}

impl TransferStore {
    async fn entry(&self, id: &str) -> Result<Arc<Mutex<Pending>>, TransferError> {
        let entry = self.0.lock().await.get(id).cloned().ok_or_else(|| {
            TransferError::bad(
                "not_found",
                "Transfer not found. Ask the agent to prepare it again.",
            )
        })?;
        if entry.lock().await.retained_until <= Instant::now() {
            return Err(TransferError::bad(
                "not_found",
                "Transfer history expired. Ask the agent to prepare a new transfer.",
            ));
        }
        Ok(entry)
    }
}

async fn update_status(state: &AppState, pending: &mut Pending) -> Result<(), TransferError> {
    if matches!(pending.view.status, "confirmed" | "failed" | "expired") {
        return Ok(());
    }
    if let Some(signature) = &pending.view.signature {
        let signature = signature.parse().map_err(rpc_error)?;
        let statuses = state
            ._rpc
            .get_signature_statuses_with_history(&[signature])
            .await
            .map_err(rpc_error)?;
        if let Some(Some(status)) = statuses.value.first() {
            if let Some(error) = &status.err {
                pending.view.status = "failed";
                pending.view.message = Some(format!("Transaction failed: {error:?}"));
            } else if status.satisfies_commitment(state._rpc.commitment()) {
                pending.view.status = "confirmed";
                pending.view.message = None;
            }
            // A landed, successful transaction must not be marked expired while awaiting confirmation.
            return Ok(());
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(rpc_error)?
        .as_secs();
    if state._rpc.get_block_height().await.map_err(rpc_error)? > pending.last_valid_height
        || (pending.view.signature.is_none() && now >= pending.view.expires_at_unix)
    {
        pending.view.status = "expired";
        pending.view.message = Some("This transaction expired. Ask the agent to prepare a fresh transfer; review it before signing.".into());
    }
    Ok(())
}

pub async fn status(
    state: &AppState,
    user: &AuthenticatedUser,
    id: &str,
) -> Result<TransferView, TransferError> {
    user.require_scope("transactions:write")
        .map_err(|_| TransferError::bad("forbidden", "Missing transactions:write permission."))?;
    let entry = state.transfers.entry(id).await?;
    let mut pending = entry.lock().await;
    if pending.view.sender != user.wallet.to_string() {
        return Err(TransferError::bad(
            "not_found",
            "Transfer not found for this wallet.",
        ));
    }
    update_status(state, &mut pending).await?;
    Ok(pending.view.clone())
}

fn check_origin(state: &AppState, headers: &HeaderMap, write: bool) -> Result<(), TransferError> {
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if (write && origin.is_none())
        || origin.is_some_and(|origin| origin != state.auth.config.frontend_origin)
    {
        return Err(TransferError::bad(
            "forbidden",
            "Use the transfer approval page.",
        ));
    }
    Ok(())
}

async fn view(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<TransferView>, TransferError> {
    check_origin(&state, &headers, false)?;
    let entry = state.transfers.entry(&id).await?;
    let mut pending = entry.lock().await;
    update_status(&state, &mut pending).await?;
    let mut view = pending.view.clone();
    if view.status == "awaiting_signature" {
        view.transaction_base64 =
            Some(STANDARD.encode(bincode::serialize(&pending.tx).map_err(rpc_error)?));
    }
    Ok(Json(view))
}

#[derive(Deserialize)]
struct SignedInput {
    signed_transaction_base64: String,
}

fn verify_signed(pending: &Transaction, encoded: &str) -> Result<Transaction, TransferError> {
    if encoded.len() > 1700 {
        return Err(TransferError::bad(
            "invalid_transaction",
            "Signed transaction exceeds packet size.",
        ));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| TransferError::bad("invalid_transaction", "Invalid base64 transaction."))?;
    let signed: Transaction = bincode::options()
        .with_fixint_encoding()
        .with_limit(1232)
        .reject_trailing_bytes()
        .deserialize(&bytes)
        .map_err(|_| TransferError::bad("invalid_transaction", "Invalid transaction encoding."))?;
    if signed.message != pending.message {
        return Err(TransferError::bad(
            "transaction_mismatch",
            "Phantom returned a changed transaction message. Nothing was submitted. Prepare a fresh transfer after updating the backend.",
        ));
    }
    if signed.signatures.len() != 1 || signed.verify().is_err() {
        return Err(TransferError::bad(
            "invalid_signature",
            "The signature does not verify against the reviewed sender and message. Nothing was submitted.",
        ));
    }
    Ok(signed)
}

async fn submit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<SignedInput>,
) -> Result<Json<TransferView>, TransferError> {
    check_origin(&state, &headers, true)?;
    require_local(&state)?;
    let entry = state.transfers.entry(&id).await?;
    let mut pending = entry.lock().await;
    let signed = verify_signed(&pending.tx, &input.signed_transaction_base64)?;
    if pending
        .view
        .signature
        .as_ref()
        .is_some_and(|signature| signature != &signed.signatures[0].to_string())
    {
        return Err(TransferError::bad(
            "transaction_mismatch",
            "This transfer already has a different signature. Check its original status.",
        ));
    }
    update_status(&state, &mut pending).await?;
    if pending.view.status != "awaiting_signature" && pending.view.status != "submitted" {
        return Ok(Json(pending.view.clone()));
    }
    if state
        ._rpc
        .get_genesis_hash()
        .await
        .map_err(rpc_error)?
        .to_string()
        != pending.view.rpc_genesis_hash
    {
        return Err(TransferError::bad(
            "network_changed",
            "The RPC network changed after review. Prepare a new transfer.",
        ));
    }
    // Persist the deterministic signature BEFORE sending: a timeout cannot cause
    // a fresh payment. Retrying these exact signed bytes has the same signature.
    let retrying_submission = pending.view.signature.is_some();
    pending.view.signature = Some(signed.signatures[0].to_string());
    pending.view.status = "submitted";
    match state._rpc.send_transaction(&signed).await {
        Ok(_) => {
            pending.view.message = Some("Submitted to Surfpool; awaiting confirmation.".into())
        }
        Err(error) => {
            use solana_rpc_client::api::{
                client_error::ErrorKind,
                request::{RpcError, RpcResponseErrorData},
            };
            if let ErrorKind::RpcError(RpcError::RpcResponseError {
                data: RpcResponseErrorData::SendTransactionPreflightFailure(result),
                ..
            }) = error.kind()
            {
                if retrying_submission {
                    pending.view.message = Some("Retry preflight was rejected, but the ORIGINAL submission outcome is still uncertain. Keep checking its signature; do not create another payment.".into());
                } else {
                    pending.view.status = "failed";
                    pending.view.message = Some(format!(
                        "RPC preflight rejected this transaction: {:?}. Prepare again only after correcting the cause.",
                        result.err
                    ));
                }
            } else {
                pending.view.message = Some("RPC did not acknowledge submission. Check status or retry the SAME signed transaction. Do not prepare another payment until its outcome is known.".into());
            }
        }
    }
    update_status(&state, &mut pending).await?;
    Ok(Json(pending.view.clone()))
}

pub fn routes() -> Router<AppState> {
    // Random 256-bit approval IDs are capability links. Only the stored sender
    // can authorize a payment: submit verifies the exact message and signature.
    Router::new()
        .route("/api/transfers/{id}", get(view))
        .route("/api/transfers/{id}/submit", post(submit))
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response
                    .headers_mut()
                    .insert("cache-control", "no-store".parse().unwrap());
                response
                    .headers_mut()
                    .insert("referrer-policy", "no-referrer".parse().unwrap());
                response
            },
        ))
}

fn transfer_budget() -> Vec<Instruction> {
    use solana_compute_budget_interface::ComputeBudgetInstruction as Budget;
    vec![
        instruction(Budget::set_compute_unit_limit(200_000)),
        instruction(Budget::set_compute_unit_price(0)),
    ]
}
