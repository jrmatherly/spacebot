//! Spacebot-authored `SourceManager` stub.
//!
//! Signatures are derived from the six call sites that trigger E0282 type
//! inference errors when this file is absent:
//!
//! - `library/mod.rs:149` — `SourceManager::new(path).await.map_err(|e| Other(format!("{e}")))`
//! - `ops/sources/create/action.rs:54` — `.create_source(&str, &str, serde_json::Value).await.map_err(format!)`
//! - `ops/sources/delete/action.rs:53` — `.delete_source(&str).await.map_err(|e| Internal(e))`
//! - `ops/sources/get/query.rs:63` — `.list_sources().await.map_err(|e| Internal(e))`
//! - `ops/sources/list_items/query.rs:72` — `.list_items(&str, usize, usize).await.map_err(|e| Internal(e))`
//! - `ops/sources/sync/job.rs:79` — `.sync_source(&str).await.map_err(format!)`
//!
//! Error type: most call sites use `.map_err(|e| ...Internal(e))` where the
//! variant holds a `String`. Using `String` as the error type everywhere makes
//! inference succeed at all six sites. `new` also goes through a `format!`, so
//! `String` is compatible there too.
//!
//! Runtime behavior: every method returns `Err("not implemented ...")` or an
//! empty success value. The goal is a buildable `sd-server` for the Task 18
//! smoke test — not real archive-source execution.

use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// In-memory summary of a source, as surfaced to callers.
///
/// Field shape matches `ops/sources/list::SourceInfo` and the projection at
/// `ops/sources/get/query.rs:78` (where `source.id: String` is parsed into a
/// `Uuid` by the caller, so we keep `id` as `String` here).
#[derive(Debug, Clone)]
pub struct SourceEntry {
	pub id: String,
	pub name: String,
	pub data_type: String,
	pub adapter_id: String,
	pub item_count: u64,
	pub last_synced: Option<DateTime<Utc>>,
	pub status: String,
}

/// One item within a source. Field shape matches the projection at
/// `ops/sources/list_items/query.rs:80-87`.
#[derive(Debug, Clone)]
pub struct SourceItemEntry {
	pub id: String,
	pub external_id: String,
	pub title: String,
	pub preview: Option<String>,
	pub subtitle: Option<String>,
}

/// Sync report. Field shape matches the consumption at
/// `ops/sources/sync/job.rs:90-105`.
#[derive(Debug, Clone)]
pub struct SyncReport {
	pub records_upserted: u64,
	pub records_deleted: u64,
	pub error: Option<String>,
}

/// Adapter-config field description. Field shape matches the projection at
/// `ops/adapters/config/query.rs:80-89`. `default` is `Option<String>` because
/// the caller invokes `.map(|d| d.to_string())` on it, which is a no-op on
/// `String` and keeps type inference happy.
#[derive(Debug, Clone)]
pub struct AdapterConfigFieldEntry {
	pub key: String,
	pub name: String,
	pub description: String,
	pub field_type: String,
	pub required: bool,
	pub secret: bool,
	pub default: Option<String>,
}

/// Adapter-update result. Field shape matches the projection at
/// `ops/adapters/update/action.rs:62-67`.
#[derive(Debug, Clone)]
pub struct UpdateAdapterResult {
	pub adapter_id: String,
	pub old_version: String,
	pub new_version: String,
	pub schema_changed: bool,
}

/// Manager for archive data sources attached to a library.
///
/// Stub implementation. `_library_path` is held only so the struct isn't a
/// ZST (future upstream impl will need the path; keeping it avoids churn when
/// the real version lands).
#[derive(Debug)]
pub struct SourceManager {
	_library_path: PathBuf,
}

impl SourceManager {
	/// Create a new manager for the given library path.
	///
	/// Caller at `library/mod.rs:149` wraps the result in `Arc::new(...)`, so
	/// this returns `Self`, not `Arc<Self>`.
	pub async fn new(library_path: PathBuf) -> Result<Self, String> {
		Ok(Self {
			_library_path: library_path,
		})
	}

	/// Create a new archive source. Stub: always fails — real creation requires
	/// upstream's sd-archive wiring.
	pub async fn create_source(
		&self,
		_name: &str,
		_adapter_id: &str,
		_config: serde_json::Value,
	) -> Result<SourceEntry, String> {
		Err("source creation not implemented in Spacebot's sd-core stub".to_string())
	}

	/// Delete an archive source by ID. Stub: always succeeds silently so
	/// cleanup flows don't block on the stub.
	pub async fn delete_source(&self, _source_id: &str) -> Result<(), String> {
		Ok(())
	}

	/// List all archive sources in this library. Stub: returns an empty list.
	pub async fn list_sources(&self) -> Result<Vec<SourceEntry>, String> {
		Ok(Vec::new())
	}

	/// List items within a source, paginated. Stub: returns an empty page.
	pub async fn list_items(
		&self,
		_source_id: &str,
		_limit: usize,
		_offset: usize,
	) -> Result<Vec<SourceItemEntry>, String> {
		Ok(Vec::new())
	}

	/// Sync a source. Stub: reports a no-op sync with no error.
	pub async fn sync_source(&self, _source_id: &str) -> Result<SyncReport, String> {
		Ok(SyncReport {
			records_upserted: 0,
			records_deleted: 0,
			error: None,
		})
	}

	/// Describe the configuration fields exposed by an adapter. Synchronous
	/// (matches the call site in `ops/adapters/config/query.rs:72-75`, which
	/// does not `.await`). Stub: returns an empty field list.
	pub fn adapter_config_fields(
		&self,
		_adapter_id: &str,
	) -> Result<Vec<AdapterConfigFieldEntry>, String> {
		Ok(Vec::new())
	}

	/// Trigger an adapter schema update. Synchronous per the call site in
	/// `ops/adapters/update/action.rs:56-58`. Stub: always fails because the
	/// real operation requires upstream's adapter-update pipeline.
	pub fn update_adapter(&self, _adapter_id: &str) -> Result<UpdateAdapterResult, String> {
		Err("adapter update not implemented in Spacebot's sd-core stub".to_string())
	}
}
