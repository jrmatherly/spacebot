//! Auth errors. Distinguishes 401 (no valid principal) from 403 (valid
//! principal, insufficient permission). Authz errors land here in Phase 4;
//! Phase 1 only uses the 401 variants plus 503 for JWKS unavailability.

use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no Authorization header")]
    MissingHeader,

    #[error("malformed Authorization header")]
    MalformedHeader,

    #[error("token signature invalid or issuer/audience mismatch")]
    InvalidToken,

    #[error("token expired or not yet valid")]
    TemporalInvalid,

    #[error("JWKS discovery unreachable and no cached key available")]
    JwksUnreachable,

    #[error("authorization decision: {0}")]
    Forbidden(String),
}

impl AuthError {
    pub fn status(&self) -> StatusCode {
        match self {
            AuthError::Forbidden(_) => StatusCode::FORBIDDEN,
            AuthError::JwksUnreachable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::UNAUTHORIZED,
        }
    }

    /// Structured label for the `spacebot_auth_failures_total{reason=…}` metric.
    /// Machine-readable. Stable across releases (add variants, do not rename).
    pub fn metric_reason(&self) -> &'static str {
        match self {
            AuthError::MissingHeader => "missing_header",
            AuthError::MalformedHeader => "malformed_header",
            AuthError::InvalidToken => "invalid_token",
            AuthError::TemporalInvalid => "temporal_invalid",
            AuthError::JwksUnreachable => "jwks_unreachable",
            AuthError::Forbidden(_) => "forbidden",
        }
    }
}
