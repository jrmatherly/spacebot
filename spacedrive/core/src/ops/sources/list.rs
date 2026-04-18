//! Spacebot-authored stub for `ops::sources::list`.
//!
//! Upstream declares `pub mod list;` in `ops/sources/mod.rs` without a source
//! file. The only external importer is `ops::sources::get::query.rs`, which
//! constructs a `SourceInfo` via `SourceInfo::new(id, name, data_type,
//! adapter_id, item_count, last_synced, status)`. This stub provides that type
//! with the exact field shape and constructor signature required. Remove when
//! upstream ships a real `sources::list` implementation. Tracked in
//! `spacedrive/SYNC.md` LOCAL_CHANGES.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Public-facing summary of an archive source.
///
/// Field types are derived from the two constructor call sites
/// (`ops/sources/get/query.rs:78`) and the sibling `CreateSourceOutput` /
/// `SourceSyncJob` structs, which expose the same shape over the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SourceInfo {
	pub id: Uuid,
	pub name: String,
	pub data_type: String,
	pub adapter_id: String,
	pub item_count: u64,
	pub last_synced: Option<DateTime<Utc>>,
	pub status: String,
}

impl SourceInfo {
	pub fn new(
		id: Uuid,
		name: String,
		data_type: String,
		adapter_id: String,
		item_count: u64,
		last_synced: Option<DateTime<Utc>>,
		status: String,
	) -> Self {
		Self {
			id,
			name,
			data_type,
			adapter_id,
			item_count,
			last_synced,
			status,
		}
	}
}
