# SpaceUI

Independent workspace for the `@spacedrive/*` design system packages consumed by `interface/` and `desktop/`. Has its own `bun.lock`, Turborepo config, and release process. Not part of the Rust Cargo workspace.

## Package Manager

**`bun` only.** Never `npm`, `pnpm`, or `yarn`. If an agent suggests `npm install`, it's wrong. See root `CLAUDE.md` for the full policy.

Note: some `spaceui/packages/*/README.md` and `spaceui/INTEGRATION.md` files use `npm install` in example blocks because they describe how *external consumers* would install the package. For internal development against this workspace, always use `bun`.

## Packages

Six packages, all published at the same version (currently `0.2.3`):

| Package | Scope |
|---------|-------|
| `@spacedrive/tokens` | CSS design tokens + Tailwind v4 `@theme` directive |
| `@spacedrive/primitives` | 40+ Radix-based base UI components |
| `@spacedrive/forms` | React Hook Form field wrappers |
| `@spacedrive/icons` | File type icons + resolution utilities (no README) |
| `@spacedrive/ai` | AI agent interaction components |
| `@spacedrive/explorer` | File management components |

`primitives`, `forms`, `ai`, `explorer` are **linked** in `.changeset/config.json`. A version bump to one bumps all four in lockstep. `tokens` and `icons` version independently.

## Release Workflow (Changesets)

1. Make code changes.
2. `bun run changeset` creates `spaceui/.changeset/<random>.md` capturing the change.
3. PR merges the changeset. Never hand-edit `CHANGELOG.md`. Changesets drive it.
4. Release workflow runs `bun run version-packages` (consumes changesets, bumps versions, rewrites CHANGELOGs) then `bun run publish`.

If you see an unreleased `.changeset/*.md` file queued, the next release will consume it. Don't delete queued changesets.

## Tailwind v4

All packages target Tailwind v4. **No `tailwind.config.js`**. Config lives in CSS via `@theme` blocks (see `packages/tokens/src/css/theme.css`). Consumer apps `@import "@spacedrive/tokens/theme.css"` to get the design tokens as auto-generated utilities.

Canonical v4 class syntax (adopted in PR #45): `class!` not `!class`, `data-X:` not `data-[X]:`, `z-N` not `z-[N]`, `*-(--X)` not `*-[var(--X)]`, numeric spacing.

## Building

```bash
just spaceui-build    # bun install + bun run build via turbo
```

`interface/` consumes these packages via the bun workspace protocol (`workspace:*` dependencies in `interface/package.json`, with `"workspaces": ["../spaceui/packages/*"]` declared there). Running `bun install` inside `interface/` creates symlinks directly into `spaceui/packages/*`. No `bun link` step is needed.

`just spaceui-link` and `just spaceui-unlink` are retired stubs — kept for discoverability but print a deprecation notice.

Before type-checking `interface/` (`bunx tsc --noEmit`), run `just spaceui-build` so each package's `dist/index.d.ts` is current. Vite dev/build does not need this because Rolldown resolves `.tsx` source directly through the symlinks.

## Deep Dive

For component-level guidance (CVA variants, token usage, component contracts), invoke `/spaceui-dev`.
