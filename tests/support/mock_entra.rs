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
