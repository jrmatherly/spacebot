-- Postgres port of migrations/global/20260322000001_task_number_sequence.sql.
-- Singleton high-water-mark for task numbering. Postgres requires `BIGINT`
-- for `next_number` (SQLite's INTEGER is dynamically-sized, F12 in the plan
-- audit log: widen on Postgres-only).

CREATE TABLE IF NOT EXISTS task_number_seq (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    next_number BIGINT NOT NULL DEFAULT 1
);

INSERT INTO task_number_seq (id, next_number)
VALUES (
    1,
    COALESCE(
        (SELECT MAX(task_number) + 1 FROM tasks),
        1
    )
)
ON CONFLICT (id) DO NOTHING;
