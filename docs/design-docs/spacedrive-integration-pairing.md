# ADR: Spacebot ↔ Spacedrive Integration — Pairing & Shared State

## Status

**Accepted 2026-04-17.** Phase 1 uses the config shape from D2; Phase 2 uses the auth patterns from D3. Track A landed on main via PRs #54 (Phase 1), #55 (Phase 2), and #56 (Phase 3).

## Context

Spacebot and Spacedrive are being wired together as two independent runtimes, per the 2026-04-16 self-reliance decisions:

- **Track A:** Spacebot gets an outbound HTTP client that calls Spacedrive. First user-visible payoff: `spacedrive_list_files` agent tool.
- **Track B:** Spacedrive's UI talks to Spacebot's existing HTTP API. First user-visible payoff: Spacebot chat embedded in the Spacedrive desktop app.

Both tracks can deliver their first milestone **without** pairing. An unpaired agent can hit `GET /health` on a localhost Spacedrive. An unpaired Spacedrive UI can embed a Spacebot chat served from a configured base URL. The real integration — where Spacebot becomes an agent *for a specific library* and Spacedrive routes multi-device operations *through* a specific Spacebot — is the pairing flow, which is Track A's and Track B's second or third milestone.

The two tracks must agree on four things before their pairing milestones ship:

1. **Identity:** what identifies a pairing?
2. **Persistence:** where does each side store pairing state?
3. **Authorization:** what authenticates the paired HTTP calls in both directions?
4. **Handshake:** what is the wire sequence that establishes a pairing?

This ADR decides each of those.

## Validated facts (cited)

These are the ground truths from reading the code, not design-doc prose:

### Spacedrive side

- **`SpacebotConfig` struct** already exists: `spacedrive/core/src/config/app_config.rs:52`
  - Current fields: `enabled: bool`, `base_url: String`, `auth_token: Option<String>`, `default_agent_id: String`, `default_sender_name: String`
  - **Does not yet have** `library_id` or `device_id` fields — Spacedrive's notion of those IDs lives in the network layer (`spacedrive/core/src/service/network/core/mod.rs:53`), as `Uuid`
- **Update ops** for `SpacebotConfig`: `spacedrive/core/src/ops/config/app/update.rs:81`
  - Mirrors the struct fields through `spacebot_enabled`, `spacebot_base_url`, `spacebot_auth_token`, `spacebot_default_agent_id`, `spacebot_default_sender_name`
- **RPC surface**: `POST /rpc` at `spacedrive/apps/server/src/main.rs:351`
  - Wire format: `QueryRequest { method: String, library_id: Option<String>, payload: Value }` (see `spacedrive/crates/sd-client/src/client.rs`)
- **Device identity**: `device_id: Uuid` at `spacedrive/core/src/service/network/core/mod.rs:53`
- **Library identity**: `library_id: Uuid`, scattered through crypto and permission layers
- **Pairing infrastructure**: `spacedrive/core/src/ops/network/pair/` has modules: `cancel`, `confirm_proxy`, `generate`, `join`, `status`, `vouch`, `vouching_session` — this is for *device* pairing in the library, not Spacebot pairing. The Spacebot pairing flow will be new.
- **No `/api/spacebot/*` route** exists in Spacedrive's server today — Spacedrive currently does not proxy to Spacebot.

### Spacebot side

- **Bearer token auth** already exists: `src/api/server.rs:364`
  - Pattern: `Authorization: Bearer <token>`, where `<token>` is a single `expected_token` loaded at startup
  - `/api/health` and `/health` are exempted
- **Secrets store**: `src/secrets/store.rs`
  - redb-backed, two secret categories (`system` for internal, `tool` for subprocess env vars)
  - Supports two modes: plaintext (default) or AES-256-GCM encrypted with master key in OS keychain
  - This is the right home for a Spacebot-side Spacedrive auth token
- **Config sections** live in `src/config/types.rs` (lines 22–1145). Pattern is `pub struct FooConfig { enabled: bool, ... }` with `#[serde(default)]` for backward compat.
- **Migrations**: `migrations/` directory, naming convention `YYYYMMDDHHMMSS_<snake_case_description>.sql`. Last migration as of 2026-04-16: `20260407000001_token_usage.sql`. Migrations are immutable after apply (enforced by PreToolUse hook).
- **No Spacedrive awareness** anywhere in `src/` today (`grep "spacedrive" src/ --include="*.rs" | grep -v spacedriveapp | grep -v spacedrive.com` returns 0 matches).

## Decisions

### D1. Identity: a pairing is identified by `(library_id, spacebot_instance_id)`

**Decision:** Spacedrive's `library_id: Uuid` is the source of truth. Spacebot's side records the `library_id` it is paired with plus a Spacebot-generated `spacebot_instance_id: Uuid` that Spacedrive records.

**Why:**
- Spacedrive's permission/device/library topology is already keyed by `library_id`. Reusing that identifier keeps semantics simple.
- A Spacebot instance can only pair with one library at a time (per Track A/B scope). Recording the `library_id` in Spacebot's config captures this constraint.
- Spacedrive side records the `spacebot_instance_id` so that if a Spacebot is replaced (reinstalled, migrated to a new host), the new instance can prove identity without needing the old token. Also supports multi-Spacebot-in-a-library as a future extension without schema breakage.
- `device_id` is **not** part of identity. A Spacebot instance doesn't "live on" one device — it runs as a separate process accessed through its paired Spacedrive node. The `device_id` of the Spacedrive node that hosts the Spacebot API proxy is a *routing* concern (recorded in Spacedrive's `SpacebotConfig` extension), not an *identity* concern.

### D2. Persistence: Spacebot uses a new SQLite table; Spacedrive extends `SpacebotConfig`

**Spacebot side:** new migration `YYYYMMDDHHMMSS_spacedrive_pairing.sql` introducing a `spacedrive_pairing` table:

```sql
CREATE TABLE spacedrive_pairing (
    id INTEGER PRIMARY KEY,
    library_id TEXT NOT NULL UNIQUE,
    spacebot_instance_id TEXT NOT NULL,
    spacedrive_base_url TEXT NOT NULL,
    paired_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

One row per pairing. `UNIQUE` on `library_id` enforces the "one library per Spacebot" rule. The auth token itself does **not** live here — it lives in the secrets store.

**Spacebot auth token persistence:** store in `src/secrets/store.rs` under the `system` category with a deterministic key naming scheme:

```
spacedrive_auth_token:<library_id>
```

Using the `system` category means the token is never passed to subprocess workers as env vars, which is correct — agents should not get raw Spacedrive auth; they call Spacedrive through the in-process `src/spacedrive/client.rs`.

**Spacedrive side:** extend the existing `SpacebotConfig` struct at `spacedrive/core/src/config/app_config.rs:52` with three new fields:

```rust
pub struct SpacebotConfig {
    // existing fields unchanged
    pub enabled: bool,
    pub base_url: String,
    pub auth_token: Option<String>,
    pub default_agent_id: String,
    pub default_sender_name: String,

    // new fields for pairing
    pub spacebot_instance_id: Option<String>,    // Uuid-as-string
    pub spacebot_host_device_id: Option<String>, // which device runs the proxy
    pub paired_at: Option<String>,               // ISO-8601 timestamp
}
```

All three new fields are `Option<String>` because `enabled = false` (default) means no pairing, no Spacebot awareness. Matches Spacedrive's existing `auth_token: Option<String>` pattern. Add corresponding fields to the update op at `spacedrive/core/src/ops/config/app/update.rs`.

**Why not a new Spacedrive table:**
- The Spacebot pairing is *per-library app config*, not multi-row data. A single config struct matches the existing `SpacebotConfig` pattern.
- Spacedrive's existing device-pairing infrastructure (`ops/network/pair/`) is about peer-to-peer device pairing in a library, a different thing. Reusing it would conflate two models.

### D3. Authorization: two separate tokens, each issued by the opposite side

**Inbound Spacebot → Spacedrive:** Spacebot presents a bearer token in `Authorization: Bearer <token>` when calling Spacedrive's `POST /rpc`. Token is a 32-byte random value, base64url-encoded. Spacedrive issues this during pairing, stores it in its app config, and validates on every request.

**Inbound Spacedrive → Spacebot:** Spacedrive presents Spacebot's existing bearer token (the same one used by Spacebot's other clients — see `src/api/server.rs:364`) in `Authorization: Bearer <token>`. Spacebot does **not** issue a pairing-specific token. Spacedrive records it in its `SpacebotConfig.auth_token` field during pairing.

**Why two separate tokens:**
- Tokens can be rotated independently.
- Revocation is per-direction: if a Spacedrive node is compromised, revoking its ability to call Spacebot doesn't break Spacebot's ability to call Spacedrive (if the Spacedrive core itself is still trusted).
- Matches existing infrastructure on both sides — neither side has to invent a new auth scheme.

**Token storage:**
- **Spacebot stores the outbound token** (Spacedrive's issued token, used for Spacebot → Spacedrive calls) in `src/secrets/store.rs` at key `spacedrive_auth_token:<library_id>`, category `system`.
- **Spacedrive stores the outbound token** (Spacebot's bearer, used for Spacedrive → Spacebot calls) in its existing `SpacebotConfig.auth_token` field — this is where Spacedrive already puts it.

**Rejected alternative:** one shared token. Simpler but couples rotation and revocation, and forces one side to invent a new token-issuance scheme. Not worth the simplification.

### D4. Handshake: five-step flow, initiated from Spacebot

```
1. Spacebot CLI: `spacebot spacedrive pair --to https://<spacedrive-base-url>`
2. Spacebot POSTs /api/spacebot/pair/init on Spacedrive, body = { spacebot_instance_id, spacebot_base_url, spacebot_bearer_token }
3. Spacedrive validates (user confirmation in Spacedrive UI), generates spacedrive_auth_token, responds with { library_id, spacebot_host_device_id, spacedrive_auth_token }
4. Spacebot writes the pairing row to `spacedrive_pairing`, stores spacedrive_auth_token in secrets, writes `[spacedrive]` config block
5. Spacebot POSTs /api/spacebot/pair/confirm on Spacedrive (optional — confirms Spacebot wrote the token; enables Spacedrive to mark the pairing "active" in its app config)
```

**Step 1 form factor:** CLI subcommand, not an interactive web UI. Rationale:
- Pairing is a one-time setup ritual that can be scripted.
- Avoids dependency on Spacebot's web UI (Track A Phase 1 should land before Spacebot's UI knows about Spacedrive at all).
- Spacebot already has clap 4.5 for CLI parsing.

**Step 2/3/5 endpoints:** new routes in Spacedrive, not in Spacebot. Spacedrive adds `/api/spacebot/pair/init` and `/api/spacebot/pair/confirm` as new routes in `spacedrive/apps/server/src/main.rs` (currently the only routes are `/health`, `/rpc`, `/events`, and a fallback web handler). This is the first `/api/spacebot/*` surface on Spacedrive.

**Step 3 user confirmation:** Spacedrive displays a confirmation dialog in its UI ("A Spacebot instance is requesting to pair. Accept?") before generating the token. This prevents a rogue Spacebot on the same network from pairing without user consent. The UI work here is a Track B Phase 2 scope item and needs to be built alongside the pair endpoints.

**Failure modes:**
- Step 3 rejected by user: Spacedrive returns 403, Spacebot leaves its local state untouched.
- Step 4 fails (DB error, secrets store locked): Spacebot returns an error to the user; Spacedrive's pairing is orphaned but inert (no Spacebot will call back). Spacedrive should have a way to prune stale pairings — deferred to a later OpenSpec.
- Step 5 never arrives: Spacedrive treats the pairing as pending. A retry from Spacebot or a manual Spacedrive UI action can complete it.

### D5. What Track A/B first milestones need (and don't need)

**Track A Phase 1 (config scaffolding):** only needs `enabled`, `base_url`, `auth_token`. Does **not** need `library_id` or `spacebot_instance_id` yet. Schema should reserve the fields as `Option<...>` so adding them in Phase 3 is an additive change.

**Track A Phase 2 (outbound client):** only needs to read `auth_token` from config (or directly from secrets store once D3 is implemented). No pairing yet.

**Track A Phase 3 (first tool, `spacedrive_list_files`):** works without pairing. The tool can call Spacedrive's `media_listing` wire method against an arbitrary configured `base_url` and `auth_token`. Pairing becomes mandatory only when we add library-scoped semantics (which paths are accessible) — a later phase.

**Track B Phase 1 (Spacedrive config UI):** only surfaces the existing `SpacebotConfig` fields. Does not implement pairing.

**Track B Phase 2 (embedded chat):** only needs `base_url` and Spacebot's bearer token in Spacedrive's config. No pairing.

**Track B Phase 2.5 (Spacebot CORS/auth hardening):** adds Origin validation to Spacebot's bearer-auth middleware. No pairing.

**Pairing itself:** a separate, joint OpenSpec that depends on Track A Phase 3 + Track B Phase 2 being complete. This ADR defines its shape; the work itself is scheduled after both first milestones ship.

## Consequences

**Positive:**
- Both tracks can land their first three milestones without waiting for the pairing work.
- When pairing arrives, both sides already have config storage, secret storage, and auth middleware. The pairing OpenSpec only adds new endpoints and the handshake flow.
- Identity is tied to Spacedrive's existing `library_id`, so no schema divergence.
- Token storage matches existing patterns on both sides.

**Negative:**
- Pairing requires **new Spacedrive routes** (`/api/spacebot/pair/init`, `/api/spacebot/pair/confirm`). This is additive but is the first time Spacedrive exposes anything Spacebot-specific on its HTTP surface.
- The CLI-based pair ritual may feel primitive compared to a fully in-UI flow. A future iteration can add a Spacebot web-UI pairing wizard, but it's not needed for v1.
- Two tokens means two rotation stories. Documented in D3 but requires future UX work.

**Neutral:**
- `spacebot_host_device_id` is recorded but not enforced. A later phase might add "only this device can call Spacebot's `/api/events` for SSE relay" as a policy. This ADR does not commit to that.

## Alternatives considered

1. **Single shared token** (rejected in D3). Simpler but couples rotation.
2. **Pairing state in config files only, no SQLite table** on Spacebot. Rejected because the secrets-store-plus-table split keeps tokens out of plaintext config and supports future multi-pairing extensions.
3. **Reuse Spacedrive's existing device pairing** (`ops/network/pair/`). Rejected because that system is keyed to library devices, not external services; conflating the two models obscures both.
4. **In-UI pairing wizard instead of CLI** (for step 1). Deferred, not rejected. Can be added later without breaking the handshake contract.
5. **Build against the full design doc (`spacebot-spacedrive-contract.md`)** with `SpacedriveIntegrationConfig { enabled, api_url, api_key, library_id, device_id }`. Rejected per the 2026-04-16 hybrid decision: we build to source, not docs. The doc's `SpacedriveIntegrationConfig` is named differently from Spacedrive's actual `SpacebotConfig`; reconciling them is a naming-only detail we defer until Spacedrive's real struct grows.

## Open items (to resolve before the pairing OpenSpec)

These are explicitly NOT resolved by this ADR; they need answers before the joint pairing proposal is written:

1. **Token rotation UX.** How does a user rotate either token after pairing? CLI subcommand? UI button on each side?
2. **Pairing teardown.** How does a user unpair? What happens to orphaned state on either side?
3. **Multiple Spacebot instances per library.** The schema allows it (one row per pairing, keyed by library_id — but we used `UNIQUE(library_id)`, which blocks it). Confirm intent: one-Spacebot-per-library enforced, or relax to many?
4. **Offline behavior.** When Spacedrive is unreachable, what does Spacebot do? Cache last-known-good? Fail hard? Retry with backoff? Recommend reusing `src/llm/` retry patterns but this needs explicit decision.
5. **Pairing-state drift.** If Spacebot and Spacedrive's records disagree (e.g., Spacedrive dropped the pairing, Spacebot still has the row), how do we detect and reconcile?

These belong in the pairing OpenSpec's design/specs artifacts, not here.

## References

- Fork-independence rationale is captured in this document and in `spacedrive/SYNC.md`; no external context is needed.
- Upstream design docs (treat as aspirational per 2026-04-16 decision): `spacedrive/docs/core/design/spacebot-integration.md`, `spacebot-remote-execution.md`, `spacebot-spacedrive-contract.md`
- Spacedrive `SpacebotConfig`: `spacedrive/core/src/config/app_config.rs:52`
- Spacedrive update op: `spacedrive/core/src/ops/config/app/update.rs:81`
- Spacedrive server routes: `spacedrive/apps/server/src/main.rs:351`
- Spacebot bearer auth: `src/api/server.rs:364`
- Spacebot secrets store: `src/secrets/store.rs`
- Existing device pairing (not to be confused with Spacebot pairing): `spacedrive/core/src/ops/network/pair/`

## Changelog

| Date | Change |
|---|---|
| 2026-04-16 | First draft. Decisions D1–D5 captured. Five open items flagged for the pairing OpenSpec. |
| 2026-04-17 | Promoted to `docs/design-docs/spacedrive-integration-pairing.md`. Line anchors re-verified against current tree. Status: Accepted. |
