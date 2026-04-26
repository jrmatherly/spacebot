-- Postgres port of migrations/global/20260420120004_service_accounts.sql.
-- CLI client-credentials principals (research Q7) and any future
-- daemon-as-client scenarios. Optional metadata service accounts need
-- but users don't (description, owner, assigned roles).

CREATE TABLE service_accounts (
    principal_key TEXT PRIMARY KEY NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    description TEXT NOT NULL,

    owner_principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    assigned_roles_json TEXT NOT NULL DEFAULT '[]',

    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
);
