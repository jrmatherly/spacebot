# Security Audit

## Purpose
Security vulnerability management for the dependency tree. Covers audit tooling, advisory handling, and CI enforcement.

## Requirements

### Requirement: No vulnerable rustls-webpki in dependency tree
The project SHALL NOT include `rustls-webpki` version 0.102.x in its compiled dependency tree.

#### Scenario: Vulnerable webpki removed after serenity upgrade
- GIVEN the serenity Discord library is pinned to the next branch
- WHEN `cargo tree -i rustls-webpki@0.102.8` is run
- THEN the output is "nothing to print" (no matches)

#### Scenario: Safe webpki used by all TLS consumers
- GIVEN the dependency tree is inspected
- WHEN `cargo tree -i rustls-webpki` is run
- THEN only version 0.103.x appears

### Requirement: cargo audit passes with only documented ignores
The CI `cargo audit` command SHALL pass with exit code 0, ignoring only `RUSTSEC-2023-0071` (rsa, no fix available).

#### Scenario: Audit passes clean
- GIVEN `cargo audit --ignore RUSTSEC-2023-0071` is run
- WHEN the audit completes
- THEN the output shows "0 vulnerabilities found"

#### Scenario: CI audit job is a hard gate
- GIVEN the Security Audit CI job runs
- WHEN it executes
- THEN it SHALL NOT have `continue-on-error: true`

### Requirement: Discord adapter functionality preserved
The serenity upgrade SHALL NOT break Discord messaging functionality.

#### Scenario: Compilation succeeds
- GIVEN the serenity upgrade is applied
- WHEN `cargo check` is run
- THEN `src/messaging/discord.rs` compiles without errors

#### Scenario: Unit tests pass
- GIVEN the serenity upgrade is applied
- WHEN `cargo test --lib` is run
- THEN all tests pass (819+)

### Requirement: rsa advisory documented as accepted risk
The `rsa` 0.9.10 advisory (RUSTSEC-2023-0071) SHALL be explicitly ignored in CI with a comment explaining why.

#### Scenario: Ignore is documented
- GIVEN `.github/workflows/ci.yml` is read
- WHEN the `cargo audit` command is inspected
- THEN it includes `--ignore RUSTSEC-2023-0071` with a comment explaining the crate is never compiled

### Requirement: Prometheus upgraded to resolve protobuf CVE
The project SHALL upgrade prometheus from 0.13 to 0.14 to resolve RUSTSEC-2024-0437 (protobuf uncontrolled recursion).

#### Scenario: Protobuf vulnerability resolved
- GIVEN the prometheus upgrade is applied
- WHEN `cargo audit` is run
- THEN RUSTSEC-2024-0437 does not appear in the output

#### Scenario: Prometheus metrics still functional
- GIVEN the project is compiled with the `metrics` feature
- WHEN compilation runs
- THEN all `CounterVec`, `HistogramVec`, `IntCounterVec`, `IntGaugeVec` usages in `src/telemetry/` compile without error

### Requirement: Notify upgraded to resolve instant unmaintained warning
The project SHALL upgrade notify from 7 to 8 to resolve RUSTSEC-2024-0384 (`instant` crate unmaintained).

#### Scenario: Instant warning resolved
- GIVEN the notify upgrade is applied
- WHEN `cargo audit` is run
- THEN RUSTSEC-2024-0384 does not appear in the warnings

#### Scenario: File watcher still functional
- GIVEN config file changes are made while the daemon is running
- WHEN the changes are detected
- THEN the `Watcher` in `src/config/watcher.rs` triggers hot-reload
