# Spacebot - Task Completion Checklist

After completing any code change, run these checks in order:

## 1. Format Check
```bash
cargo fmt --all -- --check
```
Fix any formatting issues with `cargo fmt --all`.

## 2. Compile Check
```bash
cargo check --all-targets
```

## 3. Clippy Lint
```bash
cargo clippy --all-targets
```
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
