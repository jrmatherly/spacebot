## Context

GitHub operates two related but independent Dependabot systems against every repo:

1. **Dependabot security alerts** (Security dashboard). Scans every manifest file regardless of `.github/dependabot.yml`. Cannot be scoped per-path. Controlled only by repo-level enable/disable, per-alert dismissal, or manifest deletion.
2. **Dependabot version updates** (automated upgrade PRs). Scans only directories listed in `.github/dependabot.yml`. Configurable per ecosystem and directory.

PR #18 (2026-04-16) vendored Spacedrive into `spacedrive/`. The root `Cargo.toml` declares `[workspace] exclude = ["spacedrive"]`, so Cargo skips those files. The security-alerts engine does not honor workspace exclusions — it reports every vulnerability in every manifest, producing 51 alerts for code spacebot never compiles.

Current open-alert triage:

| Scope | Alerts | Status |
|---|---|---|
| `spacedrive/**` (vendored upstream) | 51 | Not compiled by spacebot |
| `Cargo.lock` root | 3 | Deferred — upstream-blocked |
| `desktop/src-tauri/Cargo.lock` | 2 | 1 actionable (glib), 1 deferred (rand) |
| `spaceui/` sub-manifests | 2 | Actionable (vite ×2) |

Current `.github/dependabot.yml` has 3 entries: `github-actions`, root `cargo`, `npm /interface`. It is missing 6 shipped-code directories.

## Goals / Non-Goals

**Goals:**
- Resolve the 3 actionable spacebot-owned alerts via dependency upgrades
- Expand update-PR coverage so future upgrades arrive through scoped PRs
- Document the 4 deferred upstream-blocked items with expected-unblock triggers
- Maintain full visibility of the 51 spacedrive alerts as tracking signal for the planned integration

**Non-Goals:**
- Dismissing any Dependabot alert via API
- Any action on `spacedrive/` alerts (deferred to integration-time change)
- Assessing security of spacedrive upstream
- Upgrading transitive deps blocked on upstream (`rand`, `lru`, `lexical-core`)
- Migrating from `imap` 2.4 to 3.x
- CodeQL changes (separate change)

## Decisions

### 1. Upgrade vite rather than dismiss or pin

**Decision:** Upgrade vite to 6.4.2 or later across all three spaceui manifests.

**Rationale:** GHSA-4w7w-66w2-5vf9 has three vulnerability ranges: `<= 6.4.1`, `[7.0.0, 7.3.1]`, `[8.0.0, 8.0.4]`. Installed vite 5.4.21 falls into the first range. Upgrading to 6.4.2+ exits all three ranges in one move.

**Alternatives considered:**
- *Dismiss as `no_bandwidth`*: rejected — violates the non-dismissal policy; also, upgrade cost is low.
- *Pin to latest 5.x backport*: rejected — no evidence a patched 5.x exists; the GHSA lists 6.4.2 as the lowest safe version in the 5/6 series.
- *Stay on 5.4.21 and patch locally*: rejected — creates a permanent fork and doesn't resolve the alert.

**Fallback:** if the upgrade breaks storybook or the showcase demo in ways that can't be resolved within the change, leave the alerts open and document the blocker in `docs/security/deferred-advisories.md`. Do not dismiss.

### 2. Coordinate tauri plugin bump for glib upgrade

**Decision:** Attempt `cargo update -p glib --precise 0.20.x` first; if the resolver refuses, bump the blocking tauri plugin version in `desktop/src-tauri/Cargo.toml`.

**Rationale:** glib 0.18 → 0.20 is a major-version jump. Tauri plugins typically pin `glib ^0.18`, so cargo cannot advance the transitive pin alone. The minimal-impact path is a targeted plugin bump, not a full tauri major upgrade.

**Alternatives considered:**
- *Pin via `[patch.crates-io]`*: acceptable as interim if ecosystem bump is blocked, but creates long-term drag (each upstream release requires re-evaluation). Prefer ecosystem bump when available.
- *Full tauri major upgrade*: out of scope — too much risk for a single-alert resolution.

**Risk:** Linux webkit2gtk path is the most likely break surface. macOS is less affected. Test both CI matrices before merge.

### 3. Expand `dependabot.yml` for update-PR scoping only

**Decision:** Add six new `updates:` entries covering every shipped-code manifest that lacks one. Do not add entries for `spacedrive/**`.

**Rationale:** `.github/dependabot.yml` controls which directories receive automated update PRs. It does NOT control which directories receive security alerts. Contributors routinely conflate these; a top-of-file comment prevents future confusion.

**Alternatives considered:**
- *Minimal config (current 3 entries)*: rejected — missing manifests cause silent upgrade drift.
- *Maximal config including spacedrive*: rejected — we don't want upgrade PRs for vendored code. Upstream manages its own dep cadence.

### 4. Do not dismiss spacedrive alerts

**Decision:** The 51 spacedrive-scoped alerts remain open on the Security dashboard. No API dismissal calls.

**Rationale:** Visibility is more valuable than dashboard aesthetics. The alerts represent real upstream vulnerabilities in code spacebot may load at runtime once the HTTP integration lands. Leaving them open guarantees re-triage is structural (i.e., the next integration change must address them) rather than optional (i.e., easy to forget they exist because they're hidden).

**Alternatives considered:**
- *Bulk-dismiss as `tolerable_risk`*: rejected by policy. This was the obsolete approach in the archived `security-remediation` change.
- *Disable Dependabot alerts repo-wide*: nuclear option — loses visibility on all alerts, including the actionable ones. Unwise.

### 5. Document deferred items in-repo rather than only on the dashboard

**Decision:** Create `docs/security/deferred-advisories.md` listing the 4 upstream-blocked alerts (lexical-core #1, lru #3, rand root #15, rand desktop #18). Reference the doc from `CONTRIBUTING.md`.

**Rationale:** In-repo documentation survives contributor turnover, shows up in `git log` and PR reviews, and doesn't require GitHub dashboard access to audit. Each entry includes: GHSA ID, severity, blocker, unblock trigger.

**Alternatives considered:**
- *Only dashboard comments*: rejected — invisible to someone browsing the repo.
- *Inline `# SAFETY:` Cargo.toml comments*: rejected — cluttered and not discoverable.

## Risks / Trade-offs

- **Dashboard stays large (55 → 52 after fixes)** → Mitigation: `docs/security/deferred-advisories.md` and this change's scope statement make the triage state auditable in-repo. The dashboard count is not a target metric; "every spacebot-owned alert is fixed or documented" is.
- **Dependabot update-PR coverage may miss newly-added manifests** → Mitigation: a future hygiene change can add a `just lint-dependabot` CI step diffing `find` output against `dependabot.yml` entries. Out of scope for this change.
- **glib upgrade may break desktop build on Linux (webkit2gtk)** → Mitigation: test on both Linux and macOS CI matrices. If Linux breaks but macOS passes, a conditional `[patch.crates-io]` is a short-term option — document as blocker and open a follow-up.
- **vite 5 → 6 is a major upgrade with breaking changes** → Mitigation: run storybook and showcase builds before committing. If incompatible, leave alerts open with a blocker note; do not dismiss.
- **Storybook 8.6.18 may not fully support vite 6.x** → Mitigation: storybook added preview vite-6 support in 8.4+ and made it default in 9.0. If testing reveals the current storybook version is incompatible with vite 6, bump storybook to 9.x (or the minimum vite-6-compatible release) as part of task 1.7. If the storybook upgrade itself is too large to absorb in this change, leave the vite alerts open and document storybook as the blocker in `docs/security/deferred-advisories.md` — do not dismiss.
- **Spaceui workspace has a single `bun.lock`** → No per-sub-package lockfile exists at `spaceui/.storybook/bun.lock` or `spaceui/examples/showcase/bun.lock`. `bun install` runs once at `spaceui/` root and pins vite for all workspace members. Earlier drafts of this change mistakenly referenced per-sub-package lockfiles; that was corrected.
- **Spacedrive integration change may miss re-triage** → Mitigation: add the re-triage requirement to `security-audit` spec (forces the next integration change to address it).

## Migration Plan

1. Phase 1: vite upgrade in spaceui (lowest-risk, most-constrained surface)
2. Phase 2: glib upgrade in desktop (plan for resolver friction)
3. Phase 3: dependabot.yml expansion + scope comment
4. Phase 4: deferred-advisories doc + CONTRIBUTING.md reference
5. Phase 5: verification — confirm no accidental dismissals, remaining alert count = 52

Phases can be combined into a single PR or split as reviewer preference dictates. No rollback complexity — each phase is a pure-documentation or dependency-bump change.

## Open Questions

- Does the current CodeQL/Dependabot permissions config allow `gh api -X PATCH` dismissals by default? *Not relevant to this change since we're not dismissing, but worth confirming as future context.*
- Should `.claude/skills/deps-update/` get a new sub-step to review `deferred-advisories.md`, or is the `CONTRIBUTING.md` reference enough? *Defer to post-implementation feedback.*
