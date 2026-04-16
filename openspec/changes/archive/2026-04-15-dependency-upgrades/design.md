## Context

Spacebot's Cargo.lock was last bulk-updated on 2026-04-14 (semver-compatible updates patching 11 CVEs). However, 21 direct Rust dependencies and 4 frontend packages remain behind their latest major/minor versions. A 3-agent team review verified breaking changes, codebase usage, and security implications for all 27+ outdated dependencies. The full analysis is at `.scratchpad/dependency-upgrade-analysis.md` and the implementation plan at `.scratchpad/plans/dependency-upgrades-plan.md`.

Rust toolchain is 1.94.1, satisfying all MSRV requirements. The project uses bun for frontend (never npm/pnpm/yarn).

## Goals / Non-Goals

**Goals:**
- Fix RUSTSEC-2024-0437 (protobuf uncontrolled recursion via prometheus)
- Fix RUSTSEC-2024-0384 (instant unmaintained via notify)
- Eliminate rustls 0.21 copy (via twitch-irc upgrade)
- Bring all Tier 1-4 dependencies to latest versions
- Maintain 819 passing tests through every upgrade step
- Keep cargo audit vulnerability count at or below current baseline

**Non-Goals:**
- rig-core upgrade (64-file impact, wait for stability)
- redb 4.0 upgrade (requires data migration strategy)
- toml/toml_edit upgrade (19-file impact, separate effort)
- sha2/RustCrypto ecosystem upgrade (coordinated multi-crate effort)
- Vite 8 / @lobehub/icons 5 (large frontend changes)
- Fixing transitive-only vulnerabilities (rand 0.8.5, rsa, lexical-core)
- OpenTelemetry crate family (5 crates at 0.31/0.32, not yet investigated)
- @lobehub/icons 5 (pulls in antd@6 + @lobehub/ui@5 peer deps; evaluate alternatives first)
- Consolidating TLS backends (rustls + native-tls coexist; address when upgrading email/IMAP crates)

## Decisions

1. **Phase ordering: security first, then zero-risk, then code changes.** prometheus and notify upgrades fix advisories and should land first. Zero-risk bumps (tokio-tungstenite, dialoguer, cron) batch with them for efficiency.

2. **fastembed: use `std::sync::Mutex`, not `tokio::sync::Mutex`.** The `embed()` call runs inside `tokio::task::spawn_blocking`, so a std Mutex is appropriate and avoids async lock overhead.

3. **rand 0.10 upgrade is for currency, not security.** RUSTSEC-2026-0097 targets transitive `rand 0.8.5` which is not resolved by upgrading our direct dependency. The upgrade is still worthwhile for trait cleanup.

4. **Phase 4 items get individual PRs.** lancedb, bollard, twitch-irc, and zip each touch different subsystems and should be reviewed independently.

5. **Phase 5 items are documented but deferred.** Each needs its own design document when scheduled. The `.scratchpad/dependency-upgrade-analysis.md` serves as the research reference for those future efforts.

## Risks / Trade-offs

- **chromiumoxide 0.9:** Cannot easily test browser tool without a Chrome instance. CDP type compatibility is the main risk. Mitigation: compile-check only; manual testing before merge.
- **lancedb 0.27 RecordBatch API change:** Affects the vector memory storage layer. If the new API behaves differently during HNSW index creation, embeddings could be affected. Mitigation: run memory-specific tests, verify search quality.
- **bollard 0.20 option struct removal:** The entire Docker self-update flow needs re-mapping. Only 1 file but many API calls. Mitigation: audit every bollard call against 0.20 docs.
- **TypeScript 6.0 `types: []` default:** Could surface missing type declarations that were silently auto-discovered before. Mitigation: explicitly add `"types": ["vite/client"]` to tsconfig.
