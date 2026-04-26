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

**fastembed model cache**: the 4 `memory::search::tests::test_metadata_search_*` tests download the BGESmallENV15 ONNX model (~127 MB) at runtime via `EmbeddingModel::new(tempfile::tempdir())`. On macOS this download path intermittently fails inside `ureq + native-tls`. Pre-stage once with `just fetch-fastembed`, then `export HF_HOME=$(just fetch-fastembed-cache-dir)` before `cargo test --lib`. Idempotent — re-runs return in <1s. Added 2026-04-26 (commit `a1b245f`).

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

## After Major-Version Frontend Dep Bumps
```bash
cd <workspace> && bun install --force && bunx tsc --noEmit
```
Bun's install layout doesn't always prune the OLD version's type-def directory under `node_modules/.bun/<pkg>@<oldver>/`. Local tsc may resolve stale defs and pass while CI (clean install) catches new strictness. Lesson learned 2026-04-26 commit `104ee69` (vitest 3 → 4 made `mock.calls` typing stricter; local tsc passed, CI failed at `ShareResourceModal.test.tsx:181` with TS7006 implicit-any). See `.claude/skills/bun-deps-bump/SKILL.md` for the workflow.

## Manifest-Lockfile Drift (auto-checked on Edit)
`.claude/settings.json` PostToolUse hook fires `scripts/claude-hooks/manifest-lockfile-drift.sh` on every Edit/Write to a `package.json`. Catches the bug class where `bun update` (without `--latest`) bumps the lockfile but leaves the spec range stale — opens dependabot loops. If you see the warning, see `/bun-deps-bump` for the fix workflow. Hook added 2026-04-26 (commit `0367bad`).

## Additional Rules
- If the same command fails twice, stop. Capture root cause and switch strategy.
- For async/stateful path changes (worker lifecycle, cancellation, recall cache), include race/terminal-state reasoning in PR summary.
- For frontend changes: `cd interface && bun run build` to verify.
- If TypeScript types changed: `just check-typegen` to verify schema sync.
- Do not push if any gate is red.
- Storage-hygiene cadence (per INDEX § Cargo discipline): `du -sh target && just sweep-target` after ~10 compile cycles or before `just gate-pr` if target/ > 40 GB. If sweep reclaims <2 GB, deeper recovery is authorized: `rm -rf target/debug/incremental target/rust-analyzer` (10-15 GB reclaim; cost is a slower next build).
