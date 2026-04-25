//! SOC 2 evidence-gathering helpers that operate at the instance level
//! (above any single agent's scope). Currently exposes the orphan-resource
//! sweep that detects two classes of resource_ownership drift:
//!
//! - `MissingOwnership`: a resource exists in an agent DB but has no row
//!   in the instance-level `resource_ownership` table. These are
//!   pre-Entra-rollout resources awaiting an admin claim
//!   (`spacebot entra admin claim-resource`).
//! - `StaleOwnership`: a `resource_ownership` row exists but the
//!   referenced resource is gone (cross-DB FK is not enforceable in
//!   SQLite, so these accumulate when agents are deleted).
//!
//! Per Phase 10 IMPORTANT-7: the sweep runs at instance scope, not inside
//! a per-agent cortex, to keep the AgentId isolation boundary intact. It
//! is currently report-only; auto-deletion of stale rows is gated on a
//! future config flag (`[audit.orphan_sweep] auto_delete_stale = false`).

use std::path::{Path, PathBuf};

/// Direction of the orphan: `MissingOwnership` means the agent DB has the
/// resource but the instance lacks an ownership row; `StaleOwnership`
/// means the instance has the row but no agent DB has the resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrphanKind {
    MissingOwnership,
    StaleOwnership,
}

/// One detected orphan. Logged via `tracing::warn!` and (when scheduling
/// is wired up) emitted as an `OrphanDetected` audit event by the caller.
#[derive(Debug, Clone)]
pub struct Orphan {
    pub kind: OrphanKind,
    pub resource_type: String,
    pub resource_id: String,
    pub owning_agent_id: Option<String>,
}

/// Map an agent-DB path back to its agent_id. The convention is
/// `<root>/agents/<agent_id>/data/spacebot.db`, so the agent_id is the
/// grandparent's file name.
fn agent_id_from_path(p: &Path) -> String {
    p.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
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
/// and skipped, not crashed (per IMPORTANT-7 #5).
pub async fn sweep_orphans(
    instance_pool: &sqlx::SqlitePool,
    agent_db_paths: &[PathBuf],
) -> anyhow::Result<Vec<Orphan>> {
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
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM resource_ownership WHERE resource_type = 'memory' AND resource_id = ?",
            )
            .bind(&rid)
            .fetch_optional(instance_pool)
            .await?;
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
    let ownership_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT resource_type, resource_id, owner_agent_id FROM resource_ownership",
    )
    .fetch_all(instance_pool)
    .await?;

    let root = std::env::var("SPACEBOT_DIR").ok().map(PathBuf::from);
    for (rt, rid, owning_agent_id) in ownership_rows {
        let Some(agent_id) = owning_agent_id.as_deref() else {
            // Non-agent-owned ownership rows (instance-scope resources)
            // can't go stale via cross-DB FK absence; skip.
            continue;
        };
        let Some(root) = root.as_deref() else {
            // Without a root we can't resolve the back-reference path.
            // Direction 1 still produces useful output; just skip Direction 2.
            continue;
        };
        let Some(table) = table_for_resource_type(&rt) else {
            // Resource type not in the per-agent map; skip.
            continue;
        };

        let agent_db_path = root
            .join("agents")
            .join(agent_id)
            .join("data")
            .join("spacebot.db");

        if !agent_db_path.exists() {
            orphans.push(Orphan {
                kind: OrphanKind::StaleOwnership,
                resource_type: rt,
                resource_id: rid,
                owning_agent_id: Some(agent_id.to_string()),
            });
            continue;
        }

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
                    "orphan_sweep: skipping reverse-check on agent DB that won't open"
                );
                continue;
            }
        };

        let query = format!("SELECT 1 FROM {table} WHERE id = ?");
        let exists: Option<i64> = sqlx::query_scalar(&query)
            .bind(&rid)
            .fetch_optional(&agent_pool)
            .await
            .unwrap_or(None);
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
/// unreadable; the caller decides whether that's an error.
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
        .map(|e| e.path().join("data").join("spacebot.db"))
        .filter(|p| p.exists())
        .collect()
}
