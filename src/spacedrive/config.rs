//! Configuration for the Spacedrive integration.
//!
//! Shape defined by the pairing ADR (decisions D2, D3) at
//! `docs/design-docs/spacedrive-integration-pairing.md`.
//!
//! TOML-visible fields are `enabled` and `base_url` only. The auth token
//! NEVER lands in TOML; it's resolved from `src/secrets/store.rs` at
//! client-construction time using the key format
//! `spacedrive_auth_token:<library_id>` per ADR D3.
//!
//! `library_id` and `spacebot_instance_id` are reserved as `Option<Uuid>` to
//! land the config shape Phase 3's pairing flow will populate. They are not
//! hand-edited in TOML.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Top-level `[spacedrive]` config block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpacedriveIntegrationConfig {
    /// Master switch. When false, the integration is invisible at runtime.
    pub enabled: bool,

    /// Base URL for the paired Spacedrive HTTP server, e.g.
    /// `http://127.0.0.1:8080` for co-located dev, or an `https://` URL for
    /// remote deployments. Must use `https://` unless the host is
    /// `localhost` / `127.0.0.1` (enforced at config-load time in Phase 2).
    pub base_url: String,

    /// Library ID the Spacebot instance is paired with. Populated by the
    /// pairing flow (Phase 3). Not hand-edited.
    #[serde(default)]
    pub library_id: Option<Uuid>,

    /// Spacebot instance ID recorded in Spacedrive's SpacebotConfig during
    /// pairing. Populated by the pairing flow.
    #[serde(default)]
    pub spacebot_instance_id: Option<Uuid>,
}

impl Default for SpacedriveIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:8080".to_string(),
            library_id: None,
            spacebot_instance_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let cfg = SpacedriveIntegrationConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.base_url, "http://127.0.0.1:8080");
        assert!(cfg.library_id.is_none());
    }

    #[test]
    fn deserializes_minimal_block() {
        let toml_src = r#"
            enabled = true
            base_url = "http://127.0.0.1:8080"
        "#;
        let cfg: SpacedriveIntegrationConfig = toml::from_str(toml_src).unwrap();
        assert!(cfg.enabled);
        assert!(
            cfg.library_id.is_none(),
            "library_id absent from TOML is None"
        );
    }

    #[test]
    fn deserializes_with_reserved_fields() {
        let toml_src = r#"
            enabled = true
            base_url = "http://127.0.0.1:8080"
            library_id = "a1b2c3d4-1234-5678-9abc-def012345678"
        "#;
        let cfg: SpacedriveIntegrationConfig = toml::from_str(toml_src).unwrap();
        assert!(cfg.library_id.is_some());
    }
}
