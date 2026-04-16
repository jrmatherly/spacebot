---
name: docs-audit
description: Use when auditing project documentation for drift, stale claims, missing changelog entries, completed-but-still-listed TODOs, or when the user asks to review/refresh/cleanup docs across interface/, spaceui/, docs/, or repo-root markdown files. Complements session-sync (which handles CLAUDE.md + memories only).
---

# Docs Audit

Systematic audit of Spacebot's project documentation to find drift between what docs claim and what the codebase, git history, and release artifacts actually show. Produces a prioritized finding list with file:line citations and recommended remediation — never rewrites docs without confirmation.

## Scope

This skill covers documentation **outside** session-sync's scope. Explicit targets:

| Area | Files |
|------|-------|
| Repo root | `README.md`, `AGENTS.md`, `PROJECT_INDEX.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `METRICS.md`, `RUST_STYLE_GUIDE.md`, `SPACEUI_MIGRATION.md` |
| Interface | `interface/DRY_VIOLATIONS.md`, `interface/PLAN.md` |
| SpaceUI root | `spaceui/README.md`, `spaceui/CONTRIBUTING.md`, `spaceui/INTEGRATION.md` |
| SpaceUI docs | `spaceui/docs/COMPONENT-AUDIT.md`, `REPO_SUMMARY.md`, `SHARED-UI-STRATEGY.md`, `TAILWIND-V4-MIGRATION.md` |
| SpaceUI changesets | `spaceui/.changeset/README.md` + `config.json` |
| SpaceUI packages | `spaceui/packages/{tokens,primitives,forms,icons,ai,explorer}/{README.md,CHANGELOG.md}` |
| Docs site | `docs/` (README, docker.md, mattermost.md, metrics.md, design-docs/*, security/*) |

**Explicitly out of scope** (delegate):
- `CLAUDE.md` and Serena/auto-memory → `/session-sync`
- Dependency version bumps → `/deps-update`
- OpenSpec proposals → `/openspec-*`

## When to Use

- User says "audit docs", "review documentation", "find stale docs", "cleanup docs", "what's out of date in docs"
- After a batch of merged PRs when package/interface state has shifted (e.g., Tailwind v4 migration, dep upgrade wave)
- Before a release to validate CHANGELOGs match landed commits
- When a doc claim ("Status: in progress", "as of YYYY-MM-DD", "v0.33") is suspected stale
- Before handing the repo to a new contributor — to catch drift that'll confuse them

## When NOT to Use

- For CLAUDE.md or Serena memory drift → use `/session-sync`
- For dependency upgrades themselves → use `/deps-update`
- For OpenSpec change artifacts → use `/openspec-verify-change`
- For *writing new docs* from scratch — this skill audits existing docs only
- For one-file spot-fixes the user already identified — just fix directly

## The Core Discipline

**Every finding must be backed by evidence.** A finding has three required parts:

1. **Claim** — verbatim quote from the doc with file:line citation
2. **Evidence** — command output, git log, file listing, or codebase grep that contradicts the claim
3. **Recommendation** — specific, actionable change (with line number if known)

A finding without all three parts is a rumor. Drop it or go get the evidence.

**You are auditing, not editing.** Produce a report. The user decides what to fix. Only edit docs when the user explicitly says "apply the fixes" or approves specific items.

## Red Flags — Stop and Verify

These thoughts mean you're about to produce hallucinated findings:

| Thought | What to do instead |
|---------|--------------------|
| "This README looks outdated" | Run the cross-reference before writing that down. |
| "The CHANGELOG is probably missing entries" | `git log --oneline -- packages/X/` and diff against CHANGELOG.md. |
| "Let me summarize what each file says" | That's not an audit. Cite specific drift. |
| "Version is likely stale" | Read the actual `package.json` / `Cargo.toml`. Quote both. |
| "I'll note this as 'possibly out of date'" | Either prove it or drop it. No hedging. |

## Audit Procedure

### Step 1: Ground Truth Collection (do first, in parallel)

Before reading any docs, gather the objective state you'll cross-reference against:

```bash
# Git activity windows — scope the audit
git log --since="3 months ago" --oneline
git log --since="3 months ago" --stat --name-only -- spaceui/ | head -100
git log --since="3 months ago" --oneline -- interface/
git log --since="3 months ago" --oneline -- docs/

# Version sources
cat Cargo.toml | grep -E '^(version|name|edition)'
grep -r '"version"' spaceui/packages/*/package.json
cat spaceui/package.json | grep version

# Package/file counts to verify "N components" / "N packages" claims
ls spaceui/packages/
find spaceui/packages -name "README.md" -o -name "CHANGELOG.md"
find src -name "*.rs" | wc -l                       # PROJECT_INDEX source-file claim
ls migrations/ | wc -l                              # migration count claim
ls presets/ | wc -l                                 # preset count claim
ls prompts/ | wc -l                                 # prompt count claim

# Changeset state — what's queued that CHANGELOGs haven't absorbed yet
ls spaceui/.changeset/*.md 2>/dev/null | grep -v README
```

Also pull the code-review-graph summary if available (`list_graph_stats_tool`) for authoritative node/edge counts that docs sometimes quote.

### Step 2: Read Docs With Cross-Reference in Hand

For each target file, don't just read — compare against Step 1 evidence. Specific patterns to hunt:

**Version claims**
- Any "v0.X", "version 0.X", "x.y.z" mentioned in prose → diff against actual manifest
- "Rig v0.33" type references are known drift risk — memory obs #25725 flagged this

**Date-stamped claims**
- `**Last updated:** ...`, `**Status (as of YYYY-MM-DD):** ...`, `**Updated:**` headers
- If the doc is in a directory with commits newer than the stamp, flag it

**Status trackers in living plans**
- `interface/DRY_VIOLATIONS.md`: each "STILL PENDING" item — grep the codebase for the hardcoded pattern; if 0 matches, item is fixed and should move to ✅
- `interface/PLAN.md`: tasks marked incomplete — check `git log --all --grep="<task keyword>"` and current file state
- Any "TODO", "Planned", "Not yet implemented" — verify against code

**CHANGELOG completeness**
- For each `spaceui/packages/X/CHANGELOG.md`: take the latest version entry, find its commit, then `git log <that-commit>..HEAD -- spaceui/packages/X/`. Any commits not reflected in CHANGELOG or in `.changeset/` queue = missing entry.
- Cross-check version number in CHANGELOG top entry vs `package.json` version field

**Count/inventory claims**
- "6 packages", "40+ components", "42 migrations", "9 presets", "206 Rust source files"
- Each number gets a shell command to verify. Report any mismatch.

**Migration / audit doc staleness**
- `SPACEUI_MIGRATION.md`, `TAILWIND-V4-MIGRATION.md`, `COMPONENT-AUDIT.md`, `SHARED-UI-STRATEGY.md`
- If status says "in progress" for an area: is there a PR that closed it? `git log --grep="tailwind" --grep="migration"`
- If it references phases/steps: which are complete per commits, which per doc?

**Cross-doc consistency**
- Tech stack claims in `README.md` vs `AGENTS.md` vs `PROJECT_INDEX.md` vs `CLAUDE.md` — mismatches are drift
- Package count/name in `spaceui/README.md` vs actual `spaceui/packages/` listing
- `INTEGRATION.md` install instructions vs current package names in `package.json`

**Broken links**
- Relative links to moved/renamed files (`[X](../old-path/file.md)`)
- Use `grep -rE '\]\([^)]+\.md\)' <doc>` to list, then verify each target exists

### Step 3: Categorize Findings

Sort findings into these buckets in the output report:

- **🔴 Incorrect** — doc states something that's provably false (wrong version, claims complete work as pending, etc.). Must fix.
- **🟡 Stale** — doc was correct when written but no longer matches reality. Should fix.
- **🔵 Missing** — information that should be in the doc but isn't (undocumented new package, missing CHANGELOG entry, unreleased changeset). Should add.
- **⚪ Polish** — consistency, cross-reference, link hygiene, formatting. Nice to have.

### Step 4: Produce the Report

Standard report format:

```markdown
# Docs Audit — YYYY-MM-DD

**Scope:** <what you audited>
**Ground-truth sources:** git log <range>, <manifests>, <commands used>

## 🔴 Incorrect (N)

### [File:line] Brief title
- **Claim:** "<verbatim quote>"
- **Evidence:** <command output / git log / file listing that contradicts>
- **Recommend:** <specific fix, e.g., "change 'v0.33' to 'v0.35' at line 42">

## 🟡 Stale (N)
... same structure ...

## 🔵 Missing (N)
... same structure ...

## ⚪ Polish (N)
... same structure ...

## Out of Scope (deferred)
- <anything you noticed but belongs to another skill — name the skill>
```

### Step 5: Await Direction

Stop after the report. Do not begin edits. Ask:
> "Want me to apply these fixes? You can accept all, a subset (by number), or revise."

Apply only what's approved. For changelog-related fixes in `spaceui/`, remember changesets are the source of truth — add `.changeset/*.md` entries rather than hand-editing `CHANGELOG.md` unless the changelog is already published.

## Category-Specific Rules

### CHANGELOGs (spaceui/packages/*/CHANGELOG.md)

- Never hand-edit CHANGELOG.md if changesets govern it. Instead: create `spaceui/.changeset/<descriptive-name>.md` with the missing entries, and let the release workflow consume them.
- Exception: previously-released entries can be corrected (typo, wrong version) — but never add new released entries directly.
- If version mismatch between `CHANGELOG.md` top entry and `package.json`: likely an unpublished changeset queued — check `.changeset/*.md` first.

### Living plans (PLAN.md, DRY_VIOLATIONS.md)

- "FIXED ✅" items that grep-verify as still present → move back to PENDING (this is a real regression)
- "STILL PENDING" items that grep-verify as gone → move to FIXED ✅ with a commit SHA reference
- If the plan is fundamentally complete (everything in FIXED), suggest archival (`interface/docs/archive/` or deletion with a commit note)

### Migration docs (SPACEUI_MIGRATION.md, TAILWIND-V4-MIGRATION.md)

- These are narrative docs. Don't rewrite them wholesale. Update only the **Status** header and completion checklist items.
- If migration is fully done, suggest moving to `docs/design-docs/` or `docs/archive/` as historical record.

### Architectural docs (README, AGENTS, PROJECT_INDEX)

- Tech stack tables and counts are high-drift. Prioritize these.
- Cross-doc consistency matters: if `AGENTS.md` says "Rig v0.35" and `README.md` says "Rig v0.33", both need to agree with `Cargo.toml`.

### docs/ (Fumadocs site)

- Content files under `docs/content/` or `docs/app/` are published. Drift here is user-facing — high priority.
- `docs/design-docs/` is append-mostly. Don't retro-edit historical design docs; add new ones or append "Status updated" sections.
- `docs/security/deferred-advisories.md` is policy-tracked (per `project_overview` memory) — changes here need context.

## Common Mistakes

| Mistake | Why it's wrong | Do instead |
|---------|---------------|------------|
| Summarizing what each doc says | That's a description, not an audit | Find drift between claim and reality |
| Reporting "might be outdated" without checking | Hallucinated findings waste user time | Run the verification command first |
| Editing CHANGELOGs directly in a changeset-managed repo | Breaks release automation | Create a `.changeset/*.md` entry |
| Rewriting migration docs to "clean them up" | Destroys historical record | Update status, append, never replace |
| Fixing everything in one PR | Too big to review | Group by category, propose separate commits |
| Trusting memory snapshots over live state | Memory is a snapshot, not ground truth | Re-verify every claim from git/files |

## Quick Reference — Verification Commands

```bash
# Is a claim about a package version correct?
cat spaceui/packages/<name>/package.json | grep version
cat Cargo.toml | grep -A1 '^\[package\]'

# Is a CHANGELOG missing entries?
LAST_VERSION=$(head -5 spaceui/packages/<name>/CHANGELOG.md | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
git log --oneline "v$LAST_VERSION..HEAD" -- spaceui/packages/<name>/

# Are there queued changesets?
ls spaceui/.changeset/*.md 2>/dev/null | grep -v README

# Does a "FIXED ✅" item in DRY_VIOLATIONS.md actually grep as gone?
grep -rn "<hardcoded pattern>" interface/src/

# Does a count claim match reality?
ls spaceui/packages/ | wc -l                    # package count
find src -name "*.rs" | wc -l                   # rust source files
ls migrations/ | wc -l                          # migrations
ls presets/ | wc -l                             # presets

# Is a migration doc's "as of" date stale?
git log -1 --format="%cI" -- spaceui/docs/TAILWIND-V4-MIGRATION.md
git log --since="<that date>" --oneline -- spaceui/
```

## Composition With Other Skills

- Run **after** `/deps-update` lands version bumps — catches doc refs that weren't updated
- Run **before** `/pr-gates` on doc-only branches
- Run **alongside** `/session-sync` for a full documentation + memory refresh
- If findings touch CLAUDE.md → hand off to `/session-sync`
- If findings require new doc files or heavy rewrites → propose via `/openspec-propose` first
