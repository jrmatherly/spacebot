---
name: pr-remediation-batch
description: Use this skill when a PR has received review findings (typically from `/pr-review-toolkit:review-pr`, manual code review, or `/spacebot-dev`) that need to be addressed before merge. Takes an aggregated findings document (e.g., `.scratchpad/YYYY-MM-DD-prN-review-aggregated.md`) and drives a multi-commit remediation loop with narrow verification gates between commits. Enforces commit-message discipline (review-item IDs cited in every commit message), commit grouping (fix-class + test-class + doc-class commits stay separate), and prevents the common failure mode of lumping unrelated fixes into one "address review" commit. Canonical reference for the batch pattern: Phase 4 PR 2's 27-commit remediation at `feat/entra-phase-4-pr-2-handler-rollout`.
disable-model-invocation: true
---

# PR Remediation Batch

## When to Use

Invoke when:

- A PR is open and review findings are aggregated in a scratchpad document (C1/C2... / I1/I2... / M1... style bucket structure)
- The findings span multiple concern types (fix + test + docs): each type becomes its own commit
- You want mechanical commit grouping + verification discipline rather than one big "address feedback" dump
- Following the Phase 4 PR 2 pattern where each review-item ID (C1, I4, T1, W1, etc.) maps to a named remediation

Do NOT invoke for:

- One-off single-line fixes (just edit and commit)
- Review findings that require an architectural conversation before remediation (resolve the conversation first; then invoke)
- Initial PR creation (this is for post-review, pre-merge)

## The pattern

Every PR remediation looks like:

```
fix(<scope>): <group-1 summary> (PR #N review <item-IDs>)
test(<scope>): <group-2 summary> (PR #N review <item-IDs>)
docs(<scope>): <group-3 summary> (PR #N review <item-IDs>)
```

Three commits is typical. Sometimes more (fix-critical + fix-important separate; tests batched together; docs batched together). Sometimes fewer (docs-only PR has no fix/test commits).

**Core invariant:** every commit message names the review-item IDs it addresses. A reviewer doing a second pass can map commits to findings without re-reading the aggregated doc.

## Canonical reference

Phase 4 PR 2 remediation (PR #105, branch `feat/entra-phase-4-pr-2-handler-rollout`). Relevant commits:

- `4696ca9` — "refactor(auth): address T4.5 code-quality review findings (I1/I2/I3)"
- `6271428` — "refactor(auth): address T4.6+T4.6b code-quality review findings (I1, M2)"
- `07d7f75` — "refactor(auth): address T4.11 portal review findings (Critical + Important)"
- `b77cf7d` — "refactor(auth): address T4.12 code-quality review findings (C1 + I1 + I2)"
- `a6475ca` — "test(auth): rename cron system-bypass test for semantic accuracy (T4.10 follow-up)"

Each cites the review-item IDs it closes. The pattern is what this skill enforces.

## Sequence

### Step 1: Load the aggregated findings doc

From the user or the invocation context:

- **Findings doc path** — typically `.scratchpad/YYYY-MM-DD-pr<N>-review-aggregated.md`
- **PR branch** — usually the current branch if it has an open PR
- **Base SHA** — last commit of the reviewed state (findings were produced against this)

Read the doc. Identify the Critical / Important / Minor bucket tables. Cross-reference each finding ID to:

1. File(s) the finding affects (from the citation column)
2. Concern type: fix (code change that alters behavior), test (new or modified test), docs (comment/doc update)

### Step 2: Group findings into commits

For each concern type, one commit. Within a commit, group by subsystem when findings span multiple files.

**Default grouping rule:**

- **Commit 1 (fix):** all Critical + Important findings in the "silent-failure" / "code-review" / "type-design" categories that change runtime behavior
- **Commit 2 (test):** all Important findings from test-coverage review + any new regression guards for the Commit 1 fixes
- **Commit 3 (docs):** all comment-accuracy findings (rot items, em-dash sweeps, stale claims) + any module-doc updates

**When to split a commit:**

- A single fix is load-bearing enough to stand alone (e.g., a Critical security fix). It gets its own commit even if other Important fixes also change behavior.
- A fix touches a heavy subsystem (auth, migrations): isolates the blast radius in git history.

**When to merge commits:**

- A test commit's only tests are regressions for fixes that haven't landed yet. In that case, bundle the test into the fix commit (test-after-fix, single commit).

### Step 3: Implement per-commit with narrow verification between

For each commit, in order:

1. Apply the fixes for the findings this commit covers.
2. Run the narrow verification for the concern type:
   - **fix commits:** `cargo check --lib` → `cargo nextest run --test <affected-test-file>` (the file whose coverage would break if the fix is wrong)
   - **test commits:** `cargo nextest run --test <new-test-file>` (asserting green)
   - **docs commits:** `cargo fmt --all -- --check` + `grep -nE '[a-z)"0-9]\s*—\s*[a-z]' <edited-files>` (for writing-guide compliance); also `just check-typegen` if a doc edit touched utoipa annotations
3. Stage ONLY the files for this commit. Use explicit `git add <paths>`; never `git add -A` here, since parallel subagents' drift or prior-commit leftovers can sneak into the commit otherwise.
4. Write the commit message with review-item IDs cited.

**Do NOT run `just gate-pr` between commits.** Reserve it for pre-push per INDEX § Cargo discipline. Per-commit invocation is 3-5× wasted work.

### Step 4: Commit message template

Use this shape for every remediation commit:

```
<type>(<scope>): <one-line summary> (<item-IDs>)

<body paragraph 1: name each item ID and the one-line fix>

<body paragraph 2 (optional): cross-reference any deferred items this
does NOT address, so reviewers know what's intentionally left for
Phase N+1>

Verification: <commands run, pass counts>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

**Examples from PR 2:**

- `fix(auth): close list_tasks info-disclosure + add delete regression test (T4.8 remediation)`
- `refactor(auth): address T4.5 code-quality review findings (I1/I2/I3)`
- `test(auth): add admin-reading-admin regression guard (Phase 4 PR 2 T4.policy-tests, G4)`

### Step 5: Push + verify PR update

After all commits land:

```bash
# Full gate ONCE before push
just gate-pr 2>&1 | tail -15

# Push
git push origin <branch>

# Verify PR picked up the commits
gh pr view <N> --json commits --jq '.commits | length'
```

If `just gate-pr` fails at this point, the failure is isolated to the remediation batch (no commits polluted by unrelated drift, because Step 3 used explicit `git add <paths>`). Read the failure, address it with one more commit, push again.

### Step 6: Update the findings doc

Edit the aggregated findings doc to mark addressed items as ✅. If a finding is being deferred (e.g., "defer to Phase 5"), change its status to "Deferred" with a link to the phase plan that picks it up. The doc becomes the audit trail for "what did we fix in the remediation cycle, and why did we defer the rest."

## What NOT to do

- Do NOT lump unrelated fixes into one "address review feedback" commit. Reviewers can't map changes to findings.
- Do NOT skip the verification step between commits. A fix-commit that compiles but breaks a test compounds into Commit 2 and wastes debugging time.
- Do NOT `git add -A`. Parallel session drift, prior-session test artifacts, and formatter-hook residue pollute the commit.
- Do NOT run `just gate-pr` between commits. Reserve for pre-push.
- Do NOT address deferred findings as part of this batch. "Defer to Phase 5" means Phase 5's branch; don't sneak it in here.
- Do NOT rewrite the PR body. Review findings are addressed through commits; the PR description documents the original intent, not the remediation cycle.

## Relationship to other automations

- `/pr-review-toolkit:review-pr` — produces the findings this skill consumes.
- `authz-gate-conformance` / `integration-test-coverage-auditor` subagents: structural audits that produce their own finding sets; those findings plug into this skill's batch loop.
- `pr-gates` skill: the pre-push gate this skill's Step 5 invokes.
- `session-sync` skill: runs at the END of a session; this skill runs at the END of a REVIEW CYCLE. They compose.
- `writing-guide-scan` skill: invoke as part of Step 3 verification when a docs commit is in-flight, to catch em-dash violations before the commit.

## Relationship to the 2026-04-22 streamlining audit

The 2026-04-22 audit surfaced R1 (nextest default, now flipped), R5 (parallel-safe dispatch), R6 (fill-verification-gaps). This skill's Step 3 verification uses nextest by default (R1 compliant), Step 3 "group findings" honors the parallel-safe boundaries for disjoint-file fixes (R5), and Step 3's narrow-verification-between-commits is the R6 discipline applied to review-remediation cycles instead of feature-development cycles.
