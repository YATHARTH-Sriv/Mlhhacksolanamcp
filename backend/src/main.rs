use std::{env, sync::Arc};

mod auth;
mod features;
mod mcp;
mod router;
mod state;

use solana_rpc_client::nonblocking::rpc_client::RpcClient;

use crate::{router::router, state::AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let rpc_url = env::var("RPC_URL")?;
    let rpc_client = Arc::new(RpcClient::new_with_timeout(
        rpc_url,
        std::time::Duration::from_secs(15),
    ));

    let auth_config = auth::config::AuthConfig::from_env()?;

    let state = AppState {
        _rpc: rpc_client,
        network: env::var("SOLANA_NETWORK")?,
        auth: auth::AuthState::new(auth_config),
        prices: features::price::PriceClient::new(env::var("JUPITER_API_KEY").ok())?,
        transfers: Default::default(),
        tokens: features::tokens::TokenClient::new(env::var("JUPITER_API_KEY").ok())?,
        bills: features::invoice::BillService::default(),
    };

    let app = router(state);

    let port = env::var("PORT")?;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("Hover MCP listening on port {port}; endpoint /mcp");

    axum::serve(listener, app).await?;

    Ok(())
}
