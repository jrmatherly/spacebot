//! Round-trip + permission-bit tests for the CLI token cache. Uses
//! tempdir paths via `with_path` and `load_from` so the production
//! `cli_token_store_path()` resolver (which reads `$HOME` /
//! `$XDG_DATA_HOME`) is never invoked.

use spacebot::cli::store::CliTokenStore;

#[test]
fn load_missing_file_returns_empty_store() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("cli-tokens.json");
    let store = CliTokenStore::load_from(&path).expect("load");
    assert!(store.access_token.is_none());
    assert!(store.refresh_token.is_none());
    assert!(store.expires_at.is_none());
}

#[test]
fn save_then_load_round_trips_all_fields() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("cli-tokens.json");
    let mut store = CliTokenStore::with_path(path.clone());
    store.access_token = Some("at".into());
    store.refresh_token = Some("rt".into());
    store.expires_at = Some(chrono::Utc::now());
    store.save().expect("save");

    let reloaded = CliTokenStore::load_from(&path).expect("reload");
    assert_eq!(reloaded.access_token.as_deref(), Some("at"));
    assert_eq!(reloaded.refresh_token.as_deref(), Some("rt"));
    assert!(reloaded.expires_at.is_some());
}

#[cfg(unix)]
#[test]
fn save_writes_mode_0600_on_posix() {
    use std::os::unix::fs::PermissionsExt as _;
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("cli-tokens.json");
    let store = CliTokenStore::with_path(path.clone());
    store.save().expect("save");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected 0600 got {mode:o}");
}

#[test]
fn clear_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("cli-tokens.json");
    let store = CliTokenStore::with_path(path.clone());
    store.save().expect("save");
    assert!(path.exists());
    CliTokenStore::clear_at(&path).expect("first clear");
    assert!(!path.exists());
    CliTokenStore::clear_at(&path).expect("second clear (missing file)");
}
