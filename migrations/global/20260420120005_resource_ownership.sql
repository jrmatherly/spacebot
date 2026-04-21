-- Resource ownership sidecar. One row per resource that has ownership
-- semantics. Research §12 A-Alternative-1. Keyed by (resource_type,
-- resource_id) so it works across per-agent SQLite files AND the
-- instance-level DB without requiring cross-DB FKs (SQLite has none).

CREATE TABLE resource_ownership (
    -- Resource kind: 'agent' | 'memory' | 'task' | 'wiki_page' | 'channel' |
    -- 'worker' | 'cron_job' | 'cortex_chat_thread' | 'portal_conversation'
    -- | 'project' | 'notification' | 'saved_attachment'.
    -- Add new kinds via migration. Never reuse a kind string.
    resource_type TEXT NOT NULL,

    -- Resource's native ID. The interpretation depends on resource_type;
    -- the app layer handles scoping to the right DB/agent.
    resource_id TEXT NOT NULL,

    -- Optional owning agent. When present, constrains that authz also
    -- checks the owner's access to the agent.
    owner_agent_id TEXT,

    -- The principal who owns this resource. References users.principal_key.
    owner_principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    -- Sharing scope: 'personal' | 'team' | 'org'. The legacy 'global' value
    -- from the research draft is NOT accepted; use 'org' for instance-wide
    -- visibility.
    visibility TEXT NOT NULL DEFAULT 'personal',

    -- When visibility = 'team', the team it's shared with.
    shared_with_team_id TEXT REFERENCES teams(id) ON DELETE RESTRICT,

    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),

    PRIMARY KEY (resource_type, resource_id),

    -- Invariant: if visibility = 'team', shared_with_team_id MUST be set.
    CHECK (visibility != 'team' OR shared_with_team_id IS NOT NULL),
    -- Invariant: visibility is constrained.
    CHECK (visibility IN ('personal', 'team', 'org'))
);

CREATE INDEX idx_ownership_owner ON resource_ownership(owner_principal_key);
CREATE INDEX idx_ownership_team ON resource_ownership(shared_with_team_id);
CREATE INDEX idx_ownership_visibility ON resource_ownership(visibility);
CREATE INDEX idx_ownership_agent ON resource_ownership(owner_agent_id);
