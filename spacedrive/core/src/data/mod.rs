//! Spacebot-authored stub for `sd_core::data`.
//!
//! Upstream declares `pub mod data;` in `core/src/lib.rs` without a source
//! file. `library::Library` stores a `OnceCell<Arc<data::manager::SourceManager>>`
//! and the `ops/sources/*` action/query modules invoke six methods on it. This
//! module provides a minimal `SourceManager` skeleton that satisfies type
//! inference at every call site and returns empty/error placeholders at
//! runtime. Remove when upstream ships a real `data` module. Tracked in
//! `spacedrive/SYNC.md` LOCAL_CHANGES.

pub mod manager;
