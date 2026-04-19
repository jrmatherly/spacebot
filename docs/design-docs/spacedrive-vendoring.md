# Spacedrive Vendoring

> **Status:** Implemented across PR #18 (initial vendoring, 2026-04-15), b326a4a (workspace exclude guard), 467640d (SYNC.md promoted from scratchpad), and PR #57 (Spacebot-authored stubs, 2026-04-18). This document captures the architectural rationale for vendoring Spacedrive in-tree. Operational discipline lives in `spacedrive/SYNC.md`. Runtime integration lives in `docs/design-docs/spacedrive-integration-pairing.md`.

Research and rationale for `spacedrive/` — the 50 MB Spacebot-owned fork of the Spacedrive platform that ships inside the Spacebot repository as an independent Cargo workspace. Covers the build-tree contract (layout, exclusion guard, reference-clone discipline, fork-ownership framing) that is not covered by the pairing ADR's runtime contract.

**Ownership model:** `spacedrive/` is Spacebot engineering territory. The 2026-04-16 self-reliance decision retired upstream as an authority. Changes under `spacedrive/` are Spacebot decisions, reviewed in Spacebot PRs, owned by Spacebot CODEOWNERS. `spacedriveapp/spacedrive` is a historical ancestor we originally cloned from, not a source of truth we sync against.

## Scope

**In scope.** Why `spacedrive/` is vendored in-tree instead of consumed as a submodule, npm dep, or crate dep. How the workspace `exclude = ["spacedrive"]` guard prevents accidental inclusion. The reference-clone discipline and why a bulk `rsync` from any external source would silently revert Spacebot fork content. The Spacebot-authored stub modules (PR #57) and why they exist. The replacement triggers for each stub.

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
| Historical origin | `spacedriveapp/spacedrive` on GitHub |
| Reference snapshot source | `~/dev/spacedrive` (local clone, approximately 2026-04-15; research input only) |
| Origin commit at clone time | **Unknown.** The reference clone has no `.git` directory. Not a problem — we no longer track upstream. |
| Spacebot-authored stubs | 10 files under `spacedrive/core/src/` + `apps/web/dist/index.html` (PR #57) |
| Formal requirements | `openspec/specs/spacedrive-in-tree/spec.md` |
| Operational discipline | `spacedrive/SYNC.md` (LOCAL_STATE register + reference-clone workflow) |

## Why vendor, not submodule or crate dep

Three options were considered. Vendor won on the criteria specific to Spacebot's development cadence.

- **Git submodule.** Keeps the origin project's git history available. Breaks on three Spacebot workflows: `cargo` across the workspace boundary with a different toolchain, bulk `grep` across the whole tree for shared symbols, and CI caching (a submodule's `.git` directory and its ignored build artifacts are a constant source of cache misses). Also adds a cognitive tax for every new contributor, who must learn submodule discipline before a fresh clone is actually buildable. Rejected.
- **Crate / package dependency.** Impossible: the origin project does not publish to crates.io, and the sub-crates Spacebot needs (`sd-core`, `sd-server`) are workspace-internal. Adopting this path would require maintaining a published mirror on a registry Spacebot owns. Rejected as out of proportion to need.
- **Vendor in-tree.** A flattened snapshot committed directly to the Spacebot repo, becoming Spacebot's fork. Costs: 50 MB checkout size, no origin git history. Benefits: one clone is buildable, one `grep` sees everything, Spacebot-authored stubs live alongside imported source with clear divergence tracking, the workspace exclude guards against toolchain incompatibility, and ownership is unambiguous. Selected.

The cost of losing origin git history turned out to be minor in practice. The SYNC.md LOCAL_STATE register captures what the fork contains with prose rationale, which is what future maintainers actually need. If a question ever comes up about how the origin project handled something, the reference clone at `~/dev/spacedrive` and the public `spacedriveapp/spacedrive` repo are both available as research inputs.

## The workspace exclude guard

The root `Cargo.toml` declares:

```toml
[workspace]
exclude = ["spacedrive"]
```

This is not a discoverability hint. Cargo treats any `Cargo.toml` found by recursive auto-discovery as a potential workspace member. Without the exclude, `cargo check` from the project root would traverse into `spacedrive/Cargo.toml` and fail. Spacedrive uses a different toolchain pin (`stable` vs. Spacebot's `1.94.1`) and different edition (2021 vs. 2024). The build would error with toolchain or edition mismatches long before reaching the code.

Extending the exclude list is safe; removing or forgetting the guard is not. Four scenarios break silently if the guard drifts:

1. A contributor adds `[workspace.lints]` to the root `Cargo.toml` expecting it to apply to Spacebot only. Without the guard, lint rules fan out across the full Spacedrive workspace and errors multiply.
2. A `[workspace.metadata]` block intended for Spacebot's tooling is applied to Spacedrive, which ignores or conflicts with it.
3. `cargo metadata --format-version=1 --no-deps` from the root returns a `workspace_members` list that includes Spacedrive crates. Anything that consumes that output (lint configs, CI matrix generators, `cargo-bump`) now has wrong inputs.
4. A `cargo check` invocation with `--all-targets` walks into Spacedrive and trips over the toolchain mismatch.

The `openspec/specs/spacedrive-in-tree/spec.md` requirement "Future workspace additions are safe" exists to keep this guard in the front of contributor awareness. The exclude list is extensible; a similar sibling vendored project would add to it, not replace it.

## Fork ownership and reference-clone discipline

Spacebot does **not** re-sync the Spacedrive tree from any external source. The 2026-04-16 self-reliance decision was to stop treating upstream as authoritative: `spacedrive/` is our code, and we change it ourselves on our schedule. The reference clone at `~/dev/spacedrive` is a research input, not a sync target.

Reasons a bulk rsync from any external source is harmful:

- **LOCAL_STATE would be silently reverted.** The banners prepended to `README.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `AGENTS.md`, and `CONTRIBUTING.md`; the Spacebot-authored `Dockerfile` and `.dockerignore`; the Spacebot-authored stub modules. All of these would disappear with no signal.
- **Origin project stability is not our concern.** Even if an origin project ships a flood of new files, we do not adopt them on their timeline. We decide when a feature is worth lifting based on Spacebot user impact.
- **Cascading breakage is expensive.** A bulk rsync that adds hundreds of new files can change type signatures consumed by Spacebot-authored stubs, causing compile failures that are expensive to untangle relative to a targeted, intentional lift.

The discipline is captured in `spacedrive/SYNC.md`'s "Reference-clone workflow (optional)" section: if we ever decide to look at how the origin project solved something, diff against the reference clone, read the file for research, author our version intentionally, and record the decision in LOCAL_STATE when the file becomes materially ours.

`spacedrive/SYNC.md` is load-bearing. `CLAUDE.md` calls it out explicitly: "never overwrite it wholesale via rsync." Any automated tool that wants to refresh the vendored tree must honor the LOCAL_STATE register file-by-file or refuse to run.

## Spacebot-authored stubs (PR #57)

As of the 2026-04-15 reference snapshot, the origin project's `sd-core` declared nine modules in `core/src/lib.rs` and `core/src/ops/*/mod.rs` that had no backing source files. Attempting `cargo build --bin sd-server` produced sixteen errors across E0583 ("file not found for module") and E0282 ("type annotations needed"). The declarations existed; the source files did not.

PR #57 added Spacebot-authored stubs so `sd-server` compiles. This makes `spacedrive/` a genuine Spacebot fork with documented divergence. The stubs are deliberate: they satisfy type inference at the call sites and return empty-vec or always-fail placeholders at runtime. Real archive-source, adapter, and volume operations are not yet implemented, and when they are, we will implement them ourselves.

The per-stub replacement triggers live in `spacedrive/SYNC.md` under "Spacebot-authored stubs for modules the reference clone declared but did not implement". The pattern: delete the stub file when a real implementation is ready (authored by us, or adapted from a research lift), re-run the build, accept whatever type-signature change the real code introduces.

Two files are force-added (`git add -f`) because they match the snapshotted `.gitignore` rules: `core/src/data/mod.rs` and `core/src/data/manager.rs` (matched by `data` at `.gitignore:388`), and `apps/web/dist/index.html` (matched by `apps/web/.gitignore`'s `dist/**/*`). These are Spacebot-authored files tracked alongside the imported ignore rules without altering them. If the ignore rules are ever restructured as part of our fork evolution, force-adds need to be re-applied.

The `apps/web/dist/index.html` placeholder deserves special mention: it exists because `apps/server/src/main.rs` has `#[derive(Embed)] #[folder = "../web/dist/"]`, which requires the folder to exist at `rustc` time even for `--bin sd-server` builds that do not exercise the web UI. The placeholder is a five-line HTML page noting the UI is not built. It is retired the moment either `apps/web/` gets built properly via `bun run build` (which overwrites the file with the real bundle) or we make `WebAssets` optional in our fork.

## Why the asymmetric build setup

Spacebot's root `Dockerfile` cannot build `sd-server`. Three reasons, listed above in the "Docker Compose variant" design doc but load-bearing here too:

1. The workspace exclude prevents `cargo` from seeing the Spacedrive workspace.
2. The toolchain pin differs.
3. `sd-core`'s default features pull `wasmer` + Cranelift, adding 3-5 minutes of cold compile for functionality `sd-server` does not expose.

`spacedrive/Dockerfile` (Spacebot-authored, lives in our fork) builds with `--no-default-features`. It uses `rust:trixie` as builder and `debian:trixie-slim` as runtime. The runtime stage installs `libdbus-1-3` and `libsecret-1-0` because `sd-core` transitively uses the `keyring` crate for OS secret storage. `libssl3` is installed until `sd-core`'s `reqwest` can be forced onto `rustls` (tracked as a follow-up, not a blocker).

## Rationale for the SYNC.md format

`spacedrive/SYNC.md` is a single markdown file with five sections: Purpose, Provenance, LOCAL_STATE (the register of fork content), reference-clone workflow, Hold-out list. Alternatives considered and rejected:

- **Machine-readable manifest (YAML/JSON).** Would let tools verify divergence automatically. Rejected because the LOCAL_STATE entries need prose explaining *why* each file diverges and what the replacement trigger is. YAML + prose is worse than markdown + prose.
- **Per-file `// LOCAL: ...` comments.** Would scatter divergence tracking across many files. Rejected because a future tool has to find all of them; a single register file is the source of truth.
- **Git branch with origin-project history.** Would preserve context. Rejected because it breaks the "one clone is buildable" principle. Contributors would need to know which branch was "the real one" and submodule-equivalent discipline.

The format is intentionally low-tech. A diff against the reference snapshot can reconstruct LOCAL_STATE mechanically. Prose rationale lives in one place.

## Relationship to the runtime integration

`docs/design-docs/spacedrive-integration-pairing.md` is the **runtime** contract. This doc is the **build-tree** contract. They share no files:

- Runtime contract lives in `src/spacedrive/` (Spacebot root crate) and the HTTP surface exposed by a running `sd-server`.
- Build-tree contract lives in `spacedrive/SYNC.md`, the `[workspace] exclude` guard, and the `spacedrive/Dockerfile` / `spacedrive/.dockerignore` additions.

A contributor working on the runtime integration rarely needs to touch the vendored tree. A contributor authoring or lifting a feature into our fork rarely needs to touch `src/spacedrive/`. The two contracts are orthogonal and intentionally separable.

The overlap is the `spacedrive` compose profile, which builds `spacedrive/Dockerfile` and runs the resulting `sd-server` so the runtime integration can be tested end-to-end. See `docs/design-docs/docker-compose-variant.md`.

## Future work not in scope here

- **Automated LOCAL_STATE verification.** A tool that compares the in-tree state against the reference snapshot and prints drift. Useful but not blocking.
- **Retiring the `libssl3` dependency.** Requires forcing `sd-core`'s `reqwest` onto `rustls`. Tracked for the cross-cutting TLS consolidation work, not here.
- **Stub replacement tooling.** A script that walks `spacedrive/SYNC.md`'s replacement-trigger column and flags stubs ready to be replaced. Appealing but premature; ten stubs with prose triggers are manually manageable.
- **Fork identity.** Spacebot's `spacedrive/` is a genuine Spacebot fork, but it is not a git fork on GitHub. Whether to mirror the vendored tree to a Spacebot-owned fork repo (for SBOM / provenance / supply-chain reasons) is an open question deferred until compliance demands surface.
