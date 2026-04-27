-- Postgres port of migrations/global/20260420120002_teams.sql.
-- One team per Entra security group (1:1 mapping per research Q4).
-- external_id holds the Entra group's object GUID.

CREATE TABLE teams (
    id TEXT PRIMARY KEY NOT NULL,
    external_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',

    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),

    CHECK (status IN ('active', 'archived'))
);

CREATE INDEX idx_teams_status ON teams(status);
