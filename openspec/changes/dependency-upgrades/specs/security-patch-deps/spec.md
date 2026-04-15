## ADDED Requirements

### Requirement: Prometheus upgraded to resolve protobuf CVE
The project SHALL upgrade prometheus from 0.13 to 0.14 to resolve RUSTSEC-2024-0437 (protobuf uncontrolled recursion). The Cargo.toml version constraint SHALL change from `"0.13"` to `"0.14"`.

#### Scenario: Protobuf vulnerability resolved
- **WHEN** `cargo audit` is run after the prometheus upgrade
- **THEN** RUSTSEC-2024-0437 SHALL NOT appear in the output

#### Scenario: Prometheus metrics still functional
- **WHEN** the project is compiled with the `metrics` feature
- **THEN** all `CounterVec`, `HistogramVec`, `IntCounterVec`, `IntGaugeVec` usages in `src/telemetry/` SHALL compile without error

### Requirement: Notify upgraded to resolve instant unmaintained warning
The project SHALL upgrade notify from 7 to 8 to resolve RUSTSEC-2024-0384 (`instant` crate unmaintained). The Cargo.toml version constraint SHALL change from `"7"` to `"8"`.

#### Scenario: Instant warning resolved
- **WHEN** `cargo audit` is run after the notify upgrade
- **THEN** RUSTSEC-2024-0384 SHALL NOT appear in the warnings

#### Scenario: File watcher still functional
- **WHEN** config file changes are made while the daemon is running
- **THEN** the `Watcher` in `src/config/watcher.rs` SHALL detect changes and trigger hot-reload
