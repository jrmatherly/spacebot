---
name: docs-audit
description: Use when auditing project documentation for drift, stale claims, missing changelog entries, completed-but-still-listed TODOs, or when the user asks to review/refresh/cleanup docs across interface/, spaceui/, docs/, or repo-root markdown files. Complements session-sync (which handles CLAUDE.md + memories only).
---

# Docs Audit

Systematic audit of Spacebot's project documentation to find drift between what docs claim and what the codebase, git history, and release artifacts actually show. Produces a prioritized finding list with file:line citations and recommended remediation — never rewrites docs without confirmation.

## Scope

This skill covers documentation **outside** session-sync's scope. Roughly **505 markdown/MDX files** are tracked in the repo; after the exclusions below, ~210 are in scope for audit. Organized in two tiers.

### Tier 1 — Shipped / User-Facing Documentation (highest drift impact)

| Area | Files |
|------|-------|
| Repo root | `README.md`, `AGENTS.md`, `PROJECT_INDEX.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `METRICS.md`, `RUST_STYLE_GUIDE.md`, `SPACEUI_MIGRATION.md` (8 files) |
| Interface | `interface/DRY_VIOLATIONS.md` |
| SpaceUI root | `spaceui/README.md`, `spaceui/CONTRIBUTING.md`, `spaceui/INTEGRATION.md` |
| SpaceUI internal docs | `spaceui/docs/{COMPONENT-AUDIT,REPO_SUMMARY,SHARED-UI-STRATEGY,TAILWIND-V4-MIGRATION}.md` |
| SpaceUI changesets | `spaceui/.changeset/README.md` + `config.json` |
| SpaceUI packages | `spaceui/packages/{ai,explorer,forms,primitives,tokens}/README.md` + all 6 `CHANGELOG.md` (`icons/` has no README — flag as 🔵 Missing or document why) |
| Docs top-level | `docs/README.md`, `docs/docker.md`, `docs/mattermost.md`, `docs/metrics.md` |
| Published MDX content | `docs/content/docs/**/*.mdx` — **38 files** across 6 route groups: `(core)`, `(features)`, `(configuration)`, `(deployment)`, `(getting-started)`, `(messaging)` |
| Design docs | `docs/design-docs/*.md` — 47 files; historical record, append-only |
| Security policy | `docs/security/*.md` (tracked per project_overview memory) |
| Transient plans | `docs/superpowers/plans/*.md` — completed ones should move to `.scratchpad/completed/` |

### Tier 2 — Internal / Operational Documentation

These are lower-visibility but affect agent behavior, coding conventions, and internal state.

| Area | Files |
|------|-------|
| Agent personas | `presets/*/{IDENTITY,ROLE,SOUL}.md` — 27 files across 9 presets. Changes here shape runtime agent behavior. |
| Coding rules | `.claude/rules/*.md` — 7 files. Referenced by CLAUDE.md; drift propagates into every code change. |
| Custom agents | `.claude/agents/*.md` — 2 files (migration-writer, security-reviewer). |
| Project skills | `.claude/skills/*/SKILL.md` + nested references — 43 tracked files (21 top-level skills + 22 nested under `archon/`, `session-primer/`, `cluster-context/`). **Special attention:** `session-primer/references/skills-catalog.md` must list every skill, including new additions. |
| Runtime skills | `skills/builtin/*/SKILL.md` — 1 file (wiki-writing). Skills the daemon ships to agents. |
| Canonical specs | `openspec/specs/*/spec.md` — 7 files. Source-of-truth for deps/integration. **Gap:** no other skill audits these for drift; this is the docs-audit-owned slice. |

### Explicitly Out of Scope

| Area | Reason | Delegate to |
|------|--------|-------------|
| `CLAUDE.md` | Session memory + instruction drift | `/session-sync` |
| `.serena/memories/*.md` | Project memories | `/session-sync` |
| `openspec/changes/archive/*` (37 files) | Immutable historical record — never edit archived specs | `/openspec-verify-change` (active only) |
| `openspec/changes/<active>/*` | Active change artifacts | `/openspec-apply-change`, `/openspec-verify-change` |
| `.codex/skills/*` (5), `.windsurf/skills/*` (5), `.windsurf/workflows/opsx-*` (5) | Mirror copies of `.claude/skills/openspec-*` for other agent platforms. Windsurf `opsx-*` workflows are thin wrappers that invoke the same skills. | Audit canonical source in `.claude/skills/`; mirrors are derived. |
| `.full-review/*.md` (14 files) | Code-review framework templates, not project docs | N/A |
| `spacedrive/` (241 tracked files) | Vendored upstream workspace with its own maintenance | Spacedrive upstream |
| `.claude/worktrees/*` | Git-worktree checkout on another branch; not authoritative on `main` (not gitignored but 0 files tracked) | N/A — exclude |
| `vendor/`, `node_modules/`, `target/`, `dist/`, `.next/`, `.source/`, `.turbo/`, `.cargo/`, `.code-review-graph/` | Build artifacts / dependency caches | N/A — ignored |
| `.scratchpad/`, `.remember/`, `.worktrees/` | Gitignored; not authoritative state | N/A — exclude |
| `packages/api-client/` | Top-level private workspace package (`"private": true`), no README by design | Document as "intentionally undocumented" |
| `migrations/`, `prompts/`, `scripts/`, `nix/`, `examples/`, `tests/`, `src/`, `desktop/`, `.archon/`, `.github/`, `.githooks/` | Zero markdown files by design — don't re-investigate | N/A |
| Dependency version bumps in any doc | | `/deps-update` |

**Approximate scale:**
- Tier 1 (user-facing): ~120 files
- Tier 2 (operational): ~90 files
- Out of scope: ~295 files (including vendored spacedrive and sibling-platform mirrors)
- Full audit = ~210 files reviewed against ground truth

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

### OpenSpec canonical specs (`openspec/specs/*/spec.md`)

These describe the **current** state of dependency management, integration surfaces, and security posture. After an OpenSpec change is archived, its `specs/*/spec.md` content merges into the canonical `openspec/specs/*/spec.md` — but the spec file can still drift if implementation moves without a formal OpenSpec change. This is a docs-audit-owned slice because no other skill covers it.

- Grep each spec for version strings, package names, file paths, and command examples; verify against actual manifests (`Cargo.toml`, `package.json`) and the tree.
- If a spec references a file path: check the path exists. If a spec names a crate/package at a version: diff against the manifest.
- If drift is found: recommend opening a formal OpenSpec change via `/openspec-propose` rather than inline-editing the spec, unless the drift is purely cosmetic (typo, wording).
- Archived changes (`openspec/changes/archive/*`) are off-limits — never propose edits there.

### Project skills (`.claude/skills/*/SKILL.md`)

- When a new skill is added or renamed, `session-primer/references/skills-catalog.md` must be updated. This is the highest-velocity drift point in the meta-docs — check it every audit.
- Skill reference files under `.claude/skills/<name>/references/` and `.claude/skills/<name>/guides/` are in scope but often nested deep. Use `find .claude/skills -name "*.md"` to enumerate before auditing.

## Common Mistakes

| Mistake | Why it's wrong | Do instead |
|---------|---------------|------------|
| Summarizing what each doc says | That's a description, not an audit | Find drift between claim and reality |
| Reporting "might be outdated" without checking | Hallucinated findings waste user time | Run the verification command first |
| Editing CHANGELOGs directly in a changeset-managed repo | Breaks release automation | Create a `.changeset/*.md` entry |
| Rewriting migration docs to "clean them up" | Destroys historical record | Update status, append, never replace |
| Fixing everything in one PR | Too big to review | Group by category, propose separate commits |
| Trusting memory snapshots over live state | Memory is a snapshot, not ground truth | Re-verify every claim from git/files |
| Auditing archived `openspec/changes/archive/*` files | They're immutable historical record by design | Skip — only audit active changes via `/openspec-verify-change` |
| Auditing `.claude/worktrees/*` content | That's a different branch's checkout, not authoritative on `main` | Skip — those files belong to another branch's history |

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

# Does a preset file reference tools/abilities that still exist?
grep -rE "tool|command" presets/<name>/ROLE.md

# Does a coding rule reference a file path that still exists?
grep -rE '`src/[a-z]+\.rs`' .claude/rules/

# Is every skill listed in skills-catalog.md?
diff <(ls .claude/skills/ | grep -v '\.') \
     <(grep -oE '/[a-z-]+$' .claude/skills/session-primer/references/skills-catalog.md | sort -u)

# Does a canonical openspec spec still match reality?
grep -oE '`[a-z_-]+`|[0-9]+\.[0-9]+\.[0-9]+' openspec/specs/<name>/spec.md
# Then cross-check each against Cargo.toml / package.json / the tree
```

## Composition With Other Skills

- Run **after** `/deps-update` lands version bumps — catches doc refs that weren't updated
- Run **before** `/pr-gates` on doc-only branches
- Run **alongside** `/session-sync` for a full documentation + memory refresh
- If findings touch CLAUDE.md → hand off to `/session-sync`
- If findings require new doc files or heavy rewrites → propose via `/openspec-propose` first

## When to Update THIS Skill

This skill is itself Tier 2 documentation — it goes stale as the repo evolves. Revisit the scope tables above when any of these happen:

- A new top-level directory is added (check for new docs in it; add to Tier 1 or explicit out-of-scope)
- A new directory appears under `.claude/` (agents, rules, skills, or something new)
- A new package is added to `spaceui/packages/` (update the package row)
- A new agent-platform mirror is added (`.codex/`, `.windsurf/`, `.cursor/`, etc.) — add to out-of-scope
- A new route group is added under `docs/content/docs/(...)/`
- The `packages/api-client/` policy changes (currently private, no README — if it gains a README, move to Tier 1)
- A new skill is created or an existing one is renamed — update `session-primer/references/skills-catalog.md` and verify this skill's scope still matches reality
