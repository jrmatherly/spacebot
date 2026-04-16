# Spacebot - Code Style & Conventions

See `RUST_STYLE_GUIDE.md` for the full guide. Key points:

## Imports
Grouped into 3 tiers separated by blank lines, alphabetical within each:
1. Crate-local (`use crate::...`)
2. External crates (alphabetical by crate name)
3. Standard library (`use std::...`)

Suppress unused trait warnings: `use anyhow::Context as _;`

## Naming
- Variables: `snake_case`, full words, no abbreviations (`channel_history` not `ch_hist`)
- Functions (actions): verb-first (`spawn_worker`, `save_memory`)
- Functions (getters): noun-first (`fn model(&self)`)
- Boolean getters: `is_`/`has_` prefix
- Types: `PascalCase`, descriptive
- Constants: `SCREAMING_SNAKE_CASE`
- Common abbreviations like `config` are OK

## Struct Definitions
- Derive ordering: `Debug`, `Clone`, then serialization/comparison traits
- Field ordering: identity → state/data → shared resources → config → internal state → channel senders
- `#[non_exhaustive]` on public API types that may gain fields
- Use dependency bundles when 4+ shared `Arc<T>` fields

## Visibility
- Fields private by default
- `pub(crate)` for internal cross-module access
- `pub` only for actual public API

## Comments
- Explain **why**, never **what**
- Module-level `//!` doc comment at top of every file
- `///` on public APIs and constants
- `// TODO:` for tracked future work (never `todo!()` macro — it's `deny`-linted)
- No organizational/section-divider comments
- No alarmist language (`CRITICAL:`, `IMPORTANT FIX:`)

## Error Handling
- Top-level `Error` enum in `src/error.rs` with `#[from]` for domain errors
- `thiserror` for typed enums when callers need to match variants
- `anyhow::Result` for application-level code
- `.context()` for adding context to errors
- Never discard errors with `let _ =`
- `.ok()` only on channel sends where receiver may be dropped
- Validation errors: `"can't <action>: <reason>"` pattern

## Async Patterns
- `tokio::spawn` for independent concurrent work
- Clone before moving into async blocks (variable shadowing pattern)
- Fire-and-forget with logged errors
- Store `JoinHandle` to prevent cancellation

## Lint Configuration (Cargo.toml)
```toml
[lints.clippy]
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```

## Migration Safety
- Prefer creating a new timestamped migration for schema changes
- Historical migrations may be edited for formatting/clarity when semantics are preserved; SQLx stores checksums in `_sqlx_migrations` at apply time, so editing an already-applied migration will block startup on that database until checksums are repaired or the DB is reset
- Never change the SQL semantics of a historical migration (causes silent schema drift between deployments)
