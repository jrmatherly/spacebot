//! Spacebot-authored stub for `ops::libraries::list`.
//!
//! Upstream declares `pub mod list;` in `ops/libraries/mod.rs` without a source
//! file. The only in-crate importer is `ops::core::status::{query,output}`,
//! which constructs `LibraryInfo` with fields `{id, name, path, stats}`. This
//! stub exposes that type under `list::output::LibraryInfo` to match the import
//! path. Remove when upstream ships a real `libraries::list` implementation.
//! Tracked in `spacedrive/SYNC.md` LOCAL_CHANGES.

pub mod output {
	use crate::library::LibraryStatistics;
	use serde::{Deserialize, Serialize};
	use specta::Type;
	use std::path::PathBuf;
	use uuid::Uuid;

	/// Minimal library-info payload consumed by `core.status` query output.
	/// Field shape is derived from the construction site at
	/// `ops/core/status/query.rs:70-76`.
	#[derive(Debug, Clone, Serialize, Deserialize, Type)]
	pub struct LibraryInfo {
		pub id: Uuid,
		pub name: String,
		pub path: PathBuf,
		pub stats: Option<LibraryStatistics>,
	}
}
