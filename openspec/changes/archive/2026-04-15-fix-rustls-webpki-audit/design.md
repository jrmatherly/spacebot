## Context

The dependency chain causing the vulnerability:
```
serenity 0.12.5 (crates.io)
  → tokio-tungstenite 0.21.0
    → rustls 0.22.4
      → rustls-webpki 0.102.8 (VULNERABLE: 3 advisories)
```

Every other TLS consumer in Spacebot already uses `rustls` 0.23.38 with `rustls-webpki` 0.103.12 (safe). Serenity is the sole holdout.

The `rsa` 0.9.10 crate exists in `Cargo.lock` via `sqlx-mysql` but is never compiled — `cargo tree -p sqlx --depth 1` shows only `sqlx-core`, `sqlx-macros`, `sqlx-sqlite`. No patched version exists.

## Goals / Non-Goals

**Goals:**
- Resolve RUSTSEC-2026-0049, RUSTSEC-2026-0098, RUSTSEC-2026-0099 (rustls-webpki)
- Make the CI audit job pass green (not just `continue-on-error`)
- Document the rsa advisory as accepted risk with `--ignore`

**Non-Goals:**
- Upgrading serenity to a different major version (the `next` branch is still 0.12.5)
- Fixing the `rsa` advisory (no patched version exists, crate is never compiled)
- Replacing serenity with a different Discord library

## Decisions

### 1. Pin serenity to `next` branch via git dependency

The `next` branch upgrades `tokio-tungstenite` from 0.21 to 0.28, which pulls `rustls` 0.23+ (safe). The branch is still version 0.12.5 so the API should be largely compatible.

**Alternative considered:** Fork `tokio-tungstenite` 0.21 and backport `rustls` 0.23. Rejected — more maintenance burden and the serenity `next` branch already has the fix.

**Alternative considered:** Wait for serenity 0.13. Rejected — no release date, `next` branch is available now.

### 2. Ignore rsa advisory in CI

Add `--ignore RUSTSEC-2023-0071` to the `cargo audit` command. The crate is in `Cargo.lock` but never compiled (sqlx-mysql not used). No fix exists upstream.

### 3. Remove `continue-on-error` after fix

Once audit passes with only the rsa ignore, convert the audit job back to a hard gate.

## Risks / Trade-offs

- **[Serenity `next` API changes]** The `next` branch may have breaking changes beyond tungstenite. All serenity usage is in one file (`src/messaging/discord.rs`, ~1200 lines). Mitigation: fix API changes in that file.
- **[Git dependency stability]** Pinning to a branch means the resolved commit can change on `cargo update`. Mitigation: `Cargo.lock` pins the exact commit. Consider switching to a commit hash after initial verification.
- **[`next` branch instability]** The branch may receive in-progress commits. Mitigation: pin to a specific commit hash after confirming it works.
