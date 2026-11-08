mod authorization;
mod challenge;
pub mod config;
mod discovery;
mod error;
mod middleware;
mod token_routes;
mod tokens;

pub use middleware::{identity, require_auth, whoami};
pub use tokens::AuthenticatedUser;

use axum::Router;

use crate::state::AppState;

use authorization::CodeStore;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use challenge::ChallengeStore;
use config::AuthConfig;
use rand::Rng;

pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Clone)]
pub struct AuthState {
    pub(crate) config: AuthConfig,
    challenges: ChallengeStore,
    codes: CodeStore,
    tokens: tokens::TokenStore,
}

impl AuthState {
    pub fn new(config: AuthConfig) -> Self {
        Self {
            challenges: ChallengeStore::new(config.frontend_origin.clone()),
            codes: CodeStore::default(),
            tokens: tokens::TokenStore::default(),
            config,
        }
    }
}

pub fn routes() -> Router<AppState> {
    authorization::routes()
        .merge(token_routes::routes())
        .merge(discovery::routes())
}
