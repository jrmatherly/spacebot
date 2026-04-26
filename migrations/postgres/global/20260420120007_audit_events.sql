-- Postgres port of migrations/global/20260420120007_audit_events.sql.
-- Append-only at the application layer (§12 S-C2). Tamper-evidence via
-- prev_hash / row_hash chaining plus daily WORM-sink export.
--
-- `seq` is monotonic but NOT auto-increment — explicit control needed for
-- chain verification and import from external sources. BIGINT (vs SQLite
-- INTEGER) handles the unbounded growth correctly on Postgres.

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    seq BIGINT NOT NULL,
    timestamp TEXT NOT NULL,
    principal_key TEXT NOT NULL,
    principal_type TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    result TEXT NOT NULL,
    source_ip TEXT,
    request_id TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    prev_hash TEXT NOT NULL,
    row_hash TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_audit_seq ON audit_events(seq);
CREATE INDEX idx_audit_timestamp ON audit_events(timestamp);
CREATE INDEX idx_audit_principal ON audit_events(principal_key);
CREATE INDEX idx_audit_action ON audit_events(action);
CREATE INDEX idx_audit_resource ON audit_events(resource_type, resource_id);

CREATE TABLE audit_export_state (
    export_mode TEXT PRIMARY KEY NOT NULL,
    last_exported_seq BIGINT NOT NULL DEFAULT 0,
    last_exported_at TEXT,
    last_exported_row_hash TEXT,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
);
