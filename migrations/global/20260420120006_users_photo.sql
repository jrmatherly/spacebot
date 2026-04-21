-- A-19: display photo cache for Entra users. Populated after the OBO-scoped
-- fetch_user_photo (Graph `/me/photo/$value`) succeeds in the Phase 3 sync
-- path. Weekly TTL refresh is enforced via photo_updated_at. Both columns
-- are nullable: users who have never uploaded a photo to Microsoft 365 stay
-- as NULL display_photo_b64 with a non-NULL photo_updated_at (marking the
-- TTL so we don't re-fetch immediately). See Phase 3 Task 3.3c.

ALTER TABLE users ADD COLUMN display_photo_b64 TEXT;
ALTER TABLE users ADD COLUMN photo_updated_at TEXT;
