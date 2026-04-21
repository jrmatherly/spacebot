---
name: entra-phase-wrap
description: Use this skill to close out an Entra ID rollout phase. Invokes the full post-merge ritual: squash-merge the PR, prune the remote branch, fetch + prune locally, create the `.scratchpad/session-primer/phase-{N+1}-resume.md` handoff file from the current-phase template, and update `.scratchpad/plans/entraid-auth/INDEX.md` to mark the phase ✅ with the merged PR number. Terminal variant for Phase 10 creates `entra-rollout-complete.md` retrospective instead.
---

# Entra Phase Wrap

## When to Use

Invoke ONLY when:
1. The current phase's PR has CI green AND
2. The `claude-review` automated review returned zero findings OR all findings were addressed AND
3. The user explicitly approves the merge.

This skill runs destructive git operations (merge, branch delete). It is user-invocable only; do NOT invoke preemptively.

## Arguments

Accepts a single phase number as argument: `/entra-phase-wrap 3` wraps Phase 3 and primes Phase 4.

## Preconditions

Before running any git command, verify:

- [ ] Current branch matches `feat/entra-phase-{N}-{short-name}`
- [ ] `gh pr view --json mergeStateStatus` returns `CLEAN`
- [ ] No uncommitted changes (`git status` clean)
- [ ] All second-pass review findings addressed (check PR comments if any were posted)

Stop and ask the user if any precondition fails.

## Sequence (in order)

### Step 1: Merge

```bash
gh pr merge --squash --delete-branch
```

The `--delete-branch` flag removes the remote feature branch. The local branch deletion is handled automatically when you switch to main (next step).

### Step 2: Sync local main + prune stale refs

```bash
git checkout main
git pull origin main
git fetch --prune
```

`git fetch --prune` removes the stale `origin/feat/entra-phase-{N}-*` ref that the merge left behind.

### Step 3: Verify Dependabot auto-closes (optional)

If the merged PR folded in Dependabot updates (Phase 2's PR #101 folded 11), confirm those Dependabot PRs auto-closed:

```bash
gh pr list --state open --search "author:app/dependabot" --json number,title,mergeStateStatus --jq '.[] | [.number, .mergeStateStatus, .title] | @tsv'
```

Folded PRs should no longer appear. Separate PRs (major bumps, CI action bumps with zizmor failures) will remain open and are expected.

### Step 4: Create the resume file for the next phase

**Terminal variant (Phase 10):** skip this step; create `entra-rollout-complete.md` per Step 5 below.

**Normal variant (Phases 3-9):** create `.scratchpad/session-primer/phase-{N+1}-resume.md`.

Structural template: the most recent resume file already in `.scratchpad/session-primer/`. Required sections (use the phase-3-resume.md file as reference for section shape):

- Leading `/session-primer` line (first line of the file)
- One-paragraph framing: "Phase {N} landed 2026-MM-DD as PR #<num> (squash commit `<sha>`). Phase {N+1} ({next-phase-title}) is next."
- `## Current state` — main commit, push state, disk/target cache status, branch status
- `## What shipped in Phase {N} (reference)` — bullets for every notable artifact: new modules, migrations, metrics, tests, docs, dependency bumps
- `## Plan location + entry point` — path to `phase-{N+1}-{name}.md` + applicable amendments from INDEX.md
- `## Resume point` — create branch `feat/entra-phase-{N+1}-{short-name}`, read next-phase plan + amendments, invoke `/superpowers:executing-plans`
- `## Execution rules (carry over from Phase 0-{N})` — any rules that evolved during this phase
- `## Known Phase {N+1} context` — brief summary of what the next phase actually does + its gotchas
- `## Deferred from Phase {N} (candidates for later PR)` — intentional non-goals
- `## First concrete step after primer finishes` — concrete shell commands

**Data sources when filling the template:**
- Squash commit SHA: `git log -1 --oneline main` (or the PR merge commit)
- What shipped: read the phase plan's `## Phase {N} acceptance criteria` section + skim the commit log
- Amendments: `grep "^| A-" .scratchpad/plans/entraid-auth/INDEX.md` and pick rows whose `Phase(s)` column includes `{N+1}`
- Deferred items: walk back through the PR comments and any "Deferred to follow-up" entries

### Step 5: Update INDEX.md

Mark the phase row with ✅ in the Phase-sequence table:

```bash
# Example for Phase 3:
sed -i '' 's|^| 3 | Graph API client + group resolution | `phase-3-graph-client.md` | Phases 1 + 2 | 1 PR |$|\
| 3 | Graph API client + group resolution ✅ (PR #<num>) | `phase-3-graph-client.md` | Phases 1 + 2 | 1 PR |\
|' .scratchpad/plans/entraid-auth/INDEX.md
```

(Prefer a manual Edit — sed's escaping is fragile for markdown tables. Use the pattern above as the intent, not the verbatim command.)

### Step 6: Verify the handoff file is gitignored and does NOT end up in a commit

```bash
git check-ignore .scratchpad/session-primer/phase-{N+1}-resume.md
git status --short
```

The file should be reported as gitignored. `git status --short` should NOT show the resume file. If it does, something upstream changed `.gitignore`; stop and investigate.

### Step 7: Final status + report

```bash
git log --oneline -5
gh pr list --state open --search "author:app/dependabot" | wc -l  # remaining Dependabot count
```

Report:
- Squash commit SHA on main
- Dependabot PR count (with delta from pre-merge)
- Resume file location
- Next step: paste `.scratchpad/session-primer/phase-{N+1}-resume.md` into a fresh session

## Phase 10 terminal variant

Phase 10 is the final phase. Instead of a resume file, create `.scratchpad/session-primer/entra-rollout-complete.md` — a retrospective, not a continuation prompt. Required sections:

- `# Entra ID Rollout Complete — 2026-MM-DD` (NOT `/session-primer`)
- `## Shipping summary` (phase-by-phase with PR numbers)
- `## What Spacebot can now do` (user-visible capabilities)
- `## Architecture invariants locked in`
- `## Operator runbook index`
- `## Amendments applied` (A-01 through A-22)
- `## Known deferred items across the whole rollout`
- `## Follow-up tracks`
- `## Metrics and telemetry surfaces added`
- `## Historical reference`

See Phase 10's Task 10.12 section for the full required-section list.

## Red flags — stop and ask

- `mergeStateStatus` is not CLEAN
- Second-pass review findings still open
- Local branch has uncommitted work
- A `claude-review` comment requested changes you haven't addressed
- The handoff file shows up in `git status` (gitignore broken)

## What this skill does NOT do

- Does not run `just gate-pr` — that's the responsibility of the phase's final task before reaching wrap
- Does not bump Cargo.toml version (release bumps are a separate `/release-bump-changelog` skill)
- Does not update `CHANGELOG.md` — the final phase task should have already added the entry
- Does not push `.scratchpad/` files — they are gitignored by design
