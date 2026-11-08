use std::{
    collections::HashMap,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use tokio::sync::Mutex;
use url::Url;

use crate::state::AppState;

use super::{
    challenge::{AuthorizationContext, VerifiedLogin},
    config::AuthConfig,
    error::AuthError,
    random_token,
};

const CODE_TTL: Duration = Duration::from_secs(60);
const MAX_CODES: usize = 1_000;
const LOGIN_COOKIE: &str = "HoverMCPagent_login";

pub(super) struct AuthorizationCode {
    pub login: VerifiedLogin,
    pub expires_at: Instant,
}

#[derive(Clone, Default)]
pub(super) struct CodeStore {
    entries: Arc<Mutex<HashMap<String, AuthorizationCode>>>,
}

impl CodeStore {
    async fn issue(&self, login: VerifiedLogin) -> Result<String, AuthError> {
        let mut entries = self.entries.lock().await;
        entries.retain(|_, code| code.expires_at > Instant::now());

        if entries.len() >= MAX_CODES {
            return Err(AuthError::TooManyChallenges);
        }

        let code = random_token();

        entries.insert(
            code.clone(),
            AuthorizationCode {
                login,
                expires_at: Instant::now() + CODE_TTL,
            },
        );

        Ok(code)
    }

    // The token endpoint will consume codes using this method.
    pub(super) async fn take(&self, code: &str) -> Option<AuthorizationCode> {
        let entry = self.entries.lock().await.remove(code)?;

        if entry.expires_at <= Instant::now() {
            return None;
        }

        Some(entry)
    }
}

#[derive(Deserialize, Serialize)]
struct AuthorizeRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    resource: String,
    code_challenge: String,
    code_challenge_method: String,
}

#[derive(Deserialize)]
struct ChallengeRequest {
    wallet_address: String,
    authorization: AuthorizeRequest,
}

#[derive(Deserialize)]
struct VerifyRequest {
    challenge_id: String,
    signature: Vec<u8>,
}

#[derive(Serialize)]
struct VerifyResponse {
    redirect_to: String,
}

fn validate_authorization(
    request: AuthorizeRequest,
    config: &AuthConfig,
) -> Result<AuthorizationContext, AuthError> {
    if request.response_type != "code"
        || request.client_id != config.client_id
        || request.redirect_uri != config.redirect_uri
        || request.resource != config.resource
        || request.code_challenge_method != "S256"
        || request.state.is_empty()
        || request.state.len() > 1_024
    {
        return Err(AuthError::InvalidRequest);
    }

    // SHA-256 produces 32 bytes, represented as 43 unpadded base64url chars.
    let decoded = URL_SAFE_NO_PAD
        .decode(&request.code_challenge)
        .map_err(|_| AuthError::InvalidRequest)?;

    if request.code_challenge.len() != 43 || decoded.len() != 32 {
        return Err(AuthError::InvalidRequest);
    }

    let mut scopes: Vec<String> = request
        .scope
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    scopes.sort();
    scopes.dedup();

    if scopes.is_empty()
        || scopes.iter().any(|scope| {
            !matches!(
                scope.as_str(),
                "identity:read" | "portfolio:read" | "transactions:write"
            )
        })
    {
        return Err(AuthError::InvalidScope);
    }

    Ok(AuthorizationContext {
        client_id: request.client_id,
        redirect_uri: request.redirect_uri,
        state: request.state,
        resource: request.resource,
        scopes,
        code_challenge: request.code_challenge,
    })
}

fn require_origin(headers: &HeaderMap, config: &AuthConfig) -> Result<(), AuthError> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());

    if origin != Some(config.frontend_origin.as_str()) {
        return Err(AuthError::InvalidOrigin);
    }

    Ok(())
}

fn login_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            (name == LOGIN_COOKIE).then_some(value)
        })
}

fn set_login_cookie(response: &mut Response, config: &AuthConfig, value: &str, max_age: u32) {
    let secure = if config.frontend_origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };

    // value is either our base64url challenge ID or an empty string.
    let cookie = format!(
        "{LOGIN_COOKIE}={value}; Path=/oauth; HttpOnly; SameSite=Strict; \
         Max-Age={max_age}{secure}"
    );

    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("generated login cookie must be a valid header"),
    );
}

pub(super) fn private_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    response
}

async fn authorize(
    State(state): State<AppState>,
    Query(request): Query<AuthorizeRequest>,
) -> Result<Response, AuthError> {
    let query = serde_urlencoded::to_string(&request).map_err(|_| AuthError::InvalidRequest)?;
    validate_authorization(request, &state.auth.config)?;
    let destination = format!("{}/auth/connect?{query}", state.auth.config.frontend_origin);
    Ok(private_response(Redirect::to(&destination).into_response()))
}

async fn create_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChallengeRequest>,
) -> Result<Response, AuthError> {
    require_origin(&headers, &state.auth.config)?;

    // Validate again: callers can bypass the HTML page and POST directly.
    let authorization = validate_authorization(request.authorization, &state.auth.config)?;

    let wallet = Pubkey::from_str(&request.wallet_address).map_err(|_| AuthError::InvalidWallet)?;

    let challenge = state.auth.challenges.create(wallet, authorization).await?;

    let challenge_id = challenge.challenge_id.clone();
    let mut response = Json(challenge).into_response();

    set_login_cookie(&mut response, &state.auth.config, &challenge_id, 300);

    Ok(private_response(response))
}

async fn verify_wallet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VerifyRequest>,
) -> Result<Response, AuthError> {
    require_origin(&headers, &state.auth.config)?;

    if login_cookie(&headers) != Some(request.challenge_id.as_str()) {
        return Err(AuthError::InvalidChallenge);
    }

    let login = state
        .auth
        .challenges
        .verify(&request.challenge_id, &request.signature)
        .await?;

    // Build the redirect using the saved context, not new client input.
    let mut redirect =
        Url::parse(&login.authorization.redirect_uri).map_err(|_| AuthError::InvalidRequest)?;

    let oauth_state = login.authorization.state.clone();
    let code = state.auth.codes.issue(login).await?;

    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &oauth_state)
        .append_pair("iss", &state.auth.config.origin);

    let mut response = Json(VerifyResponse {
        redirect_to: redirect.to_string(),
    })
    .into_response();

    set_login_cookie(&mut response, &state.auth.config, "", 0);

    Ok(private_response(response))
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/wallet/challenge", post(create_challenge))
        .route("/oauth/wallet/verify", post(verify_wallet))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_scope_is_explicit_and_read_scopes_still_work() {
        let config = AuthConfig {
            origin: "http://localhost:3000".into(),
            frontend_origin: "http://localhost:3000".into(),
            client_id: "test".into(),
            redirect_uri: "http://localhost:3000/health/live".into(),
            resource: "http://localhost:3000/mcp".into(),
        };
        for scope in [
            "identity:read portfolio:read",
            "identity:read portfolio:read transactions:write",
        ] {
            let context = validate_authorization(
                AuthorizeRequest {
                    response_type: "code".into(),
                    client_id: config.client_id.clone(),
                    redirect_uri: config.redirect_uri.clone(),
                    scope: scope.into(),
                    state: "state".into(),
                    resource: config.resource.clone(),
                    code_challenge: random_token(),
                    code_challenge_method: "S256".into(),
                },
                &config,
            )
            .unwrap();
            assert_eq!(
                context.scopes.contains(&"transactions:write".into()),
                scope.contains("transactions:write")
            );
        }
    }
}
