use crate::state::AppState;
use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

async fn authorization_server(State(state): State<AppState>) -> Json<Value> {
    let config = &state.auth.config;
    Json(json!({
        "issuer": config.origin,
        "authorization_endpoint": format!("{}/oauth/authorize", config.origin),
        "token_endpoint": format!("{}/oauth/token", config.origin),
        "revocation_endpoint": format!("{}/oauth/revoke", config.origin),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "authorization_response_iss_parameter_supported": true,
        "scopes_supported": ["identity:read", "portfolio:read", "transactions:write"]
    }))
}

async fn resource(State(state): State<AppState>) -> Json<Value> {
    let config = &state.auth.config;
    Json(
        json!({"resource": config.resource, "authorization_servers": [config.origin],
        "scopes_supported": ["identity:read", "portfolio:read", "transactions:write"], "bearer_methods_supported": ["header"]}),
    )
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(resource))
}
