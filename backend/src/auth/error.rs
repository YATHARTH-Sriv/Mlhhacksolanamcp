use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub enum AuthError {
    InvalidChallenge,
    InvalidSignature,
    TooManyChallenges,
    ClockUnavailable,
    InvalidRequest,
    InvalidWallet,
    InvalidOrigin,
    InvalidClient,
    InvalidGrant,
    InvalidScope,
    UnsupportedGrantType,
    InsufficientScope,
    ServerBusy,
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    error_description: &'static str,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            Self::InvalidClient => (
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "Unknown OAuth client.",
            ),
            Self::InvalidGrant => (
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "Invalid, expired, or reused credential.",
            ),
            Self::InvalidScope => (
                StatusCode::BAD_REQUEST,
                "invalid_scope",
                "Unsupported permissions.",
            ),
            Self::UnsupportedGrantType => (
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "Unsupported grant type.",
            ),
            Self::InsufficientScope => (
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                "The required permission was not granted.",
            ),
            Self::ServerBusy => (
                StatusCode::SERVICE_UNAVAILABLE,
                "temporarily_unavailable",
                "Authentication capacity reached.",
            ),
            Self::InvalidChallenge => (
                StatusCode::BAD_REQUEST,
                "invalid_challenge",
                "The login challenge is invalid, expired, or already used.",
            ),
            Self::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "invalid_signature",
                "The signature does not match the requested wallet.",
            ),
            Self::TooManyChallenges => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_challenges",
                "Too many pending login attempts. Try again shortly.",
            ),
            Self::ClockUnavailable => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Unable to create a login challenge.",
            ),
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "The authorization request is invalid.",
            ),
            Self::InvalidWallet => (
                StatusCode::BAD_REQUEST,
                "invalid_wallet",
                "The wallet address is invalid.",
            ),
            Self::InvalidOrigin => (
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "This request must originate from the login page.",
            ),
        };

        let mut response = (
            status,
            Json(ErrorBody {
                error,
                error_description: message,
            }),
        )
            .into_response();
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
        if status == StatusCode::FORBIDDEN && error == "insufficient_scope" {
            response.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer error=\"insufficient_scope\""),
            );
        }
        response
    }
}
