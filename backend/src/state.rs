use std::sync::Arc;

use solana_rpc_client::nonblocking::rpc_client::RpcClient;

use crate::auth::AuthState;
use crate::features::price::PriceClient;

#[derive(Clone)]
pub struct AppState {
    pub _rpc: Arc<RpcClient>,
    pub network: String,
    pub auth: AuthState,
    pub prices: PriceClient,
    pub transfers: crate::features::transactions::TransferStore,
    pub tokens: crate::features::tokens::TokenClient,
    pub bills: crate::features::invoice::BillService,
}
