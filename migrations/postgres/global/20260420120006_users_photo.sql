-- Postgres port of migrations/global/20260420120006_users_photo.sql.
-- A-19: display photo cache for Entra users. Populated after the OBO-scoped
-- fetch_user_photo (Graph /me/photo/$value) succeeds in the Phase 3 sync
-- path. Weekly TTL refresh enforced via photo_updated_at.

ALTER TABLE users ADD COLUMN display_photo_b64 TEXT;
ALTER TABLE users ADD COLUMN photo_updated_at TEXT;
