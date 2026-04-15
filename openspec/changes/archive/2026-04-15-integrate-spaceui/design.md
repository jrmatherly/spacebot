## Context

The SpaceUI design system provides 5 `@spacedrive/*` packages (tokens, primitives, forms, ai, explorer) consumed by the Spacebot interface across 63 source files. The interface resolves these from TypeScript source via Vite aliases in `vite.config.ts`, not from published npm packages.

Currently, SpaceUI must be cloned as a sibling directory (`../spaceui/` relative to the Spacebot repo root). Three files hardcode this path: `interface/vite.config.ts` (line 6), `interface/src/styles.css` (lines 25-28), and `justfile` (lines 27-48). Three sandboxed build systems — Docker, Nix, and GitHub Actions — cannot resolve SpaceUI because they only see the Spacebot repo checkout.

## Goals / Non-Goals

**Goals:**
- Single-clone developer experience: `git clone spacebot` provides everything needed
- All build systems (Docker, Nix, `cargo build`, CI) can resolve `@spacedrive/*` packages
- SpaceUI retains its internal Turbo/Bun workspace structure unchanged
- No changes to the 63 interface source files that import from `@spacedrive/*`

**Non-Goals:**
- Merging SpaceUI packages into Spacebot's `packages/` directory (requires root-level workspace config)
- Creating a root-level Bun workspace spanning both SpaceUI and the interface
- Upgrading SpaceUI dependency versions (sonner v2, @dnd-kit v10, etc.) — deferred to Phase 7
- Adding `@spacedrive/icons` package wiring (unused, no interface imports exist)

## Decisions

### 1. Place SpaceUI at `spaceui/` in the project root

**Rationale:** One level below root keeps paths simple (`../spaceui/` from `interface/`). SpaceUI's Turbo/Bun workspace config remains self-contained. No root-level monorepo tooling needed.

**Alternative considered:** Merge into `packages/` alongside `api-client/`. Rejected because it would require a root `package.json` with Bun workspaces spanning both the Rust project and all JS packages.

**Alternative considered:** Git submodule. Rejected because submodules add auth complexity, detached HEAD confusion, and version pinning friction. Since Spacebot is a detached fork from the same upstream, direct inclusion is simpler.

### 2. Copy-based Docker integration (individual COPY per package)

**Rationale:** `COPY spaceui/packages/tokens/ spaceui/packages/tokens/` (repeated per package) avoids pulling `node_modules/`, `.storybook/`, and `examples/` into the Docker build context. The `.dockerignore` provides a safety net.

**Alternative considered:** Single `COPY spaceui/ spaceui/` with `.dockerignore` exclusions. Rejected because Docker COPY ignores `.dockerignore` for explicitly named paths — only the build context respects it.

### 3. Expand Nix `frontendSrc` fileset with SpaceUI entries

**Rationale:** The Nix build uses `pkgs.lib.fileset.toSource` to create a minimal source tree. Adding SpaceUI `src/` and `package.json` per package keeps the sandbox small. The `frontend` derivation changes from `src = "${frontendSrc}/interface"` to `src = frontendSrc` with `cd interface` in the build phase, so both `interface/` and `spaceui/` are accessible.

### 4. Add shared deps to Vite `dedupe` array

**Rationale:** With SpaceUI in-tree, Vite could resolve `framer-motion` from `spaceui/node_modules/` (v11) instead of `interface/node_modules/` (v12). Adding `framer-motion`, `sonner`, `clsx`, `class-variance-authority` to `dedupe` forces single-copy resolution. React is already deduped.

### 5. Use a git worktree for implementation

**Rationale:** The integration touches 15 files across build systems, CI, and documentation. Working in a worktree isolates these changes from the main workspace. If the Docker or Nix builds break during iteration, the main worktree remains clean for other work.

## Risks / Trade-offs

- **[Nix hash invalidation]** Adding files to `frontendSrc` changes the derivation hash. `just update-frontend-hash` must run after `flake.nix` changes. Mitigation: explicit verification step in tasks.
- **[Dual dependency resolution]** SpaceUI and interface maintain separate `bun.lock` files. A CVE in a shared transitive dependency requires patching both. Mitigation: document in CONTRIBUTING.md; acceptable for now.
- **[SpaceUI upstream drift]** The upstream `spacedriveapp/spaceui` repo may diverge. Mitigation: periodic manual cherry-picks. No submodule tracking.
- **[Docker build context size]** SpaceUI source adds ~2MB to the build context. Mitigation: negligible compared to the Rust compilation layer.
- **[postcss.config.js / tailwind.config.ts stale entries]** The Nix `frontendSrc` references these files but they don't exist (Tailwind v4 uses CSS-based config). Pre-existing issue, not introduced by this change. Nix silently ignores missing fileset entries.
