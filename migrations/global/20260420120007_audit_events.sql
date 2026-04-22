-- Audit log. Append-only at the application layer (§12 S-C2).
-- Tamper-evidence provided by `prev_hash` / `row_hash` chaining plus the
-- daily export to a WORM sink (Task 5.9).
--
-- `seq` is monotonic within the table; it is NOT an auto-increment column
-- because we need explicit control for chain verification and import of
-- events from external sources.

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    seq INTEGER NOT NULL,
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

-- A-14: export-state tracking. One row per configured export_mode kind
-- ("s3" | "http_siem" | "filesystem"). Tracks the last-exported seq so
-- exports are incremental, not full-table re-exports.
CREATE TABLE audit_export_state (
    export_mode TEXT PRIMARY KEY NOT NULL,
    last_exported_seq INTEGER NOT NULL DEFAULT 0,
    last_exported_at TEXT,
    last_exported_row_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
