//! Spacedrive integration.
//!
//! Runtime-gated (via `SpacedriveIntegrationConfig::enabled`). When disabled,
//! this module compiles but contributes no behavior. See
//! `docs/design-docs/spacedrive-integration-pairing.md` for the shared-state
//! contract with the Spacedrive side.

pub mod client;
pub mod config;
pub mod envelope;
pub mod error;
pub mod types;

pub use client::SpacedriveClient;
pub use config::SpacedriveIntegrationConfig;
pub use error::{Result, SpacedriveError};

use crate::secrets::store::SecretsStore;

/// Build a `SpacedriveClient` by reading the auth token from the secrets
/// store under the key `spacedrive_auth_token:<library_id>`.
///
/// Returns `MissingAuthToken` if the secret is absent or unreadable, and
/// `Disabled` if `library_id` is unset (unpaired). Callers should handle
/// both by surfacing a re-pair prompt or skipping tool registration with
/// a WARN log.
pub fn build_client_from_secrets(
    cfg: &SpacedriveIntegrationConfig,
    secrets: &SecretsStore,
) -> Result<SpacedriveClient> {
    let library_id = cfg.library_id.ok_or(SpacedriveError::Disabled)?;
    let key = format!("spacedrive_auth_token:{library_id}");

    let token = match secrets.get(&key) {
        Ok(decrypted) => decrypted.expose().to_string(),
        Err(crate::error::SecretsError::NotFound { .. }) => {
            return Err(SpacedriveError::MissingAuthToken {
                library_id: library_id.to_string(),
            });
        }
        Err(e) => {
            return Err(SpacedriveError::SecretsLookupFailed {
                library_id: library_id.to_string(),
                source: Box::new(e),
            });
        }
    };

    SpacedriveClient::new(cfg, token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::store::{SecretCategory, SecretsStore};
    use uuid::Uuid;

    fn temp_store() -> (SecretsStore, tempfile::NamedTempFile) {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let store = SecretsStore::new(file.path()).expect("create store");
        (store, file)
    }

    fn paired_cfg(library_id: Uuid) -> SpacedriveIntegrationConfig {
        SpacedriveIntegrationConfig {
            enabled: true,
            base_url: "http://127.0.0.1:8080".into(),
            library_id: Some(library_id),
            spacebot_instance_id: None,
        }
    }

    #[test]
    fn build_without_library_id_returns_disabled() {
        let (store, _f) = temp_store();
        let cfg = SpacedriveIntegrationConfig {
            enabled: true,
            base_url: "http://127.0.0.1:8080".into(),
            library_id: None,
            spacebot_instance_id: None,
        };
        let err = build_client_from_secrets(&cfg, &store).err().unwrap();
        assert!(matches!(err, SpacedriveError::Disabled));
    }

    #[test]
    fn build_with_missing_token_returns_missing_auth_token() {
        let (store, _f) = temp_store();
        let library_id = Uuid::new_v4();
        let cfg = paired_cfg(library_id);
        let err = build_client_from_secrets(&cfg, &store).err().unwrap();
        match err {
            SpacedriveError::MissingAuthToken {
                library_id: reported,
            } => {
                assert_eq!(reported, library_id.to_string());
            }
            other => panic!("expected MissingAuthToken, got {other:?}"),
        }
    }

    #[test]
    fn build_with_present_token_returns_client() {
        let (store, _f) = temp_store();
        let library_id = Uuid::new_v4();
        let key = format!("spacedrive_auth_token:{library_id}");
        store
            .set(&key, "secret-token", SecretCategory::Tool)
            .expect("set token");
        let cfg = paired_cfg(library_id);
        let client = build_client_from_secrets(&cfg, &store).expect("build client");
        drop(client);
    }
}
