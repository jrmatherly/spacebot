//! Pure-logic tests for the CLI device-code parser. Covers
//! `parse_device_code_response`, `parse_token_response`, and the
//! `TokenPollOutcome` mapping for `authorization_pending`, `slow_down`,
//! `expired_token`, and the success path.

use spacebot::cli::login::{parse_device_code_response, parse_token_response, TokenPollOutcome};

#[test]
fn parses_device_code_initiation() {
    let body = serde_json::json!({
        "user_code": "ABCD-1234",
        "device_code": "abc-xyz-device",
        "verification_uri": "https://microsoft.com/devicelogin",
        "expires_in": 900,
        "interval": 5,
        "message": "Enter ABCD-1234 at https://microsoft.com/devicelogin"
    })
    .to_string();
    let dc = parse_device_code_response(&body).unwrap();
    assert_eq!(dc.user_code, "ABCD-1234");
    assert_eq!(dc.device_code, "abc-xyz-device");
    assert_eq!(dc.interval, 5);
}

#[test]
fn maps_authorization_pending_to_continue() {
    let body = serde_json::json!({
        "error": "authorization_pending"
    })
    .to_string();
    let outcome = parse_token_response(200, &body);
    assert!(matches!(outcome, TokenPollOutcome::Continue));
}

#[test]
fn maps_slow_down_to_backoff() {
    let body = serde_json::json!({
        "error": "slow_down"
    })
    .to_string();
    let outcome = parse_token_response(200, &body);
    assert!(matches!(outcome, TokenPollOutcome::Backoff));
}

#[test]
fn maps_expired_token_to_error() {
    let body = serde_json::json!({
        "error": "expired_token"
    })
    .to_string();
    let outcome = parse_token_response(400, &body);
    assert!(matches!(outcome, TokenPollOutcome::Fatal(_)));
}

#[test]
fn parses_successful_token_response() {
    let body = serde_json::json!({
        "access_token": "at-abc",
        "refresh_token": "rt-xyz",
        "expires_in": 3600,
        "token_type": "Bearer"
    })
    .to_string();
    let outcome = parse_token_response(200, &body);
    match outcome {
        TokenPollOutcome::Success(tokens) => {
            assert_eq!(tokens.access_token, "at-abc");
            assert_eq!(tokens.refresh_token.as_deref(), Some("rt-xyz"));
        }
        other => panic!("expected Success, got {other:?}"),
    }
}
