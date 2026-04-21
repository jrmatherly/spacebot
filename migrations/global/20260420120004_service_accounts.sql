-- Service accounts: CLI client-credentials principals (research Q7) and any
-- future daemon-as-client scenarios. Distinct from `users` even though both
-- ultimately live under the `users` table via `principal_type = 'service_principal'`.
--
-- This table adds the optional metadata that service accounts need but users
-- don't (description, owner, assigned roles at creation time).

CREATE TABLE service_accounts (
    principal_key TEXT PRIMARY KEY NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    -- Human-readable description. Required for SOC 2 access reviews.
    description TEXT NOT NULL,

    -- The human who owns operational responsibility for this service account.
    owner_principal_key TEXT NOT NULL REFERENCES users(principal_key) ON DELETE RESTRICT,

    -- Assigned app roles (JSON array of strings). Authoritative source for
    -- this principal's roles. Graph doesn't enumerate app-role assignments
    -- for service principals the same way as users.
    assigned_roles_json TEXT NOT NULL DEFAULT '[]',

    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
