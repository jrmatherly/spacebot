-- Postgres port of migrations/global/20260321000001_global_tasks.sql.
-- Schema and column types preserved exactly so application code paths
-- bind/read identical Rust types across both backends. Timestamp defaults
-- emit ISO-8601 strings (matching SQLite's strftime output) so chrono parsing
-- stays uniform.

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    task_number BIGINT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'backlog',
    priority TEXT NOT NULL DEFAULT 'medium',

    owner_agent_id TEXT NOT NULL,
    assigned_agent_id TEXT NOT NULL,

    subtasks TEXT,
    metadata TEXT,

    source_memory_id TEXT,
    worker_id TEXT,

    created_by TEXT NOT NULL,
    approved_at TEXT,
    approved_by TEXT,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks (status);
CREATE INDEX IF NOT EXISTS idx_tasks_owner ON tasks (owner_agent_id);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned ON tasks (assigned_agent_id);
CREATE INDEX IF NOT EXISTS idx_tasks_worker ON tasks (worker_id);
CREATE INDEX IF NOT EXISTS idx_tasks_priority_status ON tasks (status, priority);
CREATE INDEX IF NOT EXISTS idx_tasks_source_memory ON tasks (source_memory_id);
