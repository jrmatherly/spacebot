-- Postgres port of migrations/global/20260409000001_project_sort_order.sql.
-- Adds user-controlled sort_order with deterministic backfill from creation
-- order. Postgres ALTER TABLE syntax is identical to SQLite for this.

ALTER TABLE projects
ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

UPDATE projects
SET sort_order = (
    SELECT COUNT(*)
    FROM projects p2
    WHERE
        p2.created_at < projects.created_at
        OR (
            p2.created_at = projects.created_at
            AND p2.id < projects.id
        )
);
