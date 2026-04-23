//! Phase 6 Task 6.A.1 — `GET /api/auth/config` contract tests.
//!
//! Asserts:
//!   1. the endpoint is reachable without an `Authorization` header,
//!   2. when Entra IS configured, the response body never contains
//!      secret-adjacent substrings (C1 remediation: previously this ran
//!      against a state with no populated `EntraAuthConfig`, so the check
//!      was vacuously true against an empty payload),
//!   3. when Entra IS configured, the response body contains exactly the
//!      four non-secret identifiers the SPA needs and none of the other
//!      `EntraAuthConfig` fields (C2 populated-path coverage),
//!   4. when Entra is NOT configured, the response reports
//!      `entra_enabled: false` and omits the identifier fields.

// F1/F2/F3 corrections (2026-04-22 pre-code audit): helpers are
// `build_test_router_entra` (re-exported at `spacebot::api::test_support`),
// `ApiState::new_test_state_with_mock_entra[_configured]` (associated
// functions on `ApiState`, not free functions in `spacebot::api::state`),
// and `new_for_tests(None::<String>)` makes the `Option<String>` type
// explicit. Numeric line citations removed per the PR #107 review (I7):
// symbol names are stable, line numbers drift.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use serde::Deserialize;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use tower::ServiceExt as _;

/// Mirror of the handler response shape so tests can assert on parsed
/// fields rather than substring-matching JSON text. Decoupled from the
/// handler's `AuthConfigResponse` (which is `pub(super)`) so integration
/// tests in `tests/*.rs` compile without widening that visibility.
#[derive(Deserialize)]
struct ParsedConfig {
    entra_enabled: bool,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    authority: Option<String>,
    #[serde(default)]
    scopes: Option<Vec<String>>,
}

async fn get_config(app: axum::Router) -> (StatusCode, String, ParsedConfig) {
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    let parsed: ParsedConfig =
        serde_json::from_str(&body).expect("handler response must parse as AuthConfigResponse");
    (status, body, parsed)
}

#[tokio::test]
async fn config_endpoint_is_unprotected() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra_configured().await;
    let app = build_test_router_entra(state);
    let (status, _, parsed) = get_config(app).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "config endpoint must be reachable without bearer token"
    );
    assert!(
        parsed.entra_enabled,
        "populated fixture must report entra_enabled=true"
    );
}

/// C1 remediation: runs against a state with a POPULATED `EntraAuthConfig`
/// so the leak-scrub assertion has substance. Previously the test used
/// `new_test_state_with_mock_entra` (no `entra_config` attached) and the
/// response was `{"entra_enabled":false}` — the forbidden-substring
/// check passed against an empty payload.
#[tokio::test]
async fn config_endpoint_never_leaks_secrets() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra_configured().await;
    let app = build_test_router_entra(state);
    let (status, body, _) = get_config(app).await;
    assert_eq!(status, StatusCode::OK);

    // Secret-adjacent key names that must NEVER appear in the response.
    // A regression that widened `PublicEntraConfig::public()` to include
    // `audience`, `allowed_scopes`, `mock_mode`, or a future secret field
    // would surface here.
    for forbidden in [
        "client_secret",
        "graph_client_secret",
        "auth_token",
        "audience",
        "allowed_scopes",
        "mock_mode",
        "jwks_url_override",
        "issuer_override",
    ] {
        assert!(
            !body.contains(forbidden),
            "secret-adjacent key `{forbidden}` appeared in public config: {body}"
        );
    }
}

/// C2 remediation: explicit populated-path assertion. Pins the exact
/// shape the handler projects from a populated `EntraAuthConfig`.
#[tokio::test]
async fn config_endpoint_projects_populated_config_fields() {
    let (state, _pool) = ApiState::new_test_state_with_mock_entra_configured().await;
    let app = build_test_router_entra(state);
    let (status, _body, parsed) = get_config(app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(parsed.entra_enabled);
    assert_eq!(
        parsed.client_id.as_deref(),
        Some("spa-22222222-2222-2222-2222-222222222222"),
        "client_id must mirror EntraAuthConfig::spa_client_id"
    );
    assert_eq!(
        parsed.tenant_id.as_deref(),
        Some("tenant-11111111-1111-1111-1111-111111111111"),
        "tenant_id must mirror EntraAuthConfig::tenant_id"
    );
    assert_eq!(
        parsed.authority.as_deref(),
        Some(
            "https://login.microsoftonline.com/tenant-11111111-1111-1111-1111-111111111111/v2.0"
        ),
        "authority must be the v2.0 login URL computed from tenant_id"
    );
    assert_eq!(
        parsed.scopes.as_deref(),
        Some(&["api://web-test/api.access".to_string()][..]),
        "scopes must mirror EntraAuthConfig::spa_scopes verbatim"
    );
}

#[tokio::test]
async fn config_returns_entra_disabled_when_unconfigured() {
    // Build a state WITHOUT Entra. `new_for_tests(None::<String>)` produces a
    // minimal state for middleware-integration tests; `None::<String>` is
    // explicit so the Option type is inferable without ambiguity.
    let state = std::sync::Arc::new(ApiState::new_for_tests(None::<String>));
    let app = build_test_router_entra(state);
    let (status, _body, parsed) = get_config(app).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !parsed.entra_enabled,
        "unconfigured Entra must report entra_enabled=false"
    );
    assert!(
        parsed.client_id.is_none(),
        "client_id must be absent when entra_enabled=false"
    );
    assert!(
        parsed.tenant_id.is_none(),
        "tenant_id must be absent when entra_enabled=false"
    );
    assert!(
        parsed.authority.is_none(),
        "authority must be absent when entra_enabled=false"
    );
    assert!(
        parsed.scopes.is_none(),
        "scopes must be absent when entra_enabled=false"
    );
}
