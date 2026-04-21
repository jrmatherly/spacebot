---
name: api-handler-add
description: Use this skill when adding a new REST handler under `src/api/`. Scaffolds the full utoipa + typegen dance: handler function with correct extractor ordering (State → Path → Query → Json), request/response structs with `#[derive(utoipa::ToSchema)]`, `#[utoipa::path(...)]` annotation, router registration in `src/api/server.rs`, and the `utoipa::OpenApi` registration block. Reminds to run `just typegen` + `just check-typegen` before commit. Canonical rule: `.claude/rules/api-handler.md`.
---

# API Handler Add

## When to Use

Invoke when:
- Adding a new route under `/api/...`
- The route returns JSON and should appear in `packages/api-client/src/schema.d.ts`
- Phase 4 authz rollout adds `require_read_access` / `require_write_access` on an existing handler (still touches the handler's signature)

Do NOT invoke for:
- Internal helper functions outside `src/api/`
- Routes that serve static assets (those don't need utoipa)
- SSE endpoints (they follow a different pattern, see `src/api/*_sse.rs` examples)

## Canonical Rule

`.claude/rules/api-handler.md` is the authoritative pattern reference. This skill is the scaffolding companion. Read the rule before scaffolding; when rule and skill disagree, the rule wins.

## Sequence

### Step 1: Collect inputs

From the user or inferred from the task:

- **Resource name** (e.g., `teams`, `audit`, `memories`) — determines the file `src/api/<resource>.rs`
- **Operation** — `list` | `get` | `create` | `update` | `delete` | custom verb
- **HTTP method** — `GET` | `POST` | `PUT` | `DELETE` | `PATCH`
- **Path** — full path including `/api/` prefix (e.g., `/api/teams/{team_id}/members`)
- **Path params** — list with types
- **Query params** — list with types; which are optional
- **Request body** — struct name + fields, or `None` for GET/DELETE
- **Response body** — struct name + fields
- **Auth requirement** — `public` (no auth), `auth_context` (extract AuthContext after middleware), `admin_only` (requires `SpacebotAdmin` role)

### Step 2: Choose or create the resource file

Check if `src/api/<resource>.rs` exists:

```bash
ls src/api/<resource>.rs 2>&1
```

- If it exists: append the new handler at the end of the file, following existing structure.
- If not: create it. Add `mod <resource>;` + `pub use <resource>::*;` in `src/api.rs`.

### Step 3: Write request/response types

Follow `rust-patterns.md` derive order: `Debug`, `Clone`, then serialization. For response types destined for JSON, use `utoipa::ToSchema`.

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FooResponse {
    foo: String,
    bar: Option<i64>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct CreateFooRequest {
    foo: String,
}
```

**Visibility:** `pub(super)` unless the type is shared across modules. The router imports them via `super::`.

### Step 4: Write the handler

Extractor order is FIXED: `State` → (auth) → `Path` → `Query` → `Json`. Auth context sits between `State` and the path extractor when the route needs authenticated access.

```rust
pub(super) async fn create_foo(
    State(state): State<Arc<ApiState>>,
    // auth_context: AuthContext,           // uncomment for authenticated routes
    Path(resource_id): Path<String>,
    Query(params): Query<FooQuery>,
    Json(body): Json<CreateFooRequest>,
) -> Result<Json<FooResponse>, StatusCode> {
    // Phase 4 authz placeholder:
    // state.check_write(&auth_context, "foo", &resource_id).await?;

    let row = sqlx::query!(
        "INSERT INTO foo (id, name) VALUES (?, ?) RETURNING *",
        resource_id, body.foo,
    )
    .fetch_one(state.instance_pool().as_ref())
    .await
    .map_err(|e| {
        tracing::warn!(?e, "create_foo failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(FooResponse {
        foo: row.name,
        bar: row.count,
    }))
}
```

**Return type:** `Result<Json<T>, StatusCode>`. NO custom `ApiError` enum (Spacebot doesn't have one; `StatusCode` is the contract).

**Error handling:** map sqlx errors inline via `.map_err`. Log the underlying error; never leak it in the HTTP response body.

### Step 5: Add the utoipa path annotation

Directly above the handler:

```rust
#[utoipa::path(
    post,
    path = "/api/foo/{resource_id}",
    params(
        ("resource_id" = String, Path, description = "Resource ID"),
    ),
    request_body = CreateFooRequest,
    responses(
        (status = 200, description = "Foo created", body = FooResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Parent resource not found"),
        (status = 500, description = "Internal error"),
    ),
    tag = "foo",
)]
pub(super) async fn create_foo(...) { ... }
```

### Step 6: Register in src/api/server.rs

Two edits in this file:

1. **Router wire-up:** find the router-building block (grep for `Router::new()` or the specific resource's `.route(` calls) and add:

```rust
.route("/api/foo/{resource_id}", post(foo::create_foo))
```

2. **OpenAPI registration:** grep for the `#[derive(OpenApi)]` block's `paths` list and append the new handler:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(
        // ... existing paths ...
        foo::create_foo,
    ),
    components(schemas(
        // ... existing schemas ...
        foo::CreateFooRequest,
        foo::FooResponse,
    )),
)]
```

If either edit is missed, `just typegen` will silently produce stale types. CI (`check-typegen`) will catch it, but your local build won't.

### Step 7: Hosted-deployment limits (if applicable)

If the handler creates resources that multiply with user activity (memories, tasks, tool calls), check for a `hosted_agent_limit()` pattern:

```bash
grep -rn 'hosted_agent_limit\|SPACEBOT_DEPLOYMENT' src/api/
```

Mirror the pattern in your new handler.

### Step 8: Verify + commit

Run in sequence:

```bash
cargo check --lib 2>&1 | tail -10
just typegen
git status --short  # confirm packages/api-client/src/schema.d.ts changed
just check-typegen
```

Commit handler + router registration + schema.d.ts in the same commit. NOT separately.

## Anti-patterns to avoid

- Wrong extractor order (compile error; rustc will guide you)
- Missing `utoipa::ToSchema` on response struct (compile-fails at the `OpenApi` derive)
- Forgetting to register in server.rs (types pass cargo check but don't appear in the client schema)
- Logging `?e` without `tracing::warn!` wrapper (violates `rust-essentials.md` — all propagated errors must go through `tracing`)
- Bare `.unwrap()` on sqlx results (denied by clippy if `unwrap_used` is active; use `.map_err(|e| { warn!; StatusCode::... })`)
- Leaking DB-level error strings in the HTTP response body (generic "internal error" only)

## Quick reference

- 32 existing handler files live under `src/api/`. Browse for patterns: `ls src/api/*.rs | head -20`.
- All handlers follow the same shape. Deviation needs justification in the PR summary.
- `ApiState` exposes shared handles. Grep `pub fn.*Arc<` in `src/api/state.rs` for what's available.
- For Phase 4 authz enforcement, the `AuthContext` extractor is already set up per Phase 1. The `require_read_access` / `require_write_access` helpers will be added in Phase 4.
