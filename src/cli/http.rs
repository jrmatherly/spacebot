//! HTTP client for CLI ↔ daemon communication. Attaches Authorization
//! from the operator-local CliTokenStore, refreshes silently on 401
//! using the refresh token (STORE-D).

use anyhow::Context as _;
use reqwest::{Client, RequestBuilder};
use tokio::sync::Mutex;

use crate::cli::login::{TokenPollOutcome, persist_tokens};
use crate::cli::store::CliTokenStore;

pub struct AuthedClient {
    http: Client,
    store: Mutex<CliTokenStore>,
    base_url: String,
    client_id: String,
    token_url: String,
    refresh_lock: Mutex<()>,
}

impl AuthedClient {
    pub fn new(
        store: CliTokenStore,
        base_url: String,
        tenant_id: String,
        client_id: String,
    ) -> Self {
        let token_url = format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token");
        Self::with_token_url(store, base_url, client_id, token_url)
    }

    /// Test seam: point the refresh path at a custom token endpoint
    /// (used by wiremock-backed tests so the production Entra URL is
    /// never contacted).
    #[doc(hidden)]
    pub fn with_token_url(
        store: CliTokenStore,
        base_url: String,
        client_id: String,
        token_url: String,
    ) -> Self {
        Self {
            http: Client::new(),
            store: Mutex::new(store),
            base_url,
            client_id,
            token_url,
            refresh_lock: Mutex::new(()),
        }
    }

    // STORE-D: token reads come from the in-memory CliTokenStore. The
    // store is loaded once at construction and refreshed in place under
    // refresh_lock when send() encounters a 401.
    async fn cached_access_token(&self) -> Option<String> {
        self.store.lock().await.access_token.clone()
    }

    async fn refresh_access_token(&self) -> anyhow::Result<String> {
        let rt = {
            let store = self.store.lock().await;
            store
                .refresh_token
                .clone()
                .context("no refresh token; run `spacebot entra login`")?
        };
        let res = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &self.client_id),
                ("refresh_token", &rt),
            ])
            .send()
            .await?;
        let body = res.text().await?;
        match crate::cli::login::parse_token_response(200, &body) {
            TokenPollOutcome::Success(t) => {
                let mut store = self.store.lock().await;
                let access_token = t.access_token.clone();
                persist_tokens(&mut store, &t)?;
                Ok(access_token)
            }
            _ => anyhow::bail!("refresh failed; run `spacebot entra login`"),
        }
    }

    pub async fn send(&self, builder: RequestBuilder) -> anyhow::Result<reqwest::Response> {
        // Refresh discipline:
        // - Refresh runs under `refresh_lock` so concurrent send() calls
        //   don't both hit Entra's /token endpoint in parallel and trip
        //   the looping-client invalid_grant cutoff
        //   (https://learn.microsoft.com/entra/identity-platform/reference-breaking-changes#march-2019).
        // - Clone the builder once, attach bearer, send. On 401, refresh
        //   and retry with a fresh clone + new bearer. Bail only on the
        //   second 401. The 401 → refresh → retry → 200 path is the
        //   common case after a token expires mid-session.
        // - Never echo the access token into error messages.
        let token = match self.cached_access_token().await {
            Some(t) => t,
            None => self.refresh_access_token_guarded(None).await?,
        };
        let first_attempt = builder
            .try_clone()
            .context("request body not cloneable; AuthedClient requires clone-capable requests")?
            .bearer_auth(&token)
            .send()
            .await?;
        if first_attempt.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(first_attempt);
        }
        // First attempt got 401. Refresh and retry once. Pass the rejected
        // token so the guarded refresh's double-check distinguishes "another
        // caller already refreshed" (cache != rejected) from "we still hold
        // the same token that just 401'd" (cache == rejected, must refresh).
        let fresh = self.refresh_access_token_guarded(Some(token)).await?;
        let second_attempt = builder.bearer_auth(&fresh).send().await?;
        if second_attempt.status() == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("401 persists after token refresh; run `spacebot entra login`");
        }
        Ok(second_attempt)
    }

    /// Single-flight token refresh. Holds a `tokio::sync::Mutex` so
    /// concurrent send() calls serialize on the refresh and share the
    /// fresh token. `failed_token`, when set, is the token that was just
    /// rejected by the server; the cache double-check skips returning it
    /// even if it's still in the store, preventing the stale-cache loop
    /// where a caller acquires the lock, sees its own already-rejected
    /// token in cache, retries, and trips the second-401 bail.
    async fn refresh_access_token_guarded(
        &self,
        failed_token: Option<String>,
    ) -> anyhow::Result<String> {
        let _guard = self.refresh_lock.lock().await;
        // Double-check the cache; another caller may have refreshed.
        if let Some(t) = self.cached_access_token().await {
            if Some(&t) != failed_token.as_ref() {
                return Ok(t);
            }
        }
        self.refresh_access_token().await
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    pub fn base(&self) -> &str {
        &self.base_url
    }
}
