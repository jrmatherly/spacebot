## Why

The SpaceUI design system lives in a separate sibling repository (`../spaceui/`), requiring contributors to clone two repos into specific relative directories. The interface already imports from 5 `@spacedrive/*` packages across 63 source files, but the Vite config hardcodes `../../spaceui/packages` as the resolution path. This breaks CI builds (Docker, Nix, GitHub Actions) because they construct the frontend in sandboxed contexts that don't include SpaceUI source. Moving SpaceUI in-tree eliminates the two-repo workflow and makes the project self-contained.

## What Changes

- Move SpaceUI from external sibling repo into `spaceui/` at the project root
- Rewire Vite aliases, Tailwind `@source` directives, and justfile recipes to reference `../spaceui/` instead of `../../spaceui/`
- Add SpaceUI source to Docker build context (`Dockerfile` COPY), Nix build sandbox (`flake.nix` frontendSrc), and `build.rs` watch paths
- Add `spaceui/packages/**` to CI trigger paths so interface typecheck runs on SpaceUI changes
- Update project documentation (CLAUDE.md, CONTRIBUTING.md, README.md, CODEOWNERS) to reflect in-tree location
- Add Vite `dedupe` entries for shared dependencies (framer-motion, sonner) to prevent dual-version resolution
- Add SpaceUI build artifacts to `.gitignore` and `.dockerignore`

## Capabilities

### New Capabilities
- `spaceui-in-tree`: Integration of the SpaceUI design system as an in-tree subdirectory with correct build system wiring across Docker, Nix, CI, and local development

### Modified Capabilities

## Impact

- **15 files modified** in Phase 1: vite.config.ts, styles.css, justfile, Dockerfile, .dockerignore, flake.nix, nix/default.nix, build.rs, interface-ci.yml, CODEOWNERS, CLAUDE.md, CONTRIBUTING.md, README.md, .gitignore, plus the directory move
- **Build systems**: Docker, Nix, and `cargo build` (via build.rs) all need SpaceUI in their source sets
- **CI**: interface-ci.yml trigger paths must include `spaceui/packages/**`
- **Dependencies**: No new npm/cargo dependencies. SpaceUI's `node_modules/` remains independent from the interface's
- **No breaking changes**: The interface continues to resolve `@spacedrive/*` from source via Vite aliases. The import paths in 63 source files do not change.
