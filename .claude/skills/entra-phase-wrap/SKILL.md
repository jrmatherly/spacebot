---
name: entra-phase-wrap
description: Use this skill to close out an Entra ID rollout phase OR an intra-phase PR continuation. Full phase-wrap (`/entra-phase-wrap N`) runs the post-merge ritual (squash-merge, prune branch, create `phase-{N+1}-resume.md`, mark INDEX ✅). PR-continuation mode (`/entra-phase-wrap N --pr M` where the M+1 PR is next) creates `phase-{N}-pr-{M+1}-resume.md` for a mid-phase handoff, assumes merge + prune already happened, and does NOT touch INDEX.md. Terminal variant for Phase 10 creates `entra-rollout-complete.md` retrospective instead.
---

# Entra Phase Wrap

## When to Use

Two modes:

**Phase-wrap mode (default).** Invoke when a full phase is complete:
1. The current phase's final PR has CI green AND
2. The `claude-review` automated review returned zero findings OR all findings were addressed AND
3. The user explicitly approves the merge AND
4. This is the LAST PR in the phase (the phase plan has no remaining PRs).

Phase-wrap runs destructive git operations (merge, branch delete). User-invocable only; do NOT invoke preemptively.

**PR-continuation mode (`--pr M`).** Invoke when a mid-phase PR landed and the next PR in the same phase needs a primer:
1. PR M of Phase N just merged (squash + branch prune already done) AND
2. The phase plan has more PRs scheduled after M AND
3. Main is up to date locally (post-merge `/session-sync` commit pushed).

PR-continuation mode is purely a file-creation helper. It does NOT run merge, branch prune, Dependabot check, or INDEX ✅ — all of those are phase-boundary concerns.

## Arguments

Phase-wrap mode (default): single phase number. `/entra-phase-wrap 3` wraps Phase 3 and primes Phase 4.

PR-continuation mode: phase number + `--pr M` where M is the PR that JUST merged. The skill writes the primer for the NEXT PR and auto-increments when M is a whole integer. `/entra-phase-wrap 7 --pr 2` primes Phase 7 PR 3 (filename `phase-7-pr-3-resume.md`).

Sub-PR id forms:
- **Integer** (`--pr 2`): auto-increment to 3; filename `phase-{N}-pr-3-resume.md`.
- **Letter** (`--pr B`): caller passes the OUTGOING letter; skill asks the user what the next letter should be (some phases skip letters). Filename `phase-{N}-pr-{next}-resume.md`.
- **Decimal** (`--pr 1.5`): decimals denote mid-PR deliverables (e.g., backend bridge between PR 1 and PR 2). Next id is always the integer ceiling. `/entra-phase-wrap 7 --pr 1.5` writes `phase-7-pr-2-resume.md`.

## Mode selector

If `--pr M` is passed → PR-continuation mode. Skip Steps 1-3, 5 of the phase-wrap sequence below and go directly to the PR-continuation sequence near the end of this file.

Otherwise → phase-wrap mode. Run the full sequence.

## Preconditions (phase-wrap mode)

Before running any git command, verify:

- [ ] Current branch matches `feat/entra-phase-{N}-{short-name}`
- [ ] `gh pr view --json mergeStateStatus` returns `CLEAN`
- [ ] No uncommitted changes (`git status` clean)
- [ ] All second-pass review findings addressed (check PR comments if any were posted)

Stop and ask the user if any precondition fails.

## Preconditions (PR-continuation mode)

Before running any file-write command, verify:

- [ ] The outgoing PR is already merged (check `git log origin/main -5` for the squash commit).
- [ ] Local main is up to date and `git status` is clean.
- [ ] The session-sync drift commit for the outgoing PR has been pushed.
- [ ] The phase plan has the next PR section present (`grep "^## PR " .scratchpad/plans/entraid-auth/phase-{N}-*.md`). If the next PR is the LAST one in the phase, run phase-wrap mode instead after it merges.

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

## PR-continuation sequence (`--pr M` mode)

Replaces the phase-wrap sequence. Five steps.

### Step C1: Resolve the next PR id

Given `N` (phase) and `M` (outgoing PR id, verbatim from user):

- If `M` is a whole integer: next id is `M+1`. Filename: `phase-{N}-pr-{M+1}-resume.md`.
- If `M` is decimal (e.g., `1.5`): next id is `ceil(M)`. Filename: `phase-{N}-pr-{ceil(M)}-resume.md`.
- If `M` is a letter (e.g., `B`): ASK the user what the next letter should be. Some phases skip letters (Phase 6 went A → B → C; a hypothetical Phase 8 could go A → C). Filename: `phase-{N}-pr-{next-letter-lowercase}-resume.md`.

Never auto-name without confirming the next id when `M` is a letter.

### Step C2: Gather primer content inputs

Read these in parallel (Read + Bash in a single turn):

1. **Squash commit SHA** of the outgoing PR: `git log -1 --oneline main` (should match the merged PR's squash).
2. **Last 5 commits on main**: `git log --oneline main -5` (used in the Current state section).
3. **Branch list**: `git branch -a --list 'origin/feat/entra-phase-*'` to list any leftover WIP branches.
4. **Outgoing PR's delivered surfaces**: `gh pr view <outgoing-pr-number> --json title,body,mergeCommit,mergedAt` so the primer's "What shipped" section quotes actual PR text not invented summaries. If the user pre-merged without `gh`, pull from the squash commit's body: `git show --format=%B -s <squash>`.
5. **Next PR's plan section**: `grep -n "^## PR {ceil(M)+?}\|^### Task" .scratchpad/plans/entraid-auth/phase-{N}-*.md` + read the body. That section usually has a task-specific audit table (D-codes) the primer must surface verbatim.
6. **Dependabot open PRs**: `gh pr list --state open --search "author:app/dependabot" --json number,title` for the deferred-list section.

### Step C3: Template the primer

Structural template: the most recent `phase-*-pr-*-resume.md` in `.scratchpad/session-primer/`. Required sections (in order):

- Leading `/session-primer` line (FIRST line, no blank line before it — callers paste this into a fresh session where the harness parses the first line as a skill invocation).
- One-paragraph framing: `"Phase {N} PR {M} ({title}, PR #{num}, squash `{sha}`) landed 2026-MM-DD. PR {next} ({next-title}) is next."` Cite the outgoing PR number so readers can fetch its body.
- `## Current state` — main HEAD SHA + title, last 5 commits, branch to create, working tree status, open Dependabot PRs.
- `## What shipped in PR {M} that PR {next} can assume` — bullets for every substantive artifact. DO NOT invent; quote from the PR body and session memory. Group by module (backend / frontend / schema / tests).
- `## Plan location + entry point` — path + line range for PR {next} section. List amendments applicable (A-XX from INDEX.md) AND D-findings applicable (D-codes from the plan's audit table that survive the outgoing PR's merge).
- `## Resume point` — numbered sequence. Steps typically are: (1) session-sync note, (2) grep-gate of PR {M}'s deliverables on main, (3) create branch `feat/entra-phase-{N}-pr-{next}-*`, (4) read pinned audit table, (5) dispatch `phase-plan-auditor` against the new baseline SHA, (6) begin first task.
- `## Execution rules (carry over from prior phases)` — any rules that evolved in PR {M}. Example: PR 2's ES2020 Error-ctor workaround.
- `## Known Phase {N} PR {next} scope` — brief summary of what the next PR does + its gotchas.
- `## Deferred items (outside PR {next} scope)` — intentional non-goals. Include PRs further in the phase + any S-suggestions deferred from PR {M}.
- `## First concrete step after primer finishes` — shell commands the implementor pastes verbatim. Must produce a go/no-go signal (grep that returns matches or fails loudly).

### Step C4: Em-dash + writing-guide self-check

Before writing the file, scan the draft content for prose em-dashes:

```
grep -nE '[a-z)"0-9]\s*—\s*[a-z]' <draft-buffer>
```

Replace any hits with colons, parentheses, or periods. Em-dashes are allowed ONLY in:
- Bullet labels: `- **Foo** — one-line definition`
- Table cells: `| Foo | bar — baz |`
- Section headers: `## Title — subtitle`

### Step C5: Write + verify + report

1. Write the primer to `.scratchpad/session-primer/phase-{N}-pr-{next}-resume.md`.
2. Verify gitignore status: `git check-ignore .scratchpad/session-primer/phase-{N}-pr-{next}-resume.md` — must return the path (gitignored) and not fail.
3. `git status --short` must NOT show the new file.
4. Report:
   - File path
   - Line count (target: 100-200 lines, similar to prior PR primers)
   - Outgoing squash SHA
   - Next branch name the primer prescribes
   - Paste-into-fresh-session instruction

### Red flags in PR-continuation mode

- The outgoing PR is still open (not merged). Stop and ask the user to merge first.
- Main is ahead of origin/main (unpushed commits). Stop and ask.
- The phase plan has no section for the next PR id. Stop and ask if the user meant phase-wrap mode.
- `git check-ignore` fails on the new file. Stop and investigate `.gitignore`.
