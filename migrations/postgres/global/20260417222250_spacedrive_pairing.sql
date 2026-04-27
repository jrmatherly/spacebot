-- Postgres port of migrations/global/20260417222250_spacedrive_pairing.sql.
-- BIGSERIAL replaces SQLite's INTEGER PRIMARY KEY (autoincrement implicit).
-- Auth token does NOT live here; it lives in the secrets store under key
-- `spacedrive_auth_token:<library_id>`. See ADR D2.

CREATE TABLE spacedrive_pairing (
    id BIGSERIAL PRIMARY KEY,
    library_id TEXT NOT NULL UNIQUE,
    spacebot_instance_id TEXT NOT NULL,
    spacedrive_base_url TEXT NOT NULL,
    paired_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
