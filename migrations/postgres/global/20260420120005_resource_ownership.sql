-- Postgres port of migrations/global/20260420120005_resource_ownership.sql.
-- Resource ownership sidecar. One row per resource that has ownership
-- semantics. Research §12 A-Alternative-1.

CREATE TABLE resource_ownership (
    resource_type TEXT NOT NULL,

    resource_id TEXT NOT NULL,

    owner_agent_id TEXT,

    owner_principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    visibility TEXT NOT NULL DEFAULT 'personal',

    shared_with_team_id TEXT REFERENCES teams(id) ON DELETE RESTRICT,

    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),

    PRIMARY KEY (resource_type, resource_id),

    CHECK (visibility != 'team' OR shared_with_team_id IS NOT NULL),
    CHECK (visibility IN ('personal', 'team', 'org'))
);

CREATE INDEX idx_ownership_owner ON resource_ownership(owner_principal_key);
CREATE INDEX idx_ownership_team ON resource_ownership(shared_with_team_id);
CREATE INDEX idx_ownership_visibility ON resource_ownership(visibility);
CREATE INDEX idx_ownership_agent ON resource_ownership(owner_agent_id);
