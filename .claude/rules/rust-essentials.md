# Rust Essentials

Core conventions that apply to every Rust change. For the full style reference, see `RUST_STYLE_GUIDE.md`.

## Project Structure

Single binary crate with no workspace **members**. The root `Cargo.toml` carries `[workspace] exclude = ["spacedrive"]` to keep the vendored `spacedrive/` directory out of Cargo auto-discovery. The `[workspace]` block is intentional — never delete it; extend the exclude list if a similar sibling project lands. Module files use `src/module.rs` pattern (NEVER `src/module/mod.rs`). Module root files contain `mod` declarations and re-exports. Prefer implementing in existing files unless it's a new logical component.

## Lint Rules

`dbg_macro`, `todo`, `unimplemented` are `deny` in `[lints.clippy]`. Use `tracing::debug!` for debug output. Use `// TODO:` comments for tracked work instead of `todo!()` panics.

## Imports

Grouped into 3 tiers separated by blank lines, alphabetical within each:

1. Crate-local (`use crate::...`)
2. External crates (alphabetical by crate name)
3. Standard library (`use std::...`)

Suppress unused trait warnings: `use anyhow::Context as _;`

## Naming

| Kind | Convention |
|------|-----------|
| Variables | `snake_case`, full words, no abbreviations (`queue` not `q`, `message` not `msg`) |
| Functions (actions) | verb-first (`spawn_worker`, `save_memory`) |
| Functions (getters) | noun-first (`fn model(&self)`) |
| Boolean getters | `is_`/`has_` prefix |
| Types | `PascalCase`, descriptive |
| Constants | `SCREAMING_SNAKE_CASE` |

## Error Handling

- Propagate with `?`, add context with `.context()` / `.with_context()`
- Never silently discard errors with `let _ =`
- `.ok()` only on channel sends where receiver may be dropped
- Log non-critical failures with `tracing::warn!`
- Use `thiserror` enums when callers match on variants, `anyhow::Result` otherwise
- Validation errors: `"can't <action>: <reason>"` pattern

## Comments

- Explain WHY, never WHAT
- Module-level `//!` doc comment at top of every file
- `///` on public APIs and constants
- `// TODO:` for tracked future work
- No organizational/section-divider comments
- No alarmist language (`CRITICAL:`, `IMPORTANT FIX:`)

## Visibility

Fields private by default. `pub(crate)` for internal cross-module access. `pub` only for actual public API.

## Panics

Never `.unwrap()` on `Result` or `Option` in production code. `.expect()` only for guaranteed invariants (hardcoded regex). No `unsafe`.

## Logging

Use `tracing` crate with structured key-value pairs:
- `error` — broken, needs attention
- `warn` — failed but can continue
- `info` — significant lifecycle events
- `debug` — operational detail
- `trace` — very verbose

Use `#[tracing::instrument(skip(self, ...))]` for function-level spans.
