---
paths:
  - "src/api/**/*.rs"
---

# API Handler Patterns

HTTP handlers for the daemon's REST API. Served by Axum on port 19898, consumed by `interface/` (React web UI) and `desktop/` (Tauri). Shape is dictated by three things: Axum's extractor ordering, `utoipa`-based OpenAPI generation, and the shared `ApiState` dependency bundle.

## File Layout

- One file per resource under `src/api/<resource>.rs` (agents, channels, cron, memories, tasks, etc. — 32 handler files currently).
- `src/api/server.rs` assembles the router.
- `src/api/state.rs` defines `ApiState` — the shared Arc-bundled dependencies every handler can extract.

## Handler Signature Shape

Axum extractors must appear in a fixed order: `State` → `Path` → `Query` → `Json`.

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

pub(super) async fn get_agent_overview(
    State(state): State<Arc<ApiState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<OverviewQuery>,
) -> Result<Json<AgentOverviewResponse>, StatusCode> {
    // ...
}
```

- Return type is `Result<Json<T>, StatusCode>`. There is no custom `ApiError` enum in this codebase — errors are mapped to `StatusCode` inline and logged via `tracing::warn!` / `tracing::error!`.
- Use `StatusCode::NOT_FOUND` for missing resources, `StatusCode::INTERNAL_SERVER_ERROR` for unexpected failures. Never leak internal error strings in the HTTP response body.
- Handlers are `pub(super)` unless there's a reason to expose them further — the router in `server.rs` imports them via `super::`.

## OpenAPI Generation (utoipa)

Every response/request struct needs `#[derive(utoipa::ToSchema)]`. Every handler worth exposing to the TypeScript client needs a `#[utoipa::path(...)]` annotation.

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct AgentsResponse {
    agents: Vec<AgentInfo>,
}
```

When adding a new route:

1. Write the handler + its request/response types with `utoipa::ToSchema`.
2. Annotate with `#[utoipa::path(method, path = "/api/...", ...)]`.
3. Register the route in `src/api/server.rs` router.
4. Register the handler in the utoipa `OpenApi` derive list (same file region).
5. `just typegen` to regenerate `interface/src/api/schema.d.ts`.
6. Commit all three: the handler, the router registration, and the updated `schema.d.ts`.

CI runs `just check-typegen` which diffs regenerated schema against committed. A new route without `just typegen` = red gate.

## State Access

`ApiState` is the single source of truth for shared handles. Don't reach into globals or process-level statics — pull what you need out of `State(state): State<Arc<ApiState>>`. If you need a new handle, add it to `ApiState` in `src/api/state.rs` rather than threading it as a separate extractor.

## Data Layer

- Never serialize raw `sqlx::Row` into responses. Always map through a domain type that implements `Serialize + utoipa::ToSchema`.
- Queries live next to the handler that uses them (per `rust-patterns.md`), not in a shared "queries" module.
- Use `sqlx::query!` / `sqlx::query_as!` macros for compile-time checked queries. Dynamic SQL only when unavoidable.

## Auth

Routes that require authentication take the auth extractor as the second argument (between `State` and the path/query/body extractors). If you're adding an authenticated route, grep existing handlers for the pattern — consistency matters more than cleverness.

## Hosted-Deployment Considerations

Some handlers check `SPACEBOT_DEPLOYMENT=hosted` and apply limits (e.g. `hosted_agent_limit()` in `agents.rs`). If your handler creates resources that multiply with user activity, mirror that pattern.

## Verification Before Calling It Done

- `cargo check --all-targets`
- `just typegen` then check `git status` — if `interface/src/api/schema.d.ts` changed, commit it
- `just check-typegen` — must pass
- Hit the endpoint manually against a running daemon (curl to `http://localhost:19898/api/<route>`) to confirm the status codes and shape
