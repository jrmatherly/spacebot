---
name: test-runtime-patterns
description: Invoke when writing or reviewing Rust unit/integration tests that exercise code which internally calls `tokio::spawn`, constructs `runtime::Tokio`-flavored opentelemetry_sdk types, or otherwise touches the Tokio runtime at construction time. Catches the "bare `#[tokio::test]` + `BatchSpanProcessor`" deadlock class before a 20+ minute rebuild discovers it.
disable-model-invocation: true
---

# Test Runtime Patterns

Rules for choosing the right Tokio runtime flavor when writing tests that touch async primitives. Invoke with `/test-runtime-patterns` before adding a test that calls into `src/daemon.rs`, `src/llm/manager.rs`, or any code that spawns tasks at construction time.

## The three failure modes

### 1. Plain `#[test]` on code that needs a runtime

Panic at first `tokio::spawn` or `Handle::current()` call:

```
there is no reactor running, must be called from the context of a Tokio 1.x runtime
```

Caller: any function that internally reaches `tokio::spawn`, `tokio::time::sleep`, `tokio::runtime::Handle::current()`, or constructs types whose `Drop`/`new` does the same.

**Fix:** use `#[tokio::test]` (not `#[test]`).

### 2. `#[tokio::test]` (current-thread default) + spawned background worker

Silent hang at function exit. The test binary runs for >60 seconds on that test with no output. cargo prints:

```
test my_test has been running for over 60 seconds
```

Root cause: `#[tokio::test]` without a `flavor` creates a current-thread runtime. If the code under test spawns a background task that blocks on `tokio::time::sleep` (driving a ticker) or otherwise doesn't yield, `Runtime::drop` waits for that task forever — deadlock.

Seen concretely in Spacebot (PR #79, `src/daemon.rs::otlp_protocol_tests`): `BatchSpanProcessor::new` spawns a ticker worker. Upstream `opentelemetry_sdk-0.31.0/src/runtime.rs:120-123` explicitly documents this exact deadlock as the reason its `TokioCurrentThread` variant spawns on a std::thread instead.

**Fix:** `#[tokio::test(flavor = "multi_thread")]`. A multi-thread runtime runs the worker on a separate worker thread, so `Runtime::drop`'s task-cancellation path completes cleanly.

### 3. `#[tokio::test]` + time-sensitive logic without `start_paused`

Nondeterministic failures under load or slow CI. Tests that assert on elapsed time or race conditions between spawned tasks fail flakily when the host is busy.

**Fix:** `#[tokio::test(start_paused = true)]` pauses the test's virtual clock so `tokio::time::sleep`, `tokio::time::interval`, etc. become controllable via `tokio::time::advance`. Does NOT help with mode 2 (spawned-worker-on-its-own-runtime case) — only the current test's runtime timer is paused.

## Decision matrix

| Code under test calls... | Test attribute |
|---|---|
| No `tokio::*` primitives, purely sync logic | `#[test]` |
| `tokio::spawn` or `Handle::current()` but no long-lived spawned task | `#[tokio::test]` |
| Constructs `opentelemetry_sdk::runtime::Tokio` or spawns a task blocking on `sleep`/`interval` | `#[tokio::test(flavor = "multi_thread")]` |
| Logic under test is timer-dependent and you want deterministic time | `#[tokio::test(start_paused = true)]` — layer on top of the flavor above |
| Test itself spawns multiple tasks that must run concurrently | `#[tokio::test(flavor = "multi_thread", worker_threads = N)]` |

## Spacebot-specific precedents

- **`src/daemon.rs::otlp_protocol_tests`** (landed in PR #79) — uses `#[tokio::test(flavor = "multi_thread")]` on all four tests that reach `build_otlp_provider`'s `BatchSpanProcessor::builder().build()` path. The three sibling tests that early-return before that line stay as plain `#[test]`. That asymmetry is intentional and documented in the module preamble.

- **`src/llm/manager.rs::rate_limit_tracking_skipped_for_litellm_prefixed_models`** (landed in PR #78) — uses default `#[tokio::test]` (current-thread) because `LlmManager::new` doesn't spawn anything. No flavor needed.

- **Upstream `opentelemetry_sdk` tests** — `span_processor_with_async_runtime.rs:618, :626, :631` all use `flavor = "multi_thread"` when the test constructs `runtime::Tokio`. Mirror this pattern for consistency.

## Verification checklist

Before committing a new test:

- [ ] Does the code under test call `tokio::spawn` (direct or transitive)? → must be `#[tokio::test]` at minimum.
- [ ] Does it construct `opentelemetry_sdk::runtime::Tokio` or any type whose `new`/`build` calls `tokio::spawn` eagerly? → must be `flavor = "multi_thread"`.
- [ ] Are there other tests in the same module using `flavor = "multi_thread"` for similar reasons? → use the same flavor for consistency (matches the module-wide preamble convention in `daemon.rs`).
- [ ] If you chose a non-default flavor, is there a module-level comment explaining why? → future maintainers will wonder.

## Quick fix recipe (when a test hangs for >60s)

1. Kill the test runner.
2. Check whether the test exercises `tokio::spawn` or `BatchSpanProcessor`-style construction.
3. Change `#[tokio::test]` → `#[tokio::test(flavor = "multi_thread")]`.
4. Re-run the narrowed test: `cargo test --lib <module>::<test_name>`.
5. Expect pass in <1s (the runtime no longer deadlocks at drop).

## Anti-patterns

- **"I'll just use `tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()` inside the test."** No. That hand-rolls what `#[tokio::test]` already does and still has the same drop-deadlock on the inner spawn.
- **"I'll add a `tokio::time::timeout` around the body so it can't hang."** The hang happens at `Runtime::drop` AFTER the body exits — a body-scoped timeout can't catch it.
- **"Let me refactor the production code to not spawn at construction time."** Almost never the right call. The production code mirrors how Spacebot's `#[tokio::main]` runtime is set up. Changing it to defer the spawn would leak runtime-awareness into the production API.

## References

- Upstream source citation: `opentelemetry_sdk-0.31.0/src/runtime.rs:120-123` (the `TokioCurrentThread` explanatory comment).
- Upstream test pattern: `opentelemetry_sdk-0.31.0/src/trace/span_processor_with_async_runtime.rs:618, :626, :631`.
- Spacebot PR #79 commit `ccd4c0c` — fix(daemon): give OTLP exporter tests a multi-thread Tokio runtime.
