## Why

`cargo audit` flags 4 advisories that block the CI security audit job (currently `continue-on-error: true`). Three are `rustls-webpki` 0.102.8 vulnerabilities (CRL matching, URI name constraints, wildcard name constraints) pulled solely by `serenity` 0.12.5 via `tokio-tungstenite` 0.21 → `rustls` 0.22.4. The fourth is `rsa` 0.9.10 (Marvin Attack) which exists in `Cargo.lock` via `sqlx-mysql` but is never compiled since Spacebot only uses the `sqlite` feature.

Serenity's `next` branch already upgrades `tokio-tungstenite` to 0.28, pulling `rustls` 0.23+ with the safe `rustls-webpki` 0.103.x. Pinning serenity to the `next` branch resolves all three webpki advisories.

## What Changes

- Pin `serenity` dependency from crates.io 0.12.5 to the `next` git branch (still version 0.12.5, but with `tokio-tungstenite` 0.28)
- Add `--ignore RUSTSEC-2023-0071` to `cargo audit` in CI for the `rsa` advisory (no fix available, never compiled)
- Fix any serenity API changes in `src/messaging/discord.rs` if the `next` branch breaks compatibility
- Remove `continue-on-error: true` from the audit CI job once all advisories are resolved or ignored

## Capabilities

### New Capabilities
- `rustls-webpki-fix`: Resolve rustls-webpki audit advisories by upgrading serenity's TLS chain and properly handling the unfixable rsa advisory

### Modified Capabilities

## Impact

- **Cargo.toml**: serenity dependency source changes from registry to git
- **Cargo.lock**: regenerated (serenity + tokio-tungstenite + rustls chain updated)
- **.github/workflows/ci.yml**: audit command updated, `continue-on-error` potentially removed
- **src/messaging/discord.rs**: only if `next` branch has API changes (all serenity usage is in this single file)
- **No breaking changes**: Discord adapter functionality unchanged, just upgraded TLS layer
