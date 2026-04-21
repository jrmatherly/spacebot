//! Authentication subsystem. Mirrors `src/secrets/` layout: a flat module
//! root with sub-files per concern.
//!
//! Phase 1: JWT validation via Entra ID. Phase 2: principal-record
//! persistence. Phase 3: Microsoft Graph client for group resolution and
//! display-photo fetch. Authorization helpers land in Phase 4.

pub mod config;
pub mod context;
pub mod errors;
pub mod graph;
pub mod jwks;
pub mod middleware;
pub mod policy;
pub mod principals;
pub mod repository;
pub mod roles;
// Plain `pub mod` (no `cfg(test)` gate): integration tests under `tests/*.rs`
// are separate compilation units that cannot see `cfg(test)` items from the
// library. Precedent: `tests/support/mock_entra.rs` and `ApiState::new_for_tests`
// at `src/api/state.rs:401`.
pub mod testing;

pub use config::EntraAuthConfig;
pub use context::{AuthContext, PrincipalType};
pub use errors::AuthError;
pub use jwks::EntraValidator;
pub use policy::{
    Access, DenyReason, can_link_channel, check_read, check_read_with_audit, check_write,
};
pub use principals::{
    ResourceOwnershipRecord, ServiceAccountRecord, TeamMembershipRecord, TeamRecord, UserRecord,
    Visibility,
};
pub use roles::{ROLE_ADMIN, ROLE_SERVICE, ROLE_USER, is_admin, require_role};
