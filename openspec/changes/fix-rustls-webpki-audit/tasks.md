## 1. Pre-flight

- [x] 1.1 Verify current audit failures: `cargo audit 2>&1 | grep "RUSTSEC"` shows 4 advisories
- [x] 1.2 Confirm serenity is sole source: `cargo tree -i rustls-webpki@0.102.8` shows only serenity chain
- [x] 1.3 Confirm `next` branch has tungstenite 0.28: `curl -sL "https://raw.githubusercontent.com/serenity-rs/serenity/next/Cargo.toml" | grep "tokio-tungstenite"`
- [x] 1.4 Clean git state: `git status` is clean

## 2. Pin Serenity to `next` Branch

- [x] 2.1 Create feature branch: `git checkout -b fix/rustls-webpki-audit`
- [x] 2.2 Update `Cargo.toml`: change serenity from `version = "0.12"` to `git = "https://github.com/serenity-rs/serenity", branch = "next"`, keeping same features
- [x] 2.3 Run `cargo update -p serenity` to refresh Cargo.lock
- [x] 2.4 Verify vulnerable chain gone: `cargo tree -i rustls-webpki@0.102.8` shows "nothing to print"
- [x] 2.5 Verify compilation: `cargo check` succeeds (initially fails — serenity next has API changes, handled in 2.6)
- [x] 2.6 If compilation fails in `src/messaging/discord.rs`, fix API changes (all serenity usage is in this one file)

## 3. Handle rsa Advisory in CI

- [x] 3.1 Update `.github/workflows/ci.yml` line 102: add `--ignore RUSTSEC-2023-0071` with documenting comment
- [x] 3.2 Verify audit passes first: `cargo audit --ignore RUSTSEC-2023-0071` shows 0 vulnerabilities
- [x] 3.3 Only after 3.2 passes, remove `continue-on-error: true` from line 94 to make the audit job a hard gate again

## 4. Verify and Commit

- [x] 4.1 Run full checks: `cargo fmt --all -- --check && cargo check && cargo clippy --all-targets && cargo test --lib && cargo check --features metrics`
- [x] 4.2 Run audit: `cargo audit --ignore RUSTSEC-2023-0071` shows 0 vulnerabilities
- [ ] 4.3 Commit with message: `fix(deps): pin serenity to next branch for rustls-webpki fix`
- [ ] 4.4 Push and create PR
- [ ] 4.5 Verify CI audit job passes green (not continue-on-error)
- [ ] 4.6 After PR merge, consider pinning serenity to a specific commit hash for stability
