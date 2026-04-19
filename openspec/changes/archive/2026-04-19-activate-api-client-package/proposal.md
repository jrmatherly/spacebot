## Why

`packages/api-client/` was extracted in v0.1.0 as a package boundary for Spacedrive consumers and other downstream embedders, but it shipped with **zero importers**. The Spacebot interface still imports from its local `interface/src/api/*` modules under the `@/api/*` TypeScript path alias. The deferred-follow-up doc (`docs/design-docs/api-client-package-followup.md`) has described this migration as "worth doing, not required for the first Spacedrive integration slice" since extraction. Spacedrive integration shipped (PR #54, #55, #56, #57) without ever activating the package. The deferral rationale no longer applies.

This proposal implements the W2-PR3 carve-out from the 2026-04-18 audit cycle. See `.scratchpad/2026-04-18-AUDIT-INDEX.md` → Wave 2 → W2-PR3 for the prior reasoning chain and locked user decision ("Option A — ship the follow-up").

This change activates the package: interface consumes it, OpenAPI codegen emits into it, and the legacy `interface/src/api/` directory is removed. The package becomes the single source of truth for the Spacebot REST API surface and SSE event types.

Locked during exploration (see `design.md`):
- Scope shape is lift-and-shift of the hand-rolled `interface/src/api/client.ts` into the package (**not** a rewrite to `openapi-fetch`).
- Package name changes from `@spacedrive/api-client` to `@spacebot/api-client` (matches content; matches 10+ existing doc references).
- `interface/src/api/client-typed.ts` is deleted as dead code (zero importers verified).
- Migration is bulk find-and-replace in one commit (85 files, 2 import patterns).
- Every deleted file is mirrored into `.scratchpad/backups/api-client-activation/` before removal.

## What Changes

**Phase 1 — Lift-and-shift the migration**

- `packages/api-client/package.json`: `name` changes from `@spacedrive/api-client` to `@spacebot/api-client`. `exports` block trimmed to subpath-only (`./client`, `./types`, `./schema`; no root `.` entry). See design.md §D5 for the rationale.
- `packages/api-client/src/client.ts`: replaced with the content of `interface/src/api/client.ts` (hand-rolled 2,781-line client with `fetchJson`, endpoint helpers, and inline SSE event type definitions).
- `packages/api-client/src/types.ts`: replaced with the content of `interface/src/api/types.ts` (511-line type re-export module from the generated schema).
- `packages/api-client/src/schema.d.ts`: replaced with the content of `interface/src/api/schema.d.ts` (10,382-line generated OpenAPI type declarations).
- `packages/api-client/src/events.ts`: deleted. The aspirational SSE event catalog is supplanted by the inline event types in the new `client.ts`.
- `packages/api-client/src/index.ts`: deleted. The package is subpath-only because `client.ts` and `types.ts` share ~120 overlapping named exports; a root barrel via `export * from "./client"; export * from "./types";` would surface as `TS2308` ambiguity errors on any root-path consumer.
- `interface/src/api/client.ts`, `types.ts`, `schema.d.ts`: removed. Content moved into the package.
- `interface/src/api/client-typed.ts`: removed. Dead code with zero importers.
- `interface/src/api/` directory: removed entirely after the above.
- `interface/package.json`: `workspaces` array gains `"../packages/*"` alongside the existing `"../spaceui/packages/*"`. `dependencies` gains `"@spacebot/api-client": "workspace:*"`. Enables `bun install` to symlink `@spacebot/api-client` into `interface/node_modules/`.
- `scripts/check-workspace-protocol.sh`: guard filter expanded to validate `@spacebot/*` deps in addition to `@spacedrive/*` (prevents silent npm fallback if the new package's `workspace:*` spec is ever replaced with a semver range).
- **No alias files edited.** The `@/` alias in `interface/vite.config.ts:71` and `interface/tsconfig.json:20-22` continues to resolve non-api `interface/src/*` paths. Only the 85 `@/api/*` import usages are rewritten to `@spacebot/api-client/*`.
- 85 `interface/src/**/*.{ts,tsx}` files: bulk rename of `@/api/client` → `@spacebot/api-client/client` and `@/api/types` → `@spacebot/api-client/types`.

**Phase 2 — Move codegen output**

- `justfile` `typegen` recipe: output path changes from `interface/src/api/schema.d.ts` to `packages/api-client/src/schema.d.ts`.
- `justfile` `check-typegen` recipe: diff target updated to the new path.
- `justfile` `typegen-package` recipe: removed (now redundant with `typegen`).

**Phase 3 — Documentation sweep**

- `docs/design-docs/api-client-package-followup.md`: moved to `docs/design-docs/archive/` (premise no longer applies).
- `interface/CLAUDE.md` "API Client" section: updated to reflect the new codegen destination and the package-based consumption pattern.
- `CLAUDE.md` (root): `## Key Directories` section retargeted (mention `packages/api-client/` as the live api-client home); `## Package Managers` example at line 30 updated to show the new `workspaces: ["../spaceui/packages/*", "../packages/*"]` declaration.
- `PROJECT_INDEX.md`: line 31 (interface tree: `src/api/` → `packages/api-client/`), line 178 (design-docs row: remove archived `api-client-package-followup` reference). Line 196 (Rust `src/api/` Axum handler reference) is intentionally untouched. That line refers to a different surface.
- `CHANGELOG.md`: single `### Changed` entry under `## Unreleased` following `.claude/rules/writing-guide.md` voice conventions; style precedent at `CHANGELOG.md:74` (the original v0.1.0 extraction entry).

## Capabilities

### New Capabilities

- **`frontend-api-client`**: codifies the package contract. Defines what the package owns, what it exports, how it is consumed by `interface/`, and how its generated types stay in sync with the Rust API surface via `just typegen` / `just check-typegen`.

### Modified Capabilities

- None. This change does not touch any existing spec.

## Impact

- **Code**: 85 interface files edited (bulk import rename), 3 file relocations via `git mv` (`interface/src/api/{client,types,schema.d}.ts` → `packages/api-client/src/`), 5 file deletions (4 package stubs + 1 dead `client-typed.ts`), 3 config edits (`interface/package.json`, `justfile`, `scripts/check-workspace-protocol.sh`). No Rust code changes. No alias file edits (the `@/` alias continues to resolve non-api `interface/src/*` paths unchanged).
- **APIs**: no Rust API changes. TypeScript export surface is preserved: the same symbols (`api`, type re-exports, SSE event interfaces) remain available from the package entry points.
- **Dependencies**: `@spacebot/api-client` becomes a workspace dependency of `interface/` under `workspace:*`. No new npm packages added.
- **Behavior**: zero runtime behavior change. This is a packaging refactor; the wire protocol, request logic, and event schemas are byte-identical.
- **Tests**: no new tests required. Success is verified by `bun run build` + `bunx tsc --noEmit` in `interface/` passing after the migration, plus `just check-typegen` passing with the new codegen path, plus `just gate-pr` green.
- **Migrations**: none.
- **Backups**: 5 files mirrored to `.scratchpad/backups/api-client-activation/` before deletion (gitignored, local-only, per W2-PR1 precedent).

## Out of Scope

- Migrating interface call sites to use `openapi-fetch` (the model the package originally proposed). If wanted, tracked as a separate future proposal.
- Consumption of `@spacebot/api-client` by any consumer other than `interface/` (no desktop, no spacedrive/apps, no third party). The package technically becomes consumable by any workspace member, but wiring additional consumers is out of scope here.
- Publishing `@spacebot/api-client` to npm as a public package. Remains `private: true` in `package.json`.
- Renaming cross-references to `@spacebot/api-client` in archived OpenSpec changes, the CHANGELOG, or other historical documents. Those stay as written (they reference a historical package name that is now correct).