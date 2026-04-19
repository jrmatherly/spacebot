# spacedrive/ Sync & Provenance

## Purpose

The `spacedrive/` directory is Spacebot's vendored copy of the Spacedrive platform source. It is not a git submodule. It is not a git fork. It is a flattened snapshot that we maintain locally. This document defines:

1. How the tree got here (provenance)
2. What we've changed since (LOCAL_CHANGES)
3. How to pull a specific upstream feature when we want one (cherry-pick recipe)
4. What never gets overwritten (hold-out list)

## Provenance

| Fact | Value |
|---|---|
| Upstream project | `spacedriveapp/spacedrive` on GitHub |
| Reference snapshot | `~/dev/spacedrive` (a local clone used as "what we started from") |
| Reference snapshot date | Approximately 2026-04-15 (see `stat ~/dev/spacedrive/Cargo.toml`) |
| Upstream commit at snapshot time | **Unknown.** `~/dev/spacedrive` has no `.git` directory; the commit pointer was not recorded when the clone was taken. |
| In-tree path | `spacedrive/` (50 MB, ~3,011 tracked files) |
| In-tree Cargo workspace | Independent — declared by `spacedrive/Cargo.toml`, excluded from the root workspace by `[workspace] exclude = ["spacedrive"]` |
| In-tree toolchain | `stable` (via `spacedrive/rust-toolchain.toml`), edition 2021 |
| Root-repo toolchain | `1.94.1`, edition 2024 — incompatible with spacedrive, so `cd spacedrive` is required before cargo commands |

## LOCAL_CHANGES (as of 2026-04-18)

The following files in `spacedrive/` differ from the `~/dev/spacedrive` reference. Everything else is byte-identical.

### Intentional additions

| File | Purpose |
|---|---|
| `spacedrive/README.md` | Prepended a 5-line HTML-comment banner stating "Vendored upstream documentation. This README describes the Spacedrive project as built and shipped by the upstream Spacedrive team. For Spacebot setup and architecture, see the root README.md." |
| `spacedrive/CODE_OF_CONDUCT.md` | Same vendored-banner prefix |
| `spacedrive/SECURITY.md` | Same vendored-banner prefix |
| `spacedrive/AGENTS.md` | Prepended a "Vendored subtree context" banner clarifying all relative paths are scoped to `spacedrive/` and that `cd spacedrive` is required before cargo commands. |
| `spacedrive/CONTRIBUTING.md` | Prepended a "Vendored upstream guide" banner redirecting contributors to the Spacebot PR workflow and issue tracker. |
| `spacedrive/.dockerignore` | Spacebot-authored ignore file for Docker build context (target, node_modules, build artifacts). No upstream counterpart. |
| `spacedrive/Dockerfile` | Spacebot-authored Dockerfile that builds the Spacedrive server from the vendored subtree. No upstream counterpart. |

### Fork-authored stubs for `sd-core` modules that upstream declares but has not written

As of 2026-04-18 the upstream `spacedriveapp/spacedrive` main branch declares nine modules in `core/src/{lib.rs,ops/*/mod.rs}` that have no backing source files. The tree in `~/dev/spacedrive` is in the same state, so re-vendoring cannot unblock a build. Spacebot authored minimal stubs so `cargo build --bin sd-server` succeeds and Track A's Task 18 smoke test can run. These files are deliberate fork divergence: they satisfy type inference at the call sites and return empty-vec / always-fail placeholders at runtime. Real archive-source, adapter, and volume operations are NOT implemented.

| File | Purpose | Retirement trigger |
|---|---|---|
| `spacedrive/core/src/data/mod.rs` | Declares `pub mod manager;` — needed because `core/src/lib.rs:11` has `pub mod data;`. | Remove when upstream ships any real `core/src/data/` implementation. |
| `spacedrive/core/src/data/manager.rs` | `SourceManager` scaffold. Exposes 8 methods: `new` (6 sources methods + 2 adapters methods) derived from E0282/E0599 errors at `library/mod.rs:149`, `ops/sources/{create,delete,get,list_items,sync}/*`, and `ops/adapters/{config,update}/*`. Error type is `String` everywhere to match `.map_err(\|e\| ...Internal(e))` call sites. | Remove once upstream ships a real `data::manager::SourceManager`. Expect method signatures to change — a real impl likely has richer error types and may change the `adapter_config_fields` / `update_adapter` sync-vs-async split. |
| `spacedrive/core/src/ops/libraries/list.rs` | `list::output::LibraryInfo` struct — consumed by `ops/core/status/{query,output}.rs` for the `core.status` response. Fields: `{id, name, path, stats}` derived from the construction site at `core/status/query.rs:70-76`. | Remove once upstream ships a real `libraries::list`. The upstream struct may have additional fields; delete the stub before copying upstream in to avoid duplicate definitions. |
| `spacedrive/core/src/ops/sources/list.rs` | `SourceInfo` struct + `SourceInfo::new` constructor — consumed by `ops/sources/get/query.rs:3,78`. Fields `{id, name, data_type, adapter_id, item_count, last_synced, status}` derived from the `SourceInfo::new(...)` call with types inferred from sibling `CreateSourceOutput` / `SourceSyncJob`. | Same as above. |
| `spacedrive/core/src/ops/volumes/list.rs` | Four concrete types (`VolumeFilter`, `VolumeListOutput`, `VolumeListQuery`, `VolumeListQueryInput`) because `ops/volumes/mod.rs:24` does a named re-export of them. Only `VolumeFilter::{All, TrackedOnly}` is observed in-tree via `apps/cli/src/domains/{volume,cloud,location}/`. `VolumeListOutput.volumes` uses `Vec<serde_json::Value>` so downstream TS bindings do not crash on a missing `Volume` type. | Same as above. TS client at `packages/ts-client/src/generated/types.ts:4741` defines `VolumeListOutput = { volumes: Volume[] }` — when the upstream implementation lands, the real `Volume` struct will replace the `serde_json::Value` placeholder and regenerate TS bindings. |
| `spacedrive/core/src/ops/adapters/list.rs` | One-line `//!` doc stub. `pub mod list;` is declared in `ops/adapters/mod.rs:4` but no external importers beyond the `pub use list::*;` glob, so an empty module is enough. | Delete when upstream ships a real file. |
| `spacedrive/core/src/ops/devices/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/jobs/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/locations/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/spaces/list.rs` | Same as above. | Same. |
| `spacedrive/apps/web/dist/index.html` | Placeholder `index.html` inside `apps/web/dist/`. `apps/server/src/main.rs:36` has `#[derive(Embed)] #[folder = "../web/dist/"]` which requires the folder to exist at `rustc` time, even for `--bin sd-server` where the web UI is not exercised. Contents: a single `<p>` noting the UI is not built; only the `/rpc` and `/health` endpoints are functional for Task 18. | Delete once either (a) the upstream web UI is actually built via `bun run build` in `apps/web/`, which overwrites this file with the real bundle, or (b) the `WebAssets` embed is made optional. |

Scope note: initial prediction was 9 missing files and 2 external importers. The actual scope turned out to be 10 missing files (9 sd-core + 1 web/dist placeholder) and 6 external importers (2 in-crate + 4 re-exported from `volumes/list.rs` consumed by `apps/cli`). The extra two `SourceManager` methods (`adapter_config_fields`, `update_adapter`) were surfaced by the compiler and live in `ops/adapters/` rather than `ops/sources/`.

### Build artifacts present only in-tree (not upstream)

These are local build outputs, not source changes. They are gitignored but the reference clone doesn't have them:

- `spacedrive/apps/mobile/android/.gradle/`
- `spacedrive/apps/tauri/crates/file-opening-macos/.build/`
- `spacedrive/apps/tauri/crates/macos/.build/`
- `spacedrive/packages/swift-client/.build/`

### Content present only in reference (not in-tree)

These directories exist in `~/dev/spacedrive` but not in our vendored copy. They were either excluded by the original vendoring rsync or removed intentionally:

- `~/dev/spacedrive/core/src/data/` — whatever this was, it's not in our tree
- `~/dev/spacedrive/core/src/ops/*/list/` — multiple `list` subdirectories under `ops/{adapters,devices,jobs,libraries,locations,sources,spaces,volumes}/`
- `~/dev/spacedrive/apps/tauri/src-tauri/gen/` — Tauri generated artifacts

Investigate these next time we re-sync. They may represent upstream work we should lift, or they may be clone-side artifacts we correctly excluded.

### Directory-level exclusions (enforced by rsync, by design)

Every file listed under "Fork-authored stubs for `sd-core` modules that upstream declares but has not written" above is also a hold-out: a future upstream rsync must not overwrite them unless upstream has shipped the real implementation, in which case the stub is retired per the table's "Retirement trigger" column.

These upstream directories are **never** vendored, regardless of their upstream content:

| Excluded | Reason |
|---|---|
| `.git/` | We are a clone, not a submodule. No upstream history kept. |
| `.github/` | Upstream's CI workflows would conflict with Spacebot's CI and CODEOWNERS. Pull from upstream's tree if ever needed. |
| `target/` | Cargo build output |
| `node_modules/` | Bun/npm dependencies |
| `.next/` | Next.js build output |
| `dist/` | Generic build output |

## Cherry-pick recipe

Per the 2026-04-16 self-reliance decision: **we do not re-sync en masse.** We manually lift specific upstream features when they unlock Spacebot user-experience wins.

### Manual cherry-pick workflow

For a single upstream file or feature:

```bash
# 1. Identify the file in the reference clone
FILE="core/src/ops/spaces/update.rs"

# 2. Diff reference vs in-tree
/usr/bin/diff ~/dev/spacedrive/$FILE spacedrive/$FILE

# 3. Review the upstream version; decide if we want it wholesale or partial
less ~/dev/spacedrive/$FILE

# 4. Apply intentionally — copy the file, or edit in-tree with upstream as guide
cp ~/dev/spacedrive/$FILE spacedrive/$FILE
cd spacedrive && cargo check       # must pass in spacedrive's independent workspace
cd .. && just gate-pr              # spacebot workspace should be unaffected

# 5. Record the lift in this file's LOCAL_CHANGES table with a reason line
```

### When to refresh the reference clone

`~/dev/spacedrive` is a point-in-time snapshot. When upstream ships features we want that aren't in this snapshot:

```bash
# Option A: rsync from a fresh upstream clone to ~/dev/spacedrive
#   (destroys reference state; the next cherry-pick has no baseline)

# Option B (recommended): clone upstream to a fresh path, diff, cherry-pick
cd ~/dev
git clone https://github.com/spacedriveapp/spacedrive.git spacedrive-fresh
# Pull the one thing we want from spacedrive-fresh into spacedrive/
# (Optionally: promote spacedrive-fresh to ~/dev/spacedrive after full review)
```

Option B is safer because it preserves the current reference baseline during the pull.

## Hold-out list (never accept upstream overwrites for these)

These files are guaranteed to contain Spacebot-specific content. A future rsync/cherry-pick **must not** blindly overwrite them:

- `spacedrive/README.md` — has our vendored-banner
- `spacedrive/CODE_OF_CONDUCT.md` — has our vendored-banner
- `spacedrive/SECURITY.md` — has our vendored-banner
- `spacedrive/AGENTS.md` — has our vendored-subtree-context banner
- `spacedrive/CONTRIBUTING.md` — has our vendored-upstream-guide banner
- `spacedrive/.dockerignore` — Spacebot-authored, no upstream counterpart
- `spacedrive/Dockerfile` — Spacebot-authored, no upstream counterpart
- This file (`spacedrive/SYNC.md`) — Spacebot-side discipline, not upstream content

If any of these get touched by an upstream pull, re-add the banner and re-record the change here.

## Relationship to Spacebot's Cargo workspace

The `[workspace] exclude = ["spacedrive"]` guard in the **root** `Cargo.toml` is load-bearing. It prevents Cargo from auto-discovering `spacedrive/Cargo.toml` as a workspace member. Removing the guard would break `cargo check` at the project root because Spacedrive's workspace declares members (`apps/server`, `apps/cli`, `core`, `crates/*`, `xtask`) that Spacebot's root doesn't have.

Do not remove the guard. If `[workspace.lints]` or `[workspace.metadata]` is added later, the guard must stay.

## Files of record

| File | Purpose |
|---|---|
| `Cargo.toml` (root) | `[workspace] exclude = ["spacedrive"]` |
| `.gitignore` | Excludes `spacedrive/target/`, `spacedrive/node_modules/`, etc. |
| `.dockerignore` | Excludes entire `spacedrive/` from Docker build context |
| `.github/CODEOWNERS` | `spacedrive/ @jrmatherly` |
| `openspec/specs/spacedrive-in-tree/spec.md` | Structural spec for the vendoring |
| `~/dev/spacedrive/` | Reference snapshot (not a git checkout) |

## Changelog

| Date | Change |
|---|---|
| 2026-04-16 | First draft. Recorded known divergences (3 doc banners). Upstream commit pointer is unknown. |
| 2026-04-17 | Promoted to `spacedrive/SYNC.md` alongside the Spacedrive pairing-prerequisites PR. |
| 2026-04-18 | Recorded fork-authored stubs for 9 unwritten `sd-core` modules plus an `apps/web/dist/index.html` placeholder so `cargo build --bin sd-server` succeeds. Needed to unblock Track A Task 18 smoke test. Scope: ~160 lines of Rust across 10 files, all placeholder-grade. |
