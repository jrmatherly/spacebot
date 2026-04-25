//! Logout: remove the operator-local CLI token cache. Idempotent:
//! clearing an already-empty store is not an error.

use crate::cli::store::CliTokenStore;

/// Remove the operator-local CLI token cache. Idempotent: clearing an
/// already-empty store is not an error.
pub fn clear_tokens() -> anyhow::Result<()> {
    let store = CliTokenStore::load()?;
    store.clear()
}

#[cfg(test)]
mod tests {
    use super::CliTokenStore;

    #[test]
    fn clear_is_idempotent_when_explicit_path() {
        // Use the test seam directly; we don't want this unit test to touch
        // the operator's real `data_dir()` location.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("cli-tokens.json");
        let store = CliTokenStore::with_path(path.clone());
        store.save().expect("save empty");
        assert!(path.exists());

        CliTokenStore::clear_at(&path).expect("first clear");
        assert!(!path.exists());

        // Second clear MUST NOT error; missing file is fine.
        CliTokenStore::clear_at(&path).expect("second clear");
    }
}
