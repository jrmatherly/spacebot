//! Spacedrive integration.
//!
//! Runtime-gated (via `SpacedriveIntegrationConfig::enabled`). When disabled,
//! this module compiles but contributes no behavior. See
//! `docs/design-docs/spacedrive-integration-pairing.md` for the shared-state
//! contract with the Spacedrive side.

pub mod config;
pub mod error;

pub use config::SpacedriveIntegrationConfig;
pub use error::{Result, SpacedriveError};
