//! Entra desktop auth via loopback. One-shot HTTP listener on a
//! pre-registered 127.0.0.1 port, PKCE + cryptographically random `state`.
//!
//! The pre-registered port range lives in `docs/design-docs/entra-app-registrations.md`
//! and must stay in sync with the `redirect_uri` values the Entra app
//! registration accepts. Widening the range without updating the app
//! registration silently breaks production sign-in.

use anyhow::Context as _;
// rand 0.10 relocated `RngCore` to the `rand_core` crate, so the
// ergonomic import is the `Rng` extension trait from the rand root.
use rand::Rng;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::time::Duration;

/// Ephemeral port range pre-registered as desktop redirect URIs on the
/// Entra app registration. See docs/design-docs/entra-app-registrations.md.
pub const DESKTOP_PORT_RANGE: std::ops::RangeInclusive<u16> = 50000..=50009;

/// Budget for malformed loopback requests before we abandon a sign-in.
/// Stops a local attacker who can send to 127.0.0.1 from burning the
/// 5-minute timeout window with 400/404 spam.
const MAX_BAD_REQUESTS: u32 = 16;

pub struct AuthorizeParams<'a> {
    pub tenant_id: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub scopes: &'a [String],
    pub state: &'a str,
    pub code_challenge: &'a str,
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    use base64::Engine;
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

pub fn build_authorize_url(p: &AuthorizeParams<'_>) -> String {
    let scope = p.scopes.join(" ");
    let mut u = url::Url::parse(&format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
        p.tenant_id
    ))
    .expect("parse authorize url");
    u.query_pairs_mut()
        .append_pair("client_id", p.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", p.redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", &scope)
        .append_pair("state", p.state)
        .append_pair("code_challenge", p.code_challenge)
        .append_pair("code_challenge_method", "S256");
    u.to_string()
}

/// Bind the first available port in the pre-registered range. Fails loudly
/// if no port is free so a foreign process can't hijack the redirect.
pub fn bind_loopback() -> anyhow::Result<(StdTcpListener, u16)> {
    for port in DESKTOP_PORT_RANGE {
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .expect("loopback SocketAddr parse is infallible for u16 port");
        if let Ok(l) = StdTcpListener::bind(addr) {
            return Ok((l, port));
        }
    }
    anyhow::bail!(
        "no loopback port available in pre-registered range {}..={}",
        DESKTOP_PORT_RANGE.start(),
        DESKTOP_PORT_RANGE.end()
    )
}

/// Wait for exactly one HTTP GET to /callback. Verify `state` matches.
/// Returns the `code` query parameter on success.
pub async fn accept_callback(
    listener: StdTcpListener,
    expected_state: &str,
) -> anyhow::Result<String> {
    listener
        .set_nonblocking(true)
        .context("set loopback listener nonblocking")?;
    let tokio_listener = tokio::net::TcpListener::from_std(listener)
        .context("convert std listener to tokio listener")?;
    let timeout = Duration::from_secs(300);

    let accept_fut = async {
        let mut bad: u32 = 0;
        loop {
            let (socket, _addr) = tokio_listener
                .accept()
                .await
                .context("accept loopback connection")?;
            if let Err(e) = socket.set_nodelay(true) {
                tracing::warn!(%e, "failed to set TCP_NODELAY on loopback socket");
            }
            match handle_one_request(socket, expected_state).await {
                Ok(code) => return Ok::<String, anyhow::Error>(code),
                Err(e) => {
                    bad = bad.saturating_add(1);
                    tracing::warn!(%e, bad_count = bad, "loopback request rejected; waiting for next");
                    if bad >= MAX_BAD_REQUESTS {
                        anyhow::bail!(
                            "too many bad loopback requests ({bad}); abandoning sign-in flow"
                        );
                    }
                }
            }
        }
    };

    tokio::time::timeout(timeout, accept_fut)
        .await
        .map_err(|_| {
            anyhow::anyhow!("loopback timeout: user did not complete sign-in within 5 minutes")
        })?
}

async fn handle_one_request(
    socket: tokio::net::TcpStream,
    expected_state: &str,
) -> anyhow::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = Vec::with_capacity(2048);
    let mut sock = socket;
    let mut tmp = [0u8; 1024];
    while buf.len() < 4096 {
        let n = sock.read(&mut tmp).await.context("read loopback request")?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    // Distinguish genuinely oversized headers from malformed first lines
    // so logs don't conflate the two failure modes.
    if buf.len() >= 4096 && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        send_400(&mut sock).await;
        anyhow::bail!("request headers exceeded 4096-byte cap before end-of-headers");
    }
    let head = std::str::from_utf8(&buf).context("loopback request is not UTF-8")?;
    let first_line = head.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_ascii_whitespace().collect();
    if parts.len() < 3 || parts[0] != "GET" {
        send_400(&mut sock).await;
        anyhow::bail!("non-GET or malformed request line");
    }
    let path_and_query = parts[1];
    let url = url::Url::parse(&format!("http://loopback{path_and_query}"))
        .context("parse callback path+query")?;
    if url.path() != "/callback" {
        send_404(&mut sock).await;
        anyhow::bail!("unexpected path {}", url.path());
    }
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = q
        .get("state")
        .cloned()
        .context("callback missing state parameter")?;
    if state != expected_state {
        send_400(&mut sock).await;
        anyhow::bail!("state mismatch (possible CSRF / race)");
    }
    let code = q
        .get("code")
        .cloned()
        .context("callback missing code parameter")?;

    let body = r#"<html><body>Sign-in complete. You can close this window and return to Spacebot.</body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    // Code is already captured; a failed courtesy response doesn't abort
    // the sign-in. Log at debug so audit still shows the failure shape.
    if let Err(e) = sock.write_all(response.as_bytes()).await {
        tracing::debug!(%e, "loopback success response write failed; code already captured");
    }
    if let Err(e) = sock.shutdown().await {
        tracing::debug!(%e, "loopback socket shutdown failed");
    }
    Ok(code)
}

async fn send_400(sock: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncWriteExt;
    if let Err(e) = sock
        .write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n")
        .await
    {
        tracing::debug!(%e, "loopback 400 response write failed");
    }
}

async fn send_404(sock: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncWriteExt;
    if let Err(e) = sock
        .write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n")
        .await
    {
        tracing::debug!(%e, "loopback 404 response write failed");
    }
}

/// Entra v2.0 token-endpoint response shape.
///
/// Custom `Debug` elides every secret-bearing field so accidental
/// `tracing::debug!(?response)` calls can't leak the bearer token.
/// Pattern mirrors `GraphConfig` at src/auth/graph.rs:48-62.
#[derive(serde::Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Seconds from issuance (relative, per RFC 6749 §5.1). Do not
    /// persist as absolute epoch without explicit conversion.
    pub expires_in: u64,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "<redacted>"))
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

pub async fn exchange_code(
    tenant_id: &str,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
    scopes: &[String],
) -> anyhow::Result<TokenResponse> {
    let url = format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token");
    let params = [
        ("client_id", client_id),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", pkce_verifier),
        ("scope", &scopes.join(" ")),
    ];
    let client = reqwest::Client::new();
    let res = client
        .post(&url)
        .form(&params)
        .send()
        .await
        .context("POST to Entra token endpoint failed")?;
    let status = res.status();
    // Buffer the body regardless of status so we never lose the
    // AADSTS error payload that lives in the response text of a
    // non-2xx Entra reply.
    let body = res.text().await.context("read token response body")?;
    if !status.is_success() {
        anyhow::bail!("token exchange failed: {status} {body}");
    }
    serde_json::from_str::<TokenResponse>(&body).with_context(|| {
        let prefix_end = body.len().min(256);
        format!(
            "decode token response ({status}); body prefix: {:?}",
            &body[..prefix_end]
        )
    })
}
