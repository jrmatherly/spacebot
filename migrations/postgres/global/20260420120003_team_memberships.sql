-- Postgres port of migrations/global/20260420120003_team_memberships.sql.
-- Populated by the Graph reconciliation loop. Read by authz helpers.
-- Overage-resolved memberships cached here per group_cache_ttl_secs.

CREATE TABLE team_memberships (
    principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE CASCADE,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,

    observed_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),

    source TEXT NOT NULL,

    PRIMARY KEY (principal_key, team_id),

    CHECK (source IN ('token_claim', 'graph_overage'))
);

CREATE INDEX idx_memberships_principal ON team_memberships(principal_key);
CREATE INDEX idx_memberships_team ON team_memberships(team_id);
