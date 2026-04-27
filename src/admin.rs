//! Instance-level evidence-gathering helpers. The sweep runs above any
//! single agent's scope and detects two classes of `resource_ownership`
//! drift:
//!
//! - `MissingOwnership`: a resource exists in an agent DB but has no row
//!   in the instance-level `resource_ownership` table. These are
//!   pre-Entra-rollout resources awaiting an admin claim via
//!   `spacebot entra admin claim-resource`.
//! - `StaleOwnership`: a `resource_ownership` row exists but the
//!   referenced resource is gone (cross-DB FK is not enforceable in
//!   SQLite, so these accumulate when agents are deleted).
//!
//! Running at instance scope (not inside any per-agent cortex) keeps the
//! AgentId isolation boundary intact: the sweep is the only authorized
//! cross-agent reader, gated by `SpacebotAdmin` at the calling endpoint.
//! It is currently report-only; auto-deletion of stale rows is gated on
//! a future config flag (`[audit.orphan_sweep] auto_delete_stale = false`).

use serde::{Deserialize, Serialize};

use std::path::{Path, PathBuf};

/// Direction of the orphan: `MissingOwnership` means the agent DB has the
/// resource but the instance lacks an ownership row; `StaleOwnership`
/// means the instance has the row but no agent DB has the resource.
///
/// Serialized snake_case to match the wire convention established by
/// `Visibility` (`src/auth/principals.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OrphanKind {
    MissingOwnership,
    StaleOwnership,
}

/// One detected orphan. Logged via `tracing::warn!` and (when scheduling
/// is wired up) emitted as an `OrphanDetected` audit event by the caller.
/// Fields are crate-scoped; external consumers (including integration
/// tests) read through the accessor methods or the wire DTO
/// (`crate::api::admin_orphans::OrphanReport`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Orphan {
    pub(crate) kind: OrphanKind,
    pub(crate) resource_type: String,
    pub(crate) resource_id: String,
    pub(crate) owning_agent_id: Option<String>,
}

impl Orphan {
    pub fn kind(&self) -> OrphanKind {
        self.kind
    }
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
    pub fn owning_agent_id(&self) -> Option<&str> {
        self.owning_agent_id.as_deref()
    }
}

/// Map an agent-DB path back to its agent_id. The convention is
/// `<root>/agents/<agent_id>/data/spacebot.db`, so the agent_id is the
/// grandparent's file name.
fn agent_id_from_path(p: &Path) -> String {
    let id = p
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if id.is_empty() {
        tracing::warn!(
            path = %p.display(),
            "orphan_sweep: agent_id_from_path could not extract agent_id; using 'unknown'",
        );
        return "unknown".to_string();
    }
    id.to_string()
}

/// Defense-in-depth path-component validator. Rejects strings that contain
/// path separators, parent-directory references, leading dots, or any
/// character outside the expected agent-id charset (alphanumeric plus `-`
/// and `_`). Used before joining a database-supplied `agent_id` into a
/// filesystem path so a malicious or corrupted `resource_ownership` row
/// can't redirect the orphan sweep to read arbitrary files.
fn is_safe_agent_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Per-agent resource_type to table-name map. Add to this when a new
/// per-agent resource_type is introduced; missing entries are tolerated
/// in the StaleOwnership reverse-check (the row stays in the report but
/// is not auto-flagged).
fn table_for_resource_type(rt: &str) -> Option<&'static str> {
    match rt {
        "memory" => Some("memories"),
        // Other resource_types live in the instance pool (tasks, projects,
        // wiki, channels, agents themselves) and don't need per-agent
        // enumeration. Extend this match if a future resource_type is
        // declared at per-agent scope.
        _ => None,
    }
}

/// Sweep every supplied agent DB plus every `resource_ownership` row,
/// returning the union of MissingOwnership + StaleOwnership findings.
/// Race-tolerant: a missing or partially-initialized agent DB is logged
/// via `tracing::warn!` and skipped, not crashed. Concurrent agent
/// creation or deletion during a sweep is expected and surfaces as a
/// follow-up cycle's findings.
pub async fn sweep_orphans(
    instance_pool: &crate::db::DbPool,
    agent_db_paths: &[PathBuf],
) -> anyhow::Result<Vec<Orphan>> {
    use crate::db::DbPool;
    let mut orphans = Vec::new();

    // Direction 1: each agent DB's resources should have a row in the
    // instance-level `resource_ownership` table. Anything not in the
    // ownership table is MissingOwnership.
    for agent_db_path in agent_db_paths {
        let agent_id = agent_id_from_path(agent_db_path);

        let agent_pool = match sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            agent_db_path.display()
        ))
        .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    path = %agent_db_path.display(),
                    %e,
                    "orphan_sweep: skipping agent DB that won't open (likely partial init or concurrent delete)"
                );
                continue;
            }
        };

        // The agent's `memories` table is the only per-agent resource the
        // backfill policy currently tracks. Tolerate missing tables: a
        // freshly-created agent DB may not have run migrations yet.
        let memory_ids: Vec<(String,)> = match sqlx::query_as("SELECT id FROM memories")
            .fetch_all(&agent_pool)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    %e,
                    "orphan_sweep: agent has no memories table (probably pre-migration); skipping"
                );
                agent_pool.close().await;
                continue;
            }
        };

        for (rid,) in memory_ids {
            let owned: Option<i64> = match instance_pool {
                DbPool::Sqlite(p) => sqlx::query_scalar(
                    "SELECT 1 FROM resource_ownership WHERE resource_type = 'memory' AND resource_id = ?",
                )
                .bind(&rid)
                .fetch_optional(p)
                .await?,
                DbPool::Postgres(p) => sqlx::query_scalar(
                    "SELECT 1 FROM resource_ownership WHERE resource_type = 'memory' AND resource_id = $1",
                )
                .bind(&rid)
                .fetch_optional(p)
                .await?,
            };
            if owned.is_none() {
                orphans.push(Orphan {
                    kind: OrphanKind::MissingOwnership,
                    resource_type: "memory".into(),
                    resource_id: rid,
                    owning_agent_id: Some(agent_id.clone()),
                });
            }
        }

        agent_pool.close().await;
    }

    // Direction 2: each `resource_ownership` row should reference a real
    // resource. Anything pointing at a missing resource (or a vanished
    // agent directory) is StaleOwnership.
    let ownership_rows: Vec<(String, String, Option<String>)> = match instance_pool {
        DbPool::Sqlite(p) => sqlx::query_as(
            "SELECT resource_type, resource_id, owner_agent_id FROM resource_ownership",
        )
        .fetch_all(p)
        .await?,
        DbPool::Postgres(p) => sqlx::query_as(
            "SELECT resource_type, resource_id, owner_agent_id FROM resource_ownership",
        )
        .fetch_all(p)
        .await?,
    };

    let root = std::env::var("SPACEBOT_DIR").ok().map(PathBuf::from);
    if root.is_none() {
        // Without a root the reverse-check can't resolve any path, so
        // Direction 2 is silently disabled. Log once before the loop so
        // operators reviewing CC6.1 evidence can distinguish "no stale
        // ownership" from "the env was unset and the scan didn't run".
        tracing::warn!(
            "orphan_sweep: SPACEBOT_DIR unset; StaleOwnership detection disabled for this run",
        );
    }
    for (rt, rid, owning_agent_id) in ownership_rows {
        let Some(agent_id) = owning_agent_id.as_deref() else {
            // Non-agent-owned ownership rows (instance-scope resources)
            // can't go stale via cross-DB FK absence; skip.
            continue;
        };
        let Some(root) = root.as_deref() else {
            continue;
        };
        let Some(table) = table_for_resource_type(&rt) else {
            // Resource type not in the per-agent map; skip.
            continue;
        };

        if !is_safe_agent_id(agent_id) {
            // Defense-in-depth: a malicious or corrupted resource_ownership
            // row could carry an agent_id like `../../etc` that would
            // redirect SqlitePool::connect to an arbitrary path. Reject
            // any value outside the expected slug/UUID charset.
            tracing::warn!(
                agent_id = %agent_id,
                resource_type = %rt,
                resource_id = %rid,
                "orphan_sweep: skipping reverse-check for agent_id that fails shape validation",
            );
            continue;
        }
        let agent_db_path = root
            .join("agents")
            .join(agent_id)
            .join("data")
            .join("spacebot.db");
        // Belt-and-suspenders prefix check after the join, in case the
        // shape validator above misses an edge case on a future platform.
        let agents_root = root.join("agents");
        if !agent_db_path.starts_with(&agents_root) {
            tracing::warn!(
                agent_id = %agent_id,
                path = %agent_db_path.display(),
                "orphan_sweep: constructed path escaped the agents root; refusing to open",
            );
            continue;
        }

        if !agent_db_path.exists() {
            orphans.push(Orphan {
                kind: OrphanKind::StaleOwnership,
                resource_type: rt,
                resource_id: rid,
                owning_agent_id: Some(agent_id.to_string()),
            });
            continue;
        }

        let agent_pool =
            match sqlx::SqlitePool::connect(&format!("sqlite://{}", agent_db_path.display())).await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        agent_id = %agent_id,
                        path = %agent_db_path.display(),
                        %e,
                        "orphan_sweep: skipping reverse-check on agent DB that won't open"
                    );
                    continue;
                }
            };

        let query = format!("SELECT 1 FROM {table} WHERE id = ?");
        let exists: Option<i64> = match sqlx::query_scalar(&query)
            .bind(&rid)
            .fetch_optional(&agent_pool)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                // A genuine sqlx error (locked DB, schema drift, IO) was
                // previously swallowed via `.unwrap_or(None)`, which then
                // wrote a false-positive StaleOwnership row into the
                // SOC 2 evidence report. Skip the row instead so operators
                // don't act on a transient query failure.
                tracing::warn!(
                    agent_id = %agent_id,
                    resource_type = %rt,
                    resource_id = %rid,
                    %e,
                    "orphan_sweep: reverse-check query failed; skipping row to avoid false-positive StaleOwnership",
                );
                agent_pool.close().await;
                continue;
            }
        };
        if exists.is_none() {
            orphans.push(Orphan {
                kind: OrphanKind::StaleOwnership,
                resource_type: rt,
                resource_id: rid,
                owning_agent_id: Some(agent_id.to_string()),
            });
        }
        agent_pool.close().await;
    }

    Ok(orphans)
}

/// Discover agent DB paths by scanning `$SPACEBOT_DIR/agents/*/data/spacebot.db`.
/// Returns an empty Vec if the env var is unset or the directory is
/// unreadable; the caller decides whether that's an error. Subdirectory
/// names that fail [`is_safe_agent_id`] are skipped with a warning so a
/// stray symlink or accidental directory can't redirect the sweep.
pub fn discover_agent_db_paths() -> Vec<PathBuf> {
    let Ok(root) = std::env::var("SPACEBOT_DIR") else {
        return Vec::new();
    };
    let agents_dir = PathBuf::from(root).join("agents");
    let Ok(entries) = std::fs::read_dir(&agents_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            let safe = e.file_name().to_str().is_some_and(is_safe_agent_id);
            if !safe {
                tracing::warn!(
                    name = ?e.file_name(),
                    "discover_agent_db_paths: skipping subdirectory that fails agent_id shape",
                );
            }
            safe
        })
        .map(|e| e.path().join("data").join("spacebot.db"))
        .filter(|p| p.exists())
        .collect()
}
