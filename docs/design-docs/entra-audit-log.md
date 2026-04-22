# Audit Log — Operator Guide

Phase 5 ships a hash-chained, append-only audit log with daily WORM export.
This guide covers the table layout, chain verification procedure, export
mode semantics, and separation-of-duties expectations for SOC 2 CC7.2 /
CC6.6 evidence.

## Table: `audit_events`

Instance-level SQLite table. Migration: `migrations/global/20260420120007_audit_events.sql`.

Hash-chained: each row's `prev_hash` equals the prior row's `row_hash`.
The first row's `prev_hash` is 64 zero characters. `row_hash` is computed
as `sha256(canonical(row) || prev_hash)` where `canonical` is the
deterministic serialization defined in `src/audit/types.rs::canonical_bytes`.

Columns:

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PRIMARY KEY | UUIDv7 — temporal ordering + chain-handle |
| `seq` | INTEGER UNIQUE | Monotonic within the table. Not auto-increment: the appender assigns explicitly so external-source import is unambiguous. |
| `timestamp` | TEXT | RFC3339 UTC milliseconds |
| `principal_key` | TEXT | `{tid}:{oid}` for authenticated principals, `"legacy-static"` for the pre-Entra static-token branch, `"unknown"` for `AuthFailure` events |
| `principal_type` | TEXT | `"user" \| "service_principal" \| "system" \| "legacy_static" \| "unknown"` |
| `action` | TEXT | snake_case AuditAction variant |
| `resource_type` | TEXT? | e.g. `"memory"`, `"agent"`, `"cron_job"`, `"portal_conversation"` |
| `resource_id` | TEXT? | Per-resource identifier |
| `result` | TEXT | `"allowed" \| "denied"` (+ free-text classifier on `AuthFailure`) |
| `source_ip` | TEXT? | From `X-Forwarded-For` / `X-Real-IP` if present |
| `request_id` | TEXT? | Reserved; not populated in Phase 5 |
| `metadata_json` | TEXT | Scrubbed via `crate::secrets::scrub::scrub_leaks` BEFORE hashing |
| `prev_hash` | TEXT | 64 hex chars |
| `row_hash` | TEXT | 64 hex chars |

Indexes: `(seq UNIQUE)`, `(timestamp)`, `(principal_key)`, `(action)`, `(resource_type, resource_id)`.

## Retention

- **Online in SQLite:** 1 year minimum. Operator-tuned via external backup/archive tooling; the daemon does not prune.
- **WORM archive:** 7 years (SOC 2 typical). Achieved via the export sinks documented below.

## Chain verification procedure (for auditors)

### Automated

```
GET /api/admin/audit/verify
Authorization: Bearer <admin-role JWT>
```

Returns `{ "valid": bool, "first_mismatch_seq": Option<i64>, "total_rows": i64 }`.

The handler invokes `AuditAppender::verify_chain()`, which holds the
write mutex so concurrent appends cannot produce false-negative
mismatches.

### Manual

1. Select all rows: `SELECT * FROM audit_events ORDER BY seq`.
2. For each row, recompute `row_hash` using the canonical serialization
   in `src/audit/types.rs::canonical_bytes` (field concatenation with `\n`
   delimiters, strict JSON canonicalization for `metadata_json`).
3. Confirm `prev_hash` equals the prior row's `row_hash`. Row 1's
   `prev_hash` must be `"0" * 64`.

Mismatch at seq N means row N (or any row before it, if `prev_hash` was
tampered post-hoc) was modified. Investigate via the WORM export (which
carries the same hashes) and operator-level forensics tooling.

## Read endpoint

```
GET /api/admin/audit
  ?from=<iso8601>&to=<iso8601>
  &principal=<key>&action=<variant>
  &limit=<n>&offset=<n>
Accept: application/x-ndjson  (default)
        text/csv              (alternative)
```

Admin-role gated (`ROLE_ADMIN`). Non-admin principals receive 403.
Paginated by `seq DESC` (newest first); `limit` clamped to [1, 1000].

## Daily export (WORM)

The daemon runs the export scheduler when `[audit.export].enabled = true`.
Interval configurable via `interval_secs` (default 86400 = 24h).
Scheduler uses `MissedTickBehavior::Delay` — first export fires one
interval after startup, not during boot-phase DB warmup.

On each tick:

1. Read `last_exported_seq` from `audit_export_state` keyed on the
   configured `mode.kind_str()`.
2. SELECT all rows with `seq > last_exported_seq`.
3. Write to the configured sink.
4. On sink success, UPSERT `audit_export_state` forward.
5. On sink failure, `tracing::error!` and leave the cursor intact —
   the next tick retries the same range (A-14 incremental semantics).

Three independent cursors (one per mode) allow `filesystem + s3 + http_siem`
to run concurrently with disjoint retention policies.

### Mode: `filesystem` (dev-only)

```toml
[audit.export]
enabled = true
mode = "filesystem"
dir = "./audit-exports"      # default
interval_secs = 86400        # 24h
```

Writes to `dir/`:

- `audit-{ts}.ndjson` — one JSON row per line (full `AuditRow` shape).
- `audit-{ts}.manifest.json` — `rows_exported`, `first_seq`, `last_seq`,
  `chain_head_hash`, `exported_at`, `ndjson_file` (basename).

Per A-15, filesystem mode is **dev-only** and is NOT claimed as
tamper-evident. The 2026-04-22 Phase 5 audit removed the original
`chattr +i` immutability claim because it silently fails in non-root
containers (Spacebot's Talos deployment runs as non-root). A local
filesystem can be modified freely after write; the tamper-evidence
boundary is the external sink.

### Mode: `s3` (production, not yet wired)

```toml
[audit.export]
enabled = true
mode = "s3"
# endpoint, bucket, prefix, region, retention_days, object_lock_mode
# credentials resolved via env:/secret: references
```

Per A-15, production deployments MUST use S3 Object Lock in
**COMPLIANCE** mode. GOVERNANCE mode is admin-bypassable and does not
meet SOC 2 WORM requirements. Supported backends: AWS S3, MinIO,
Cloudflare R2, OCI Object Storage.

Phase 5 ships this as an `anyhow::bail!` stub referencing Phase 10
SOC 2 hardening for the real upload pipeline; the config surface +
enum variant + `audit_export_state` cursor are in place so the Phase 10
wire-up is a bounded drop-in.

### Mode: `http_siem` (production, not yet wired)

```toml
[audit.export]
enabled = true
mode = "http_siem"
# endpoint, auth_header (secret:), batch_size
```

Ships NDJSON to an external SIEM (Splunk HEC, Datadog ingest, generic
HTTPS POST). SIEM owns retention; the daemon is stateless w.r.t.
long-term retention. Stub same as `s3`; real wire-up in Phase 10.

## Retention for export files

Operator responsibility. The daemon writes exports and advances the
cursor; it does NOT delete or archive past exports. Use:

- Filesystem ACLs + external backup (dev only).
- S3 Object Lock retention timer (production).
- SIEM append-only ingestion policy (production).

## Separation of duties (SOC 2 CC6.6)

The audit log is designed around four distinct actors, each with
minimal overlap:

| Actor | Scope |
|---|---|
| Daemon process | Writes `audit_events` exclusively via `AuditAppender` (A-13 singleton, `pub(crate) fn new`, production construction only via `ApiState::set_instance_pool`). |
| Auditor | Reads via `GET /api/admin/audit` (admin-role gated). Cannot modify. |
| Export scheduler | Separate top-level tokio task in main.rs; reads `audit_events` + `audit_export_state`, writes to the configured sink. Does NOT write to `audit_events`. |
| OS-level exporter consumer | The OS user that owns the SQLite file and the OS user that consumes exports should differ in production per CC6.6. |

## Emission sites (Phase 5 coverage)

### Middleware

- `AuthSuccess` — fires on every successful Entra auth. Carries the
  canonical snake_case `principal_type`, `source_ip` from
  X-Forwarded-For / X-Real-IP.
- `AuthFailure` — fires on every Entra auth rejection. `principal_key`
  = `"unknown"`, `result` carries the `AuthError::metric_reason()`
  classifier (`header_missing`, `token_invalid`, `scope_denied`, etc.).

### Handler layer (25 sites across 10 families)

All emission routes through `crate::auth::policy::fire_admin_read_audit`
or `crate::auth::policy::fire_denied_audit` helpers.

| Family | `AdminRead` sites | `AuthzDenied` sites |
|---|---|---|
| `memories.rs` | 4 | ✅ |
| `agents.rs` | 6 | ✅ |
| `tasks.rs` | 2 | ✅ |
| `cron.rs` | 2 | ✅ |
| `wiki.rs` | 2 | ✅ |
| `portal.rs` | 2 | ✅ |
| `projects.rs` | 3 | ✅ |
| `attachments.rs` | 2 | ✅ |
| `notifications.rs` | 1 | ✅ |
| `ingest.rs` | 1 | ✅ |

`AdminRead` events carry `metadata = {"reason": "break_glass"}`.
`AuthzDenied` events carry empty metadata (resource identity is
captured at top level).

Acceptance criterion:

```
$ grep -rn "admin_read override (audit event queued for Phase 5)" src/api/ | wc -l
0
```

## Secret scrubbing (A-01)

`AuditAppender::append` invokes `crate::secrets::scrub::scrub_leaks`
on the `metadata_json` BEFORE canonical_bytes hashes the row. JWT
shapes (three base64url segments separated by `.`), full PEM blocks,
and other `LEAK_PATTERNS` regexes are redacted. The redaction itself
is hashed, so post-hoc un-scrubbing would invalidate the chain.

Regression test: `tests/audit_scrubbing.rs::jwt_in_metadata_is_scrubbed`.
