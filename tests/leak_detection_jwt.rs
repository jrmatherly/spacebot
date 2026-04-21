//! Regression tests for §12 C-Log-Scrubbing. JWTs must not survive log output.
//!
//! Uses the 1-arg `scrub_leaks(&str) -> String` function. The 2-arg variant
//! `scrub_secrets(text, tool_secrets)` is for exact-match scrubbing against
//! known stored tool secrets and is not exercised here.

use spacebot::secrets::scrub::scrub_leaks;

#[test]
fn scrubs_jwt_shape() {
    let jwt = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let logline = format!("Authorization: Bearer {jwt}");
    let scrubbed = scrub_leaks(&logline);
    assert!(
        !scrubbed.contains(jwt),
        "Full JWT survived scrubbing: {scrubbed}"
    );
}

#[test]
fn does_not_over_scrub_non_jwt_dots() {
    let benign = "request /api/foo.bar.baz returned 404";
    let scrubbed = scrub_leaks(benign);
    assert_eq!(scrubbed, benign);
}

#[test]
fn scrubs_multiple_jwts_in_one_line() {
    let jwt_a = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJhIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let jwt_b = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJiIn0.abcxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let logline = format!("token1={jwt_a} token2={jwt_b}");
    let scrubbed = scrub_leaks(&logline);
    assert!(!scrubbed.contains(jwt_a), "first JWT survived: {scrubbed}");
    assert!(!scrubbed.contains(jwt_b), "second JWT survived: {scrubbed}");
}
