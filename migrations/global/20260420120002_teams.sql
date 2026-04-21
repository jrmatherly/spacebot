-- Teams. One team per Entra security group (1:1 mapping per research Q4).
-- `external_id` holds the Entra group's object GUID. Research §12 S-I4
-- mandates GUID, never display name or UPN-style SID.

CREATE TABLE teams (
    -- Internal team ID. Not exposed externally.
    id TEXT PRIMARY KEY NOT NULL,

    -- Entra group object GUID. MUST be the `objectId` from Microsoft Graph.
    -- Renaming the group in Entra does NOT invalidate this row.
    external_id TEXT NOT NULL UNIQUE,

    -- Display name (refreshed from Graph). Never used as a key.
    display_name TEXT NOT NULL,

    -- Status: 'active' | 'archived' (group deleted in Entra).
    status TEXT NOT NULL DEFAULT 'active',

    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    CHECK (status IN ('active', 'archived'))
);

CREATE INDEX idx_teams_status ON teams(status);
