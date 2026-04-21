-- Users table. One row per Entra principal (user OR service_principal).
-- Created lazily on first successful auth. Never deleted (soft-delete only).
--
-- Identity key is the composite (tid, oid). Never email/upn (research §12 E-7).
-- `principal_type` discriminates user vs service vs system (research §12 A-5).

CREATE TABLE users (
    -- Composite key surfaced as a single string "{tid}:{oid}" for convenience
    -- in FK constraints (SQLite handles composite FKs poorly).
    principal_key TEXT PRIMARY KEY NOT NULL,

    -- Entra tenant ID. Immutable.
    tenant_id TEXT NOT NULL,

    -- Entra object ID. Immutable within the tenant.
    object_id TEXT NOT NULL,

    -- Principal discriminator: 'user' | 'service_principal' | 'system'.
    -- 'legacy_static' principals do NOT get rows here.
    principal_type TEXT NOT NULL,

    -- Display-only fields. Refreshed on each login. Never used for authz.
    display_name TEXT,
    display_email TEXT,

    -- User state. 'active' | 'disabled' | 'deleted'. Research §12 S-I3.
    status TEXT NOT NULL DEFAULT 'active',

    -- Last successful auth timestamp (ISO-8601). Used for JML detection.
    last_seen_at TEXT,

    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    -- Domain-bound value enforcement. Mirrors resource_ownership.visibility's
    -- CHECK pattern so every authz-critical enum column has a DB-level guard.
    CHECK (principal_type IN ('user', 'service_principal', 'system')),
    CHECK (status IN ('active', 'disabled', 'deleted'))
);

-- Uniqueness on (tenant_id, object_id) redundant with principal_key
-- but explicit for query planner clarity.
CREATE UNIQUE INDEX idx_users_tid_oid ON users(tenant_id, object_id);
CREATE INDEX idx_users_status ON users(status);
