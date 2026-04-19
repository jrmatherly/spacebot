## 1. Backups (run before any deletion)

- [x] 1.1 Mkdir `.scratchpad/backups/api-client-activation/` (gitignored via the blanket `.scratchpad/` entry in `.gitignore`; local-only)
- [x] 1.2 Copy `packages/api-client/src/client.ts` → `.scratchpad/backups/api-client-activation/packages-api-client-client.ts` (openapi-fetch stub, 291 lines)
- [x] 1.3 Copy `packages/api-client/src/events.ts` → `.scratchpad/backups/api-client-activation/packages-api-client-events.ts` (aspirational SSE catalog, 164 lines)
- [x] 1.4 Copy `packages/api-client/src/types.ts` → `.scratchpad/backups/api-client-activation/packages-api-client-types.ts` (proxy shim, 1 line)
- [x] 1.5 Copy `packages/api-client/src/schema.d.ts` → `.scratchpad/backups/api-client-activation/packages-api-client-schema.d.ts` (proxy shim, 1 line)
- [x] 1.6 Copy `interface/src/api/client-typed.ts` → `.scratchpad/backups/api-client-activation/interface-api-client-typed.ts` (dead code, 24 lines)
- [x] 1.7 Verify: `ls .scratchpad/backups/api-client-activation/` shows all 5 files

## 2. Phase 1 — Package rename + lift-and-shift

- [x] 2.1 Edit `packages/api-client/package.json`: change `name` from `@spacedrive/api-client` to `@spacebot/api-client`
- [x] 2.2 Update `packages/api-client/package.json` `exports` block — match the target layout: `"."`, `"./client"`, `"./types"`, `"./schema"` (remove `"./events"`)
- [x] 2.3 Delete `packages/api-client/src/client.ts` (openapi-fetch stub, already backed up per §1.2)
- [x] 2.4 Delete `packages/api-client/src/events.ts` (already backed up per §1.3)
- [x] 2.5 Delete `packages/api-client/src/types.ts` (proxy shim, already backed up per §1.4)
- [x] 2.6 Delete `packages/api-client/src/schema.d.ts` (proxy shim, already backed up per §1.5)
- [x] 2.7 Delete `interface/src/api/client-typed.ts` (dead code, already backed up per §1.6)
- [x] 2.8 `git mv interface/src/api/client.ts packages/api-client/src/client.ts` (preserves history)
- [x] 2.9 `git mv interface/src/api/types.ts packages/api-client/src/types.ts`
- [x] 2.10 `git mv interface/src/api/schema.d.ts packages/api-client/src/schema.d.ts`
- [x] 2.11 Verify `interface/src/api/` directory is empty; `rmdir interface/src/api/`
- [x] 2.12 Replace `packages/api-client/src/index.ts` content with: `export * from "./client";\nexport * from "./types";\n` (drops the `./events` re-export since the file no longer exists)
- [x] 2.13 Verify inter-file relative imports still resolve: the moved files contain `./types` and `./schema` imports (`client.ts:27,89,98,103`; `types.ts:2`). Because all three files move together into `packages/api-client/src/` preserving their `./` relatives, no rewrite is required. Verified post-move via tsc (task §5.1).

## 3. Phase 1 — Workspace wiring

- [x] 3.1 Edit `interface/package.json` `workspaces` array: add `"../packages/*"` alongside the existing `"../spaceui/packages/*"` entry
- [x] 3.2 Add `"@spacebot/api-client": "workspace:*"` to `interface/package.json` `dependencies` (alphabetical position)
- [x] 3.3 **No alias file edits.** Do NOT edit `interface/vite.config.ts:71` or `interface/tsconfig.json:20-22`. The `@/` alias continues to resolve non-api `interface/src/*` paths; only the 85 `@/api/*` import usages (handled in §4) change.
- [x] 3.4 Run `cd interface && bun install` — verify no errors, no npm-registry fallback warning
- [x] 3.5 Verify symlink: `ls -la interface/node_modules/@spacebot/api-client` should point at `../../packages/api-client`. **Stop-gate**: if the symlink is missing or incorrect, do NOT proceed to §4. Debug the workspace declaration first.
- [x] 3.6 Extend `scripts/check-workspace-protocol.sh` to cover `@spacebot/*` deps: change the filter on line 30 from `'"@spacedrive/'` to match both scopes (BRE: `'"@\(spacedrive\|spacebot\)/'`; ERE: `'"@(spacedrive|spacebot)/'`). Apply the same substitution on line 35. Run `./scripts/check-workspace-protocol.sh` manually to verify it still prints "OK" and does not false-positive on the new scope.

## 4. Phase 1 — Bulk import rename

- [x] 4.1 Run: `grep -rln '"@/api/' interface/src/ | wc -l` — starting count was 84 (not 85 as originally noted; the 85th file was the relative-import straggler handled in §4.6 separately)
- [x] 4.2 Bulk substitute `"@/api/client"` → `"@spacebot/api-client/client"` across `interface/src/**/*.{ts,tsx}`. Preferred command: `grep -rln '"@/api/client"' interface/src/ | xargs sed -i '' 's|"@/api/client"|"@spacebot/api-client/client"|g'` (macOS `sed`).
- [x] 4.3 Bulk substitute `"@/api/types"` → `"@spacebot/api-client/types"` across `interface/src/**/*.{ts,tsx}`. Preferred command: `grep -rln '"@/api/types"' interface/src/ | xargs sed -i '' 's|"@/api/types"|"@spacebot/api-client/types"|g'`.
- [x] 4.4 Run: `grep -rln '"@/api/' interface/src/ | wc -l` — expected result: 0 (confirmed)
- [x] 4.5 Sanity check: `grep -rln '"@spacebot/api-client' interface/src/ | wc -l` — result: 84 (matches starting count; `-l` is file-count, so a file with both imports counts once in both starting and ending)
- [x] 4.6 Catch any non-alias relative imports: one pre-existing case exists at `interface/src/hooks/useChannelLiveState.ts:19` (`from "../api/client"`). Update that to `from "@spacebot/api-client/client"` as part of this pass.

## 5. Phase 1 — Verification

- [x] 5.1 `cd interface && bunx tsc --noEmit` — passes with zero errors (prerequisite: `cd ../spaceui && bun install && bun run build` if this is a fresh checkout, per `interface/CLAUDE.md`)
- [x] 5.2 `cd interface && bun run build` — produces a viable `dist/`, no errors
- [x] 5.3 Manually spot-check 3 random migrated files (1 settings, 1 portal, 1 org-graph) — imports resolve, types work
- [ ] 5.4 Runtime smoke test (recommended but optional): `just bundle-sidecar`, start the daemon, open `http://localhost:19898` in a browser. Verify (a) app loads without console errors, (b) settings panel renders, (c) a Portal conversation opens and SSE events arrive without errors.
- [x] 5.5 From repo root: `just gate-pr` — green (preflight + fmt + check + clippy + test-lib + integration-compile). Result: 850 lib tests passed, integration tests compile, all gate checks passed.

## 6. Phase 2 — Codegen retarget

- [x] 6.1 Edit `justfile` `typegen` recipe. Current:
  ```
  typegen:
      cargo run --bin openapi-spec > /tmp/spacebot-openapi.json
      cd interface && bunx openapi-typescript /tmp/spacebot-openapi.json -o src/api/schema.d.ts
  ```
  Replace with (drops the `cd interface &&` — no longer needed):
  ```
  typegen:
      cargo run --bin openapi-spec > /tmp/spacebot-openapi.json
      bunx openapi-typescript /tmp/spacebot-openapi.json -o packages/api-client/src/schema.d.ts
  ```
- [x] 6.2 Edit `justfile` `check-typegen` recipe. Current:
  ```
  check-typegen:
      cargo run --bin openapi-spec > /tmp/spacebot-openapi-check.json
      cd interface && bunx openapi-typescript /tmp/spacebot-openapi-check.json -o /tmp/spacebot-schema-check.d.ts
      diff interface/src/api/schema.d.ts /tmp/spacebot-schema-check.d.ts
  ```
  Replace with:
  ```
  check-typegen:
      cargo run --bin openapi-spec > /tmp/spacebot-openapi-check.json
      bunx openapi-typescript /tmp/spacebot-openapi-check.json -o /tmp/spacebot-schema-check.d.ts
      diff packages/api-client/src/schema.d.ts /tmp/spacebot-schema-check.d.ts
  ```
- [x] 6.3 Delete the `typegen-package` recipe from `justfile`. It is now redundant: the updated `typegen` already writes to the package.
- [x] 6.4 Run `just typegen` — verify it writes to `packages/api-client/src/schema.d.ts` (check file mtime). Verified: recipe wrote 305 KB file to the new path; interface/src/ has no `api/` subdirectory.
- [x] 6.5 Run `just check-typegen` — passes with zero diff. Exit 0.
- [x] 6.6 `git status` on `packages/api-client/src/schema.d.ts` — should show either no change (if regen produced identical output) or a reviewable diff that reflects any intervening API changes only. Result: 1-line diff (`Streams` → `Reads`) in the attachment-serve endpoint description. Pre-existing doc drift between committed schema and current `src/api/*.rs` annotations; unrelated to this migration but naturally surfaces when regen runs. Commit as part of this PR.
- [x] 6.7 Runtime assumption verified: `bunx openapi-typescript` auto-installed on first invocation at repo root (saw "Resolving dependencies / Resolved, downloaded and extracted [132] / Saved lockfile" during §6.4). Recipe works without a root-level `bun.lock` by relying on bun's default auto-install behavior.

## 7. Phase 3 — Documentation sweep

- [x] 7.1 Move `docs/design-docs/api-client-package-followup.md` → `docs/design-docs/archive/api-client-package-followup.md`. Created `archive/` subdir via `mkdir -p`, moved via `git mv`. Archive README deferred (no precedent yet; not blocking).
- [x] 7.2 Edit `interface/CLAUDE.md` "API Client (OpenAPI → TypeScript)" section: codegen destination path updated; added opening paragraph about `@spacebot/api-client` consumption; workspace declaration in opening paragraph updated.
- [x] 7.3a `## Package Managers` line 30 updated to show both `../spaceui/packages/*` and `../packages/*` workspaces; symlinks both `@spacedrive/*` and `@spacebot/*`; guard scope note updated.
- [x] 7.3b `## Key Directories` updated: `interface/` description notes workspace consumption; new `packages/` entry added describing `api-client/` as the codegen target and consumption surface.
- [x] 7.4a PROJECT_INDEX.md line 31 `src/api/` removed from interface tree; new `packages/` tree row added describing `api-client/`.
- [x] 7.4b PROJECT_INDEX.md line 178 Frontend design-docs row: `api-client-package-followup` replaced with `frontend-api-client` (the new capability spec name).
- [x] 7.4c Line 196 left untouched (different surface — Rust Axum handlers). Verified.
- [x] 7.5 Verified `docs/design-docs/conversation-settings.md` references. All 5 hits (lines 568, 599, 601, 947, 970) already use `@spacebot/api-client` and `packages/api-client/src/client.ts`; all correct post-migration. No edits required.
- [x] 7.6 CHANGELOG.md `## Unreleased` → `### Changed` entry added after the `cluster-deploy` skill bullet, before the `### Removed` section. Writing-guide compliant: no em-dashes in prose, direct voice.
- [x] 7.7 Ran the grep. Remaining hits classified: (a) design docs (`docs/design-docs/*.md`) — historical/proposal-state records referencing the pre-migration path; not in scope for this PR (will be touched organically as related changes land). (b) `.claude/rules/api-handler.md` — **live rule doc, updated** (codegen path + verification check). (c) `.scratchpad/backups/...` — our own backups, expected. (d) `openspec/changes/activate-api-client-package/...` — this proposal's own artifacts, expected. (e) `docs/design-docs/archive/api-client-package-followup.md` — just archived, historical, do not edit.

## 8. Final gate

- [x] 8.1 `just gate-pr` — green at §5.5 before Phase 2/3 edits. Phase 2 touched only `justfile` recipes (non-Rust, non-test code); Phase 3 touched only markdown + `.github/workflows/release.yml`. Zero Rust source changes since §5.5, so the earlier gate result stands. `just fmt-check` re-run here post-edits (exit 0). Full gate-pr re-run deferred to pre-push per user's disk-budget request (earlier run already rebuilt target/ from scratch once at ~15 GB; a redundant second rebuild would add no signal the rules don't already cover).
- [x] 8.2 `cd interface && bun run build` — green. Viable `dist/` produced, 3.34s. Re-run after all Phase 3 edits.
- [x] 8.3 `git status` clean of `.scratchpad/backups/`. The backup dir is covered by the blanket `.gitignore` entry at line 46; zero staged references confirmed via `git status --short | grep scratchpad` returning empty.
- [ ] 8.4 Review the diff once end-to-end before pushing. PR title: `feat(api-client): activate @spacebot/api-client package and migrate interface consumers`. **User action.**
- [ ] 8.5 Open PR with description summarizing: (a) the three phases, (b) the 85-file rename pattern for reviewer skim, (c) the backup location (`.scratchpad/backups/api-client-activation/`, gitignored), (d) link to this OpenSpec change directory, (e) note that the capability spec at `openspec/changes/activate-api-client-package/specs/frontend-api-client/spec.md` defines the new contract, (f) cross-link to the audit cycle source at `.scratchpad/2026-04-18-AUDIT-INDEX.md` (Wave 2, W2-PR3). **User action.**

## 9. Post-merge

- [ ] 9.1 Archive the change: invoke `/openspec-archive-change` per the standard four-step lifecycle. This moves this directory to `openspec/changes/archive/2026-04-19-activate-api-client-package/` (archive retains the date prefix per convention) and merges the capability spec into `openspec/specs/frontend-api-client/spec.md`.
- [ ] 9.2 Update `.scratchpad/2026-04-18-AUDIT-INDEX.md` if still tracked: mark the W2-PR3 carve-out as `✅ MERGED` with the commit SHA and one-line scope summary.
