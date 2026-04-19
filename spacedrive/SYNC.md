# spacedrive/ Provenance & Fork Discipline

## Purpose

The `spacedrive/` directory is **Spacebot's fork** of the Spacedrive platform, vendored in-tree. It is not a git submodule and not a git fork on the file-system level; it is a flattened snapshot that we own outright. We diverged from `spacedriveapp/spacedrive` on purpose on 2026-04-16 and have been making our own decisions about this code ever since.

This document defines:

1. How the tree got here (historical provenance)
2. What shape our fork is in today (LOCAL_STATE)
3. How we optionally reference upstream as a research input when useful (reference-clone workflow)
4. What never gets overwritten if someone does pull an upstream file for comparison (hold-out list)

**Mental model:** `spacedrive/` is Spacebot engineering territory. Changes under this directory are Spacebot decisions, reviewed in Spacebot PRs, owned by Spacebot CODEOWNERS. The upstream project is a historical ancestor and an occasional reference, not an authority we defer to.

## Provenance

| Fact | Value |
|---|---|
| Historical origin | `spacedriveapp/spacedrive` on GitHub (clone taken approximately 2026-04-15) |
| Reference clone | `~/dev/spacedrive` (the state we started from; used as a diff baseline for occasional research) |
| Divergence decision | 2026-04-16 — self-reliance decision documented in CHANGELOG; we stopped treating upstream as authoritative |
| Origin commit at clone time | **Unknown.** `~/dev/spacedrive` has no `.git` directory; the commit pointer was not recorded when the clone was taken. Not a problem — we no longer track upstream. |
| In-tree path | `spacedrive/` (50 MB, ~3,011 tracked files) |
| In-tree Cargo workspace | Independent — declared by `spacedrive/Cargo.toml`, excluded from the root workspace by `[workspace] exclude = ["spacedrive"]` |
| In-tree toolchain | `stable` (via `spacedrive/rust-toolchain.toml`), edition 2021 |
| Root-repo toolchain | `1.94.1`, edition 2024 — incompatible with spacedrive, so `cd spacedrive` is required before cargo commands |

## LOCAL_STATE (as of 2026-04-18)

The following files under `spacedrive/` have Spacebot-specific content. Everything else started as a byte-identical copy of the 2026-04-15 reference clone but is now ours to modify freely.

### Spacebot-authored docs and infrastructure

| File | Purpose |
|---|---|
| `spacedrive/README.md` | Prepended a 5-line HTML-comment banner stating this is Spacebot's vendored copy and pointing at the root README for Spacebot setup. |
| `spacedrive/CODE_OF_CONDUCT.md` | Same vendored-banner prefix |
| `spacedrive/SECURITY.md` | Same vendored-banner prefix |
| `spacedrive/AGENTS.md` | Prepended a "Vendored subtree context" banner clarifying all relative paths are scoped to `spacedrive/` and that `cd spacedrive` is required before cargo commands. |
| `spacedrive/CONTRIBUTING.md` | Prepended a banner redirecting contributors to the Spacebot PR workflow and issue tracker. |
| `spacedrive/.dockerignore` | Spacebot-authored. No counterpart in the original clone. |
| `spacedrive/Dockerfile` | Spacebot-authored. Builds the Spacedrive server from the vendored subtree. No counterpart in the original clone. |

### Spacebot-authored stubs for modules the reference clone declared but did not implement

As of 2026-04-18, the reference clone (and the public `spacedriveapp/spacedrive` main branch at the time of cloning) declares nine modules in `core/src/{lib.rs,ops/*/mod.rs}` that have no backing source files. Spacebot authored minimal stubs so `cargo build --bin sd-server` succeeds and Track A's Task 18 smoke test can run. These are deliberate Spacebot fork divergence: they satisfy type inference at the call sites and return empty-vec / always-fail placeholders at runtime. Real archive-source, adapter, and volume operations are not yet implemented. When they are, we will implement them ourselves on our own schedule.

| File | Purpose | Replace with a real implementation when |
|---|---|---|
| `spacedrive/core/src/data/mod.rs` | Declares `pub mod manager;` — needed because `core/src/lib.rs:11` has `pub mod data;`. | We (or a future Spacebot contributor) decide to ship a real `core/src/data/` implementation. |
| `spacedrive/core/src/data/manager.rs` | `SourceManager` scaffold. Exposes 8 methods: `new` (6 sources methods + 2 adapters methods) derived from E0282/E0599 errors at `library/mod.rs:149`, `ops/sources/{create,delete,get,list_items,sync}/*`, and `ops/adapters/{config,update}/*`. Error type is `String` everywhere to match `.map_err(\|e\| ...Internal(e))` call sites. | A real `SourceManager` design is ready. Expect method signatures to change — a real implementation likely has richer error types and may change the `adapter_config_fields` / `update_adapter` sync-vs-async split. |
| `spacedrive/core/src/ops/libraries/list.rs` | `list::output::LibraryInfo` struct — consumed by `ops/core/status/{query,output}.rs` for the `core.status` response. Fields: `{id, name, path, stats}` derived from the construction site at `core/status/query.rs:70-76`. | A real `libraries::list` design is ready. The struct may get additional fields; delete the stub before landing a real implementation to avoid duplicate definitions. |
| `spacedrive/core/src/ops/sources/list.rs` | `SourceInfo` struct + `SourceInfo::new` constructor — consumed by `ops/sources/get/query.rs:3,78`. Fields `{id, name, data_type, adapter_id, item_count, last_synced, status}` derived from the `SourceInfo::new(...)` call with types inferred from sibling `CreateSourceOutput` / `SourceSyncJob`. | Same as above. |
| `spacedrive/core/src/ops/volumes/list.rs` | Four concrete types (`VolumeFilter`, `VolumeListOutput`, `VolumeListQuery`, `VolumeListQueryInput`) because `ops/volumes/mod.rs:24` does a named re-export of them. Only `VolumeFilter::{All, TrackedOnly}` is observed in-tree via `apps/cli/src/domains/{volume,cloud,location}/`. `VolumeListOutput.volumes` uses `Vec<serde_json::Value>` so downstream TS bindings do not crash on a missing `Volume` type. | A real `Volume` design is ready. TS client at `packages/ts-client/src/generated/types.ts:4741` defines `VolumeListOutput = { volumes: Volume[] }` — the real struct will replace the `serde_json::Value` placeholder and regenerate TS bindings. |
| `spacedrive/core/src/ops/adapters/list.rs` | One-line `//!` doc stub. `pub mod list;` is declared in `ops/adapters/mod.rs:4` but no external importers beyond the `pub use list::*;` glob, so an empty module is enough. | Replaced by a real module. |
| `spacedrive/core/src/ops/devices/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/jobs/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/locations/list.rs` | Same as above. | Same. |
| `spacedrive/core/src/ops/spaces/list.rs` | Same as above. | Same. |
| `spacedrive/apps/web/dist/index.html` | Placeholder `index.html` inside `apps/web/dist/`. `apps/server/src/main.rs:36` has `#[derive(Embed)] #[folder = "../web/dist/"]` which requires the folder to exist at `rustc` time, even for `--bin sd-server` where the web UI is not exercised. Contents: a single `<p>` noting the UI is not built; only the `/rpc` and `/health` endpoints are functional for Task 18. | Either (a) a real web UI is built via `bun run build` in `apps/web/`, overwriting this file with the real bundle, or (b) the `WebAssets` embed is made optional. |

Scope note: initial scope prediction was 9 missing files and 2 external importers. The actual scope turned out to be 10 missing files (9 sd-core + 1 web/dist placeholder) and 6 external importers (2 in-crate + 4 re-exported from `volumes/list.rs` consumed by `apps/cli`). The extra two `SourceManager` methods (`adapter_config_fields`, `update_adapter`) were surfaced by the compiler and live in `ops/adapters/` rather than `ops/sources/`.

### Build artifacts present only in-tree (not in reference clone)

These are local build outputs, not source changes. They are gitignored but the reference clone doesn't have them:

- `spacedrive/apps/mobile/android/.gradle/`
- `spacedrive/apps/tauri/crates/file-opening-macos/.build/`
- `spacedrive/apps/tauri/crates/macos/.build/`
- `spacedrive/packages/swift-client/.build/`

### Content present only in reference clone (not in-tree)

These directories exist in `~/dev/spacedrive` but not in our vendored copy. They were either excluded by the original vendoring rsync or removed intentionally:

- `~/dev/spacedrive/core/src/data/` — whatever this was, it's not in our tree
- `~/dev/spacedrive/core/src/ops/*/list/` — multiple `list` subdirectories under `ops/{adapters,devices,jobs,libraries,locations,sources,spaces,volumes}/`
- `~/dev/spacedrive/apps/tauri/src-tauri/gen/` — Tauri generated artifacts

These are not known gaps we're obligated to fill. If we want these behaviors, we design them ourselves.

### Directories never vendored

These directories are **never** brought in from any source, regardless of what the reference clone or any future upstream lift contains:

| Excluded | Reason |
|---|---|
| `.git/` | We are a flattened snapshot, not a submodule. No origin history kept. |
| `.github/` | Upstream's CI workflows would conflict with Spacebot's CI and CODEOWNERS. |
| `target/` | Cargo build output |
| `node_modules/` | Bun/npm dependencies |
| `.next/` | Next.js build output |
| `dist/` | Generic build output |

## Reference-clone workflow (optional)

Per the 2026-04-16 self-reliance decision, **we do not re-sync en masse and we do not treat upstream as authoritative.** The reference clone at `~/dev/spacedrive` is a research input, not a source of truth.

If we ever want to look at how the original project solved a problem, or lift a specific file as a starting point, the workflow is:

```bash
# 1. Identify the file in the reference clone
FILE="core/src/ops/spaces/update.rs"

# 2. Diff reference vs in-tree to see what the original looked like
/usr/bin/diff ~/dev/spacedrive/$FILE spacedrive/$FILE

# 3. Read the reference version as research
less ~/dev/spacedrive/$FILE

# 4. Author our version intentionally. Copy-paste as a starting point is fine;
#    blind full-file copy is not. Our code, our decisions.
cp ~/dev/spacedrive/$FILE spacedrive/$FILE  # or edit in-tree
cd spacedrive && cargo check                # must pass in spacedrive's workspace
cd .. && just gate-pr                        # spacebot workspace should be unaffected

# 5. Record the decision in this file's LOCAL_STATE tables if the file is now
#    materially ours (new behavior, new types, new public API)
```

### Refreshing the reference clone

`~/dev/spacedrive` is a point-in-time snapshot. If we want a newer reference for research purposes only:

```bash
cd ~/dev
git clone https://github.com/spacedriveapp/spacedrive.git spacedrive-fresh
# Inspect or diff against spacedrive-fresh. Do NOT rsync it over our in-tree
# spacedrive/ directory. That would silently revert our fork.
```

Keep the old reference alongside the fresh one until you're sure the fresh one is what you want to compare against.

## Hold-out list (never accept overwrites for these)

These files contain Spacebot-specific content. Any research-driven cherry-pick **must not** blindly overwrite them:

- `spacedrive/README.md` — Spacebot vendored-banner
- `spacedrive/CODE_OF_CONDUCT.md` — Spacebot vendored-banner
- `spacedrive/SECURITY.md` — Spacebot vendored-banner
- `spacedrive/AGENTS.md` — Spacebot vendored-subtree-context banner
- `spacedrive/CONTRIBUTING.md` — Spacebot vendored-guide banner
- `spacedrive/.dockerignore` — Spacebot-authored
- `spacedrive/Dockerfile` — Spacebot-authored
- Every file in the Spacebot-authored stubs table above, until we replace it with a real implementation
- This file (`spacedrive/SYNC.md`) — Spacebot-side discipline, not reference content

If any of these get touched during a research-driven cherry-pick, restore the Spacebot content and record the lift in LOCAL_STATE.

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
| `~/dev/spacedrive/` | Reference snapshot (not a git checkout, not authoritative) |

## Changelog

| Date | Change |
|---|---|
| 2026-04-16 | First draft. Recorded known divergences (3 doc banners). Origin commit pointer is unknown. |
| 2026-04-17 | Promoted to `spacedrive/SYNC.md` alongside the Spacedrive pairing-prerequisites PR. |
| 2026-04-18 | Recorded Spacebot-authored stubs for 9 unwritten `sd-core` modules plus an `apps/web/dist/index.html` placeholder so `cargo build --bin sd-server` succeeds. Needed to unblock Track A Task 18 smoke test. Scope: ~160 lines of Rust across 10 files, all placeholder-grade. |
| 2026-04-19 | Reframed the doc to reflect the actual ownership model: `spacedrive/` is Spacebot territory, not an upstream we track. "Upstream" references retired where they implied we were waiting on someone else. Retirement triggers for stubs now read "when we decide to implement" instead of "when upstream ships." |
