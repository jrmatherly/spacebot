# Postgres Migration Roadmap (stub)

Status: **ROADMAP — NOT SCHEDULED.** This document is a placeholder; the migration has not been designed in detail.

> Superseded by the planned `phase-11-postgres-backend.md` track. Created so the SOC 2 evidence index has a stable home for future Postgres-related decisions.

## Target outcome

Production Kubernetes deployments of Spacebot run against a managed PostgreSQL instance (cluster-provided or external) instead of SQLite-on-PVC. Desktop app and self-hosted single-tenant deployments keep SQLite; those surfaces have neither the scale nor the multi-tenancy pressure that motivates Postgres.

## Migration cost drivers

- **55 migrations** to port. 41 under `migrations/` (per-agent), 14 under `migrations/global/`. Many use SQLite-specific idioms (`INTEGER PRIMARY KEY` autoincrement, `datetime()`, type-affinity reliance). Direct translation to Postgres syntax is mechanical for most but semantic for some.
- **Per-agent DB model.** `src/db.rs` gives each agent its own `agent.db` file under `~/.spacebot/agents/{id}/data/`. Postgres equivalent requires either schema-per-agent (operationally expensive at scale), row-level tenancy with `agent_id` column on every per-agent table (a big rewrite), or database-per-agent (even more expensive). Decision deferred to this design doc's first real pass.
- **LanceDB and redb unaffected.** They are separate backends; vector embeddings stay in LanceDB, key-value in redb.

## Open questions

- Tenancy model (schema-per-agent vs row-level `agent_id`).
- Cutover story for existing SQLite deployments (live dump, offline export, one-way or dual-write).
- Compile-time verification during transition (dual `DATABASE_URL` CI, or feature-flag split).
- Connection pool sizing and pgbouncer placement.
- Backup strategy (`pg_dump` plus WAL archive vs managed-service snapshots).
- Performance characterization. LanceDB joins do not go through Postgres, but cross-backend queries (SQL plus vector) are common. Confirm no regressions.

## Not blocking

SOC 2 controls for the K8s tier rely on volume-level encryption at the storage layer (Talos system-disk encryption, verified in the cluster repo), NOT on Postgres. The current evidence package targets the SQLite-on-PVC posture; Postgres is a performance and operational upgrade for later.

## Next step (when this gets prioritized)

Decide the tenancy model first. That choice gates everything else (migration translation, query rewrites, connection pooling).
