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
        Err(_) => {
            return Err(SpacedriveError::MissingAuthToken {
                library_id: library_id.to_string(),
            });
        }
    };

    SpacedriveClient::new(cfg, token)
}
