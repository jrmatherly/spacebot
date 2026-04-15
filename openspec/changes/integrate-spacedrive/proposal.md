## Why

Spacedrive is the upstream file management platform that Spacebot integrates with via HTTP API. It provides file indexing, P2P sync, cloud volumes, data archival, and safety screening. The two projects are maintained by the same team but live in separate repositories, requiring separate clones and making coordinated development harder. Co-locating Spacedrive inside the Spacebot repo gives a single-clone developer experience and keeps the integration design documents adjacent to the code they describe.

## What Changes

- Move Spacedrive source from `.scratchpad/spacedrive/` to `spacedrive/` at the project root
- Add a `[workspace] exclude = ["spacedrive"]` guard to Spacebot's `Cargo.toml` to prevent accidental workspace nesting
- Exclude Spacedrive build artifacts from `.gitignore` (target/, node_modules/)
- Exclude the entire `spacedrive/` directory from `.dockerignore` (not needed for Spacebot Docker builds)
- Add `spacedrive/` to CODEOWNERS
- Update CLAUDE.md, CONTRIBUTING.md, and README.md to reference the in-tree location
- Exclude Spacedrive's `.github/` directory (dead CI workflows, conflicting CODEOWNERS)

**No Cargo workspace merge.** The two projects remain separate workspaces with separate dependency resolution. Integration stays HTTP-based at runtime.

## Capabilities

### New Capabilities
- `spacedrive-in-tree`: Co-location of the Spacedrive platform as a self-contained subdirectory with independent Cargo workspace, explicit workspace exclude guard, and documentation updates

### Modified Capabilities

## Impact

- **8 files modified**: Cargo.toml (workspace exclude), .gitignore, .dockerignore, CODEOWNERS, CLAUDE.md, CONTRIBUTING.md, README.md, plus the directory copy
- **Build systems**: Spacebot's Cargo, Docker, Nix, and CI are unaffected. Spacedrive builds independently with its own toolchain (`stable` vs Spacebot's `1.94.1`)
- **No new dependencies**: No Cargo crates added, no npm packages added
- **No breaking changes**: Spacebot compiles identically with or without Spacedrive present. The `[workspace] exclude` guard ensures this remains true even if the Cargo.toml structure evolves
- **Repo size**: +53 MB source (1,142 Rust files, 399 TS files, documentation, adapters). Build artifacts excluded via .gitignore
