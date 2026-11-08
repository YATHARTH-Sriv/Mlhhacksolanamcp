use axum::{Json, Router, extract::DefaultBodyLimit, middleware, routing::get};
use serde::Serialize;

use crate::{auth::require_auth, features::portfolio::load_portfolio, state::AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

pub async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "backend",
    })
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/whoami", get(crate::auth::whoami))
        .route("/portfolio", get(load_portfolio))
        .merge(crate::mcp::routes(state.clone()))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    Router::new()
        .route("/health/live", get(live))
        .merge(protected)
        .merge(crate::auth::routes())
        .merge(crate::features::transactions::routes())
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state)
}
