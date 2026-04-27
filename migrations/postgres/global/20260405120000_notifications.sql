-- Postgres port of migrations/global/20260405120000_notifications.sql.
-- The partial unique index translates 1:1 — Postgres supports WHERE clauses
-- on UNIQUE INDEX and `ON CONFLICT DO NOTHING` honors them in the same way
-- SQLite's INSERT OR IGNORE does for partial unique indexes.

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info',
    title TEXT NOT NULL,
    body TEXT,
    agent_id TEXT,
    related_entity_type TEXT,
    related_entity_id TEXT,
    action_url TEXT,
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
    read_at TEXT,
    dismissed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_notifications_inbox ON notifications (dismissed_at, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_agent ON notifications (agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_entity ON notifications (
    related_entity_type,
    related_entity_id
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_entity_active ON notifications (
    kind,
    related_entity_type,
    related_entity_id
)
WHERE
    dismissed_at IS NULL;
