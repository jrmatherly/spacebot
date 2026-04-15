# Rust Patterns

Patterns for specific subsystems. Reference when working on the relevant area. Full details in `RUST_STYLE_GUIDE.md`.

## Struct Definitions

Derive order: `Debug`, `Clone`, then serialization/comparison traits. Field order: identity, state/data, shared handles, config, internal state, channels. Use `#[non_exhaustive]` on public API boundary types.

## Function Signatures

Parameter order: `&self`/`&mut self`, primary data, shared resource handles, config/options, callbacks (last). Use `_` prefix for unused params. `impl Trait` in argument position, `where` clauses for multi-bound generics.

## Async Patterns

- `tokio::spawn` for independent concurrent work
- Clone before moving into async blocks (variable shadowing)
- Fire-and-forget with logged errors for non-critical writes
- `JoinHandle` storage prevents cancellation; underscore prefix = held for lifetime
- `tokio::select!` for racing operations
- `watch::channel` for state signaling, `mpsc::channel` for event streams, `broadcast::channel` for multi-consumer

## Trait Design

Native RPITIT for async traits (not `#[async_trait]`). Add companion `Dyn` trait with blanket impl only when `dyn Trait` is needed. Group inherent methods first, then trait impls. `Arc<dyn Trait>` for shared cross-task, `Box<dyn Trait>` for owned single-use. Bounds: `Send + Sync + 'static` when crossing task boundaries.

## Serde

- `#[serde(default)]` for backward compatibility
- `#[serde(rename_all = "snake_case")]` for enum variants
- `#[serde(tag = "type")]` for internally tagged enums
- `#[serde(untagged)]` for response types (most common case first)
- `#[serde(flatten)]` for extensible fields

## Pattern Matching

Prefer exhaustive matching (list all variants). `_ => {}` only for `#[non_exhaustive]` or foreign enums. Use `let-else` for early returns.

## State Machines

Enums with data-carrying variants. `can_transition_to()` using `matches!` for validation. Illegal transitions are runtime errors.

## Rig Integration

Agent construction: `AgentBuilder::new(model).preamble(&prompt).hook(hook).tool_server_handle(tools).build()`. History is external: `.with_history(&mut history)`. Branching is a clone of history. Always set `max_turns` explicitly. Handle `MaxTurnsError` and `PromptCancelled` for recovery.

## Strings

`Arc<str>` for immutable IDs shared across tasks. `String` for owned mutable. `&str` for borrowed. `impl Into<String>` for constructor params that will be stored.

## Database Patterns

Queries live in the modules that use them. Fire-and-forget `tokio::spawn` for non-critical writes. Use `sqlx::query!` macro for compile-time checked queries.

## Dependency Bundles

When a struct needs 4+ `Arc<T>` fields, group into a deps struct. Expose convenience accessors on the owning struct.

## Prompts Are Files

System prompts live in `prompts/` as markdown files, not string constants. Load at startup or on demand.

## Graceful Shutdown

All long-running loops respect a shutdown signal via `tokio::select!` with `shutdown_rx.recv()`.
