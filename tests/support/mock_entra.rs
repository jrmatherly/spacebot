//! Test helper: spin up a Wiremock-backed fake Entra OIDC discovery + JWKS
//! endpoint, and issue test JWTs signed with a generated RSA key.
//!
//! Not a production path. Only Phase 1 integration tests use this.

use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rsa::RsaPrivateKey;
use rsa::pkcs8::EncodePrivateKey;
use rsa::rand_core::OsRng;
use rsa::traits::PublicKeyParts;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct MockTenant {
    pub server: MockServer,
    pub tenant_id: String,
    pub audience: String,
    pub signing_key: EncodingKey,
    pub kid: String,
    #[allow(dead_code)]
    pub jwks: serde_json::Value,
}

impl MockTenant {
    pub async fn start() -> Self {
        // A-07: pure-Rust RSA (no libssl system dep).
        // `rsa::rand_core::OsRng` is re-exported from `rand_core` 0.6 which
        // the project's `rand = "0.10"` (rand_core 0.9) can't provide.
        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let public_key = private_key.to_public_key();

        // EncodingKey::from_rsa_pem expects PKCS#8 PEM.
        let pkcs8 = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("pkcs8 pem serialize");
        let signing_key = EncodingKey::from_rsa_pem(pkcs8.as_bytes()).expect("rsa encoding key");

        // JWK `n` and `e` are base64url-unpadded encodings of modulus and
        // public exponent.
        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(e_bytes);
        let kid = "test-kid-1".to_string();

        let jwks = json!({
            "keys": [{
                "kid": kid,
                "kty": "RSA",
                "alg": "RS256",
                "use": "sig",
                "n": n,
                "e": e,
            }]
        });

        let server = MockServer::start().await;
        let tenant_id = "00000000-0000-0000-0000-000000000001".to_string();
        let audience = "api://test".to_string();

        Mock::given(method("GET"))
            .and(path(format!(
                "/{tenant_id}/v2.0/.well-known/openid-configuration"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "issuer": format!("{}/{}/v2.0", server.uri(), tenant_id),
                "jwks_uri": format!("{}/{}/discovery/v2.0/keys", server.uri(), tenant_id),
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/{tenant_id}/discovery/v2.0/keys")))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks.clone()))
            .mount(&server)
            .await;

        Self {
            server,
            tenant_id,
            audience,
            signing_key,
            kid,
            jwks,
        }
    }

    /// Issuer URL to use in minted tokens and in `EntraAuthConfig::issuer`
    /// comparisons. Points at the mock server, not login.microsoftonline.com.
    pub fn issuer(&self) -> String {
        format!("{}/{}/v2.0", self.server.uri(), self.tenant_id)
    }

    /// JWKS URL override to hand to `EntraAuthConfig::jwks_url_override`.
    pub fn jwks_url(&self) -> String {
        format!(
            "{}/{}/discovery/v2.0/keys",
            self.server.uri(),
            self.tenant_id
        )
    }

    /// Mint a user (delegated) token. Has `scp` claim so validator
    /// classifies the principal as `User`.
    pub fn mint_user_token(&self, oid: &str, roles: &[&str], groups: &[&str]) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "scp": "api.access",
            "roles": roles,
            "groups": groups,
            "preferred_username": format!("{oid}@example.com"),
            "name": "Test User",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a token with a bad signature (signed with a throwaway key).
    pub fn mint_wrong_sig_token(&self, oid: &str) -> String {
        let mut rng = OsRng;
        let other_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let pkcs8 = other_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("pkcs8 serialize");
        let other_signing_key =
            EncodingKey::from_rsa_pem(pkcs8.as_bytes()).expect("rsa encoding key");
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "scp": "api.access",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &other_signing_key).expect("jwt encode")
    }

    /// Mint a user token with a caller-chosen `scp` value. Used to exercise
    /// the scope-mismatch rejection path.
    pub fn mint_user_token_with_scope(&self, oid: &str, scp: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "scp": scp,
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a service-principal (app-only) token. No `scp` claim, so the
    /// validator classifies the principal as `ServicePrincipal`. `roles`
    /// controls whether the required-role gate rejects.
    pub fn mint_service_principal_token(&self, oid: &str, roles: &[&str]) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "roles": roles,
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a token whose `exp` is in the past (expired).
    pub fn mint_expired_token(&self, oid: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now - 3600,
            "nbf": now - 7200,
            "iat": now - 7200,
            "scp": "api.access",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a token whose `nbf` is in the future (not-yet-valid).
    pub fn mint_not_yet_valid_token(&self, oid: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 7200,
            "nbf": now + 3600,
            "iat": now,
            "scp": "api.access",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a token with a caller-chosen `aud` value.
    pub fn mint_token_with_aud(&self, oid: &str, aud: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": self.issuer(),
            "aud": aud,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "scp": "api.access",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }

    /// Mint a token with a caller-chosen `iss` value.
    pub fn mint_token_with_iss(&self, oid: &str, iss: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({
            "iss": iss,
            "aud": self.audience,
            "tid": self.tenant_id,
            "oid": oid,
            "sub": oid,
            "exp": now + 3600,
            "nbf": now - 60,
            "iat": now,
            "scp": "api.access",
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        encode(&header, &claims, &self.signing_key).expect("jwt encode")
    }
}

/// Mount the OBO token-exchange stub. Returns a fixed `access_token` so
/// downstream Graph stubs see a `Bearer` header but don't need to verify it.
/// Phase 3 callers point `GraphConfig::obo_token_endpoint` at the URL
/// returned by `obo_endpoint_url(&server)`.
#[allow(dead_code)] // used only by tests/graph_integration.rs
pub async fn mount_obo_stub(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fake-graph-token",
            "token_type": "Bearer",
            "expires_in": 3600,
        })))
        .mount(server)
        .await;
}

/// Convenience: the URL Phase 3 should set as `GraphConfig::obo_token_endpoint`
/// when wired against a Wiremock-backed `mount_obo_stub`.
#[allow(dead_code)] // used only by tests/graph_integration.rs
pub fn obo_endpoint_url(server: &MockServer) -> String {
    format!("{}/oauth2/v2.0/token", server.uri())
}

/// Mount Wiremock stubs for Phase 3 Graph endpoints. Serves
/// `/me/getMemberObjects` (returns the GUIDs in `groups`) and a single
/// `/groups?$filter=...` stub that returns ALL stubbed groups in one
/// response body. This matches the chunked-filter wire shape the Phase 3
/// `list_member_groups` implementation produces (one filter request per
/// chunk of 15 IDs). Pass an empty vec to simulate "user is in no groups".
#[allow(dead_code)] // used only by tests/graph_integration.rs
pub async fn mount_graph_stub(server: &MockServer, groups: Vec<(String, String)>) {
    let ids: Vec<String> = groups.iter().map(|(id, _)| id.clone()).collect();

    Mock::given(method("POST"))
        .and(path("/me/getMemberObjects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": ids })))
        .mount(server)
        .await;

    let groups_json: Vec<serde_json::Value> = groups
        .iter()
        .map(|(id, name)| json!({ "id": id, "displayName": name }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/groups"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": groups_json })))
        .mount(server)
        .await;
}

/// Mount the `/me/photo/$value` binary-response stub. Pass `None` to
/// simulate user-has-no-photo (Graph returns 404).
#[allow(dead_code)] // used only by tests/graph_integration.rs
pub async fn mount_photo_stub(server: &MockServer, photo_bytes: Option<Vec<u8>>) {
    let resp = match photo_bytes {
        Some(bytes) => ResponseTemplate::new(200)
            .set_body_bytes(bytes)
            .insert_header("Content-Type", "image/jpeg"),
        None => ResponseTemplate::new(404),
    };
    Mock::given(method("GET"))
        .and(path("/me/photo/$value"))
        .respond_with(resp)
        .mount(server)
        .await;
}
