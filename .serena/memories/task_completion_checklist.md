# Spacebot - Task Completion Checklist

After completing any code change, run these checks in order:

## 1. Format Check
```bash
cargo fmt --all -- --check
```
Fix any formatting issues with `cargo fmt --all`.

## 2. Clippy Lint (supersets cargo check)
```bash
cargo clippy --all-targets
```
Clippy invokes rustc with the full lint set — running `cargo check` separately is redundant and was dropped from `just gate-pr` as of 2026-04-19 (Sprint 1 local-build-optimization). Use `just check-all` if you need a clippy-free compile check. `just gate-pr-fast` runs `cargo check` + skips clippy for tight iteration.
Remember: `dbg!`, `todo!()`, and `unimplemented!()` are `deny`-linted.

## 3. Library Tests (nextest default per R1)
```bash
cargo nextest run --lib
# or: just test-lib-nextest
```
R1 from the 2026-04-22 streamlining audit flipped `just gate-pr`'s default runner to cargo-nextest. Prefer `cargo nextest run` for per-file integration tests too: `cargo nextest run --test <file>`. The plain `cargo test --lib` / `just test-lib` still works and is kept for debugging shared-state issues that nextest's process-per-test isolation masks.

## 4. Integration Test Compilation
```bash
cargo test --tests --no-run
```
Use the `--no-run` form for TDD red-phase first passes too — skipping execution on known-red tests saves ~60s of re-link per cycle.

## 5. Typegen Drift Check (for any src/api/**/*.rs edit)
```bash
just check-typegen
```
Mandatory when a task touches `src/api/**/*.rs` — utoipa annotations regenerate `packages/api-client/src/schema.d.ts`, and CI's `check-typegen` job fails the PR if schema drift lands without the regenerated output. Added as an explicit step per the R8 streamlining amendment.

## 6. Full Gate (combines all above)
```bash
just gate-pr
```
This runs `just preflight` first (git/remote/auth validation; checks rustfmt + clippy components are available), then the gate script. As of 2026-04-20 the gate also runs 3 frontend invariant guards (check-workspace-protocol, check-vite-dedupe, check-adr-anchors) between check-sidecar-naming and cargo fmt. Fast-mode caveat: `just gate-pr-fast` does NOT propagate `RUSTFLAGS=-Dwarnings`, so a warning introduced during fast-mode iteration only surfaces in the full gate at push time. Reserve `just gate-pr` for pre-push; per-commit invocation is 3-5× wasted work.

## 7. Security Audit
```bash
cargo audit --ignore RUSTSEC-2023-0071
```
Must exit 0. The rsa advisory is ignored (sqlx-mysql, never compiled).

## Additional Rules
- If the same command fails twice, stop. Capture root cause and switch strategy.
- For async/stateful path changes (worker lifecycle, cancellation, recall cache), include race/terminal-state reasoning in PR summary.
- For frontend changes: `cd interface && bun run build` to verify.
- If TypeScript types changed: `just check-typegen` to verify schema sync.
- Do not push if any gate is red.
- Storage-hygiene cadence (per INDEX § Cargo discipline): `du -sh target && just sweep-target` after ~10 compile cycles or before `just gate-pr` if target/ > 40 GB. If sweep reclaims <2 GB, deeper recovery is authorized: `rm -rf target/debug/incremental target/rust-analyzer` (10-15 GB reclaim; cost is a slower next build).
