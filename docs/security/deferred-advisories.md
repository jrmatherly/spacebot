# Deferred Security Advisories

In-repo tracking of open Dependabot alerts in spacebot-owned code whose resolution is blocked on an upstream crate or package update. Each entry documents the advisory, the blocker, and the trigger that would let us resolve it locally.

These alerts remain open on the GitHub Security dashboard as tracking signal. They are not dismissed via the API. See the non-dismissal policy at the bottom of this document.

## Current Deferred Items

### lexical-core 0.7.6 — GHSA-2326-pfpj-vx3h

- **Alert:** #1
- **Manifest:** `Cargo.lock`
- **Severity:** low
- **Current:** 0.7.6
- **Patched:** 1.0.0
- **Blocker:** the `imap` crate. Our IMAP messaging adapter uses `imap` 2.4.x, which pins `nom` 5.x, which pins `lexical-core` 0.7.x. `imap` 3.x migrated to an async-only API and cannot be adopted without rewriting the adapter.
- **Unblock trigger:** the `imap` crate publishes a stable 3.x release and the spacebot IMAP adapter is migrated, or an `imap` 2.x release bumps `nom` to 7+ (which uses `lexical-core` 1.x).

### lru 0.12.5 — GHSA-rhfx-m35p-ff5j

- **Alert:** #3
- **Manifest:** `Cargo.lock`
- **Severity:** low
- **Current:** 0.12.5
- **Patched:** 0.16.3
- **Blocker:** `lancedb` depends on `tantivy` 0.24, which pins `lru` 0.12. `tantivy` 0.25+ uses `lru` 0.13+. No `lancedb` release yet depends on `tantivy` 0.25+.
- **Unblock trigger:** `lancedb` publishes a release depending on `tantivy` 0.25+.

### rand 0.8.5 (root) — GHSA-cq8v-f236-94qc

- **Alert:** #15
- **Manifest:** `Cargo.lock`
- **Severity:** low
- **Current:** 0.8.5
- **Patched:** 0.9.3
- **Blocker:** `rig-core` (via `nanoid`) and `lancedb` (via `tantivy-stacker`) pin `rand` 0.8. `rand` 0.9 has API changes (`thread_rng()` → `rng()`, distribution-trait refactor) that require each dependent crate to migrate.
- **Unblock trigger:** either `rig-core` or `lancedb` releases using `rand` 0.9.
- **Note:** the advisory concerns unsoundness only when a custom panicking `log::Logger` intercepts a rand call. Spacebot does not install such a logger.

### rand 0.8.5 (desktop) — GHSA-cq8v-f236-94qc

- **Alert:** #18
- **Manifest:** `desktop/src-tauri/Cargo.lock`
- **Severity:** low
- **Current:** 0.8.5
- **Patched:** 0.9.3
- **Blocker:** tauri transitive chain pins `rand` 0.8.
- **Unblock trigger:** tauri releases using `rand` 0.9, or the blocking tauri plugin bumps its own rand dependency.

## Review Cadence

Review this file whenever dependencies are refreshed (`just deps-update` or equivalent). For each entry:

1. Re-run `gh api repos/jrmatherly/spacebot/dependabot/alerts/{n}` to confirm the alert is still open.
2. Check whether the upstream blocker has released a compatible version.
3. If the blocker has moved, attempt the upgrade and remove the entry once the alert auto-closes.

## Non-Dismissal Policy

Open Dependabot alerts in spacebot-owned code that are blocked on upstream crate or package updates **are not dismissed** via the GitHub API. Reasons:

1. Dashboard visibility preserves the nudge to check upstream on each refresh cycle.
2. Dismissing with `tolerable_risk` hides a real vulnerability whose blast radius may change if our code's use of the affected crate evolves.
3. The entries in this file provide the audit trail that a dismissed alert would normally carry in the comment field — kept in the repo so `git log` shows when each deferral was decided and why.

Do not run `gh api -X PATCH .../dependabot/alerts/{n}` on any alert tracked in this file.
