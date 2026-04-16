## Why

Three dependencies are outdated. Two are actionable now (`@lobehub/icons` 4→5, `rig-core` 0.33→0.35), one is blocked upstream (`arrow` 57→58 via lancedb). The rig-core upgrade brings a history API redesign that shifts history management to the caller, a ToolServer performance improvement (actor → RwLock), and removes double-encoding of string tool output. The lobehub/icons upgrade was incorrectly classified as blocked — the antd@6 peer dep is already present in v4.12.0 and irrelevant to our usage pattern.

## What Changes

- Bump `@lobehub/icons` from 4.12.0 to 5.4.0 in `interface/package.json`. Zero code changes — all 14 `es/` subpath icon imports are stable across versions.
- **BREAKING** Bump `rig-core` from 0.33 to 0.35 in `Cargo.toml`. History API changes from mutable (`&mut Vec<Message>`) to immutable (`impl IntoIterator`). Callers must use `.extended_details()` to get updated history via `PromptResponse.messages`. ~30 edit sites across 5 files.
- Remove `Box::new()` wrappers on `PromptError` field construction if fields were unboxed in 0.34 (conflicting reports — compiler resolves).
- Document `arrow-array/schema` 57→58 as blocked upstream (lance-format PR #6496 in draft, waiting on opendal 0.56).

## Capabilities

### New Capabilities

- `upgrade-lobehub-icons`: Bump @lobehub/icons 4.12.0 → 5.4.0 with zero code changes
- `upgrade-rig-core`: Migrate rig-core 0.33 → 0.35 including history API, PromptError fields, and ToolServer changes
- `monitor-arrow-upgrade`: Track upstream lancedb/arrow-rs for arrow 58 readiness

### Modified Capabilities

## Impact

- `interface/package.json` — version bump only
- `src/hooks/spacebot.rs` — 3 `with_history` call sites, 7 `Box::new(chat_history)` construction sites, history reconstruction via `.extended_details()`
- `src/agent/channel_history.rs` — 9 `Box::new(history.clone())` test construction sites
- `src/agent/cortex_chat.rs` — 1 `with_history` call site
- `src/agent/ingestion.rs` — 1 `Box::new(Vec::new())` construction site
- `src/tools.rs` — 5 functions returning `Result<(), ToolServerError>` (likely unchanged)
- `Cargo.toml` — rig-core version bump
