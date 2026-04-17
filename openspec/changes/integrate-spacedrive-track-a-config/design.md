## Context

Spacebot has the Spacedrive platform vendored in-tree as of 2026-04-16 but has no runtime coupling yet. The `.scratchpad/plans/2026-04-17-track-a-spacebot-outbound.md` plan sequences the integration into three phases:

1. Phase 1 (this change): config scaffolding, no runtime behavior.
2. Phase 2: outbound HTTP client (`reqwest`, `POST /rpc` with `{"Query":...}` / `{"Action":...}` envelope).
3. Phase 3: first agent tool + prompt-injection envelope + pairing migration.

Landing Phase 1 independently means Phase 2's client work has a typed config surface ready, and the config shape is settled before operators start producing `.spacebot.toml` files that reference it.

The pairing ADR (`docs/design-docs/spacedrive-integration-pairing.md`) decides the shared-state contract between Spacebot and Spacedrive. Decisions D2 (persistence substrate) and D3 (auth-token storage) directly constrain what this config can and cannot expose.

## Goals / Non-Goals

**Goals:**
- Land a config surface that future Phase 2 and Phase 3 code can build against without guessing at shape.
- Match existing Spacebot config conventions (TomlConfig mirror → runtime struct, `#[serde(default)]` on all sections, explicit `Default` impls).
- Ensure the config is invisible at runtime when disabled; nothing about Phase 1 should change behavior for existing operators.
- Ensure the auth token never lives in the TOML schema, matching pairing ADR D3.

**Non-Goals:**
- No HTTP client, RPC types, error types, tools, or migrations. Those are Phases 2 and 3.
- No feature flag at compile time. The integration is runtime-gated via `enabled`.
- No pairing flow. `library_id` and `spacebot_instance_id` are reserved but not populated in this change.
- No config-migration tooling. Absent sections resolve to disabled defaults; no migration needed.

## Decisions

**Decision 1: `src/spacedrive.rs` module root, not `src/spacedrive/mod.rs`.**

Spacebot's CLAUDE.md and `.claude/rules/rust-essentials.md` mandate the `src/module.rs` pattern. Every existing multi-file module in the crate (`src/secrets`, `src/tools`, `src/config`, etc.) follows this. The plan document said `mod.rs` from habit; the implementation uses `.rs` to stay consistent.

*Alternative considered:* `src/spacedrive/mod.rs`. Rejected because it violates the project convention for no functional benefit (semantically identical to the compiler).

**Decision 2: Place `spacedrive` field between `metrics` and `telemetry` in `Config`.**

Observability-adjacent sections cluster together in the existing struct. While Spacedrive integration isn't strictly observability, it is an optional runtime capability, and grouping it with the other observability-adjacent opt-in sections keeps the struct layout predictable.

*Alternative considered:* alphabetical order (would place it between `messaging` and `links`). Rejected because grouping is more meaningful than alphabetization in this struct.

**Decision 3: Add a `TomlSpacedriveConfig` mirror rather than using `SpacedriveIntegrationConfig` directly.**

Every other `TomlConfig` field uses a `Toml*` mirror type. This separation lets the TOML schema evolve independently of the runtime schema (e.g., TOML fields can be renamed for operator ergonomics without breaking the runtime shape, and vice versa).

*Alternative considered:* use `SpacedriveIntegrationConfig` directly in `TomlConfig`. Rejected for consistency with the rest of the config module.

**Decision 4: Reserved `library_id` and `spacebot_instance_id` as `Option<Uuid>` in Phase 1.**

The pairing flow that populates these lands in Phase 3. Having the fields reserved now means Phase 2 code can read them without needing a config migration when Phase 3 ships. They are `Option` because in the unpaired state they are genuinely absent.

*Alternative considered:* omit the fields now and add them in Phase 3. Rejected because it would force Phase 2 code to either deal with two config versions or couple the pairing change to the HTTP client change. Having the slots reserved decouples the phases cleanly.

**Decision 5: Default `base_url = "http://127.0.0.1:8080"`.**

Matches the default Spacedrive HTTP server bind when running co-located in dev. The localhost default is safe because `enabled` defaults to `false`, so the URL is only consequential once the operator explicitly opts in. Phase 2 adds `https://` enforcement for non-loopback hosts.

*Alternative considered:* no default (require operator to set `base_url` explicitly when enabling). Rejected because the most common setup — dev running co-located — wouldn't need to set it.

**Decision 6: Combine plan Tasks 2 + 3 into one commit.**

The plan assumed `#[serde(default)]` on a top-level `Config` field would make adding the field independently compilable. But `Config` is not `Deserialize` — it's constructed programmatically in `from_toml_inner`. Adding the field requires simultaneously updating both `Ok(Config { ... })` construction sites, so Tasks 2 and 3 land as one commit. The commit message documents the divergence.

*Alternative considered:* add the field first with a temporary `Default` initializer in both sites, then wire it up in a second commit. Rejected because it produces two near-identical commits without improving bisection value.

## Risks / Trade-offs

- **Risk: Reserved fields drift from Phase 3's pairing flow shape.** → Mitigation: the pairing ADR already committed to `library_id: Uuid` and `spacebot_instance_id: Uuid` as the primary identifiers, so the Phase 1 shape matches that ADR. Any drift would be an ADR amendment, not a Phase 1 defect.

- **Risk: Default `base_url = "http://127.0.0.1:8080"` could be accidentally consequential when `enabled` is flipped on.** → Mitigation: Phase 2 adds HTTPS enforcement for non-loopback hosts, so a misconfigured `https://` is impossible to leave on localhost, and a misconfigured `http://` outside localhost fails validation. Phase 1 itself does nothing with the URL.

- **Risk: `TomlSpacedriveConfig` drift from `SpacedriveIntegrationConfig`.** → Mitigation: a round-trip test verifies every TOML field produces the expected runtime field. Adding a new TOML field without updating the runtime mirror would fail the round-trip test.

- **Trade-off: No compile-time feature gate.** Runtime-only gating means the Spacedrive code is always in the binary. Accepted because the added binary size is minimal (Phase 1 is ~100 lines of pure config) and avoiding a feature gate simplifies operator ergonomics.

## Migration Plan

No runtime migration required. Existing `.spacebot.toml` files work unchanged; absent `[spacedrive]` sections default to disabled.

Rollback: revert the commits. Because nothing reads the new config, rollback is a pure revert with no data implications.

## Open Questions

None. All pairing-ADR-derived questions are deferred to their natural phase (Phase 3 for pairing flow, Phase 2 for wire-format and auth).
