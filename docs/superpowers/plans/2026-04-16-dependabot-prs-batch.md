# Dependabot PR Batch — Implementation Plan (revised 2026-04-16)

> **✅ COMPLETED 2026-04-18.** All five workspace-grouped PRs landed on main. Landing SHAs: `fbb8fa3` (Storybook 10 + React 19 coordinated bump), `0b12ba2` (react-markdown 10, graphology 0.26, react-spring 10, vite 8, plugin-react 6), `402c4c0` (spaceui-showcase april 2026 batch), `d88e3e3` (docs fumadocs-ui, fumadocs-mdx, next), plus `7b9f054` (workspace alignment drift fixes) and `04f1bb7` (TypeScript 5.4 → 6.0.2 follow-up). The original Dependabot PRs (#20-33) auto-closed as expected. This plan file is preserved as the historical record of the execution; see `git log --oneline --grep="^deps(spaceui\\|^deps(docs\\|^deps(spaceui-showcase"` for the full commit sequence. No further action needed.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the dependency upgrades from all 14 open Dependabot PRs (#20–#33) by writing them locally as 5 workspace-grouped feature branches, opening one PR per branch, and letting Dependabot's PRs auto-close when their bumped versions land in `main`.

**Architecture:** Five sequential PRs grouped by workspace. The Storybook 10 / React 19 / Vite 8 peer-dependency chain dictates sub-ordering inside the spaceui workspaces — Storybook 10 must land before React 19 / Vite 8 because `@storybook/react-vite@8.6.18` only supports React `^19.0.0-beta` and Vite `^4||^5||^6`. Each PR is built and smoke-tested locally with captured logs before opening.

**Tech Stack:** bun (frontend package manager), Storybook 10, Vite 8, React 19, react-spring 10, react-markdown 10, graphology 0.26, fumadocs, Next.js, gh CLI.

**Source spec:** `.scratchpad/dependabot-prs-2026-04-16.md` (revised 2026-04-16 after self-audit).

**Branch protection (verified via API):** `Protect main` ruleset is `ACTIVE`. Direct push to `main` is blocked. Pull request is required (any merge method allowed: merge, squash, rebase). 0 required approving reviews. Current user can bypass.

**Smoke-test protocol:** For every Storybook touchpoint, the implementer runs `bun run storybook` in the background with stdout/stderr piped to `/tmp/storybook-PHASE.log`, waits for `Storybook started` in the log, then asks the user to manually verify the listed stories. After the user confirms, the implementer captures the final log tail, kills the dev server, and validates that no errors appeared.

**Auto-close behavior:** When a local PR merges to `main`, Dependabot's PR for the same dep version (or older) will auto-close because the bumped version already exists in the manifest. We do not need to manually close Dependabot PRs.

---

## PR-to-Workspace Mapping (verified via `gh pr list`)

| Local PR | Workspace | Dependabot PRs replaced | Notes |
|----------|-----------|--------------------------|-------|
| **PR A** | `/docs` | #29, #31, #33 | All patch/minor bumps. No Storybook/React coupling. |
| **PR B** | `/spaceui/examples/showcase` | #21, #22, #25 | Showcase doesn't use Storybook. zod/hookform/plugin-react. |
| **PR C** | `/spaceui` + `/spaceui/.storybook` | #20, #26 (replaced) | Storybook 8 → 10 coordinated bump. **Must land before PR D and PR E.** |
| **PR D** | `/spaceui` + `/spaceui/.storybook` + `/spaceui/examples/showcase` | #23, #24 | React 18 → 19 across all spaceui manifests. **Must land after PR C.** |
| **PR E** | `/spaceui/packages/*` | #27, #28, #30, #32 | react-markdown 10, graphology 0.26, react-spring 10, vite 8. **Must land after PR C and PR D.** |

---

## File Structure

| Path | Owned by PR | Action |
|------|-------------|--------|
| `docs/package.json` | PR A | 3 version bumps |
| `spaceui/examples/showcase/package.json` | PR B and PR D | PR B: 3 deps; PR D: react/react-dom + @types/react align |
| `spaceui/package.json` | PR C and PR D | PR C: storybook devDeps; PR D: (only if it has react fields — currently only `overrides` block) |
| `spaceui/.storybook/package.json` | PR C and PR D | PR C: storybook devDeps + chromatic; PR D: react/react-dom |
| `spaceui/.storybook/main.ts` | PR C | Drop unpublished addons from `addons[]` |
| `spaceui/packages/ai/package.json` | PR E | react-markdown ^10, graphology ^0.26 |
| `spaceui/packages/primitives/package.json` | PR E | @react-spring/web ^10 |
| `.github/dependabot.yml` | PR C (optional) | Add `groups:` for storybook |

---

## Pre-Flight (do once, before PR A)

### Task 0: Confirm baseline state

**Files:**
- Read: `.scratchpad/dependabot-prs-2026-04-16.md`
- Verify: branch `main` is clean and up-to-date

- [ ] **Step 1: Verify clean working tree on main**

Run:
```bash
git checkout main && git pull --ff-only && git status
```

Expected:
```
On branch main
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean
```

- [ ] **Step 2: Confirm `bun` and `gh` are available**

Run:
```bash
bun --version && gh --version | head -1
```

Expected: bun 1.x and gh 2.x.

- [ ] **Step 3: Verify the 14 target Dependabot PRs are still open and confirm their head shas (for traceability when they auto-close)**

Run:
```bash
gh pr list --state open --json number,title,headRefName,headRefOid --limit 100 \
  | jq -r '.[] | select(.number >= 20 and .number <= 33) | "#\(.number) \(.headRefOid[0:7]) \(.title)"' \
  | sort
```

Expected: 14 lines. Save the output to `/tmp/dependabot-baseline.txt` for the post-merge audit:
```bash
gh pr list --state open --json number,title,headRefName,headRefOid --limit 100 \
  | jq -r '.[] | select(.number >= 20 and .number <= 33) | "#\(.number) \(.headRefOid[0:7]) \(.title)"' \
  | sort > /tmp/dependabot-baseline.txt
```

---

## PR A — `/docs` workspace (replaces #29, #31, #33)

Three patch/minor bumps to docs. No Storybook coupling. No `Interface Quality` CI for `/docs/**`. Safe to land first.

### Task A.1: Branch off and edit `docs/package.json`

**Files:**
- Modify: `docs/package.json:14-16`

- [ ] **Step 1: Create branch**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/docs-april-2026
```

- [ ] **Step 2: Apply three version bumps**

Edit `docs/package.json` `dependencies` block.

Before:
```json
    "fumadocs-core": "16.7.16",
    "fumadocs-mdx": "14.2.14",
    "fumadocs-ui": "16.7.15",
    "lucide-react": "^1.8.0",
    "next": "16.2.3",
```

After:
```json
    "fumadocs-core": "16.7.16",
    "fumadocs-mdx": "14.3.0",
    "fumadocs-ui": "16.7.16",
    "lucide-react": "^1.8.0",
    "next": "16.2.4",
```

(`fumadocs-core` is already at 16.7.16; #29 only bumps `fumadocs-ui` to match.)

### Task A.2: Install + smoke-test docs

**Files:** none (validation only)

- [ ] **Step 1: Install dependencies**

Run:
```bash
cd docs && bun install 2>&1 | tee /tmp/docs-install.log
```

Expected: install completes. Inspect `/tmp/docs-install.log` — no `ERESOLVE`, no `peer` warnings about fumadocs/next versions.

- [ ] **Step 2: Run typecheck**

Run:
```bash
bun run types:check 2>&1 | tee /tmp/docs-typecheck.log
```

Expected: exit 0. Inspect `/tmp/docs-typecheck.log` — no TypeScript errors.

- [ ] **Step 3: Run production build**

Run:
```bash
bun run build 2>&1 | tee /tmp/docs-build.log
```

Expected: build succeeds. Look for `✓ Compiled successfully` in `/tmp/docs-build.log`.

- [ ] **Step 4: Boot dev server in background and ask user to verify**

Run:
```bash
PORT=19830 bun run dev > /tmp/docs-dev.log 2>&1 &
echo $! > /tmp/docs-dev.pid
# Wait for "Ready" or "Local:" in the log (max 30 s)
for i in $(seq 1 30); do
  if grep -qE "Ready|Local:" /tmp/docs-dev.log; then break; fi
  sleep 1
done
tail -20 /tmp/docs-dev.log
cd ..
```

Then **ask the user**:

> Docs dev server is running on http://localhost:19830. Please open it in a browser and verify:
> 1. Home page renders
> 2. Click into a docs page (e.g., `/docs/architecture`) and confirm content loads
> 3. Open the search bar (top right) and type a few characters — confirm results appear
> 4. Reply `pass` or `fail: <reason>`

- [ ] **Step 5: After user confirms, capture logs and stop dev server**

Run:
```bash
tail -100 /tmp/docs-dev.log > /tmp/docs-dev-final.log
kill $(cat /tmp/docs-dev.pid) 2>/dev/null
rm -f /tmp/docs-dev.pid
grep -iE "error|warn" /tmp/docs-dev-final.log | head -20
```

Expected: no `Error:` lines. Warnings about font preload or hydration are tolerated unless they're new in this branch (compare against main if uncertain).

### Task A.3: Commit, push, open PR

**Files:** local git, remote PR

- [ ] **Step 1: Stage and commit**

Run:
```bash
git add docs/package.json
git commit -m "$(cat <<'EOF'
deps(docs): april 2026 batch — fumadocs-ui, fumadocs-mdx, next

- fumadocs-ui 16.7.15 → 16.7.16 (Dependabot #29)
- fumadocs-mdx 14.2.14 → 14.3.0 (Dependabot #31)
- next 16.2.3 → 16.2.4 (Dependabot #33)

All patch/minor bumps. Verified locally: bun install, types:check,
production build, dev server boot.
EOF
)"
```

- [ ] **Step 2: Push and open PR**

Run:
```bash
git push -u origin deps/docs-april-2026
gh pr create --title "deps(docs): april 2026 batch — fumadocs + next" --body "$(cat <<'EOF'
## Summary

Workspace-grouped local replacement for Dependabot PRs #29, #31, #33.

- `fumadocs-ui` 16.7.15 → 16.7.16
- `fumadocs-mdx` 14.2.14 → 14.3.0
- `next` 16.2.3 → 16.2.4

## Test plan

- [x] `cd docs && bun install` clean (no peer warnings)
- [x] `bun run types:check` exits 0
- [x] `bun run build` succeeds
- [x] Dev server boots; home, docs page, and search verified manually

Dependabot PRs #29, #31, #33 will auto-close once this merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Wait for CI green, then merge**

Run:
```bash
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

- [ ] **Step 4: Verify Dependabot PRs auto-closed**

Run:
```bash
sleep 30  # give dependabot time to detect the new main
gh pr view 29 --json state -q .state
gh pr view 31 --json state -q .state
gh pr view 33 --json state -q .state
```

Expected: all three return `CLOSED`. If any still says `OPEN`, comment `@dependabot rebase` on it and verify it auto-closes after the rebase compares against the new main.

---

## PR B — `/spaceui/examples/showcase` workspace (replaces #21, #22, #25)

Three bumps in showcase. None of these deps are actually imported in `spaceui/examples/showcase/src/` (verified — src has only App.tsx, main.tsx, index.css, none import zod/hookform/plugin-react). Manifest-only alignment with the rest of the workspace.

### Task B.1: Branch off and edit showcase `package.json`

**Files:**
- Modify: `spaceui/examples/showcase/package.json:14-30`

- [ ] **Step 1: Create branch**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/spaceui-showcase-april-2026
```

- [ ] **Step 2: Apply three version bumps**

Edit `spaceui/examples/showcase/package.json`.

Before (`dependencies`):
```json
    "react-hook-form": "^7.51.0",
    "zod": "^3.22.0",
    "@hookform/resolvers": "^3.3.0"
```

After:
```json
    "react-hook-form": "^7.51.0",
    "zod": "^4.3.6",
    "@hookform/resolvers": "^5.2.2"
```

Before (`devDependencies`):
```json
    "@vitejs/plugin-react": "^4.2.1",
```

After:
```json
    "@vitejs/plugin-react": "^6.0.1",
```

### Task B.2: Install + smoke-test showcase

**Files:** none (validation only)

- [ ] **Step 1: Install dependencies**

Run:
```bash
cd spaceui && bun install 2>&1 | tee /tmp/showcase-install.log
```

Expected: install completes. Inspect log for `ERESOLVE` or peer warnings about react/react-hook-form. Peer warnings about `react: ^18` from a transitive (Storybook 8.6) are acceptable and will be cleared by PR C.

- [ ] **Step 2: Build showcase (validates plugin-react 6 + Vite 6 still works together)**

Run:
```bash
bun run showcase:build 2>&1 | tee /tmp/showcase-build.log
cd ..
```

Expected: build exits 0. Inspect `/tmp/showcase-build.log` — final line should be `✓ built in <time>` from Vite. Look for the produced `examples/showcase/dist/` directory.

- [ ] **Step 3: Boot showcase dev server in background and ask user to verify**

Run:
```bash
cd spaceui
bun run showcase > /tmp/showcase-dev.log 2>&1 &
echo $! > /tmp/showcase-dev.pid
for i in $(seq 1 30); do
  if grep -qE "Local:|ready in" /tmp/showcase-dev.log; then break; fi
  sleep 1
done
tail -20 /tmp/showcase-dev.log
cd ..
```

Then **ask the user**:

> Showcase dev server is running. The log shows the local URL (typically http://localhost:5173). Please open it in a browser and verify:
> 1. The showcase index page renders
> 2. At least one component example renders without console errors (open DevTools console)
> 3. Reply `pass` or `fail: <reason>` and paste any console errors

- [ ] **Step 4: After user confirms, capture logs and stop dev server**

Run:
```bash
tail -100 /tmp/showcase-dev.log > /tmp/showcase-dev-final.log
kill $(cat /tmp/showcase-dev.pid) 2>/dev/null
rm -f /tmp/showcase-dev.pid
grep -iE "error|fail" /tmp/showcase-dev-final.log | head -20
```

Expected: no `Error:` lines. The showcase doesn't import zod or hookform/resolvers, so the manifest bumps cannot cause runtime regressions.

### Task B.3: Commit, push, open PR

**Files:** local git, remote PR

- [ ] **Step 1: Stage, commit, push, open**

Run:
```bash
git add spaceui/examples/showcase/package.json
git commit -m "$(cat <<'EOF'
deps(spaceui-showcase): april 2026 batch — zod 4, hookform/resolvers 5, plugin-react 6

- zod 3.22 → 4.3.6 (Dependabot #25)
- @hookform/resolvers 3.3 → 5.2.2 (Dependabot #21)
- @vitejs/plugin-react 4.2 → 6.0.1 (Dependabot #22)

None of these deps are imported in showcase/src; manifest-only alignment
with interface/. Verified locally: bun install, showcase:build, dev boot.
EOF
)"
git push -u origin deps/spaceui-showcase-april-2026
gh pr create --title "deps(spaceui-showcase): april 2026 batch — zod 4 + hookform 5 + plugin-react 6" --body "$(cat <<'EOF'
## Summary

Workspace-grouped local replacement for Dependabot PRs #21, #22, #25.

- `zod` 3.22 → ^4.3.6
- `@hookform/resolvers` 3.3 → ^5.2.2
- `@vitejs/plugin-react` 4.2 → ^6.0.1

None of these deps are imported in `spaceui/examples/showcase/src/`
(verified — src has App.tsx, main.tsx, index.css only). Manifest-only
alignment with `interface/` which already runs all three at the target
versions.

## Test plan

- [x] `cd spaceui && bun install` (peer warnings about Storybook 8.6 React 18 expected, cleared in PR C)
- [x] `bun run showcase:build` succeeds
- [x] Dev server boots; index + at least one component verified manually

Dependabot PRs #21, #22, #25 will auto-close once this merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

- [ ] **Step 2: Verify Dependabot PRs auto-closed**

Run:
```bash
sleep 30
for pr in 21 22 25; do
  echo "#$pr: $(gh pr view $pr --json state -q .state)"
done
```

Expected: all `CLOSED`.

---

## PR C — Storybook 8 → 10 coordinated bump (replaces #20, #26; unblocks PR D and PR E)

This is the critical sequencing PR. `@storybook/react-vite@8.6.18` declares `peerDependencies.react: '^16.8.0 || ^17.0.0 || ^18.0.0 || ^19.0.0-beta'` and `peerDependencies.vite: '^4.0.0 || ^5.0.0 || ^6.0.0'`. Bumping React to 19 stable or Vite to 8 *before* Storybook 10 lands triggers peer warnings at minimum and may break Storybook boot at worst. Storybook 10 explicitly supports React 19 stable + Vite 7+.

The bump must touch both `spaceui/package.json` and `spaceui/.storybook/package.json` simultaneously, remove three packages that are unpublished in v10, and prune the `addons[]` array in `main.ts`.

### Task C.1: Branch off

**Files:** local git

- [ ] **Step 1: Create branch from latest main**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/spaceui-storybook-10
```

### Task C.2: Update top-level `spaceui/package.json`

**Files:**
- Modify: `spaceui/package.json:31-43`

- [ ] **Step 1: Replace `@storybook/*` block — bump kept packages, remove 3 unpublished**

Apply this exact edit using the editor.

Before (in `spaceui/package.json`, devDependencies):
```json
    "@storybook/addon-essentials": "8.6.18",
    "@storybook/addon-interactions": "8.6.18",
    "@storybook/addon-onboarding": "8.6.18",
    "@storybook/addon-themes": "8.6.18",
    "@storybook/blocks": "8.6.18",
    "@storybook/react": "8.6.18",
    "@storybook/react-vite": "8.6.18",
    "@storybook/test": "8.6.18",
    "@storybook/theming": "8.6.18",
    "storybook": "8.6.18",
```

After:
```json
    "@storybook/addon-onboarding": "^10.3.5",
    "@storybook/addon-themes": "^10.3.5",
    "@storybook/react": "^10.3.5",
    "@storybook/react-vite": "^10.3.5",
    "@storybook/test": "^10.3.5",
    "@storybook/theming": "^10.3.5",
    "storybook": "^10.3.5",
```

Note: `addon-essentials`, `addon-interactions`, and `blocks` are removed — empty packages in v10, unpublished going forward.

### Task C.3: Update `spaceui/.storybook/package.json` the same way

**Files:**
- Modify: `spaceui/.storybook/package.json:14-26`

- [ ] **Step 1: Apply matching edit**

Before (in `spaceui/.storybook/package.json`, devDependencies):
```json
    "@chromatic-com/storybook": "^2.0.0",
    "@storybook/addon-essentials": "8.6.18",
    "@storybook/addon-interactions": "8.6.18",
    "@storybook/addon-onboarding": "8.6.18",
    "@storybook/addon-themes": "8.6.18",
    "@storybook/blocks": "8.6.18",
    "@storybook/react": "8.6.18",
    "@storybook/react-vite": "8.6.18",
    "@storybook/test": "8.6.18",
    "@storybook/theming": "8.6.18",
    "storybook": "8.6.18",
```

After:
```json
    "@chromatic-com/storybook": "^5.1.2",
    "@storybook/addon-onboarding": "^10.3.5",
    "@storybook/addon-themes": "^10.3.5",
    "@storybook/react": "^10.3.5",
    "@storybook/react-vite": "^10.3.5",
    "@storybook/test": "^10.3.5",
    "@storybook/theming": "^10.3.5",
    "storybook": "^10.3.5",
```

`@chromatic-com/storybook` v5.1.2 declares `peerDependencies.storybook: '^0.0.0-0 || ^10.1.0 || ^10.1.0-0 || ^10.2.0-0 || ^10.3.0-0 || ^10.4.0-0'` (verified via npm registry). Top-level `spaceui/package.json` already uses `^5.1.2` so this aligns the two manifests.

### Task C.4: Prune `spaceui/.storybook/main.ts` addons[]

**Files:**
- Modify: `spaceui/.storybook/main.ts:7-13`

- [ ] **Step 1: Remove the two unpublished addon entries**

Before:
```ts
  addons: [
    '@storybook/addon-onboarding',
    '@storybook/addon-essentials',
    '@chromatic-com/storybook',
    '@storybook/addon-interactions',
    '@storybook/addon-themes',
  ],
```

After:
```ts
  addons: [
    '@storybook/addon-onboarding',
    '@chromatic-com/storybook',
    '@storybook/addon-themes',
  ],
```

### Task C.5: (Optional) Add Storybook grouping to `dependabot.yml`

**Files:**
- Modify: `.github/dependabot.yml`

This is a quality-of-life change so future Storybook majors arrive as one PR. Skip if you want to keep the diff small.

- [ ] **Step 1: Add `groups:` block to the `/spaceui/.storybook` entry**

Locate the existing entry. Before:
```yaml
  - package-ecosystem: "npm"
    directory: "/spaceui/.storybook"
    schedule:
      interval: "weekly"
    commit-message:
      prefix: "deps(spaceui-storybook)"
    open-pull-requests-limit: 3
```

After:
```yaml
  - package-ecosystem: "npm"
    directory: "/spaceui/.storybook"
    schedule:
      interval: "weekly"
    commit-message:
      prefix: "deps(spaceui-storybook)"
    open-pull-requests-limit: 3
    groups:
      storybook:
        patterns:
          - "storybook"
          - "@storybook/*"
          - "@chromatic-com/storybook"
```

- [ ] **Step 2: Apply the same `groups:` block to the `/spaceui` entry** so spaceui-root storybook bumps also coalesce. Insert under the existing `prefix: "deps(spaceui)"` entry.

### Task C.6: Install + smoke-test Storybook 10

**Files:** none (validation only)

- [ ] **Step 1: Clean install in spaceui**

Run:
```bash
cd spaceui
rm -rf node_modules .storybook/node_modules examples/showcase/node_modules
bun install 2>&1 | tee /tmp/storybook10-install.log
```

Expected: install completes. Inspect `/tmp/storybook10-install.log` for `ERESOLVE`. Peer warnings are acceptable; `ERESOLVE` is not.

- [ ] **Step 2: Typecheck the workspace**

Run:
```bash
bun run typecheck 2>&1 | tee /tmp/storybook10-typecheck.log
```

Expected: exit 0. If `@storybook/addon-essentials` or `@storybook/addon-interactions` are referenced anywhere outside `main.ts`, fix the import or delete the reference now.

- [ ] **Step 3: Build Storybook static output (catches build-time regressions)**

Run:
```bash
bun run storybook:build 2>&1 | tee /tmp/storybook10-build.log
cd ..
```

Expected: `storybook-static/` directory produced; build exits 0. Search the log for `error` (lowercase) — should be zero matches outside the literal addon names.

- [ ] **Step 4: Boot Storybook dev server in background**

Run:
```bash
cd spaceui
bun run storybook > /tmp/storybook10-dev.log 2>&1 &
echo $! > /tmp/storybook10-dev.pid
for i in $(seq 1 60); do
  if grep -qE "Storybook .* started|Local:" /tmp/storybook10-dev.log; then break; fi
  sleep 1
done
tail -30 /tmp/storybook10-dev.log
cd ..
```

Then **ask the user**:

> Storybook 10 dev server is running on http://localhost:6006. Please open it in a browser and verify:
> 1. The Storybook UI loads (sidebar with stories visible)
> 2. Click into the **Dialog** story (under primitives) and confirm it renders + open/close transition animates
> 3. Click into the **Form** story (under primitives) and confirm fields render
> 4. Open the browser DevTools console and verify no red errors
> 5. Reply `pass` or `fail: <reason>` and paste any console errors

- [ ] **Step 5: After user confirms, capture logs and stop dev server**

Run:
```bash
tail -200 /tmp/storybook10-dev.log > /tmp/storybook10-dev-final.log
kill $(cat /tmp/storybook10-dev.pid) 2>/dev/null
rm -f /tmp/storybook10-dev.pid
grep -iE "error|fail|deprecated" /tmp/storybook10-dev-final.log | head -30
```

Expected: no `Error:` lines or stack traces. `deprecated` warnings about Storybook 8 patterns are acceptable in this transition window.

### Task C.7: Commit, push, open PR

**Files:** local git, remote PR

- [ ] **Step 1: Stage and commit**

Run:
```bash
git add spaceui/package.json spaceui/.storybook/package.json spaceui/.storybook/main.ts
# Include dependabot.yml only if Task C.5 was done:
git add .github/dependabot.yml 2>/dev/null || true
git commit -m "$(cat <<'EOF'
deps(spaceui): coordinated Storybook 8 → 10 bump

Replaces partial Dependabot bumps in #20 and #26 with a single
coordinated change across both /spaceui and /spaceui/.storybook
manifests:

- Bump every @storybook/* package and `storybook` itself to ^10.3.5
- Bump @chromatic-com/storybook to ^5.1.2 (storybook 10 compatibility)
- Remove @storybook/addon-essentials, @storybook/addon-interactions,
  @storybook/blocks (empty/unpublished in v10)
- Prune the same two addons from .storybook/main.ts addons[]
- (Optional) Add `groups:` block to dependabot.yml so future Storybook
  majors arrive as a single grouped PR

Storybook 10 unblocks React 19 stable and Vite 7+ peer ranges, which
PR D and PR E rely on.

Validated locally: clean install, typecheck, storybook:build,
dev server boot with Dialog + Form smoke-tested.
EOF
)"
git push -u origin deps/spaceui-storybook-10
gh pr create --title "deps(spaceui): coordinated Storybook 8 → 10 bump" --body "$(cat <<'EOF'
## Summary

Workspace-grouped local replacement for Dependabot PRs #20 and #26
(both partial Storybook 10 bumps that would have broken the workspace
because they only touched 2 of the 9 @storybook/* packages).

- All `@storybook/*` and `storybook` → ^10.3.5 in both manifests
- `@chromatic-com/storybook` → ^5.1.2 in `.storybook/package.json`
- Drops `@storybook/addon-essentials`, `@storybook/addon-interactions`, `@storybook/blocks` (unpublished in v10)
- Prunes the same two from `.storybook/main.ts` addons[]
- Adds dependabot grouping for future Storybook majors

## Why this PR sequence matters

Storybook 8.6 has narrow peer ranges (React `^19.0.0-beta`, Vite `^4||^5||^6`). PR D (React 19) and PR E (Vite 8 + react-spring 10 + react-markdown 10) cannot land cleanly until Storybook 10 is in place.

## Test plan

- [x] `cd spaceui && bun install` clean (no ERESOLVE)
- [x] `bun run typecheck` exits 0
- [x] `bun run storybook:build` produces static output
- [x] `bun run storybook` boots; Dialog and Form stories verified manually

Dependabot PRs #20 and #26 will auto-close once this merges (their bumped versions are subsumed by the ^10.3.5 baseline).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Wait for CI green and merge**

Run:
```bash
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

- [ ] **Step 3: Verify Dependabot PRs auto-closed**

Run:
```bash
sleep 30
for pr in 20 26; do
  echo "#$pr: $(gh pr view $pr --json state -q .state)"
done
```

Expected: both `CLOSED`.

---

## PR D — React 18 → 19 across spaceui (replaces #23, #24, plus showcase alignment)

After PR C, all `@storybook/*` packages support React 19 stable. Now we can bump React to 19 across `spaceui/.storybook/package.json` and `spaceui/examples/showcase/package.json`. The top-level `spaceui/package.json` has no `react` field (only an `overrides` block for `@types/react`), so it doesn't need a React bump itself.

### Task D.1: Branch off

- [ ] **Step 1: Create branch from latest main**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/spaceui-react-19
```

### Task D.2: Edit `spaceui/.storybook/package.json`

**Files:**
- Modify: `spaceui/.storybook/package.json` (devDependencies block)

- [ ] **Step 1: Bump react and react-dom**

Before:
```json
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
```

After:
```json
    "react": "^19.2.5",
    "react-dom": "^19.2.5",
```

### Task D.3: Edit `spaceui/examples/showcase/package.json`

**Files:**
- Modify: `spaceui/examples/showcase/package.json` (dependencies and devDependencies)

- [ ] **Step 1: Bump react/react-dom in dependencies**

Before:
```json
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
```

After:
```json
    "react": "^19.2.5",
    "react-dom": "^19.2.5",
```

- [ ] **Step 2: Bump @types/react and @types/react-dom in devDependencies**

Before:
```json
    "@types/react": "^18.2.64",
    "@types/react-dom": "^18.2.21",
```

After:
```json
    "@types/react": "^19.2.14",
    "@types/react-dom": "^19.2.3",
```

### Task D.4: Install + smoke-test React 19

**Files:** none (validation only)

- [ ] **Step 1: Clean install**

Run:
```bash
cd spaceui
rm -rf node_modules .storybook/node_modules examples/showcase/node_modules
bun install 2>&1 | tee /tmp/react19-install.log
```

Expected: install completes. Look for any `Invalid hook call` warnings now (none should appear at install time but Storybook 10's react-vite peer should now be satisfied).

- [ ] **Step 2: Typecheck**

Run:
```bash
bun run typecheck 2>&1 | tee /tmp/react19-typecheck.log
```

Expected: exit 0. React 19's stricter types may surface latent issues — fix them inline if minor (e.g., remove `React.FC` defaultProps), escalate to user if structural.

- [ ] **Step 3: Build Storybook + showcase**

Run:
```bash
bun run storybook:build 2>&1 | tee /tmp/react19-storybook-build.log
bun run showcase:build 2>&1 | tee /tmp/react19-showcase-build.log
cd ..
```

Expected: both builds succeed. No `Invalid hook call`, no React 18-only API errors.

- [ ] **Step 4: Boot Storybook dev server**

Run:
```bash
cd spaceui
bun run storybook > /tmp/react19-storybook-dev.log 2>&1 &
echo $! > /tmp/react19-storybook-dev.pid
for i in $(seq 1 60); do
  if grep -qE "Storybook .* started|Local:" /tmp/react19-storybook-dev.log; then break; fi
  sleep 1
done
tail -30 /tmp/react19-storybook-dev.log
cd ..
```

Then **ask the user**:

> Storybook (React 19 baseline) is running on http://localhost:6006. Please verify:
> 1. Sidebar loads with stories
> 2. Dialog story renders + transitions still animate
> 3. Form story renders + fields are interactive
> 4. DevTools console: no `Invalid hook call`, no React 19-specific errors
> 5. Reply `pass` or `fail: <reason>` with console output

- [ ] **Step 5: After user confirms, capture logs and stop dev server**

Run:
```bash
tail -200 /tmp/react19-storybook-dev.log > /tmp/react19-storybook-dev-final.log
kill $(cat /tmp/react19-storybook-dev.pid) 2>/dev/null
rm -f /tmp/react19-storybook-dev.pid
grep -iE "error|invalid hook" /tmp/react19-storybook-dev-final.log | head -30
```

Expected: no `Invalid hook call`, no `Error:` lines.

### Task D.5: Commit, push, open PR

- [ ] **Step 1: Stage, commit, push, open**

Run:
```bash
git add spaceui/.storybook/package.json spaceui/examples/showcase/package.json
git commit -m "$(cat <<'EOF'
deps(spaceui): React 18 → 19 across .storybook + showcase

Workspace-grouped replacement for Dependabot PRs #23 and #24, plus the
showcase React 18 → 19 alignment that Dependabot didn't open.

- spaceui/.storybook: react/react-dom 18.2 → ^19.2.5
- spaceui/examples/showcase: react/react-dom + @types/react/react-dom
  bumped to React 19

Top-level spaceui/package.json has no react field (only an overrides
block for @types/react which already pins 19.2.14), so no edit there.

Storybook 10 (PR #X — replace with the actual PR number once known)
landed first so Storybook's react-vite peer range now satisfies React
19 stable instead of just ^19.0.0-beta.

Validated: clean install, typecheck, storybook:build, showcase:build,
dev server boot with Dialog + Form smoke-tested under React 19.
EOF
)"
git push -u origin deps/spaceui-react-19
gh pr create --title "deps(spaceui): React 18 → 19 across .storybook + showcase" --body "$(cat <<'EOF'
## Summary

Workspace-grouped local replacement for Dependabot PRs #23 and #24, plus the showcase alignment Dependabot didn't open (it never did because showcase peers allowed React 18).

- `spaceui/.storybook/package.json`: react / react-dom → ^19.2.5
- `spaceui/examples/showcase/package.json`: react / react-dom → ^19.2.5; @types/react → ^19.2.14; @types/react-dom → ^19.2.3

Top-level `spaceui/package.json` has no `react` field, just an `overrides` block already pinning `@types/react: 19.2.14` — no edit needed there.

## Test plan

- [x] `cd spaceui && bun install` clean (Storybook 10 peer satisfied)
- [x] `bun run typecheck` exits 0
- [x] `bun run storybook:build` succeeds
- [x] `bun run showcase:build` succeeds
- [x] Dev server boots; Dialog + Form verified under React 19; no `Invalid hook call`

Dependabot PRs #23 and #24 will auto-close once this merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

- [ ] **Step 2: Verify Dependabot PRs auto-closed**

Run:
```bash
sleep 30
for pr in 23 24; do
  echo "#$pr: $(gh pr view $pr --json state -q .state)"
done
```

Expected: both `CLOSED`.

---

## PR E — `/spaceui/packages/*` bumps (replaces #27, #28, #30, #32)

After PR C (Storybook 10) and PR D (React 19), all peer constraints are satisfied. Land the four package-level bumps in one branch.

### Task E.1: Branch off

- [ ] **Step 1: Create branch**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/spaceui-packages-april-2026
```

### Task E.2: Edit `spaceui/packages/ai/package.json`

**Files:**
- Modify: `spaceui/packages/ai/package.json`

- [ ] **Step 1: Bump react-markdown in dependencies**

Before:
```json
    "react-markdown": "^9.0.0",
```

After:
```json
    "react-markdown": "^10.1.0",
```

- [ ] **Step 2: Bump graphology in optionalDependencies**

Before:
```json
    "graphology": "^0.25.0",
```

After:
```json
    "graphology": "^0.26.0",
```

### Task E.3: Edit `spaceui/packages/primitives/package.json`

**Files:**
- Modify: `spaceui/packages/primitives/package.json`

- [ ] **Step 1: Bump @react-spring/web**

Before:
```json
    "@react-spring/web": "^9.7.0",
```

After:
```json
    "@react-spring/web": "^10.0.3",
```

### Task E.4: Edit `spaceui/.storybook/package.json` and `spaceui/examples/showcase/package.json` for Vite 8

**Files:**
- Modify: `spaceui/.storybook/package.json`
- Modify: `spaceui/examples/showcase/package.json`

Vite 8 lives in `.storybook` and `examples/showcase` — the top-level `spaceui/package.json` has no `vite` dep. (PR title `in /spaceui` is a Dependabot artifact; the actual diff is in those two manifests.)

- [ ] **Step 1: Bump vite in `spaceui/.storybook/package.json` devDependencies**

Before:
```json
    "vite": "^6.4.2",
```

After:
```json
    "vite": "^8.0.8",
```

- [ ] **Step 2: Bump vite in `spaceui/examples/showcase/package.json` devDependencies**

Before:
```json
    "vite": "^6.4.2"
```

After:
```json
    "vite": "^8.0.8"
```

### Task E.5: Install + smoke-test the package bumps

**Files:** none (validation only)

- [ ] **Step 1: Clean install**

Run:
```bash
cd spaceui
rm -rf node_modules .storybook/node_modules examples/showcase/node_modules
bun install 2>&1 | tee /tmp/packages-install.log
```

Expected: install completes. Look for `ERESOLVE` (none expected — Storybook 10 supports Vite 7+, react-spring 10 supports React 19, react-markdown 10 supports React 19).

- [ ] **Step 2: Typecheck**

Run:
```bash
bun run typecheck 2>&1 | tee /tmp/packages-typecheck.log
```

Expected: exit 0. react-markdown v10 changed some type exports — fix imports if surfaced.

- [ ] **Step 3: Build Storybook + showcase**

Run:
```bash
bun run storybook:build 2>&1 | tee /tmp/packages-storybook-build.log
bun run showcase:build 2>&1 | tee /tmp/packages-showcase-build.log
cd ..
```

Expected: both build cleanly under Vite 8 + Storybook 10.

- [ ] **Step 4: Boot Storybook dev server (final smoke test for the whole upgrade)**

Run:
```bash
cd spaceui
bun run storybook > /tmp/packages-storybook-dev.log 2>&1 &
echo $! > /tmp/packages-storybook-dev.pid
for i in $(seq 1 60); do
  if grep -qE "Storybook .* started|Local:" /tmp/packages-storybook-dev.log; then break; fi
  sleep 1
done
tail -30 /tmp/packages-storybook-dev.log
cd ..
```

Then **ask the user**:

> Storybook (full upgraded stack: Storybook 10 + React 19 + Vite 8 + react-spring 10 + react-markdown 10) is running on http://localhost:6006. Please verify the components touched by these bumps:
> 1. **Dialog** story (uses @react-spring/web): open + close, confirm transitions animate
> 2. **Form** story (uses @react-spring/web): renders, field transitions work
> 3. **Markdown** story (under @spacedrive/ai, uses react-markdown 10): renders sample markdown with bold, code, lists
> 4. **MemoryGraph** story if present (uses graphology 0.26): renders nodes/edges
> 5. DevTools console: no errors related to these libraries
> 6. Reply `pass` or `fail: <reason>` with console output

- [ ] **Step 5: After user confirms, capture logs and stop dev server**

Run:
```bash
tail -200 /tmp/packages-storybook-dev.log > /tmp/packages-storybook-dev-final.log
kill $(cat /tmp/packages-storybook-dev.pid) 2>/dev/null
rm -f /tmp/packages-storybook-dev.pid
grep -iE "error|deprecated|invalid" /tmp/packages-storybook-dev-final.log | head -30
```

Expected: no `Error:` lines. Vite 8 may emit `[plugin] deprecated` warnings about the removed `handleHotUpdate` hook from Storybook addons — those are upstream and tolerated.

### Task E.6: Commit, push, open PR

- [ ] **Step 1: Stage, commit, push, open**

Run:
```bash
git add spaceui/packages/ai/package.json spaceui/packages/primitives/package.json spaceui/.storybook/package.json spaceui/examples/showcase/package.json
git commit -m "$(cat <<'EOF'
deps(spaceui): packages april 2026 batch — react-markdown 10, graphology 0.26, react-spring 10, vite 8

Workspace-grouped replacement for Dependabot PRs #27, #28, #30, #32.

- spaceui/packages/ai: react-markdown 9 → ^10.1.0 (Dependabot #27)
- spaceui/packages/ai: graphology 0.25 → ^0.26.0 (Dependabot #28)
- spaceui/packages/primitives: @react-spring/web 9 → ^10.0.3 (Dependabot #30)
- spaceui/.storybook + spaceui/examples/showcase: vite 6 → ^8.0.8 (Dependabot #32)

All four bumps require Storybook 10 (PR #X) and React 19 (PR #Y) to be
in place — peer ranges of these new majors are React-19-stable and the
Storybook 8.6 builder-vite was capped at Vite ^6.

Validated: clean install, typecheck, storybook:build, showcase:build,
dev server boot with Dialog/Form/Markdown/MemoryGraph stories
smoke-tested.
EOF
)"
git push -u origin deps/spaceui-packages-april-2026
gh pr create --title "deps(spaceui): packages april 2026 — react-markdown 10 + graphology 0.26 + react-spring 10 + vite 8" --body "$(cat <<'EOF'
## Summary

Workspace-grouped local replacement for Dependabot PRs #27, #28, #30, #32.

- `spaceui/packages/ai`: `react-markdown` 9 → ^10.1.0
- `spaceui/packages/ai`: `graphology` 0.25 → ^0.26.0
- `spaceui/packages/primitives`: `@react-spring/web` 9 → ^10.0.3
- `spaceui/.storybook` + `spaceui/examples/showcase`: `vite` 6 → ^8.0.8

## Why this is the last PR in the sequence

All four bumps require either React 19 stable (which Storybook 8.6 would not allow) or Vite 7+ (which Storybook 8.6 would not allow). PR C (Storybook 10) and PR D (React 19) had to land first.

## Test plan

- [x] `cd spaceui && bun install` clean
- [x] `bun run typecheck` exits 0
- [x] `bun run storybook:build` succeeds under Storybook 10 + Vite 8 + React 19
- [x] `bun run showcase:build` succeeds under Vite 8 + React 19
- [x] Dev server boots; Dialog (react-spring 10), Form (react-spring 10), Markdown (react-markdown 10), MemoryGraph (graphology 0.26) all verified

Dependabot PRs #27, #28, #30, #32 will auto-close once this merges.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

- [ ] **Step 2: Verify Dependabot PRs auto-closed and final PR audit**

Run:
```bash
sleep 30
for pr in 27 28 30 32; do
  echo "#$pr: $(gh pr view $pr --json state -q .state)"
done
echo "---"
echo "Open PRs in #20-33 range (should be empty):"
gh pr list --state open --json number --limit 100 \
  | jq -r '.[] | select(.number >= 20 and .number <= 33) | "#\(.number) STILL OPEN"'
```

Expected: all four `CLOSED`. The "STILL OPEN" listing should be empty.

---

## Phase F — Post-merge cleanup (optional)

### Task F.1: Verify the migration succeeded end-to-end

- [ ] **Step 1: Show recent main history**

Run:
```bash
git log --oneline -10
```

Expected: 5 squash-merge commits from PR A through PR E on top of the pre-baseline.

- [ ] **Step 2: Run the spacebot Rust gate to ensure no regression in the daemon**

Run:
```bash
just preflight
just gate-pr
```

Expected: both green. None of these PRs touch Rust code, but running the gate confirms `main` is shippable.

- [ ] **Step 3: Update `.scratchpad/dependabot-prs-2026-04-16.md` with the resolution status** (optional housekeeping)

Append a "Resolved" section noting the PR numbers (A-E) and their merge commit shas.

### Task F.2: (Optional) framer-motion alignment

`spaceui/packages/primitives/package.json` declares `framer-motion: ^11.0.0` while `interface/package.json` is on `^12.38.0`. Both v11 and v12 are React-19 compatible. If desired, open a follow-up PR.

- [ ] **Step 1: Branch + edit**

Run:
```bash
git checkout main && git pull --ff-only
git checkout -b deps/primitives-framer-motion-12
```

Edit `spaceui/packages/primitives/package.json`:

Before: `"framer-motion": "^11.0.0",`
After: `"framer-motion": "^12.0.0",`

- [ ] **Step 2: Smoke-test Dialog/Form**

Run:
```bash
cd spaceui && bun install && bun run typecheck && bun run storybook:build
cd ..
```

Then boot Storybook (same protocol as Task E.5) and verify Dialog + Form animations.

- [ ] **Step 3: Commit + PR + merge**

Run:
```bash
git add spaceui/packages/primitives/package.json
git commit -m "deps(spaceui-primitives): align framer-motion to ^12.0.0 with interface/"
git push -u origin deps/primitives-framer-motion-12
gh pr create --fill
gh pr checks --watch
gh pr merge --squash --delete-branch
git checkout main && git pull --ff-only
```

---

## Rollback Plan

Every PR is a single squash commit on `main`. Roll back per-PR:

```bash
git revert <sha> --no-edit
git push -u origin "revert-<sha>"
gh pr create --title "Revert: <original title>" --body "Rolling back due to <reason>"
gh pr merge --squash --delete-branch
```

Branch protection requires the revert to also go through a PR. If a Dependabot PR was already auto-closed, reopen it manually after the revert lands:
```bash
gh pr reopen <pr-number>
```

---

## Self-Review

Spec coverage check (against `.scratchpad/dependabot-prs-2026-04-16.md` and the user's revised constraints):

- ✅ All 14 PRs accounted for: PR A (#29, #31, #33), PR B (#21, #22, #25), PR C (#20, #26), PR D (#23, #24), PR E (#27, #28, #30, #32)
- ✅ Workspace grouping respected (5 PRs total per user choice)
- ✅ Storybook 10 / React 19 / Vite 8 sequencing preserved (PR C → PR D → PR E)
- ✅ Local-only changes (no `gh pr merge` against Dependabot branches)
- ✅ Branch protection respected (every change goes through a PR; never push to main)
- ✅ Auto-close behavior used (no manual `gh pr close` for Dependabot PRs)
- ✅ Smoke-test protocol: log-capture + manual user verification + post-test log validation
- ✅ Rollback plan included
- ✅ Documentation drift confirmed already-fixed; no doc edits needed

Type/command consistency check:

- ✅ Branch names follow `deps/<scope>-<topic>` convention used by `feat/upgrade-rig`
- ✅ All bun scripts referenced (`typecheck`, `storybook`, `storybook:build`, `showcase`, `showcase:build`) verified present in `spaceui/package.json`
- ✅ Manifest line numbers verified against current files
- ✅ Log file paths use `/tmp/<phase>-<step>.log` consistently
- ✅ Log capture pattern is identical across PR A, B, C, D, E (background + PID file + grep pattern + tail-on-stop)
- ✅ `@chromatic-com/storybook ^5.1.2` peer range verified via npm registry (supports Storybook ^10.1.0+)
