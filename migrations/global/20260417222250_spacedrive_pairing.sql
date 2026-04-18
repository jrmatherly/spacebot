-- Spacedrive pairing state.
--
-- One row per pairing. UNIQUE(library_id) enforces the one-library-per-
-- Spacebot rule. Auth token does NOT live here; it lives in the secrets
-- store under key `spacedrive_auth_token:<library_id>`. See
-- docs/design-docs/spacedrive-integration-pairing.md (ADR D2).

-- SQLite auto-creates a backing index for UNIQUE(library_id), so no explicit
-- CREATE INDEX is needed on that column.
CREATE TABLE spacedrive_pairing (
    id INTEGER PRIMARY KEY,
    library_id TEXT NOT NULL UNIQUE,
    spacebot_instance_id TEXT NOT NULL,
    spacedrive_base_url TEXT NOT NULL,
    paired_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
