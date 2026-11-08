use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::sync::Mutex;

use super::{error::AuthError, random_token};

const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING_CHALLENGES: usize = 1_000;

// Created by our OAuth handler AFTER validating the incoming request.
// Do not deserialize this directly from an unvalidated request body.
#[derive(Clone)]
pub(super) struct AuthorizationContext {
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub code_challenge: String,
}

// Stored only on the server.
struct PendingChallenge {
    wallet: Pubkey,
    message: String,
    authorization: AuthorizationContext,
    expires_at: Instant,
}

// Returned to the browser so the wallet can sign the exact message.
#[derive(Serialize)]
pub(super) struct ChallengeResponse {
    pub challenge_id: String,
    pub wallet_address: String,
    pub message: String,
    pub expires_in: u64,
}

// Internal result of successful signature verification.
// The OAuth handler will use this to create an authorization code.
pub(super) struct VerifiedLogin {
    pub wallet: Pubkey,
    pub authorization: AuthorizationContext,
}

#[derive(Clone)]
pub(super) struct ChallengeStore {
    application_origin: String,
    pending: Arc<Mutex<HashMap<String, PendingChallenge>>>,
}

impl ChallengeStore {
    pub(super) fn new(application_origin: String) -> Self {
        Self {
            application_origin,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(super) async fn create(
        &self,
        wallet: Pubkey,
        authorization: AuthorizationContext,
    ) -> Result<ChallengeResponse, AuthError> {
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::ClockUnavailable)?
            .as_secs();

        let expires_at = issued_at + CHALLENGE_TTL.as_secs();

        let challenge_id = random_token();
        let nonce = random_token();

        let message = format!(
            "Sign in to Hover MCP\n\
             \n\
             Origin: {}\n\
             Wallet: {}\n\
             Client: {}\n\
             Resource: {}\n\
             Permissions: {}\n\
             Nonce: {}\n\
             Issued at: {}\n\
             Expires at: {}\n\
             \n\
             This signature grants the permissions listed above. \
             It does not sign a blockchain transaction. Every transfer requires a separate wallet signature.",
            self.application_origin,
            wallet,
            authorization.client_id,
            authorization.resource,
            authorization.scopes.join(" "),
            nonce,
            issued_at,
            expires_at,
        );

        let mut pending = self.pending.lock().await;

        // Remove expired entries before accepting another challenge.
        pending.retain(|_, entry| entry.expires_at > Instant::now());

        if pending.len() >= MAX_PENDING_CHALLENGES {
            return Err(AuthError::TooManyChallenges);
        }

        pending.insert(
            challenge_id.clone(),
            PendingChallenge {
                wallet,
                message: message.clone(),
                authorization,
                expires_at: Instant::now() + CHALLENGE_TTL,
            },
        );

        Ok(ChallengeResponse {
            challenge_id,
            wallet_address: wallet.to_string(),
            message,
            expires_in: CHALLENGE_TTL.as_secs(),
        })
    }

    pub(super) async fn verify(
        &self,
        challenge_id: &str,
        signature_bytes: &[u8],
    ) -> Result<VerifiedLogin, AuthError> {
        // Remove under a lock: only one request can consume this challenge.
        let pending = {
            let mut challenges = self.pending.lock().await;
            challenges.remove(challenge_id)
        }
        .ok_or(AuthError::InvalidChallenge)?;

        if pending.expires_at <= Instant::now() {
            return Err(AuthError::InvalidChallenge);
        }

        let signature =
            Signature::try_from(signature_bytes).map_err(|_| AuthError::InvalidSignature)?;

        if !signature.verify(pending.wallet.as_ref(), pending.message.as_bytes()) {
            return Err(AuthError::InvalidSignature);
        }

        Ok(VerifiedLogin {
            wallet: pending.wallet,
            authorization: pending.authorization,
        })
    }
}
