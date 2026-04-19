## Design Decisions

This document captures the four locked decisions from explore-mode (`/openspec-explore` on 2026-04-19), the evidence that grounded each, and the alternatives considered.

---

### D1. Scope shape: lift-and-shift the hand-rolled client

**Decision:** move `interface/src/api/client.ts`, `types.ts`, and `schema.d.ts` into `packages/api-client/src/` verbatim. Replace the package's existing `client.ts` (openapi-fetch stub, 291 lines) and `events.ts` (aspirational SSE catalog, 164 lines) with the interface's hand-rolled versions. Preserve implementation semantics; change only the import path surface.

**Evidence:**

The package and the interface currently hold **two different implementations of the same boundary**, not duplicates:

| File | `interface/src/api/` | `packages/api-client/src/` |
|---|---|---|
| `client.ts` | 2,781 lines — hand-rolled `api` export, `fetchJson`, per-endpoint methods, inline SSE event types | 291 lines — `openapi-fetch` generic client factory, token-via-localStorage |
| `types.ts` | 511 lines — type re-exports from schema | 1 line — `export * from "../../../interface/src/api/types"` (proxy shim) |
| `schema.d.ts` | 10,382 lines — committed generated output | 1 line — `export * from "../../../interface/src/api/schema"` (proxy shim) |
| `events.ts` | (none — SSE event types inlined into `client.ts`) | 164 lines — extracted SSE event catalog |
| `index.ts` | (none) | 3 lines — `export * from ./client + ./types + ./events` |

Git history is a dead end: `git log --follow` on both sides returns a single `0a12f99 initial commit`. The repo was squash-rebased at some point; authorship trail is gone. We have to reason from content, not history.

The package was not trimmed-by-accident. It was an **aspirational v2** proposing a different consumption style (generic typed wrapper via `openapi-fetch<paths>()`) that was never adopted by `interface/`. The interface kept its hand-rolled 2,781-line client because the grep-audit shows 85 files using it across portal, settings, org-graph, workers, top-bar, setup-banner, and dialogs. All of those call sites interact with endpoint-specific methods that `openapi-fetch` would not provide ergonomically without wrappers.

**Alternatives considered:**

- **(B) Adopt the openapi-fetch model.** Rewrite the 85 interface call sites to use `createClient<paths>()`. Effectively a rewrite, not a migration. Violates the audit's locked decision ("Option A — ship the follow-up," which was "complete the extraction," not "re-architect").
- **(C) Hybrid — adopt openapi-fetch for new calls, keep hand-rolled for existing.** Two coexisting styles indefinitely. Rejected: creates contributor confusion and doubles the surface future migrations must reckon with.

**Consequence:** the openapi-fetch model is preserved in git history but not in the live tree. If the team later wants to migrate to that style, it is its own separate proposal with its own scope (rewrite 85 call sites + the package client). Keeping that future option open is one click of `git show 0a12f99 -- packages/api-client/src/client.ts` away.

**Inter-file dependencies survive the move cleanly.** The three files being relocated (`client.ts`, `types.ts`, `schema.d.ts`) contain these relative imports among themselves: `types.ts:2` imports `./schema`, `client.ts:27` imports `./types`, `client.ts:89` + `:98` + `:103` import `./types`, and `client-typed.ts:2` imports `./schema` (deleted, so the dangling reference dies with it). Because all four files move together into `packages/api-client/src/` and preserve their `./` relative-paths, no rewrite of internal imports is required. This is a property of the lift-and-shift shape; verify post-move via `bunx tsc --noEmit`.

---

### D2. Package name: `@spacebot/api-client`

**Decision:** rename from `@spacedrive/api-client` to `@spacebot/api-client`.

**Evidence (reference graph):**

Live references to **`@spacedrive/api-client`** (old name):
- `packages/api-client/package.json:2` (the one technically-binding reference).

Live references to **`@spacebot/api-client`** (new name):
- `CHANGELOG.md:74` (v0.1.0 release note for package creation).
- `docs/design-docs/spaceui-migration.md:344`.
- `docs/design-docs/api-client-package-followup.md` (3 occurrences).
- `docs/design-docs/conversation-settings.md` (4 occurrences).
- `spaceui/docs/SHARED-UI-STRATEGY.md` (2 occurrences).

Tally: 1 binding reference to the old name, 10+ doc references to the new name. The tree is already talking about the package as `@spacebot/*`; only the package.json disagrees.

**Why the package was originally named `@spacedrive/`:**

The sibling packages under `spaceui/packages/*` (`forms`, `explorer`, `primitives`, `ai`, `icons`, `tokens`) all use `@spacedrive/` because `spaceui/` is a vendored fork of the Spacedrive UI library. The namespace preserves upstream cherry-pick compatibility (`spaceui/SYNC.md` discipline). The api-client was named by copy-paste from that convention during the v0.1.0 extraction.

But `packages/api-client/` is not vendored from upstream Spacedrive. It targets the Spacebot daemon's REST API (port 19898) and its Utoipa-generated OpenAPI schema. There is no upstream to sync with; the namespace does not need preserving.

**Alternatives considered:**

- **Keep `@spacedrive/api-client`.** Preserves the theoretical option of sharing the client with Spacedrive consumers. The audit doc notes: "Spacedrive integration shipped without it." The original justification no longer applies.
- **`@spacebot-internal/api-client` or similar.** Over-prefixed. `@spacebot/` is cleaner and establishes the namespace precedent for future internal packages (e.g., a skills SDK, a memory-schema package).

**Consequence:** `@spacebot/` becomes a second workspace scope alongside `@spacedrive/`. This is the first package in that scope; future internal packages should use it too.

**Workspace-protocol guard gap (must fix in this change).** `scripts/check-workspace-protocol.sh:30,35` filters packages on the literal string `@spacedrive/`. It does **not** validate `@spacebot/*` deps. Left unchanged, the guard would let a future edit silently replace `"@spacebot/api-client": "workspace:*"` with `"@spacebot/api-client": "^1.0.0"` and fall back to a nonexistent npm package at install time. The filter MUST be extended to match both scopes. Proposed edit at two call sites in the script: change the anchor pattern from `"@spacedrive/` to match either scope (BRE: `'"@\(spacedrive\|spacebot\)/'`; ERE: `'"@(spacedrive|spacebot)/'`).

---

### D3. `client-typed.ts` disposition: delete as dead code

**Decision:** delete `interface/src/api/client-typed.ts` during the Phase 1 migration. Do not carry it into the package.

**Evidence:**

The file is 24 lines, exports `getClient()` (returning `createClient<paths>()`) and `paths` type. It is functionally equivalent to the package's current `client.ts` — both build `openapi-fetch` wrappers on top of the generated `paths` type.

Two orthogonal searches confirmed zero importers:
- `grep -rn "from.*client-typed" interface/src/` returned no matches.
- `grep -rn "client-typed" interface/` returned only the file itself.

The file appears to be an experimental second client path that was staged alongside the hand-rolled one and never adopted. The package's `client.ts` already captures the same idea if anyone wants it back.

**Alternatives considered:**

- **Bring `client-typed.ts` into the package alongside `client.ts`.** Would produce two parallel client APIs in the same package, no documentation distinguishing when to use which, and no live importer to validate either. Rejected.
- **Keep in `interface/src/api/` and leave the directory non-empty post-migration.** Conflicts with D1's "remove the directory entirely" step and preserves dead code. Rejected.

**Consequence:** the file is backed up to `.scratchpad/backups/api-client-activation/client-typed.ts` before deletion, per the user's backup discipline. The same discipline applies to the four package stub files being replaced (see `tasks.md` §1 for the full list).

---

### D4. Migration mechanics: bulk find-and-replace in one commit

**Decision:** rename `@/api/client` → `@spacebot/api-client/client` and `@/api/types` → `@spacebot/api-client/types` across all 85 affected files in a single commit. Do not migrate incrementally.

**Evidence:**

- **85 files, 2 distinct import patterns.** A scan of `interface/src/` surfaced only two import paths in use: `"@/api/client"` (majority) and `"@/api/types"` (minority). Substitution is purely mechanical.
- **TypeScript compiler is the net.** After the rename, `bunx tsc --noEmit` surfaces every misrouted import synchronously. There is no silent-drift window where a subset of the codebase is half-migrated and still appears to build.
- **Per-batch commits add bookkeeping tax with no safety gain.** At this scope, "migrate one file, build, verify, repeat 85 times" does not protect against a failure mode that bulk does not. Bulk fails loudly and quickly; incremental fails loudly and quickly too, but 85 times as slowly.
- **Atomic git blame.** One commit titled "migrate interface to consume @spacebot/api-client" reads better than 10 commits all saying the same thing, from a future-blame-archaeology perspective.

**Alternatives considered:**

- **Incremental per-batch (audit doc's original recommendation).** The audit recommendation assumed unique migration decisions per file. There are really only 2 distinct substitutions. Rejected.
- **Scripted migration (e.g., a codemod).** Overkill for 2 literal string replacements. A `sed` one-liner plus manual verification via tsc is sufficient.

**Consequence:** the migration commit is large (85 files) but shallow (single-line edits). The PR description explicitly calls this out so reviewers can skim the diff for pattern conformance rather than read every hunk.

---

## Execution Flow Diagrams

### Before

```
                   interface/src/api/                  ← source of truth
                   ├─ client.ts       (2,781L) ────────┐
                   ├─ client-typed.ts    (24L)         │
                   ├─ types.ts          (511L)         │ used by 85 files
                   └─ schema.d.ts    (10,382L)         │ via @/api/*
                             ▲                         │
                             │                         │
    just typegen writes here│                         │
                             │                         │
                             │                         ▼
                   interface/src/**/*.{ts,tsx}  (85 files)
                             │
                             ▼
                   packages/api-client/              ← stub, unused
                   ├─ package.json  @spacedrive/api-client
                   └─ src/
                      ├─ client.ts    (291L)  openapi-fetch stub
                      ├─ events.ts    (164L)  aspirational SSE catalog
                      ├─ types.ts       (1L)  proxy shim → interface
                      ├─ schema.d.ts    (1L)  proxy shim → interface
                      └─ index.ts       (3L)
```

### After Phase 1

```
                   packages/api-client/              ← source of truth
                   ├─ package.json  @spacebot/api-client
                   └─ src/
                      ├─ client.ts     (2,781L)  ◀── moved from interface
                      ├─ types.ts        (511L)  ◀── moved from interface
                      ├─ schema.d.ts  (10,382L)  ◀── moved from interface
                      └─ index.ts          (2L)  re-exports client + types
                               ▲
                               │
                               │ imported by 85 files
                               │ via @spacebot/api-client/{client,types}
                               │
                   interface/src/**/*.{ts,tsx}  (85 files)
                               │
                   interface/src/api/  ◀── directory removed
                               │
                   .scratchpad/backups/api-client-activation/
                   ├─ packages-api-client-client.ts      (openapi-fetch stub)
                   ├─ packages-api-client-events.ts      (aspirational SSE)
                   ├─ packages-api-client-types.ts       (proxy shim)
                   ├─ packages-api-client-schema.d.ts    (proxy shim)
                   └─ interface-api-client-typed.ts      (dead code)
```

### After Phase 2

```
                   packages/api-client/src/schema.d.ts  ◀─── just typegen writes here
                                                              (recipe updated)
                                                              (just check-typegen diffs against this)
```

---

## Risk Analysis

### Risk matrix

| Risk | Severity | Likelihood | Mitigation |
|---|---|---|---|
| TypeScript compile break after bulk rename | High | Low | `bunx tsc --noEmit` + `bun run build` run after the rename and before commit. The TS compiler catches every misrouted import synchronously. |
| `bun install` fails to symlink `@spacebot/api-client` (two distinct parent-directory globs in `workspaces`) | High | Low | Adding `"../packages/*"` alongside `"../spaceui/packages/*"` is two sibling-directory globs in one array. Bun supports this, but verification is explicit: task §3.4 runs `bun install` and task §3.5 asserts the symlink exists. **Stop-gate**: if the symlink is missing, do not proceed to the 85-file rename. |
| Codegen writes to the wrong location after recipe change | Medium | Low | `just typegen && just check-typegen` is an atomic verify step. If `check-typegen` diffs cleanly after `typegen`, the path is right. |
| Workspace-protocol guard lets `@spacebot/*` deps silently fall back to npm | Medium | Medium | The existing `scripts/check-workspace-protocol.sh` only filters `@spacedrive/*`. Without extending it, a future edit to `interface/package.json` replacing `"workspace:*"` with a semver range would pass the guard. Fix: extend the filter regex to match both scopes (task §3.6). |
| 85-file rename introduces a typo in one import | Medium | Low | The rename is a literal `sed` substitution; the TS compiler catches any misroutes. Reviewer guidance in the PR description tells the reviewer to skim for pattern conformance. |
| Desktop app build breaks | Low | Low | `desktop/` consumes `interface/` transitively per `desktop/CLAUDE.md` (runs `bun install && bun run build` in `interface/` via `beforeBuildCommand`). As long as interface builds, desktop builds. Tasks §5.2 verifies. |
| Async/state-path concerns | None | — | Per `.claude/rules/async-state-safety.md`: this is a packaging refactor. No event-loop, worker-lifecycle, or state-machine code is touched. No race-window analysis required. |

### Non-risks

- **Backwards compatibility.** `interface/` is the sole consumer. Migrating it in place has no downstream effect.
- **Runtime regressions.** Wire format, request semantics, and SSE event shapes are byte-identical. The move is purely in the import graph.
- **OpenAPI schema drift.** The schema file content does not change during the move. Codegen continues to produce identical output; only the target path changes.

---

## Workspace Protocol Notes

`interface/package.json` currently declares:

```json
"workspaces": ["../spaceui/packages/*"]
```

Phase 1 adds `"../packages/*"` — note the **relative-to-interface** path, not `./packages/*` or `/packages/*`. The `interface/` dir is one level below the repo root; `../packages/*` resolves to the repo's `packages/` directory, which contains `api-client/`.

The existing `scripts/check-workspace-protocol.sh` preinstall hook validates that every `workspace:*` dep uses the correct protocol — **but only for `@spacedrive/*` packages** (line 30 and line 35 both filter on that literal string). This change extends the filter to match both `@spacedrive/` and `@spacebot/` scopes so that `@spacebot/api-client` gains the same silent-fallback protection. See `design.md` §D2 and tasks §3.6 for the exact edit.

---

## Post-Migration Verification

Six validation checks span all three phases. Tasks §5 lists them as explicit gates:

1. `bun install` in `interface/` completes cleanly; `node_modules/@spacebot/api-client` is a symlink to `../packages/api-client`.
2. `bunx tsc --noEmit` in `interface/` passes.
3. `bun run build` in `interface/` produces a viable `dist/`.
4. `just typegen` writes `packages/api-client/src/schema.d.ts` (not `interface/src/api/schema.d.ts`).
5. `just check-typegen` passes against the new path.
6. `just gate-pr` green (formatting, Rust compile, clippy, unit tests, integration compile, preflight).

If any check fails, the diagnostic path is clear — TS errors point at the exact file, and the rename's `sed` substitution is easy to re-run. No hidden failure modes.

---

## Rollback Plan

Full revert is `git revert <merge-sha>`. Safe because:

- No schema migrations (zero SQL changes).
- No runtime state changes (wire format, SSE events, request logic all byte-identical).
- No hot-reload config changes (all touched files are build-time artifacts).
- No dependency changes that would require a `bun install` cycle to re-pin lockfiles.

Partial revert (restoring only one file's imports) is not meaningful at this scope. The 85-file rename is atomic: either the package is the source of truth, or the `interface/src/api/` directory is. There is no coherent intermediate state.

If a regression surfaces **after** merge and **before** archive:

1. `git revert <sha>` on `main`.
2. Restore the backed-up files from `.scratchpad/backups/api-client-activation/` locally if the reverter needs to inspect the pre-migration state (post-revert, git already holds the originals via the revert commit).
3. Update `.scratchpad/2026-04-18-AUDIT-INDEX.md` W2-PR3 entry to reflect the reverted state.
4. Keep the openspec change in `openspec/changes/` (do not archive). Revise based on what broke, then re-attempt.
