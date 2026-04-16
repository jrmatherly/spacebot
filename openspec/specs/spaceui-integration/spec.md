# SpaceUI Integration

## Purpose
In-tree integration of the SpaceUI design system. Covers source availability, build resolution, Docker/Nix support, and CI triggers.

## Requirements

### Requirement: SpaceUI source available in project tree
The SpaceUI design system SHALL reside at `spaceui/` in the Spacebot project root, with its internal Turbo/Bun workspace structure preserved unchanged.

#### Scenario: Fresh clone contains SpaceUI
- GIVEN a developer clones the Spacebot repository
- WHEN they inspect the project root
- THEN the `spaceui/` directory exists with all package source files

#### Scenario: SpaceUI internal structure preserved
- GIVEN `cd spaceui && bun install && bun run build` is executed
- WHEN the build runs
- THEN all packages build successfully without modification to SpaceUI's `package.json`, `turbo.json`, or `tsconfig.base.json`

### Requirement: Interface resolves @spacedrive packages from in-tree SpaceUI
The interface's Vite configuration SHALL resolve all `@spacedrive/*` imports to source files under `spaceui/packages/*/src/` via path aliases.

#### Scenario: Vite dev server resolves SpaceUI components
- GIVEN `cd interface && bun run dev` is executed
- WHEN the dev server starts
- THEN `@spacedrive/*` imports resolve to `../spaceui/packages/*/src/`

#### Scenario: Vite production build resolves SpaceUI components
- GIVEN `cd interface && bun run build` is executed
- WHEN the build completes
- THEN `dist/` contains compiled SpaceUI components

#### Scenario: Shared dependencies deduplicated
- GIVEN the interface loads SpaceUI components at runtime
- WHEN dependencies are resolved
- THEN `framer-motion`, `sonner`, `clsx`, and `class-variance-authority` each resolve to a single copy from `interface/node_modules/`

### Requirement: Docker build includes SpaceUI source
The Dockerfile SHALL copy SpaceUI package source into the build context before running the interface build.

#### Scenario: Docker image builds with SpaceUI
- GIVEN `docker build --target builder .` is executed
- WHEN the build runs
- THEN the interface frontend is compiled successfully

#### Scenario: Docker context excludes SpaceUI artifacts
- GIVEN the Docker build context is assembled
- WHEN the context is inspected
- THEN `spaceui/node_modules/`, `spaceui/packages/*/dist/`, `spaceui/examples/`, and `spaceui/.storybook/` are excluded via `.dockerignore`

### Requirement: Nix build includes SpaceUI source
The Nix `frontendSrc` fileset SHALL include SpaceUI package source files and configuration so the `frontend` derivation can resolve `@spacedrive/*` imports.

#### Scenario: Nix frontend derivation builds
- GIVEN `nix build .#frontend` is executed (after `just update-frontend-hash`)
- WHEN the build runs
- THEN it completes successfully and produces the interface `dist/` output

### Requirement: Cargo build watches SpaceUI source
The `build.rs` file SHALL include `cargo:rerun-if-changed` directives for `spaceui/packages/` so that `cargo build` triggers a frontend rebuild when SpaceUI source changes.

#### Scenario: SpaceUI change triggers frontend rebuild
- GIVEN a file under `spaceui/packages/` is modified
- WHEN `cargo build` is run
- THEN the frontend build step re-executes

### Requirement: CI triggers on SpaceUI changes
The interface CI workflow SHALL trigger on changes to `spaceui/packages/**` in addition to `interface/**`.

#### Scenario: SpaceUI change triggers interface CI
- GIVEN a pull request modifies files under `spaceui/packages/`
- WHEN the PR is created
- THEN the `interface-ci.yml` workflow runs the typecheck job

### Requirement: Project documentation reflects in-tree SpaceUI
CLAUDE.md, CONTRIBUTING.md, README.md, and CODEOWNERS SHALL reference SpaceUI as an in-tree directory, not an external sibling repository.

#### Scenario: CLAUDE.md lists spaceui directory
- GIVEN a developer reads the Key Directories section of CLAUDE.md
- WHEN they look for SpaceUI
- THEN `spaceui/` is listed with its purpose

#### Scenario: CONTRIBUTING.md describes in-tree workflow
- GIVEN a developer reads the SpaceUI section of CONTRIBUTING.md
- WHEN they look for development instructions
- THEN the instructions describe running `cd spaceui && bun run dev`, not cloning an external repo

#### Scenario: CODEOWNERS covers spaceui
- GIVEN a PR modifies files under `spaceui/`
- WHEN the PR is created
- THEN GitHub requests review from the designated owner

### Requirement: SpaceUI build artifacts gitignored
The `.gitignore` SHALL exclude `spaceui/node_modules/`, `spaceui/packages/*/dist/`, and other build artifacts so only source files are tracked.

#### Scenario: Clean build artifacts not tracked
- GIVEN `cd spaceui && bun install && bun run build` is executed
- WHEN `git status` is run
- THEN no untracked files appear under `spaceui/node_modules/` or `spaceui/packages/*/dist/`

### Requirement: Implementation uses git worktree
The implementation SHALL be performed in an isolated git worktree to avoid disrupting the main workspace during the multi-file integration.

#### Scenario: Worktree created for integration branch
- GIVEN the implementation begins
- WHEN the workspace is set up
- THEN a new worktree is created on a feature branch and all changes are made there
