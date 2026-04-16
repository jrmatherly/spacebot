## ADDED Requirements

### Requirement: Glib in desktop/src-tauri advanced to 0.20 or later
`desktop/src-tauri/Cargo.lock` SHALL resolve `glib` to version 0.20.0 or later. This upgrade resolves GHSA-wrw7-89jp-8q8g (unsoundness in `Iterator` and `DoubleEndedIterator` impls for `glib::VariantStrIter`). The `desktop/src-tauri/` crate is a standalone Cargo project independent of the root workspace and has its own lockfile.

#### Scenario: Lockfile pin verified
- GIVEN the glib upgrade is applied to `desktop/src-tauri/`
- WHEN `cargo tree -p glib` is run from `desktop/src-tauri/`
- THEN the output shows `glib v0.20.x` or a later major version and does not show `glib v0.18.x`

#### Scenario: Desktop build succeeds on macOS
- GIVEN the glib upgrade is applied
- WHEN `just desktop-build` is invoked on macOS
- THEN the command exits 0 and produces a valid desktop binary

### Requirement: Tauri plugin coordination when resolver refuses glib upgrade
If `cargo update -p glib --precise 0.20.X` fails because tauri plugins pin `glib ^0.18` in `desktop/src-tauri/Cargo.toml`, the change SHALL bump the blocking plugin's version in `desktop/src-tauri/Cargo.toml` to a release whose transitive glib dependency allows 0.20 or later.

#### Scenario: Blocking plugin identified
- GIVEN the glib upgrade has not yet been applied
- WHEN `cargo tree -i glib` is run from `desktop/src-tauri/`
- THEN the output identifies the tauri plugin (or plugins) holding the `^0.18` constraint

#### Scenario: Plugin version bumped when resolver refuses glib upgrade
- GIVEN `cargo update -p glib --precise 0.20.X` fails with a resolver constraint error
- WHEN the blocking tauri plugin version in `desktop/src-tauri/Cargo.toml` is bumped to a release depending on `glib` 0.20+
- THEN re-running `cargo update -p glib --precise 0.20.X` succeeds

### Requirement: Desktop build verified on both platforms
After the glib upgrade, the `desktop/src-tauri/` crate SHALL build on both macOS and Linux targets. The Linux build exercises the webkit2gtk path that consumes glib most heavily.

#### Scenario: Linux build succeeds
- GIVEN the glib upgrade is applied
- WHEN the Linux CI matrix job for desktop runs (or an equivalent local Linux build is executed)
- THEN the build completes without glib-related compilation errors
