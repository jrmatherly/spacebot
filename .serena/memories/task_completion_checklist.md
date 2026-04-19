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

## 4. Library Tests
```bash
cargo test --lib
```

## 5. Integration Test Compilation
```bash
cargo test --tests --no-run
```

## 6. Full Gate (combines all above)
```bash
just gate-pr
```
This runs `just preflight` first (git/remote/auth validation), then the gate script.

## 7. Security Audit\n```bash\ncargo audit --ignore RUSTSEC-2023-0071\n```\nMust exit 0. The rsa advisory is ignored (sqlx-mysql, never compiled).\n\n## Additional Rules
- If the same command fails twice, stop. Capture root cause and switch strategy.
- For async/stateful path changes (worker lifecycle, cancellation, recall cache), include race/terminal-state reasoning in PR summary.
- For frontend changes: `cd interface && bun run build` to verify.
- If TypeScript types changed: `just check-typegen` to verify schema sync.
- Do not push if any gate is red.
