## Why

The project carries 3 known security vulnerabilities and 8 unmaintained/unsound dependency warnings in its Rust dependency tree, plus 2 npm vulnerabilities in the docs site. 21 direct Rust dependencies and 4 frontend packages have newer major versions available. The most actionable fix — upgrading prometheus from 0.13 to 0.14 — resolves a protobuf uncontrolled recursion CVE (RUSTSEC-2024-0437) with minimal code changes. Three redundant copies of the rustls TLS stack ship in the binary due to pinned transitive dependencies from twitch-irc and serenity.

## What Changes

- **Rust Cargo.toml version bumps (Phase 1):** prometheus 0.13→0.14, tokio-tungstenite 0.28→0.29, dialoguer 0.11→0.12, cron 0.12→0.16, notify 7→8
- **Rust code changes (Phase 2):** fastembed 4→5 (Mutex wrapper), chromiumoxide 0.8→0.9 (CDP update), rand 0.9→0.10 (trait renames)
- **Frontend upgrades (Phase 3):** TypeScript 5.9→6.0 (tsconfig defaults), lucide-react 0.563→1.8 (brand icons removed)
- **Moderate refactors (Phase 4):** lancedb 0.26→0.27 (RecordBatch API), bollard 0.18→0.20 (option structs), twitch-irc 5→6 (IRCTags type), zip 2→8 (DateTime API)
- **Major upgrades documented but deferred (Phase 5):** rig-core, toml/toml_edit, redb, sha2/RustCrypto, Vite 8, @lobehub/icons 5, serenity, daemonize replacement, OpenTelemetry family (unchecked)

## Capabilities

### New Capabilities
- `security-patch-deps`: Upgrade dependencies with known security advisories (prometheus, notify) to patched versions
- `rust-dep-currency`: Bring Rust direct dependencies to latest compatible versions with necessary code adaptations
- `frontend-dep-currency`: Upgrade TypeScript and icon libraries across interface/ and docs/
- `moderate-refactors`: Targeted subsystem refactors for lancedb, bollard, twitch-irc, and zip upgrades

### Modified Capabilities

_(none — these are dependency upgrades, not behavioral requirement changes)_

## Impact

- **Cargo.toml:** 13 version constraint changes across Phases 1-4
- **Rust source files:** ~15 files modified across Phases 1-4 (2 in telemetry, 1 in embedding, 1 in browser, 3 in auth/secrets/harness, 4 in memory/lance, 1 in update, 1 in twitch, 2 in zip usage)
- **Frontend files:** tsconfig.json (both dirs), package.json (both dirs)
- **Security posture:** 3 vulns → 2 vulns, 8 warnings → 6-7 warnings
- **TLS stack:** Eliminates rustls 0.21 (via twitch-irc upgrade)
- **No migration risk** in Phases 1-3. Phase 4 lancedb upgrade changes vector storage API patterns.
