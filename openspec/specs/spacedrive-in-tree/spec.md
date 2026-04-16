# spacedrive-in-tree Specification

## Purpose
The Spacedrive platform is co-located inside the Spacebot repository at `spacedrive/` so the planned HTTP integration between the two projects can be developed from a single clone. Spacedrive remains an independent Cargo workspace with its own toolchain, edition, and dependency versions. This spec captures the structural rules that keep the two workspaces isolated while sharing one repo.

## Requirements
### Requirement: Spacedrive source available in project tree
The Spacedrive platform source SHALL reside at `spacedrive/` in the Spacebot project root, excluding `.git/`, `.github/`, `target/`, `node_modules/`, `.next/`, and `dist/` directories.

#### Scenario: Fresh clone contains Spacedrive source
- **WHEN** a developer clones the Spacebot repository
- **THEN** the `spacedrive/` directory EXISTS with `Cargo.toml`, `core/`, `crates/`, and `apps/`

#### Scenario: Spacedrive .github excluded
- **WHEN** the `spacedrive/` directory is listed
- **THEN** it SHALL NOT contain a `.github/` directory

#### Scenario: Spacedrive .git excluded
- **WHEN** the `spacedrive/` directory is listed
- **THEN** it SHALL NOT contain a `.git/` directory (source is co-located, not a submodule)

### Requirement: Cargo workspaces remain independent
Spacebot's `Cargo.toml` SHALL contain a `[workspace]` section with an `exclude` list that includes `"spacedrive"`, preventing Cargo from discovering Spacedrive as a workspace member.

#### Scenario: Spacebot builds without Spacedrive
- **WHEN** `cargo check` is run from the project root
- **THEN** only Spacebot's crate is compiled; no `spacedrive/` crate is referenced

#### Scenario: Workspace exclude guard present
- **WHEN** `grep -A2 '\[workspace\]' Cargo.toml` is run
- **THEN** the output contains `exclude` with `"spacedrive"` in the list

#### Scenario: Workspace has exactly one member
- **WHEN** `cargo metadata --format-version=1 --no-deps` is run and parsed
- **THEN** `workspace_members` contains exactly one entry (Spacebot's package)

#### Scenario: Future workspace additions are safe
- **WHEN** a `[workspace.lints]` or `[workspace.metadata]` section is added to Spacebot's Cargo.toml
- **THEN** Cargo SHALL NOT auto-discover `spacedrive/Cargo.toml` as a workspace member

### Requirement: Spacedrive build artifacts gitignored
The `.gitignore` SHALL exclude `spacedrive/target/`, `spacedrive/node_modules/`, and other build artifacts.

#### Scenario: Build artifacts not tracked after Spacedrive build
- **WHEN** `cd spacedrive && cargo check` is run followed by `cd .. && git status`
- **THEN** no files under `spacedrive/target/` appear as untracked

### Requirement: Spacedrive excluded from Docker build context
The `.dockerignore` SHALL exclude the entire `spacedrive/` directory since Spacebot's Docker build does not depend on Spacedrive source.

#### Scenario: Docker build ignores Spacedrive
- **WHEN** `docker build .` is run
- **THEN** the build context does not include `spacedrive/` files

### Requirement: CODEOWNERS covers spacedrive
The `.github/CODEOWNERS` SHALL include a `spacedrive/` entry assigning ownership.

#### Scenario: PR modifying Spacedrive requests review
- **WHEN** a pull request modifies files under `spacedrive/`
- **THEN** GitHub requests review from the designated owner

### Requirement: Project documentation references Spacedrive
CLAUDE.md SHALL list `spacedrive/` in Key Directories. CONTRIBUTING.md SHALL include a Spacedrive development section. README.md SHALL note that Spacedrive is co-located.

#### Scenario: CLAUDE.md lists spacedrive
- **WHEN** a developer reads the Key Directories section
- **THEN** `spacedrive/` is listed with its purpose

#### Scenario: CONTRIBUTING.md describes independent builds
- **WHEN** a developer reads the Spacedrive section of CONTRIBUTING.md
- **THEN** the instructions explain that Spacedrive builds independently with `cd spacedrive && cargo check`

### Requirement: Toolchain files preserved and self-scoped
Spacedrive's `rust-toolchain.toml` (channel `stable`) and `.rustfmt.toml` (hard tabs) SHALL be preserved in the copy and SHALL apply only within the `spacedrive/` directory.

#### Scenario: Spacedrive toolchain resolved correctly
- **WHEN** `cd spacedrive && rustup show active-toolchain` is run
- **THEN** the output shows `stable` (not Spacebot's `1.94.1`)

#### Scenario: Spacebot formatting unaffected
- **WHEN** `cargo fmt --all -- --check` is run from the project root
- **THEN** only Spacebot code is checked; Spacedrive's `.rustfmt.toml` does not apply

