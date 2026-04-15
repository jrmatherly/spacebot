## ADDED Requirements

### Requirement: No vulnerable rustls-webpki in dependency tree
The project SHALL NOT include `rustls-webpki` version 0.102.x in its compiled dependency tree.

#### Scenario: Vulnerable webpki removed after serenity upgrade
- **WHEN** `cargo tree -i rustls-webpki@0.102.8` is run
- **THEN** the output is "nothing to print" (no matches)

#### Scenario: Safe webpki used by all TLS consumers
- **WHEN** `cargo tree -i rustls-webpki` is run
- **THEN** only version 0.103.x appears

### Requirement: cargo audit passes with only documented ignores
The CI `cargo audit` command SHALL pass with exit code 0, ignoring only `RUSTSEC-2023-0071` (rsa, no fix available).

#### Scenario: Audit passes clean
- **WHEN** `cargo audit --ignore RUSTSEC-2023-0071` is run
- **THEN** the output shows "0 vulnerabilities found"

#### Scenario: CI audit job is a hard gate
- **WHEN** the Security Audit CI job runs
- **THEN** it SHALL NOT have `continue-on-error: true`

### Requirement: Discord adapter functionality preserved
The serenity upgrade SHALL NOT break Discord messaging functionality.

#### Scenario: Compilation succeeds
- **WHEN** `cargo check` is run
- **THEN** `src/messaging/discord.rs` compiles without errors

#### Scenario: Unit tests pass
- **WHEN** `cargo test --lib` is run
- **THEN** all tests pass (819+)

### Requirement: rsa advisory documented as accepted risk
The `rsa` 0.9.10 advisory (RUSTSEC-2023-0071) SHALL be explicitly ignored in CI with a comment explaining why.

#### Scenario: Ignore is documented
- **WHEN** `.github/workflows/ci.yml` is read
- **THEN** the `cargo audit` command includes `--ignore RUSTSEC-2023-0071` with a comment explaining the crate is never compiled
