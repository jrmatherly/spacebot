//! Authentication subsystem. Mirrors `src/secrets/` layout: a flat module
//! root with sub-files per concern.
//!
//! Phase 1 scope: JWT validation via Entra ID. Authorization helpers land in
//! Phase 4.

pub mod config;
pub mod context;
pub mod errors;
pub mod jwks;
pub mod middleware;

pub use config::EntraAuthConfig;
pub use context::{AuthContext, PrincipalType};
pub use errors::AuthError;
pub use jwks::EntraValidator;
