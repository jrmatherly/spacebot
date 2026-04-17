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
