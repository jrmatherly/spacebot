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

For patterns Spacebot doesn't use day-to-day (`JoinSet`, `tokio_util::sync::CancellationToken`, `Semaphore`-based resource pools, `futures::stream` and `async-stream`), see the `rust-async-patterns` skill from the `systems-programming` plugin.

The skill's Pattern 5 demonstrates `#[async_trait]`. Spacebot uses native RPITIT instead, which avoids the `Box<dyn Future>` allocation that `#[async_trait]` introduces. When you reach for Pattern 5, substitute the RPITIT pattern documented in the Trait Design section below and in `RUST_STYLE_GUIDE.md:459`.

## Streams

When a stream is needed, prefer wrapping an existing mpsc receiver:

- `tokio_stream::wrappers::ReceiverStream::new(rx)` wraps `mpsc::Receiver<T>` as `Stream<Item = T>`. Used by every `Messaging::inbound_stream()` implementation.
- `futures::TryStreamExt` for iterating over `Stream<Item = Result<T, E>>` (e.g., LanceDB query results). Prefer `.try_next().await?` over `.next().await` to preserve error propagation.
- `futures::StreamExt` only when a third-party crate exposes a `Stream` directly (e.g., `chromiumoxide` browser events).

Do not build streams from scratch with `async_stream::stream! {}`. If you need a producer-consumer pattern, use an `mpsc` channel and wrap with `ReceiverStream`. Reserve `stream::iter()` + `.buffer_unordered()` for bounded-concurrency parallel iteration over a known finite collection. Even then, prefer `tokio::spawn` + `JoinHandle` storage for parity with the rest of the codebase.

## Resource Pools

For any pooled resource (database connection, HTTP client, etc.), use the crate's built-in pool. Do not hand-roll a `Semaphore` + `Mutex<Vec<Resource>>` pattern.

- `sqlx::SqlitePool::connect()` for SQLite (per-agent and instance-wide databases)
- `reqwest::Client` for HTTP, which does built-in connection pooling; just clone the client
- `lancedb::Connection` for vector storage, as a single connection wrapped in `Arc`
- `Arc<redb::Database>` for redb, which is already `Send + Sync`

If you find a third-party crate that genuinely lacks a pool, reach for the `rust-async-patterns` skill's Pattern 7 (`Semaphore` + `Drop`-released `PooledConnection`). Verify the crate does not already expose pool semantics first; most do.

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

Agent construction: `AgentBuilder::new(model).preamble(&prompt).hook(hook).tool_server_handle(tools).build()`. History is external: `.with_history(&history)`. Branching is a clone of history. Always set `max_turns` explicitly. Handle `MaxTurnsError` and `PromptCancelled` for recovery.

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
