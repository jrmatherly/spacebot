# Entra Integration — Backfill Strategy for Pre-existing Resources

> Source: research §11.2(4), §12 S-I3, §12 A-6.

## Problem

When a hosted Spacebot instance upgrades to the Entra-authenticated build,
it already has thousands of rows in per-agent `memories`, `tasks`, `wiki_pages`,
`portal_conversations`, etc. None of them have a `resource_ownership` row.
Phase 4's authz helpers must not reject these resources outright. That is
data loss from the user's perspective.

## Policy

1. **No auto-broadening to `org` visibility.** The prior research draft
   suggested "backfill to system-user + org visibility"; the security review
   (§12 S-I3) vetoed this as a privacy regression.

2. **"Not-yet-owned" is a valid state.** Phase 4's `require_read_access`
   helper treats `resource_ownership` miss as:
   - Read: allowed ONLY to principals with role `SpacebotAdmin`.
   - Write: denied unconditionally. Admin must claim or assign the resource
     first.
   - List: not returned in non-admin queries (filtered out).

3. **Admin-driven claim workflow.** Ship a CLI subcommand in Phase 9
   (`spacebot admin claim-resource --type <t> --id <id> --owner <key>`) that
   writes a `resource_ownership` row. Until the resource is claimed, it's
   invisible to everyone except admins.

4. **No bulk backfill migration.** This is deliberate: (a) per-agent DBs
   are opaque to the instance-level migration, (b) bulk data changes during
   migration can fail mid-way on large instances, (c) incremental claim via
   admin action is auditable (each claim hits `audit_events`).

5. **Orphaned-resource sweep (Phase 10 deliverable).** Two directions of orphan are handled by the same weekly cortex task:
   - **Resource deleted, ownership row remains** (cross-DB FK limitation, see A-09): the sweep enumerates every `agent.db`, reads its resource IDs per resource_type, and deletes `resource_ownership` rows pointing at non-existent referents.
   - **Ownership row missing for an existing resource** (pre-Entra data): the sweep reports these to an admin-only endpoint for claim via `spacebot admin claim-resource`.
   SOC 2 CC6.7 requires this cadence be documented. Phase 10 Task 10.5 owns the implementation. This Phase 2 doc only records the design link.

## What this means for Phase 4

Every authz helper returns one of:

- `Access::Allowed` — either owner, team member, org-visible, or admin.
- `Access::Denied { reason: NotOwned }` — resource has no ownership row.
  Handler returns 404 (not 403) to avoid leaking resource existence.
- `Access::Denied { reason: NotYours }` — ownership row exists but this
  principal isn't included.

## Admin role behavior

`SpacebotAdmin` sees all resources but every admin read hits `audit_events`
with `action = 'admin_read'` and the resource IDs. This is the "break-glass"
audit trail SOC 2 CC6.6 asks for.
