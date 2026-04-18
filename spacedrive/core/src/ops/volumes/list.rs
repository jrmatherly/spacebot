//! Spacebot-authored stub for `ops::volumes::list`.
//!
//! Upstream declares `pub mod list;` in `ops/volumes/mod.rs` without a source
//! file, yet also performs a named re-export of four concrete types:
//! `pub use list::{VolumeFilter, VolumeListOutput, VolumeListQuery, VolumeListQueryInput};`
//!
//! This stub provides those four types so `sd-core` compiles. Remove when
//! upstream ships a real `volumes::list` implementation. Tracked in
//! `spacedrive/SYNC.md` LOCAL_CHANGES.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Filter for volume listing. Stub: matches the variants used by
/// `apps/cli/src/domains/{volume,cloud,location}` (only `TrackedOnly` is
/// observed in-tree).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, Default)]
pub enum VolumeFilter {
	#[default]
	All,
	TrackedOnly,
}

/// Input for a volume list query. Matches the shape used by
/// `apps/cli/src/domains/cloud/setup.rs` (`VolumeListQueryInput { filter: ... }`).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
pub struct VolumeListQueryInput {
	#[serde(default)]
	pub filter: VolumeFilter,
}

/// Output of a volume list query. The CLI assigns
/// `sd_core::ops::volumes::list::VolumeListOutput` directly from a query result,
/// so this type carries the actual payload shape. Stub: empty list placeholder.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Default)]
pub struct VolumeListOutput {
	#[serde(default)]
	pub volumes: Vec<serde_json::Value>,
}

/// Query struct marker. Stub: not registered as a real `CoreQuery` / `LibraryQuery`
/// impl — upstream would wire that registration once this module is filled in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeListQuery {
	pub input: VolumeListQueryInput,
}
