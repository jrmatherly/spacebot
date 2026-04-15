## Context

Spacedrive is a 15-crate Rust workspace (236K Rust LOC) with frontend apps (desktop, web, mobile), a CLI, and a headless server. It integrates with Spacebot via HTTP API + SSE on port 19898. The two projects share zero Rust crates. Spacedrive uses Rust edition 2021 (MSRV 1.81, toolchain `stable`), while Spacebot uses edition 2024 (MSRV 1.94, toolchain `1.94.1`). They have incompatible versions of axum (0.7 vs 0.8), reqwest (0.12 vs 0.13), and lancedb (0.15 vs 0.27).

## Goals / Non-Goals

**Goals:**
- Single-clone developer experience for the Spacebot + Spacedrive stack
- Adjacent integration design documents (`spacedrive/docs/core/design/spacebot-*.md`)
- Explicit workspace isolation guard preventing accidental Cargo nesting
- Spacedrive builds independently with its own toolchain, formatter, and dependencies

**Non-Goals:**
- Merging Cargo workspaces (incompatible deps, editions, and toolchains)
- Sharing Rust crate dependencies between projects
- Adding Spacedrive to Spacebot's CI pipeline (separate concern)
- Building Spacedrive as part of Spacebot's Docker image or Nix derivation
- Modifying any of Spacedrive's internal files

## Decisions

### 1. Place at `spacedrive/` in the project root

Same pattern as SpaceUI (`spaceui/`). Keeps all three projects (Spacebot, SpaceUI, Spacedrive) at the same directory level. Spacedrive's internal structure (Cargo workspace, Bun workspaces, Turbo) stays intact.

### 2. Add `[workspace] exclude` to Spacebot's Cargo.toml

Spacebot currently has no `[workspace]` section. This isolation is implicit and fragile — if anyone adds `[workspace]` (e.g., for workspace lints), Cargo would auto-discover `spacedrive/Cargo.toml`. Adding `[workspace] exclude = ["spacedrive", "spaceui", "desktop"]` makes isolation explicit.

**Alternative considered:** Rely on the implicit behavior. Rejected because a future `[workspace]` addition would silently break the build with confusing nesting errors.

### 3. Exclude `.github/` from the copy

Spacedrive's `.github/` contains 6 CI workflows, a CODEOWNERS file, and FUNDING.yml. These are irrelevant inside Spacebot (GitHub Actions only reads `.github/workflows/` at the repo root). Including them adds 2.3 MB of confusing dead content and creates a second CODEOWNERS file with different ownership rules.

### 4. Exclude entire `spacedrive/` from .dockerignore

Unlike SpaceUI (which Vite resolves from source during the Docker frontend build), Spacedrive is not needed for Spacebot's Docker image at all. Excluding the entire directory keeps the Docker build context small.

### 5. Do NOT add Spacedrive to CI trigger paths

Spacedrive changes don't affect Spacebot's typecheck or build. Adding trigger paths would run Spacebot's CI on irrelevant changes. A future integration test workflow is a separate concern.

## Risks / Trade-offs

- **[Repo size +53 MB]** Source files tracked in git. Mitigation: build artifacts excluded via .gitignore. Consider git-lfs for Spacedrive's image assets (443 SVGs, 277 PNGs) if repo performance degrades.
- **[Toolchain divergence]** Spacedrive uses `stable`, Spacebot uses `1.94.1`. Mitigation: rustup resolves `rust-toolchain.toml` per working directory. Document "always `cd spacedrive` before Cargo commands."
- **[Formatter divergence]** Spacedrive uses `hard_tabs = true`, Spacebot uses default spaces. Mitigation: `.rustfmt.toml` is per-directory; `cargo fmt --all` from root only touches Spacebot. Document in CONTRIBUTING.md.
- **[`.cargo/config.toml` inheritance]** Spacebot's `[alias]` section is visible inside `spacedrive/` builds. Current aliases are benign (only `bump`). Mitigation: document; add Spacedrive's own `.cargo/config.toml` if Spacebot's config grows.
