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

use std::time::Duration;

use crate::cli::store::CliTokenStore;

pub struct LoginArgs {
    pub tenant_id: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

pub async fn execute_login(args: LoginArgs) -> anyhow::Result<TokenResponseBody> {
    // SPACEBOT_CLIENT_ID + SPACEBOT_CLIENT_SECRET trigger client_credentials
    // (machine-to-machine). Otherwise fall through to device-code (humans).
    if let (Ok(client_id), Ok(client_secret)) = (
        std::env::var("SPACEBOT_CLIENT_ID"),
        std::env::var("SPACEBOT_CLIENT_SECRET"),
    ) {
        return execute_client_credentials(
            &args.tenant_id,
            &client_id,
            &client_secret,
            &args.scopes,
        )
        .await;
    }

    let http = reqwest::Client::new();
    let device_url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode",
        args.tenant_id
    );
    // Device-code flow must include `offline_access` in the scope set;
    // without it Entra never issues a refresh_token, and every
    // `AuthedClient` call after `expires_in` forces a re-login. Ref:
    // https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code#authenticating-the-user
    let scope = {
        let mut s = args.scopes.clone();
        if !s.iter().any(|x| x == "offline_access") {
            s.push("offline_access".into());
        }
        s.join(" ")
    };
    let dc_resp = http
        .post(&device_url)
        .form(&[
            ("client_id", args.client_id.as_str()),
            ("scope", scope.as_str()),
        ])
        .send()
        .await?;
    let dc_body = dc_resp.text().await?;
    let dc = parse_device_code_response(&dc_body)?;

    // Display to the terminal. The phishing-warning text is required by
    // §12 S-C3: device-code is a known phishing vector, and the only
    // mitigation at the prompt is warning the operator.
    println!();
    println!();
    println!("  To sign in, open {}", dc.verification_uri);
    println!("  and enter this code: {}", dc.user_code);
    println!();
    println!(
        "  ⚠  Only enter this code if YOU initiated `spacebot entra login` just now."
    );
    println!(
        "  ⚠  Never enter a device code someone else sent you. That is a phishing attack."
    );
    println!();
    println!();
    println!("This prompt expires in {} seconds.", dc.expires_in);

    let token_url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        args.tenant_id
    );
    let mut interval = Duration::from_secs(dc.interval);
    let deadline = std::time::Instant::now() + Duration::from_secs(dc.expires_in);
    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("device code expired before user completed sign-in");
        }
        tokio::time::sleep(interval).await;
        let res = http
            .post(&token_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", args.client_id.as_str()),
                ("device_code", dc.device_code.as_str()),
            ])
            .send()
            .await?;
        let status = res.status().as_u16();
        let body = res.text().await?;
        match parse_token_response(status, &body) {
            TokenPollOutcome::Success(t) => return Ok(t),
            TokenPollOutcome::Continue => continue,
            TokenPollOutcome::Backoff => {
                interval += Duration::from_secs(5);
            }
            TokenPollOutcome::Fatal(msg) => anyhow::bail!("{msg}"),
        }
    }
}

async fn execute_client_credentials(
    tenant_id: &str,
    client_id: &str,
    client_secret: &str,
    scopes: &[String],
) -> anyhow::Result<TokenResponseBody> {
    let url = format!(
        "https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token"
    );
    let scope = scopes.first().cloned().unwrap_or_default();
    let scope_default = format!("{scope}/.default");
    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("scope", &scope_default),
    ];
    let http = reqwest::Client::new();
    let res = http.post(&url).form(&params).send().await?;
    let status = res.status().as_u16();
    let body = res.text().await?;
    match parse_token_response(status, &body) {
        TokenPollOutcome::Success(t) => Ok(t),
        TokenPollOutcome::Fatal(msg) => anyhow::bail!("{msg}"),
        _ => anyhow::bail!("unexpected client-credentials poll outcome"),
    }
}

/// Mutate the in-memory store with the freshly-minted tokens, then flush
/// to disk. Synchronous: callers MUST NOT `.await` this.
pub fn persist_tokens(
    store: &mut CliTokenStore,
    tokens: &TokenResponseBody,
) -> anyhow::Result<()> {
    store.access_token = Some(tokens.access_token.clone());
    if let Some(rt) = &tokens.refresh_token {
        store.refresh_token = Some(rt.clone());
    }
    // The wall-clock `expires_at` is for operator inspection only; the
    // JWT `exp` claim is authoritative for the auth path.
    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(tokens.expires_in.min(i64::MAX as u64) as i64);
    store.expires_at = Some(expires_at);
    store.save()?;
    Ok(())
}
