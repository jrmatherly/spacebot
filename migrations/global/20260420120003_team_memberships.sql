-- Team memberships. Populated by the Graph reconciliation loop (future phase).
-- Read by the authz helpers (future phase). Overage-resolved memberships are
-- cached here per `group_cache_ttl_secs`.

CREATE TABLE team_memberships (
    principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE CASCADE,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,

    -- When this membership was last observed (via JWT `groups` claim or
    -- Graph overage resolution). Staleness check: if older than
    -- group_cache_ttl_secs, re-resolve.
    observed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    -- Source of the observation: 'token_claim' or 'graph_overage'.
    source TEXT NOT NULL,

    PRIMARY KEY (principal_key, team_id)
);

CREATE INDEX idx_memberships_principal ON team_memberships(principal_key);
CREATE INDEX idx_memberships_team ON team_memberships(team_id);
