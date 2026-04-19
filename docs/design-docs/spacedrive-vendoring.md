# Spacedrive Vendoring

> **Status:** Implemented across PR #18 (initial vendoring, 2026-04-15), b326a4a (workspace exclude guard), 467640d (SYNC.md promoted from scratchpad), and PR #57 (fork-authored stubs, 2026-04-18). This document captures the architectural rationale for vendoring Spacedrive in-tree. Operational discipline lives in `spacedrive/SYNC.md`. Runtime integration lives in `docs/design-docs/spacedrive-integration-pairing.md`.

Research and rationale for `spacedrive/` — the 50 MB vendored copy of the Spacedrive platform that ships inside the Spacebot repository as an independent Cargo workspace. Covers the build-tree contract (layout, exclusion guard, cherry-pick discipline, fork-vs-vendor framing) that is not covered by the pairing ADR's runtime contract.

## Scope

**In scope.** Why `spacedrive/` is vendored in-tree instead of consumed as a submodule, npm dep, or crate dep. How the workspace `exclude = ["spacedrive"]` guard prevents accidental inclusion. The cherry-pick discipline and why upstream `rsync` would break LOCAL_CHANGES. The fork-authored stub modules (PR #57) and why they exist despite being a genuine divergence from upstream. The retirement triggers for each stub.

**Out of scope.** The runtime HTTP contract between Spacebot and Spacedrive (`docs/design-docs/spacedrive-integration-pairing.md`), the prompt-injection envelope (`docs/design-docs/spacedrive-tool-response-envelope.md`), and anything that happens after Spacebot opens a TCP connection to a running Spacedrive instance.

## Ground truth

| Fact | Source |
|---|---|
| In-tree path | `spacedrive/` (50 MB, ~3,011 tracked files) |
| Independent Cargo workspace | `spacedrive/Cargo.toml` declares its own `[workspace]` |
| Root workspace guard | `[workspace] exclude = ["spacedrive"]` in root `Cargo.toml` |
| In-tree toolchain | `stable` via `spacedrive/rust-toolchain.toml`, edition 2021 |
| Root-repo toolchain | `1.94.1`, edition 2024 — incompatible, so `cd spacedrive` is required for cargo commands |
| Formatting discipline | `spacedrive/.rustfmt.toml` (hard tabs) scoped to the directory; root `cargo fmt` does not touch it |
| Upstream project | `spacedriveapp/spacedrive` on GitHub |
| Reference snapshot source | `~/dev/spacedrive` (local clone, approximately 2026-04-15) |
| Upstream commit at snapshot time | **Unknown.** The reference clone has no `.git` directory. |
| Fork-authored stubs | 10 files under `spacedrive/core/src/` + `apps/web/dist/index.html` (PR #57) |
| Formal requirements | `openspec/specs/spacedrive-in-tree/spec.md` |
| Operational discipline | `spacedrive/SYNC.md` (LOCAL_CHANGES register + cherry-pick recipe) |

## Why vendor, not submodule or crate dep

Three options were considered. Vendor won on the criteria specific to Spacebot's development cadence.

- **Git submodule.** Keeps upstream history available. Breaks on three Spacebot workflows: `cargo` across the workspace boundary with a different toolchain, bulk `grep` across the whole tree for shared symbols, and CI caching (a submodule's `.git` directory and its ignored build artifacts are a constant source of cache misses). Also adds a cognitive tax for every new contributor, who must learn submodule discipline before a fresh clone is actually buildable. Rejected.
- **Crate / package dependency.** Impossible: Spacedrive's upstream does not publish to crates.io, and the sub-crates Spacebot needs (`sd-core`, `sd-server`) are workspace-internal. Adopting this path would require maintaining a published mirror on a registry Spacebot owns. Rejected as out of proportion to need.
- **Vendor in-tree.** A flattened snapshot committed directly to the Spacebot repo. Costs: 50 MB checkout size, no upstream history, cherry-pick discipline required when upstream lands features Spacebot wants. Benefits: one clone is buildable, one `grep` sees everything, fork-authored stubs can live alongside upstream source with clear divergence tracking, the workspace exclude guards against toolchain incompatibility. Selected.

The cost of losing upstream git history turned out to be minor in practice. The SYNC.md LOCAL_CHANGES register captures Spacebot-side divergence with prose rationale, which is what future maintainers actually need. Upstream's commit history is still reachable through the upstream repo itself when a specific question comes up.

## The workspace exclude guard

The root `Cargo.toml` declares:

```toml
[workspace]
exclude = ["spacedrive"]
```

This is not a discoverability hint. Cargo treats any `Cargo.toml` found by recursive auto-discovery as a potential workspace member. Without the exclude, `cargo check` from the project root would traverse into `spacedrive/Cargo.toml` and fail — Spacedrive uses a different toolchain pin (`stable` vs. Spacebot's `1.94.1`) and different edition (2021 vs. 2024). The build would error with toolchain or edition mismatches long before reaching the code.

Extending the exclude list is safe; removing or forgetting the guard is not. Four scenarios break silently if the guard drifts:

1. A contributor adds `[workspace.lints]` to the root `Cargo.toml` expecting it to apply to Spacebot only. Without the guard, lint rules fan out across the full Spacedrive workspace and errors multiply.
2. A `[workspace.metadata]` block intended for Spacebot's tooling is applied to Spacedrive, which ignores or conflicts with it.
3. `cargo metadata --format-version=1 --no-deps` from the root returns a `workspace_members` list that includes Spacedrive crates. Anything that consumes that output (lint configs, CI matrix generators, `cargo-bump`) now has wrong inputs.
4. A `cargo check` invocation with `--all-targets` walks into Spacedrive and trips over the toolchain mismatch.

The `openspec/specs/spacedrive-in-tree/spec.md` requirement "Future workspace additions are safe" exists to keep this guard in the front of contributor awareness. The exclude list is extensible; a similar sibling vendored project would add to it, not replace it.

## Cherry-pick discipline

Spacebot does **not** re-sync the Spacedrive tree en masse. The 2026-04-16 self-reliance decision was: manually lift specific upstream features when they unlock a concrete Spacebot win, and never bulk-rsync from upstream.

Reasons not to re-sync:

- **LOCAL_CHANGES would be overwritten.** The banners prepended to `README.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `AGENTS.md`, and `CONTRIBUTING.md`; the Spacebot-authored `Dockerfile` and `.dockerignore`; the fork-authored stub modules. All of these would be silently reverted by an `rsync` that treats upstream as source-of-truth.
- **Upstream is not stable.** Spacedrive is under active development. A re-sync picks up whatever partially-finished refactor is in flight, including modules that are declared but not yet authored (the exact condition that motivated the PR #57 stubs).
- **Breakage from re-sync cascades.** A re-sync that adds 100 new upstream files could change type signatures consumed by Spacebot's fork-authored stubs, causing compile failures that are expensive to untangle relative to a targeted cherry-pick.

The discipline is captured in `spacedrive/SYNC.md`'s "Cherry-pick recipe" section: identify the specific upstream commit that introduced the wanted feature, apply only the files it touches, resolve LOCAL_CHANGES conflicts consciously, update the SYNC.md register to record the lift.

`spacedrive/SYNC.md` is load-bearing. `CLAUDE.md` calls it out explicitly: "never overwrite it wholesale via rsync." Any automated tool that wants to refresh the vendored tree must either honor the LOCAL_CHANGES register file-by-file or refuse to run.

## Fork-authored stubs (PR #57)

As of the 2026-04-15 reference snapshot, upstream's `sd-core` declared nine modules in `core/src/lib.rs` and `core/src/ops/*/mod.rs` that had no backing source files. Attempting `cargo build --bin sd-server` produced sixteen errors across E0583 ("file not found for module") and E0282 ("type annotations needed"). The missing modules were real declarations upstream had not yet authored.

PR #57 added minimal stubs so `sd-server` compiles. This makes `spacedrive/` a genuine fork with documented divergence. The stubs are deliberate: they satisfy type inference at the call sites and return empty-vec or always-fail placeholders at runtime. Real archive-source, adapter, and volume operations are not implemented.

The per-stub retirement triggers live in `spacedrive/SYNC.md` under "Fork-authored stubs for sd-core modules". The pattern: delete the stub file when upstream ships a real implementation, re-run the build, accept whatever type-signature change the real code introduced.

Two files are force-added (`git add -f`) because they match upstream's gitignore rules: `core/src/data/mod.rs` and `core/src/data/manager.rs` (matched by `data` at `.gitignore:388`), and `apps/web/dist/index.html` (matched by `apps/web/.gitignore`'s `dist/**/*`). Re-vendoring does not touch those gitignores; the force-add tracks Spacebot-specific files without altering upstream's ignore behavior. If upstream restructures the ignore rules, force-adds need to be re-applied.

The `apps/web/dist/index.html` placeholder deserves special mention: it exists because `apps/server/src/main.rs` has `#[derive(Embed)] #[folder = "../web/dist/"]`, which requires the folder to exist at `rustc` time even for `--bin sd-server` builds that do not exercise the web UI. The placeholder is a five-line HTML page noting the UI is not built. It is retired the moment either `apps/web/` gets built properly via `bun run build` (which overwrites the file with the real bundle) or `WebAssets` becomes optional in upstream.

## Why the asymmetric build setup

Spacebot's root `Dockerfile` cannot build `sd-server`. Three reasons, listed above in the "Docker Compose variant" design doc but load-bearing here too:

1. The workspace exclude prevents `cargo` from seeing the Spacedrive workspace.
2. The toolchain pin differs.
3. `sd-core`'s default features pull `wasmer` + Cranelift, adding 3-5 minutes of cold compile for functionality `sd-server` does not expose.

`spacedrive/Dockerfile` (Spacebot-authored, local-only) builds with `--no-default-features`. It uses `rust:trixie` as builder and `debian:trixie-slim` as runtime. The runtime stage installs `libdbus-1-3` and `libsecret-1-0` because `sd-core` transitively uses the `keyring` crate for OS secret storage. `libssl3` is installed until `sd-core`'s `reqwest` can be forced onto `rustls` (tracked as a follow-up, not a blocker).

## Rationale for the SYNC.md format

`spacedrive/SYNC.md` is a single 14 KB markdown file with five sections: Purpose, Provenance, LOCAL_CHANGES (the register of divergence), Cherry-pick recipe, Hold-out list. Alternatives considered and rejected:

- **Machine-readable manifest (YAML/JSON).** Would let tools verify divergence automatically. Rejected because the LOCAL_CHANGES entries need prose explaining *why* each file diverges and what the retirement trigger is. YAML + prose is worse than markdown + prose.
- **Per-file `// LOCAL: ...` comments.** Would scatter divergence tracking across ~10 files. Rejected because a future re-sync tool has to find all of them; a single register file is the source of truth.
- **Git branch with upstream history.** Would preserve context. Rejected because it breaks the "one clone is buildable" principle. Contributors would need to know which branch was "the real one" and submodule-equivalent discipline.

The format is intentionally low-tech. A diff against the reference snapshot can reconstruct LOCAL_CHANGES mechanically. Prose rationale lives in one place. Cherry-picks are a documented recipe, not an automated workflow.

## Relationship to the runtime integration

`docs/design-docs/spacedrive-integration-pairing.md` is the **runtime** contract. This doc is the **build-tree** contract. They share no files:

- Runtime contract lives in `src/spacedrive/` (Spacebot) and the HTTP surface exposed by a running `sd-server`.
- Build-tree contract lives in `spacedrive/SYNC.md`, the `[workspace] exclude` guard, and the `spacedrive/Dockerfile` / `spacedrive/.dockerignore` local additions.

A contributor working on the runtime integration rarely needs to touch the vendored tree. A contributor lifting an upstream feature rarely needs to touch `src/spacedrive/`. The two contracts are orthogonal and intentionally separable.

The overlap is the `spacedrive` compose profile, which builds `spacedrive/Dockerfile` and runs the resulting `sd-server` so the runtime integration can be tested end-to-end. See `docs/design-docs/docker-compose-variant.md`.

## Future work not in scope here

- **Automated LOCAL_CHANGES verification.** A tool that compares the in-tree state against the reference snapshot and prints drift. Useful but not blocking.
- **Retiring the `libssl3` dependency.** Requires forcing `sd-core`'s `reqwest` onto `rustls`. Tracked for the cross-cutting TLS consolidation work, not here.
- **Stub retirement tooling.** A script that walks `spacedrive/SYNC.md`'s retirement-trigger column, checks upstream for the trigger condition, and opens a PR to delete satisfied stubs. Appealing but premature; ten stubs with prose triggers are manually manageable.
- **Fork identity.** Spacebot's `spacedrive/` is now a genuine fork per PR #57, but it is not a git fork on GitHub. Whether to mirror the vendored tree to a Spacebot-owned fork repo (for SBOM / provenance / supply-chain reasons) is an open question deferred until compliance demands surface.
