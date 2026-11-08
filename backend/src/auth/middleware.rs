use super::{authorization::private_response, error::AuthError, tokens::AuthenticatedUser};
use crate::state::AppState;
use axum::{
    Extension, Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            let mut parts = v.split_whitespace();
            let scheme = parts.next()?;
            let token = parts.next()?;
            (scheme.eq_ignore_ascii_case("bearer") && parts.next().is_none() && token.len() <= 1024)
                .then(|| token.to_owned())
        });
    if let Some(token) = token {
        if let Some(user) = state
            .auth
            .tokens
            .authenticate(&token, &state.auth.config)
            .await
        {
            request.extensions_mut().insert(user);
            return private_response(next.run(request).await);
        }
    }
    let challenge = format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\", error=\"invalid_token\", scope=\"identity:read portfolio:read transactions:write\"",
        state.auth.config.origin
    );
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error":"invalid_token"})),
    )
        .into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_str(&challenge).expect("validated origin"),
    );
    private_response(response)
}

#[derive(Serialize)]
pub struct IdentityResponse {
    wallet_address: String,
    network: String,
    scopes: Vec<String>,
}

pub fn identity(user: &AuthenticatedUser, network: &str) -> Result<IdentityResponse, AuthError> {
    user.require_scope("identity:read")?;
    Ok(IdentityResponse {
        wallet_address: user.wallet.to_string(),
        network: network.to_owned(),
        scopes: user.scopes.clone(),
    })
}

pub async fn whoami(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<IdentityResponse>, AuthError> {
    Ok(Json(identity(&user, &state.network)?))
}
