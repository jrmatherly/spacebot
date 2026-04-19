# spaceui/ Sync & Provenance

This document describes the discipline for maintaining the `spaceui/` directory, which is a clone (**not** a git fork) of upstream SpaceUI. It is the checked-in source of truth, not a scratchpad.

## Purpose

The `spaceui/` directory is Spacebot's vendored copy of the SpaceUI design system — unlike `spacedrive/`, this one is a **runtime build dependency**. The frontend at `interface/` resolves `@spacedrive/*` imports through the bun workspace protocol to `spaceui/packages/*`. If this directory drifts silently from a working upstream, the frontend build breaks. This document defines:

1. How the tree got here (provenance)
2. What we've changed since (LOCAL_CHANGES)
3. How the workspace protocol prevents silent npm fallbacks
4. How to pull a specific upstream feature when we want one (cherry-pick recipe)

## Provenance

| Fact | Value |
|---|---|
| Upstream project | `spacedriveapp/spaceui` on GitHub |
| Reference snapshot | `~/dev/spaceui` (a local clone used as "what we started from") |
| Reference snapshot date | Approximately 2026-04-14 (see `stat ~/dev/spaceui/package.json`) |
| Upstream commit at snapshot time | **Unknown.** `~/dev/spaceui` has a stale `.git/` directory but no working remote config. |
| In-tree path | `spaceui/` |
| In-tree workspace manager | Bun (own workspace, own lockfile at `spaceui/bun.lock`) |
| Consumption model | `interface/package.json` declares `"workspaces": ["../spaceui/packages/*"]` and imports `@spacedrive/{ai,explorer,forms,primitives,tokens}` as `workspace:*` |
| Icons package | `@spacedrive/icons` is vendored and built but not yet imported by `interface/src/` (see [Open items](#open-items)) |

## LOCAL_CHANGES (as of 2026-04-16)

Unlike `spacedrive/`, this tree has meaningfully diverged from its reference clone. We did substantial refactors for breaking-change migrations and project-specific hygiene.

### Intentional additions

| File | Purpose |
|---|---|
| `spaceui/CLAUDE.md` | Spacebot-specific instructions for AI assistants working in this tree |
| `spaceui/.storybook/shims.d.ts` | TypeScript shim for storybook's module resolution (local need) |
| `spaceui/.storybook/tsconfig.json` | Local TS config for storybook (split from root) |
| `spaceui/examples/showcase/src/vite-env.d.ts` | Vite env types for the local showcase app |
| `spaceui/packages/icons/README.md` | Icons package docs (upstream may also have this; verify on next sync) |

### Documentation rewrites

| File | Note |
|---|---|
| `spaceui/README.md` | Spacebot-flavored intro; preserves the "shared design system" framing but references our integration model |
| `spaceui/CONTRIBUTING.md` | Rewrites to reflect our workflow (bun workspace protocol, not the upstream link-packages script) |
| `spaceui/INTEGRATION.md` | Describes how `interface/` consumes the packages; upstream version targets a generic consumer |
| `spaceui/docs/COMPONENT-AUDIT.md` | Updated component inventory reflecting our refactors |
| `spaceui/docs/REPO_SUMMARY.md` | Updated to describe our 6-package layout |
| `spaceui/docs/SHARED-UI-STRATEGY.md` | Spacebot-flavored strategy discussion |
| `spaceui/docs/TAILWIND-V4-MIGRATION.md` | Post-migration record of the Tailwind v3 → v4 upgrade we did |

### Configuration & build changes

| File | Note |
|---|---|
| `spaceui/package.json` | Dependency upgrades and metadata edits |
| `spaceui/tsconfig.base.json` | Adjusted for TS 6 / stricter options |
| `spaceui/.storybook/main.ts` | Updated for storybook 10.x and local showcase paths |
| `spaceui/.storybook/package.json` | Deps aligned with upstream root's dep graph |
| `spaceui/.storybook/preview.ts` | Theme provider wiring adjusted |
| `spaceui/.storybook/storybook.css` | Style adjustments for Spacebot's design tokens |
| `spaceui/examples/showcase/package.json` | Dep upgrades |
| `spaceui/examples/showcase/src/App.tsx` | Demo app reorganized |
| `spaceui/examples/showcase/src/index.css` | CSS token imports updated |
| `spaceui/examples/showcase/vite.config.ts` | Vite config reshaped for our showcase layout |
| `spaceui/packages/icons/package.json` | Package metadata edits |
| `spaceui/packages/icons/tsconfig.json` | TS 6 alignment |
| `spaceui/packages/icons/CHANGELOG.md` | Updated with our changes |

### Build artifacts present in-tree

These are build outputs, gitignored but not in the reference clone:

- `spaceui/examples/showcase/dist/` — showcase app build output
- `spaceui/.storybook/storybook-static/` — storybook static build

### Directory-level exclusions (enforced by rsync, by design)

| Excluded | Reason |
|---|---|
| `.git/` | Clone, not submodule |
| `.github/` | Upstream's CI would conflict. Removed in commit `6ce7867`. |
| `node_modules/` | bun deps |
| `dist/` (at package level) | Build output |

## Open items

1. **`@spacedrive/icons` is staged but not imported.** `interface/src/` has zero imports of `@spacedrive/icons` as of 2026-04-16. The package is:
   - Copied in `Dockerfile` stage 3.5 (line 48)
   - Enumerated in `flake.nix` (line 92)
   - Present in `spaceui/packages/icons/` with full source

   Decision 2026-04-17: keep icons wired in; revisit when a concrete importer lands or removal pressure returns. See the SpaceUI hygiene PR (#52) for the reasoning.

2. **`@spacedrive/*` npm scope not renamed.** Per 2026-04-16 self-reliance decision, the rename to `@spacebot/*` is deferred to a future session. Interim guard (see recommendation #3 in the self-reliance doc) will block silent npm fallbacks.

## Workspace protocol as self-reliance guarantee

`interface/package.json` declares:

```json
"workspaces": ["../spaceui/packages/*"],
"dependencies": {
  "@spacedrive/ai":         "workspace:*",
  "@spacedrive/explorer":   "workspace:*",
  "@spacedrive/forms":      "workspace:*",
  "@spacedrive/primitives": "workspace:*",
  "@spacedrive/tokens":     "workspace:*"
}
```

The `workspace:*` protocol is a hard guarantee. Bun resolves these packages from `../spaceui/packages/*` locally. It does **not** fall back to the npm registry. If a workspace member is missing, `bun install` fails loudly.

The one failure mode: if someone edits `interface/package.json` and removes the `workspaces` declaration, or changes `workspace:*` to a version range like `^0.2.3`, bun will silently fetch the upstream `@spacedrive/*` package from npm — overwriting our local customizations with no warning. This is why the scope rename matters eventually, and why the interim guard exists.

## Cherry-pick recipe

Per the 2026-04-16 self-reliance decision: we do not re-sync en masse. Manual cherry-picks only.

### Manual cherry-pick workflow

```bash
# 1. Identify the file in the reference clone
FILE="packages/primitives/src/Button.tsx"

# 2. Diff reference vs in-tree (many of our files have diverged; this is normal)
/usr/bin/diff ~/dev/spaceui/$FILE spaceui/$FILE

# 3. Review upstream; decide if we want it wholesale or partial
less ~/dev/spaceui/$FILE

# 4. Apply — copy file or hand-merge with our existing customizations
cp ~/dev/spaceui/$FILE spaceui/$FILE

# 5. Validate both the spaceui typecheck/build and the downstream interface build
cd spaceui && bun install && bun run typecheck && bun run build
cd ../interface && bun install && bun run build

# 6. Record the lift in this file's LOCAL_CHANGES table with a reason line
```

### Validation before committing a cherry-pick

Minimum gate (not yet wired into `just gate-pr`):

```bash
cd spaceui && bun run typecheck && bun run build
cd ../interface && bun run build
```

The `just spaceui-gate` recipe (landed in PR #52) wraps these two commands plus the workspace-protocol + vite-dedupe guards.

## Hold-out list (never accept upstream overwrites for these)

Files guaranteed to contain Spacebot-specific content. A future rsync/cherry-pick **must not** blindly overwrite them:

- `spaceui/CLAUDE.md` — our addition
- `spaceui/README.md` — Spacebot-flavored
- `spaceui/CONTRIBUTING.md` — describes our workflow
- `spaceui/INTEGRATION.md` — describes our consumption model
- `spaceui/docs/*.md` — all four have been rewritten for our project
- `spaceui/.storybook/shims.d.ts` — our addition
- `spaceui/.storybook/tsconfig.json` — our addition
- `spaceui/examples/showcase/src/vite-env.d.ts` — our addition
- This file (`spaceui/SYNC.md`) — Spacebot-side discipline

If any of these get touched by an upstream pull, re-verify and restore.

## Files of record

| File | Purpose |
|---|---|
| `interface/package.json` | `workspaces: ["../spaceui/packages/*"]` + 5× `workspace:*` dep entries |
| `interface/vite.config.ts` | Dedupes shared deps (react, framer-motion, sonner, clsx, cva) |
| `interface/src/styles.css` | Imports CSS from `@spacedrive/tokens` |
| `Dockerfile` (stage 3.5) | Copies all 6 spaceui packages into the build context |
| `flake.nix` | Enumerates spaceui source paths in the frontend derivation |
| `justfile` (`spaceui-*` recipes) | Retired helpers — workspace protocol handles linking |
| `openspec/specs/spaceui-integration/spec.md` | Structural spec for SpaceUI in-tree integration |
| `~/dev/spaceui/` | Reference snapshot (has stale `.git/` but no working remote) |

## Changelog

| Date | Change |
|---|---|
| 2026-04-16 | First draft authored. |
| 2026-04-17 | Landed at `spaceui/SYNC.md` as part of the SpaceUI hygiene PR. |
