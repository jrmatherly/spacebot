---
name: docs-audit
description: Use when auditing project documentation for drift, stale claims, missing changelog entries, completed-but-still-listed TODOs, or when the user asks to review/refresh/cleanup docs across interface/, spaceui/, docs/, or repo-root markdown files. Complements session-sync (which handles CLAUDE.md + memories only).
---

# Docs Audit

Systematic audit of Spacebot's project documentation to find drift between what docs claim and what the codebase, git history, and release artifacts actually show. Produces a prioritized finding list with file:line citations and recommended remediation. Never rewrites docs without confirmation.

## Scope

This skill covers documentation **outside** session-sync's scope. Several hundred markdown/MDX files are tracked in the repo. The majority are excluded below (most live under vendored `spacedrive/`). The remainder is in scope for audit, organized in two tiers. Per-category commands in Step 1 produce the exact counts. Avoid quoting a single aggregate here to prevent drift.

### Tier 1 — Shipped / User-Facing Documentation (highest drift impact)

| Area | Files |
|------|-------|
| Repo root | `README.md`, `AGENTS.md`, `PROJECT_INDEX.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `METRICS.md`, `RUST_STYLE_GUIDE.md` (7 files) |
| Interface | `interface/DRY_VIOLATIONS.md` |
| SpaceUI root | `spaceui/README.md`, `spaceui/CONTRIBUTING.md`, `spaceui/INTEGRATION.md` (the last is a reference for external consumers, not internal workflow — audit as such) |
| Vendored Spacedrive policy files | `spacedrive/README.md`, `spacedrive/SECURITY.md`, `spacedrive/CONTRIBUTING.md`, `spacedrive/CODE_OF_CONDUCT.md`, `spacedrive/AGENTS.md` — these 5 root-level files are IN scope because they've been reframed as Spacebot-owned policy documents for the vendored subtree (banners, clone URLs, issue/PR routing pointed at `jrmatherly/spacebot`). The rest of `spacedrive/` (crates/, core/, apps/, docs/, extensions/, adapters/, schemas/) remains out of scope as upstream-maintained source. |
| SpaceUI internal docs | `spaceui/docs/{COMPONENT-AUDIT,REPO_SUMMARY,SHARED-UI-STRATEGY,TAILWIND-V4-MIGRATION}.md` |
| SpaceUI changesets | `spaceui/.changeset/README.md` + `config.json` |
| SpaceUI packages | `spaceui/packages/{ai,explorer,forms,icons,primitives,tokens}/README.md` + all 6 `CHANGELOG.md` |
| Docs top-level | `docs/README.md`, `docs/docker.md`, `docs/mattermost.md`, `docs/metrics.md` |
| Published MDX content | `docs/content/docs/**/*.mdx` — **38 files** across 6 route groups: `(core)`, `(features)`, `(configuration)`, `(deployment)`, `(getting-started)`, `(messaging)` |
| Design docs | `docs/design-docs/*.md` — 50 files; historical record, append-only |
| Security policy | `docs/security/*.md` (tracked per project_overview memory) |
| Transient plans | `docs/superpowers/plans/*.md` — completed ones should move to `.scratchpad/completed/` |
| Deployment values | `deploy/helm/spacebot/{values.yaml,values.local.yaml,README.md}` — Kubernetes/Helm values for the Talos cluster. Consumes `bjw-s-labs/app-template` (not a wrapper chart). Drift risk: image tag vs actual release, port/env mismatches against `src/config/`, probe paths against API handlers. |

### Tier 2 — Internal / Operational Documentation

These are lower-visibility but affect agent behavior, coding conventions, and internal state.

| Area | Files |
|------|-------|
| Nested CLAUDE.md | `spaceui/CLAUDE.md`, `interface/CLAUDE.md`, `desktop/CLAUDE.md`, `openspec/CLAUDE.md` — subtree-scoped instructions loaded on-demand when agents edit those subtrees. Distinct from the root `CLAUDE.md` (which is owned by `/session-sync`). |
| Agent personas | `presets/*/{IDENTITY,ROLE,SOUL}.md` — 27 files across 9 presets. Changes here shape runtime agent behavior. |
| Coding rules | `.claude/rules/*.md` — 10 files (`rust-essentials`, `rust-iteration-loop`, `rust-patterns`, `writing-guide`, `coding-discipline`, `async-state-safety`, `messaging-adapter-parity`, `provider-integration`, `tool-authoring`, `api-handler`). Referenced by CLAUDE.md; drift propagates into every code change. |
| Custom agents | `.claude/agents/*.md` — 2 files (migration-writer, security-reviewer). |
| Project skills | `.claude/skills/*/SKILL.md` + nested references — 43 tracked files (22 top-level skills + 21 nested under `archon/`, `session-primer/`, `cluster-context/`). **Special attention:** `session-primer/references/skills-catalog.md` must list every skill, including new additions. |
| Runtime skills | `skills/builtin/*/SKILL.md` — 3 files (memory-writing, task-triage, wiki-writing). Skills the daemon ships to agents. |
| Canonical specs | `openspec/specs/*/spec.md` — 8 files. Source-of-truth for deps/integration. **Gap:** no other skill audits these for drift; this is the docs-audit-owned slice. |

### Explicitly Out of Scope

| Area | Reason | Delegate to |
|------|--------|-------------|
| `CLAUDE.md` | Session memory + instruction drift | `/session-sync` |
| `.serena/memories/*.md` | Project memories | `/session-sync` |
| `openspec/changes/archive/*` (7 archived changes, immutable) | Immutable historical record — never edit archived specs | `/openspec-verify-change` (active only) |
| `openspec/changes/<active>/*` | Active change artifacts | `/openspec-apply-change`, `/openspec-verify-change` |
| `.codex/skills/*` (5), `.windsurf/skills/*` (5), `.windsurf/workflows/opsx-*` (5) | Mirror copies of `.claude/skills/openspec-*` for other agent platforms. Windsurf `opsx-*` workflows are thin wrappers that invoke the same skills. | Audit canonical source in `.claude/skills/`; mirrors are derived. |
| `.full-review/*.md` (14 files) | Code-review framework templates, not project docs | N/A |
| `spacedrive/` (241 tracked files) **except** the five policy files listed in Tier 1 above | Vendored upstream source (`crates/`, `core/`, `apps/`, `docs/`, `extensions/`, `adapters/`, `schemas/`, `whitepaper/`). Own maintenance lifecycle. | Spacedrive upstream |
| `.claude/worktrees/*` | Git-worktree checkout on another branch; not authoritative on `main` (not gitignored but 0 files tracked) | N/A — exclude |
| `vendor/`, `node_modules/`, `target/`, `dist/`, `.next/`, `.source/`, `.turbo/`, `.cargo/`, `.code-review-graph/` | Build artifacts / dependency caches | N/A — ignored |
| `.scratchpad/`, `.remember/`, `.worktrees/` | Gitignored; not authoritative state | N/A — exclude |
| `packages/api-client/` | Top-level private workspace package (`"private": true`), no README by design | Document as "intentionally undocumented" |
| `migrations/`, `prompts/`, `scripts/`, `nix/`, `examples/`, `tests/`, `src/`, `.archon/`, `.github/`, `.githooks/` | Zero markdown files by design — don't re-investigate | N/A |
| `desktop/` top-level | Has `desktop/CLAUDE.md` (covered by the "Nested CLAUDE.md" Tier 2 row) — no other docs | N/A |
| Dependency version bumps in any doc | | `/deps-update` |

**Scale guidance:** Tier 1 (user-facing) dominates drift impact. Tier 2 (operational) governs agent and tooling behavior. Out-of-scope is the majority (vendored `spacedrive/`, sibling-platform mirrors). Per-category verify commands in Step 1 produce exact counts at audit time. Don't bake aggregates into this skill.

## When to Use

- User says "audit docs", "review documentation", "find stale docs", "cleanup docs", "what's out of date in docs"
- After a batch of merged PRs when package/interface state has shifted (e.g., Tailwind v4 migration, dep upgrade wave)
- Before a release to validate CHANGELOGs match landed commits
- When a doc claim ("Status: in progress", "as of YYYY-MM-DD", "v0.33") is suspected stale
- Before handing the repo to a new contributor, to catch drift that'll confuse them

## When NOT to Use

- For CLAUDE.md or Serena memory drift → use `/session-sync`
- For dependency upgrades themselves → use `/deps-update`
- For OpenSpec change artifacts → use `/openspec-verify-change`
- For *writing new docs* from scratch. This skill audits existing docs only.
- For one-file spot-fixes the user already identified. Just fix directly.

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

# Nested CLAUDE.md inventory — each file scopes agent behavior in a subtree
find . -maxdepth 3 -name "CLAUDE.md" -not -path "./spacedrive/*" -not -path "./node_modules/*"

# Coding rules inventory + path-scoped frontmatter audit
ls .claude/rules/*.md
grep -l '^paths:' .claude/rules/*.md    # which rules are path-scoped?
grep -H '^paths:' .claude/rules/*.md    # verify no two rules collide on the same path

# Deploy/Helm ground truth
grep -E '^\s*(tag|repository|port|path):' deploy/helm/spacebot/values.yaml
git tag -l | tail -5                    # image tag should match a real release
ls openspec/changes/                    # active proposals vs archived-only state
ls openspec/changes/archive/            # archived changes (directories, not files)
```

Also pull the code-review-graph summary if available (`list_graph_stats_tool`) for authoritative node/edge counts that docs sometimes quote.

### Step 2: Read Docs With Cross-Reference in Hand

For each target file, don't just read. Compare against Step 1 evidence. Specific patterns to hunt:

**Version claims**
- Any "v0.X", "version 0.X", "x.y.z" mentioned in prose → diff against actual manifest
- "Rig v0.33" type references are known drift risk (memory obs #25725 flagged this; resolved 2026-04-16)

**Date-stamped claims**
- `**Last updated:** ...`, `**Status (as of YYYY-MM-DD):** ...`, `**Updated:**` headers
- If the doc is in a directory with commits newer than the stamp, flag it

**Status trackers in living plans**
- `interface/DRY_VIOLATIONS.md`: for each "STILL PENDING" item, grep the codebase for the hardcoded pattern. If 0 matches, the item is fixed and should move to ✅.
- `interface/PLAN.md`: for tasks marked incomplete, check `git log --all --grep="<task keyword>"` and current file state.
- Any "TODO", "Planned", "Not yet implemented" claim. Verify against code.

**CHANGELOG completeness**
- For each `spaceui/packages/X/CHANGELOG.md`: take the latest version entry, find its commit, then `git log <that-commit>..HEAD -- spaceui/packages/X/`. Any commits not reflected in CHANGELOG or in `.changeset/` queue = missing entry.
- Cross-check version number in CHANGELOG top entry vs `package.json` version field

**Count/inventory claims**
- "6 packages", "40+ components", "42 migrations", "9 presets", "206 Rust source files"
- Each number gets a shell command to verify. Report any mismatch.

**Migration / audit doc staleness**
- `docs/design-docs/spaceui-migration.md`, `spaceui/docs/TAILWIND-V4-MIGRATION.md`, `spaceui/docs/COMPONENT-AUDIT.md`, `spaceui/docs/SHARED-UI-STRATEGY.md`
- If status says "in progress" for an area: is there a PR that closed it? `git log --grep="tailwind" --grep="migration"`
- If it references phases/steps: which are complete per commits, which per doc?

**Cross-doc consistency**
- Tech stack claims in `README.md` vs `AGENTS.md` vs `PROJECT_INDEX.md` vs `CLAUDE.md`. Mismatches are drift.
- Package count/name in `spaceui/README.md` vs actual `spaceui/packages/` listing
- `INTEGRATION.md` install instructions vs current package names in `package.json`

**Broken links**
- Relative links to moved/renamed files (`[X](../old-path/file.md)`)
- Use `grep -rE '\]\([^)]+\.md\)' <doc>` to list, then verify each target exists

**References to gitignored paths**
- Tracked files must NOT link into `.scratchpad/`, `.remember/`, `.worktrees/`, `.serena/`, `.claude/worktrees/`, or any other gitignored directory. Those paths are invisible to everyone who clones the repo.
- Sweep: `grep -rnE "\.scratchpad/|\.remember/|\.worktrees/|\.serena/" --include="*.md" . | grep -v "^\./\.scratchpad" | grep -v "^\./\.claude/" | grep -v "^\./\.serena/" | grep -v "^\./\.codex/" | grep -v "^\./\.windsurf/" | grep -v "^\./\.full-review/" | grep -v "^\./spacedrive/"`
- When a tracked file needs to reference content that currently lives in `.scratchpad/completed/`, port it to `docs/design-docs/` (for decision records) or another appropriate tracked location before adding the reference.
- Exceptions: `openspec/changes/archive/*` files are immutable — their `.scratchpad/` references are historical record and must not be edited. Transient plans at `docs/superpowers/plans/*` may reference `.scratchpad/*` source specs; those references describe historical input, not live links.

**Upstream-repo URLs in vendored subtrees**
- The `spacedrive/` subtree is vendored from `spacedriveapp/spacedrive` and `spaceui/` was imported from `spacedriveapp/spaceui`. Both now belong to this repo. Clone URLs, issue links, and PR workflow prose in their policy files (`spacedrive/README.md`, `spacedrive/CONTRIBUTING.md`, `spacedrive/SECURITY.md`, `spacedrive/CODE_OF_CONDUCT.md`, `spacedrive/AGENTS.md`, `spaceui/README.md`, `spaceui/CONTRIBUTING.md`) must point at `jrmatherly/spacebot`, not `spacedriveapp/*`.
- Sweep: `grep -rnE "spacedriveapp|v2\.spacedrive\.com|discord\.gg" spacedrive/README.md spacedrive/CONTRIBUTING.md spacedrive/SECURITY.md spacedrive/CODE_OF_CONDUCT.md spacedrive/AGENTS.md spaceui/README.md spaceui/CONTRIBUTING.md`
- Expected: zero hits in the 5 spacedrive policy files + 2 spaceui policy files. Deeper content (narrative prose describing upstream's architecture) is out of scope.
- `spaceui/INTEGRATION.md` is a deliberate exception: it documents the external-consumer pattern, so npm-publication and upstream-workflow claims there are correct for its purpose. The file banner must mark it as reference-only, not live-workflow for this fork.

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
- <anything you noticed but belongs to another skill; name the skill>
```

### Step 5: Await Direction

Stop after the report. Do not begin edits. Ask:
> "Want me to apply these fixes? You can accept all, a subset (by number), or revise."

Apply only what's approved. For changelog-related fixes in `spaceui/`, remember changesets are the source of truth. Add `.changeset/*.md` entries rather than hand-editing `CHANGELOG.md` unless the changelog is already published.

## Category-Specific Rules

### CHANGELOGs (spaceui/packages/*/CHANGELOG.md)

- Never hand-edit CHANGELOG.md if changesets govern it. Instead: create `spaceui/.changeset/<descriptive-name>.md` with the missing entries, and let the release workflow consume them.
- Exception: previously-released entries can be corrected (typo, wrong version), but never add new released entries directly.
- If version mismatch between `CHANGELOG.md` top entry and `package.json`: likely an unpublished changeset queued. Check `.changeset/*.md` first.

### Living plans (PLAN.md, DRY_VIOLATIONS.md)

- "FIXED ✅" items that grep-verify as still present → move back to PENDING (this is a real regression)
- "STILL PENDING" items that grep-verify as gone → move to FIXED ✅ with a commit SHA reference
- If the plan is fundamentally complete (everything in FIXED), suggest archival (`interface/docs/archive/` or deletion with a commit note)

### Migration docs (docs/design-docs/spaceui-migration.md, spaceui/docs/TAILWIND-V4-MIGRATION.md)

- These are narrative docs. Don't rewrite them wholesale. Update only the **Status** header and completion checklist items.
- If migration is fully done, suggest moving to `docs/design-docs/` or `docs/archive/` as historical record.

### Architectural docs (README, AGENTS, PROJECT_INDEX)

- Tech stack tables and counts are high-drift. Prioritize these.
- Cross-doc consistency matters: if `AGENTS.md` states one dependency version and `README.md` states another, both need to agree with `Cargo.toml`.

### docs/ (Fumadocs site)

- Content files under `docs/content/` or `docs/app/` are published. Drift here is user-facing and high priority.
- `docs/design-docs/` is append-mostly. Don't retro-edit historical design docs; add new ones or append "Status updated" sections.
- `docs/security/deferred-advisories.md` is policy-tracked (per `project_overview` memory). Changes here need context.

### Deploy/Helm values (`deploy/helm/spacebot/`)

The Helm bundle is a values-only wrapper around `bjw-s-labs/app-template`. Drift shows up in three places: image tag vs released version, port/env alignment with `src/config/`, probe paths against API handlers.

- Image tag: `grep -E '^\s*tag:' deploy/helm/spacebot/values.yaml` and compare with `git tag -l`. A tag that doesn't exist in the repo is 🔴 Incorrect.
- Port/env: ports declared in `values.yaml` must match what `src/config/runtime.rs` binds. If docs claim port 19898 for API and 9090 for metrics, verify both sides.
- Probe paths: `grep -E 'livenessProbe|readinessProbe|httpGet|path:' deploy/helm/spacebot/values.yaml` → verify each path resolves to a real handler via `grep -rE '"/health"|"/ready"' src/api/`.
- README: `deploy/helm/spacebot/README.md` should not list values that aren't in `values.yaml`. A diff between the two is a quick way to spot drift.

### Nested CLAUDE.md (subtree-scoped)

Four nested files: `spaceui/CLAUDE.md`, `interface/CLAUDE.md`, `desktop/CLAUDE.md`, `openspec/CLAUDE.md`. Root `CLAUDE.md` is owned by `/session-sync`. These four are owned by this skill.

- **Cross-consistency with root:** shared facts (bun-only, `src/module.rs` convention, `just gate-pr` gate, `[workspace] exclude` guard) must agree across root and nested. If the root says "bun only" and a nested CLAUDE.md says "npm or bun", that's 🔴 Incorrect.
- **Subtree accuracy:** each nested file's claims about its own subtree must be verifiable. Package list in `spaceui/CLAUDE.md` must match `ls spaceui/packages/`. Commands in `interface/CLAUDE.md` must run from `interface/`.
- **Boundary policy:** a nested CLAUDE.md shouldn't duplicate content from a `.claude/rules/*.md` that already covers the same path via `paths:` frontmatter. Flag duplication. The rule file is canonical.
- **Orphan check:** a nested CLAUDE.md under a directory that no longer exists is 🔴 Incorrect (rare, but happens after subtree deletion).

### Coding rules (`.claude/rules/`)

Ten rule files, some path-scoped via `paths:` frontmatter. Rules are loaded automatically when their paths match, so drift here propagates into every code change in the covered subtree.

- **Path frontmatter correctness:** for each rule with `paths:`, verify the globs match real files. `grep -H '^paths:' .claude/rules/*.md` then sanity-check each glob.
- **No conflicting rules:** two rules that both claim authority over the same path but give different guidance is 🔴 Incorrect. The narrower scope should win; the broader rule should link out.
- **No duplication with `RUST_STYLE_GUIDE.md`:** if a rule restates the style guide verbatim, either cite it or trim it. Duplicated content drifts independently.
- **No duplication with nested CLAUDE.md:** see the Nested CLAUDE.md note above — rule files are canonical for their `paths:`.
- **Writing-guide compliance:** rule files are Tier-2 docs but user-visible via agent behavior. The `.claude/rules/writing-guide.md` constraints (no em dashes in prose, etc.) apply to rule prose too.

### OpenSpec canonical specs (`openspec/specs/*/spec.md`)

These describe the **current** state of dependency management, integration surfaces, and security posture. After an OpenSpec change is archived, its `specs/*/spec.md` content merges into the canonical `openspec/specs/*/spec.md`. The spec file can still drift if implementation moves without a formal OpenSpec change. This is a docs-audit-owned slice because no other skill covers it.

- Grep each spec for version strings, package names, file paths, and command examples; verify against actual manifests (`Cargo.toml`, `package.json`) and the tree.
- If a spec references a file path: check the path exists. If a spec names a crate/package at a version: diff against the manifest.
- If drift is found: recommend opening a formal OpenSpec change via `/openspec-propose` rather than inline-editing the spec, unless the drift is purely cosmetic (typo, wording).
- Archived changes (`openspec/changes/archive/*`) are off-limits. Never propose edits there.

### Project skills (`.claude/skills/*/SKILL.md`)

- When a new skill is added or renamed, `session-primer/references/skills-catalog.md` must be updated. This is the highest-velocity drift point in the meta-docs, so check it every audit.
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
# skills-catalog.md uses "### skill-name (NNN lines)" format for entries
diff <(ls .claude/skills/ | grep -v '\.' | sort -u) \
     <(grep -oE '^### [a-z][a-z0-9-]+' .claude/skills/session-primer/references/skills-catalog.md | sed 's/^### //' | sort -u)

# Does a canonical openspec spec still match reality?
grep -oE '`[a-z_-]+`|[0-9]+\.[0-9]+\.[0-9]+' openspec/specs/<name>/spec.md
# Then cross-check each against Cargo.toml / package.json / the tree
```

## Composition With Other Skills

- Run **after** `/deps-update` lands version bumps, to catch doc refs that weren't updated
- Run **before** `/pr-gates` on doc-only branches
- Run **alongside** `/session-sync` for a full documentation + memory refresh
- If findings touch CLAUDE.md → hand off to `/session-sync`
- If findings require new doc files or heavy rewrites → propose via `/openspec-propose` first

## When to Update THIS Skill

This skill is itself Tier 2 documentation and goes stale as the repo evolves. Revisit the scope tables above when any of these happen:

- A new top-level directory is added (check for new docs in it; add to Tier 1 or explicit out-of-scope)
- A new directory appears under `.claude/` (agents, rules, skills, or something new)
- A new package is added to `spaceui/packages/` (update the package row)
- A new agent-platform mirror is added (`.codex/`, `.windsurf/`, `.cursor/`, etc.): add to out-of-scope
- A new route group is added under `docs/content/docs/(...)/`
- The `packages/api-client/` policy changes (currently private, no README). If it gains a README, move to Tier 1.
- A new skill is created or an existing one is renamed: update `session-primer/references/skills-catalog.md` and verify this skill's scope still matches reality
- A new nested `CLAUDE.md` is added under a subtree (not the repo root): add it to the Tier 2 "Nested CLAUDE.md" row
- A new `.claude/rules/*.md` file is added: bump the count in the Tier 2 "Coding rules" row and check whether its `paths:` frontmatter overlaps with an existing rule
