use super::{authorization::private_response, error::AuthError, tokens::scope_matches};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Form, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    resource: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

fn required(value: Option<&str>) -> Result<&str, AuthError> {
    value
        .filter(|v| !v.is_empty())
        .ok_or(AuthError::InvalidRequest)
}

async fn token(
    State(state): State<AppState>,
    Form(request): Form<TokenRequest>,
) -> Result<Response, AuthError> {
    let config = &state.auth.config;
    if request.client_id != config.client_id {
        return Err(AuthError::InvalidClient);
    }
    if request.resource != config.resource {
        return Err(AuthError::InvalidGrant);
    }
    let tokens = match request.grant_type.as_str() {
        "authorization_code" => {
            let verifier = required(request.code_verifier.as_deref())?;
            if !(43..=128).contains(&verifier.len())
                || !verifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
            {
                return Err(AuthError::InvalidGrant);
            }
            let redirect = required(request.redirect_uri.as_deref())?;
            let code = state
                .auth
                .codes
                .take(required(request.code.as_deref())?)
                .await
                .ok_or(AuthError::InvalidGrant)?;
            let context = &code.login.authorization;
            let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            if context.client_id != request.client_id
                || context.resource != request.resource
                || context.redirect_uri != redirect
                || context.code_challenge != challenge
            {
                return Err(AuthError::InvalidGrant);
            }
            if !scope_matches(request.scope.as_deref(), &context.scopes) {
                return Err(AuthError::InvalidScope);
            }
            state.auth.tokens.issue(code.login, config).await?
        }
        "refresh_token" => {
            state
                .auth
                .tokens
                .refresh(
                    required(request.refresh_token.as_deref())?,
                    request.scope.as_deref(),
                    config,
                )
                .await?
        }
        _ => return Err(AuthError::UnsupportedGrantType),
    };
    Ok(private_response(Json(tokens).into_response()))
}

#[derive(Deserialize)]
struct RevokeRequest {
    client_id: String,
    token: String,
}

async fn revoke(
    State(state): State<AppState>,
    Form(request): Form<RevokeRequest>,
) -> Result<Response, AuthError> {
    if request.client_id != state.auth.config.client_id {
        return Err(AuthError::InvalidClient);
    }
    state
        .auth
        .tokens
        .revoke(&request.token, &request.client_id)
        .await;
    Ok(private_response(StatusCode::OK.into_response()))
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
}
