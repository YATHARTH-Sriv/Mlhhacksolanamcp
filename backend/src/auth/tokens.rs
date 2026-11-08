use super::{challenge::VerifiedLogin, config::AuthConfig, error::AuthError, random_token};
use serde::Serialize;
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const ACCESS_TTL: Duration = Duration::from_secs(900);
const CONNECTION_TTL: Duration = Duration::from_secs(86400);
type Hash = [u8; 32];

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub wallet: Pubkey,
    pub scopes: Vec<String>,
}

impl AuthenticatedUser {
    pub fn require_scope(&self, scope: &str) -> Result<(), AuthError> {
        if self.scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err(AuthError::InsufficientScope)
        }
    }
}

struct Connection {
    user: AuthenticatedUser,
    client: String,
    resource: String,
    issuer: String,
    access: Hash,
    refresh: Hash,
    used_refresh: HashSet<Hash>,
    access_expiry: Instant,
    expiry: Instant,
}

#[derive(Serialize)]
pub(super) struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    refresh_token: String,
    scope: String,
}

#[derive(Clone, Default)]
pub(super) struct TokenStore(Arc<Mutex<HashMap<Hash, Connection>>>);

fn hash(value: &str) -> Hash {
    Sha256::digest(value.as_bytes()).into()
}

pub(super) fn scope_matches(requested: Option<&str>, granted: &[String]) -> bool {
    requested.is_none_or(|scope| {
        let requested: HashSet<_> = scope.split_whitespace().collect();
        let granted: HashSet<_> = granted.iter().map(String::as_str).collect();
        requested == granted
    })
}

fn rotate(connection: &mut Connection) -> TokenResponse {
    let access_token = random_token();
    let refresh_token = random_token();
    let now = Instant::now();
    let lifetime = ACCESS_TTL.min(connection.expiry.saturating_duration_since(now));
    connection.access = hash(&access_token);
    connection.refresh = hash(&refresh_token);
    connection.access_expiry = now + lifetime;
    TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer",
        expires_in: lifetime.as_secs(),
        scope: connection.user.scopes.join(" "),
    }
}

impl TokenStore {
    pub(super) async fn issue(
        &self,
        login: VerifiedLogin,
        config: &AuthConfig,
    ) -> Result<TokenResponse, AuthError> {
        let mut entries = self.0.lock().await;
        let now = Instant::now();
        entries.retain(|_, entry| entry.expiry > now);
        if entries.len() >= 1000 {
            return Err(AuthError::ServerBusy);
        }
        let mut entry = Connection {
            user: AuthenticatedUser {
                wallet: login.wallet,
                scopes: login.authorization.scopes,
            },
            client: login.authorization.client_id,
            resource: login.authorization.resource,
            issuer: config.origin.clone(),
            access: [0; 32],
            refresh: [0; 32],
            used_refresh: HashSet::new(),
            access_expiry: now,
            expiry: now + CONNECTION_TTL,
        };
        let tokens = rotate(&mut entry);
        entries.insert(hash(&random_token()), entry);
        Ok(tokens)
    }

    pub(super) async fn authenticate(
        &self,
        token: &str,
        config: &AuthConfig,
    ) -> Option<AuthenticatedUser> {
        let token = hash(token);
        let now = Instant::now();
        let mut entries = self.0.lock().await;
        entries.retain(|_, entry| entry.expiry > now);
        entries
            .values()
            .find(|entry| {
                entry.access == token
                    && entry.access_expiry > now
                    && entry.client == config.client_id
                    && entry.resource == config.resource
                    && entry.issuer == config.origin
            })
            .map(|entry| entry.user.clone())
    }

    pub(super) async fn refresh(
        &self,
        token: &str,
        scope: Option<&str>,
        config: &AuthConfig,
    ) -> Result<TokenResponse, AuthError> {
        let token = hash(token);
        let now = Instant::now();
        let mut entries = self.0.lock().await;
        entries.retain(|_, entry| entry.expiry > now);
        let id = entries
            .iter()
            .find(|(_, entry)| {
                entry.client == config.client_id
                    && entry.resource == config.resource
                    && entry.issuer == config.origin
                    && (entry.refresh == token || entry.used_refresh.contains(&token))
            })
            .map(|(id, _)| *id)
            .ok_or(AuthError::InvalidGrant)?;
        let entry = entries.get_mut(&id).ok_or(AuthError::InvalidGrant)?;
        // Detect refresh replay and bound retained history per connection.
        if entry.used_refresh.contains(&token) || entry.used_refresh.len() >= 256 {
            entries.remove(&id);
            return Err(AuthError::InvalidGrant);
        }
        if !scope_matches(scope, &entry.user.scopes) {
            return Err(AuthError::InvalidScope);
        }
        entry.used_refresh.insert(entry.refresh);
        Ok(rotate(entry))
    }

    pub(super) async fn revoke(&self, token: &str, client: &str) {
        let token = hash(token);
        self.0.lock().await.retain(|_, entry| {
            !(entry.client == client
                && (entry.access == token
                    || entry.refresh == token
                    || entry.used_refresh.contains(&token)))
        });
    }
}
