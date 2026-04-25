//! Device code flow + client credentials for the CLI. Pure-logic types
//! and parsers live here and are independently tested. Side effects
//! (HTTP, token-store writes, terminal IO) live in `execute_login`.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponseBody {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    #[serde(default)]
    pub token_type: Option<String>,
}

#[derive(Debug)]
pub enum TokenPollOutcome {
    /// `authorization_pending`: keep polling at `interval`.
    Continue,
    /// `slow_down`: increase polling interval.
    Backoff,
    /// Tokens obtained.
    Success(TokenResponseBody),
    /// Fatal error: expired token, user declined, bad request.
    Fatal(String),
}

pub fn parse_device_code_response(body: &str) -> anyhow::Result<DeviceCodeResponse> {
    serde_json::from_str(body).map_err(|e| anyhow::anyhow!("parse devicecode: {e}"))
}

pub fn parse_token_response(status: u16, body: &str) -> TokenPollOutcome {
    if (200..300).contains(&status) {
        // Entra returns HTTP 200 with `error: authorization_pending` while
        // the user is signing in, so success and soft-error share status 200.
        if let Ok(success) = serde_json::from_str::<TokenResponseBody>(body) {
            return TokenPollOutcome::Success(success);
        }
    }
    #[derive(Deserialize)]
    struct ErrBody {
        error: Option<String>,
        #[serde(default)]
        error_description: Option<String>,
    }
    let err: ErrBody = match serde_json::from_str(body) {
        Ok(e) => e,
        Err(_) => {
            return TokenPollOutcome::Fatal(format!(
                "unparseable response (status {status}): {body}"
            ))
        }
    };
    match err.error.as_deref() {
        Some("authorization_pending") => TokenPollOutcome::Continue,
        Some("slow_down") => TokenPollOutcome::Backoff,
        Some("expired_token") | Some("access_denied") | Some("invalid_grant") => {
            TokenPollOutcome::Fatal(
                err.error_description
                    .unwrap_or_else(|| err.error.unwrap_or_default()),
            )
        }
        Some(other) => TokenPollOutcome::Fatal(format!("unexpected error: {other}")),
        None => TokenPollOutcome::Fatal(format!("response with neither tokens nor error: {body}")),
    }
}
