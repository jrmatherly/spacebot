## Why

GitHub Dependabot reports 55 open alerts. Triage shows 7 affect spacebot-owned code; 51 are in the vendored `spacedrive/` upstream that spacebot does not compile (`[workspace] exclude = ["spacedrive"]`). Of the 7 spacebot-owned alerts, 3 are actionable upgrades (vite ×2 in spaceui dev tooling, glib ×1 in desktop) and 4 are low-severity items blocked on upstream crate updates. Addressing the actionable items now resolves real vulnerabilities without dismissing anything — spacedrive alerts remain visible as tracking signal for the planned runtime integration.

## What Changes

- Upgrade `vite` in `spaceui/`, `spaceui/.storybook/`, and `spaceui/examples/showcase/` to a version outside every range declared by GHSA-4w7w-66w2-5vf9 (target 6.4.2 or later)
- Upgrade `glib` from 0.18.5 to 0.20.x in `desktop/src-tauri/Cargo.lock`, coordinating a tauri plugin ecosystem bump if the resolver cannot advance the transitive pin in isolation
- Expand `.github/dependabot.yml` with entries for `/desktop/src-tauri`, `/packages/api-client`, `/spaceui`, `/spaceui/.storybook`, `/spaceui/examples/showcase`, `/docs` so automated update PRs arrive scoped to every shipped-code manifest
- Add a top-of-file comment in `.github/dependabot.yml` clarifying that this file controls update-PR scoping, not security-alert visibility
- Create `docs/security/deferred-advisories.md` listing the 4 upstream-blocked alerts (lexical-core #1, lru #3, rand root #15, rand desktop #18) with GHSA, severity, blocker, unblock trigger
- Reference `deferred-advisories.md` from `CONTRIBUTING.md` so the review cadence survives contributor turnover
- **Explicitly NOT changing:** no Dependabot or CodeQL API dismissal calls; no action on `spacedrive/**` alerts; no CodeQL workflow migration; no changes to application runtime behavior

## Capabilities

### New Capabilities
- `spaceui-frontend-dependencies`: Version currency for frontend packages under the spaceui workspace (storybook, showcase, and workspace root). Separate from the existing `frontend-dependencies` spec which covers `interface/` and `docs/`.
- `desktop-rust-dependencies`: Version currency for Rust crate dependencies in the standalone `desktop/src-tauri/` Cargo project. Separate from the existing `rust-dependencies` spec which covers the root `Cargo.toml` workspace.

### Modified Capabilities
- `security-audit`: Adds requirements for (a) deferred-advisories documentation, (b) Dependabot update-PR config coverage of every shipped-code manifest, (c) non-dismissal policy for open advisories blocked on upstream, (d) post-merge verification for vite and glib alerts, (e) spacedrive-alert re-triage trigger at integration time

## Impact

- `.github/dependabot.yml` — 6 new `updates:` entries, top-of-file scope comment
- `desktop/src-tauri/Cargo.lock` — glib 0.18 → 0.20 (and possibly `desktop/src-tauri/Cargo.toml` tauri plugin version bumps)
- `spaceui/bun.lock` — vite pin advances off 5.4.21 (single workspace lockfile; `.storybook/` and `examples/showcase/` are workspace members without their own lockfiles)
- `spaceui/package.json`, `spaceui/.storybook/package.json`, `spaceui/examples/showcase/package.json` — vite version range updated; storybook version may need to bump if 8.6.18 is not vite-6-compatible
- `docs/security/deferred-advisories.md` — new file
- `CONTRIBUTING.md` — reference to deferred-advisories doc
- GitHub Security dashboard — alerts #17 (glib), #35 (vite storybook), #36 (vite showcase) transition to `state: fixed` after merge + rescan; all other alerts remain open and unchanged
- Build surface: unchanged
- Runtime behavior: unchanged (glib and vite are transitive/dev-tooling deps)
- API: no changes
- Dependency policy: establishes the convention that upstream-blocked advisories are tracked in-repo, not dismissed
