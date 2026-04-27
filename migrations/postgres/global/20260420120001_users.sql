-- Postgres port of migrations/global/20260420120001_users.sql.
-- Identity key is the composite (tid, oid) per Phase 1 research §12 E-7.
-- Never email/upn. principal_type discriminator drives authz dispatch.

CREATE TABLE users (
    principal_key TEXT PRIMARY KEY NOT NULL,

    tenant_id TEXT NOT NULL,
    object_id TEXT NOT NULL,

    principal_type TEXT NOT NULL,

    display_name TEXT,
    display_email TEXT,

    status TEXT NOT NULL DEFAULT 'active',

    last_seen_at TEXT,

    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),

    CHECK (principal_type IN ('user', 'service_principal', 'system')),
    CHECK (status IN ('active', 'disabled', 'deleted'))
);

CREATE UNIQUE INDEX idx_users_tid_oid ON users(tenant_id, object_id);
CREATE INDEX idx_users_status ON users(status);
