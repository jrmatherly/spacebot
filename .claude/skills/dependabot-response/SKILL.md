---
name: dependabot-response
description: Audit open Dependabot PRs, replace them with direct-to-main SHA-pinned commits carrying audited intent, and add `ignore:` rules that prevent policy-violating majors from being re-proposed. Codifies the pattern established 2026-04-21 responding to PRs #83-#88. Use when the user says "dependabot has identified", "these dependabot PRs", "review these updates", or lists bumps + asks for validation. Starts with the dependabot-triager subagent for classification.
disable-model-invocation: true
---

# /dependabot-response

End-to-end pattern for responding to a batch of open Dependabot PRs without letting them auto-merge and without leaving the review burden on a human. Output: a set of direct-to-main commits that (a) apply the audited version bumps in the project's SHA-pin style, (b) add guard rules where the bump violates policy, (c) cause Dependabot to auto-close its own PRs on the next scan.

The pattern is load-bearing. Reproducing it from scratch each time loses 45-60 minutes and often forgets one of the three guards. Use this skill.

## When to invoke

Triggers:
- User lists open Dependabot PRs and asks for review/validation/analysis.
- User pastes a `dependabot[bot]` PR title list and says "proceed".
- A `gh pr list --state open --author "app/dependabot"` returns more than two results and the user wants to drain the queue.

Not for:
- A single non-major patch bump — just `gh pr merge --squash` it directly.
- Dependabot security advisories (different path: check `docs/security/deferred-advisories.md` first).

## Phase 0 — classify the queue

Run the `dependabot-triager` subagent first. It produces a verdict table (SAFE-FOLD / DEFER / SKIP) matching the pattern established by PR #101. Do not skip this — the subagent has context this skill intentionally doesn't replicate (build state, mergeStateStatus, bump semver distance).

```
Dispatch Agent(
  subagent_type="dependabot-triager",
  description="Triage open Dependabot queue",
  prompt="Triage all open Dependabot PRs on jrmatherly/spacebot. Classify each as SAFE-FOLD / DEFER / SKIP using the Phase 2 PR #101 pattern. Return a verdict table."
)
```

Wait for its output. Every downstream phase reads from it.

## Phase 1 — gather ground truth before acting

For each PR in the triager's SAFE-FOLD list:

1. Read the PR body and release notes via `gh pr view <N>`.
2. Identify the bump type:
   - **GitHub Action bump** (`.github/workflows/*.yml`) — treat separately per Phase 2.
   - **Cargo/npm dependency bump** — fold per Phase 3.
   - **@types/node or similar policy-sensitive dep** — check project policy first, may need Phase 4 ignore rule.

3. For each action bump, resolve the target SHA:
   ```bash
   gh api repos/actions/checkout/git/ref/tags/v6.0.2 --jq '.object.sha'
   ```
   This project's CI already SHA-pins actions. Replacing tag-form bumps (`@v6`) with SHA-pinned ones is the right form — do not land tag-form updates.

4. For each dependency bump, check whether the project has a pinned version elsewhere (Cargo.toml comments, CLAUDE.md policy statements). If a policy-violation is possible (e.g., `@types/node` major going past the runtime Node version), flag it for Phase 4.

## Phase 2 — CI action bumps

Replace Dependabot's tag-form bumps with SHA-pinned form in a single commit per action family.

**Locate unpinned refs across all workflows:**

```bash
grep -rnE "uses: (actions/checkout|docker/login-action|actions/setup-node)@v[0-9]+$" .github/workflows/
```

**Replace with SHA-pin + trailing comment** (match existing repo style — see `.github/workflows/ci.yml` for the format):

```bash
sed -i '' 's|actions/checkout@v4$|actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2|' \
  .github/workflows/<file>.yml
```

**Verify diff scope.** Commit should touch only `uses:` lines. No `with:` param changes, no `run:` block modifications, no concurrency drift.

**Runner-compatibility check.** `docker/login-action@v4` requires GitHub Actions Runner ≥ v2.327.1; `actions/checkout@v6` requires ≥ v2.329.0. GitHub-hosted runners are fine. Self-hosted runners need verification — check `runs-on:` values.

**Commit message shape:**

```
ci: SHA-pin action versions (checkout v6.0.2, docker/login v4.1.0, setup-node v6.4.0)

Brings the remaining unpinned workflow refs into line with the SHA-pin
discipline the rest of the repo already follows (see ci.yml, codeql.yml,
release.yml, zizmor.yml). Replaces Dependabot PRs #83, #84, #86 with a
single direct-to-main update; Dependabot will auto-close on its next scan.

Changes: [bullet list of each ref and its new SHA + version]
```

## Phase 3 — dependency bumps that need a local policy check

For each `/**/package.json` or `Cargo.toml` bump:

1. **Find all declaration sites** (the bump may target one file but the dep may be used across multiple manifests):
   ```bash
   grep -nH "\"@types/node\"" **/package.json
   grep -nH "^fastembed" Cargo.toml
   ```

2. **Check for stated policy**. Example from this project's CLAUDE.md / `project_overview.md` Serena memory:
   - Node 24 is the runtime target (`.github/workflows/release.yml:153`). `@types/node` should stay on `^24.x`.
   - `fastembed` is exact-pinned at `5.13.2` due to hf-hub 0.5 incompatibility.
   - Serenity is rev-pinned to `next` branch at `1cbceb275b10566145b0bdca1c57da9502079a6a`.

3. **If the bump violates policy**, go to Phase 4. Otherwise, apply the bump and run the lockfile regeneration:
   ```bash
   # TypeScript (bun workspaces)
   cd spaceui && bun install
   cd docs && bun install

   # Rust
   cargo update -p <crate>
   ```

4. **Verify with the project's gate:**
   ```bash
   cd /Users/jason/dev/spacebot
   just gate-pr-fast   # quick sanity
   ```

5. **Commit per logical group.** Don't bundle TypeScript + Rust bumps in one commit. Separate concerns = easier bisect.

## Phase 4 — add `ignore:` rules for policy violations

If a Dependabot bump violates a stated project policy (e.g., `@types/node@25` when policy is Node 24), reverting to the policy value is only half the fix — next Tuesday's scan will re-propose the same bump. Add a `dependabot.yml` ignore rule in the same commit.

**Identify the correct ecosystem entry.** This is where `@types/node` PRs tripped up the session this skill codifies: bun workspace hoisting causes a dep declared in `spaceui/.storybook/package.json` to resolve in `spaceui/bun.lock` — so Dependabot opens PRs under the `/spaceui` ecosystem root even though the file it modifies is under `/spaceui/.storybook`. Add the ignore rule to BOTH entries.

**Rule shape:**

```yaml
  - package-ecosystem: "npm"
    directory: "/spaceui"
    schedule:
      interval: "weekly"
    # existing config...
    # The root spaceui/bun.lock hoists @types/node from the workspace member
    # that declares it (.storybook). Dependabot scans the hoisted dep under
    # this root ecosystem. Duplicate the policy pin here so the hoisted
    # resolution doesn't bypass the .storybook block.
    ignore:
      - dependency-name: "@types/node"
        update-types: ["version-update:semver-major"]
```

The `update-types` form lets patch/minor bumps through — critical for security patches. Do NOT use `versions: [">=25"]` which also blocks security fixes.

## Phase 5 — push and verify auto-close

1. Push the direct-to-main commits.
2. Wait for Dependabot's next scan (weekly per `.github/dependabot.yml`, or trigger manually via "Check for updates" in the GitHub UI → Dependabot tab).
3. Confirm auto-close: `gh pr list --state open --author "app/dependabot"` should show the targeted PRs missing.
4. If a PR stays open after the scan, Dependabot may not have recognized the SHA as equivalent to the tag it proposed. Close manually:
   ```bash
   gh pr close <N> --comment "Closed — bump landed in <commit-sha> with SHA-pinned form. See commit for rationale."
   ```

## Pattern calibration (from 2026-04-21 session)

The original invocation of this pattern:
- Input: 5 Dependabot PRs (#83, #84, #86, #87, #88).
- Output: 3 direct-to-main commits (CI actions, `@types/node` downgrade, `baseUrl` tsconfig fix) + 2 follow-up commits (ignore rules, additional ignore rules after hoist-surprise).
- Time: ~45 minutes including web research (TypeScript 6.0 migration guide for the `baseUrl` deprecation that surfaced via the `@types/node` bump).
- Dependabot behavior: 4 of 5 PRs auto-closed on next scan. PR #87 re-appeared with a new title once rebased against the downgrade; required the additional `/spaceui` ecosystem ignore rule to fully suppress.

Two lessons baked into the steps above:
1. **Always check the PR's `files:` list, not just its title.** `@types/node bump in /spaceui` actually modifies `spaceui/.storybook/package.json` — surprise.
2. **Ignore rules do not retroactively close open PRs.** If an ignore rule is added after a PR opens, that PR stays open until manual close or Dependabot's next full rescan reconciles against the new policy.

## Related skills and agents

- `dependabot-triager` subagent — Phase 0 prerequisite.
- `session-sync` skill — run after this skill completes, before next context switch, to update memories with any policy changes.
- `deps-update` skill — use when YOU are initiating a dep bump, not responding to one.

## Honesty rules

- Never merge a Dependabot PR without reading the release notes.
- Never apply a tag-form action bump when the repo uses SHA pins elsewhere.
- Never add an `ignore` rule broader than `update-types: version-update:semver-major` — broader rules suppress security fixes.
- Never promise "Dependabot will auto-close" without noting the known failure case (hoist-surprise, rebased-title, ignore-added-post-open).
