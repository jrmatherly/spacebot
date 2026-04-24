//! Entra desktop auth via loopback. One-shot HTTP listener on a
//! pre-registered 127.0.0.1 port. PKCE + cryptographically random `state`.

use rand::Rng;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::time::Duration;

/// Ephemeral port range pre-registered as desktop redirect URIs on the
/// Entra app registration. See docs/design-docs/entra-app-registrations.md.
pub const DESKTOP_PORT_RANGE: std::ops::RangeInclusive<u16> = 50000..=50009;

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
pub fn bind_loopback() -> Result<(StdTcpListener, u16), String> {
    for port in DESKTOP_PORT_RANGE {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        match StdTcpListener::bind(addr) {
            Ok(l) => return Ok((l, port)),
            Err(_) => continue,
        }
    }
    Err("no loopback port available in pre-registered range 50000-50009".into())
}

/// Wait for exactly one HTTP GET to /callback. Verify `state` matches.
/// Returns the `code` query parameter on success.
pub async fn accept_callback(
    listener: StdTcpListener,
    expected_state: &str,
) -> Result<String, String> {
    listener.set_nonblocking(true).ok();
    let tokio_listener =
        tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
    let timeout = Duration::from_secs(300);

    let accept_fut = async {
        loop {
            let (socket, _addr) = tokio_listener
                .accept()
                .await
                .map_err(|e| e.to_string())?;
            socket.set_nodelay(true).ok();
            match handle_one_request(socket, expected_state).await {
                Ok(code) => return Ok::<String, String>(code),
                Err(e) => {
                    tracing::warn!(%e, "loopback request rejected; waiting for next");
                    continue;
                }
            }
        }
    };

    tokio::time::timeout(timeout, accept_fut)
        .await
        .map_err(|_| {
            "loopback timeout: user did not complete sign-in within 5 minutes".to_string()
        })?
}

async fn handle_one_request(
    socket: tokio::net::TcpStream,
    expected_state: &str,
) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = Vec::with_capacity(2048);
    let mut sock = socket;
    let mut tmp = [0u8; 1024];
    while buf.len() < 4096 {
        let n = sock.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let head = std::str::from_utf8(&buf).map_err(|e| e.to_string())?;
    let first_line = head.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_ascii_whitespace().collect();
    if parts.len() < 3 || parts[0] != "GET" {
        send_400(&mut sock).await;
        return Err("non-GET or malformed request".into());
    }
    let path_and_query = parts[1];
    let url = url::Url::parse(&format!("http://loopback{path_and_query}"))
        .map_err(|e| e.to_string())?;
    if url.path() != "/callback" {
        send_404(&mut sock).await;
        return Err(format!("unexpected path {}", url.path()));
    }
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let state = q.get("state").cloned().ok_or("missing state")?;
    if state != expected_state {
        send_400(&mut sock).await;
        return Err("state mismatch (possible CSRF / race)".into());
    }
    let code = q.get("code").cloned().ok_or("missing code")?;

    let body = r#"<html><body>Sign-in complete. You can close this window and return to Spacebot.</body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = sock.write_all(response.as_bytes()).await;
    let _ = sock.shutdown().await;
    Ok(code)
}

async fn send_400(sock: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncWriteExt;
    let _ = sock
        .write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n")
        .await;
}

async fn send_404(sock: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncWriteExt;
    let _ = sock
        .write_all(b"HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n")
        .await;
}

#[derive(Debug, serde::Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: u64,
}

pub async fn exchange_code(
    tenant_id: &str,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
    scopes: &[String],
) -> Result<TokenResponse, String> {
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
        .map_err(|e| e.to_string())?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("token exchange failed: {status} {body}"));
    }
    res.json::<TokenResponse>().await.map_err(|e| e.to_string())
}
