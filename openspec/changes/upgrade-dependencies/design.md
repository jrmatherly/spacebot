## Context

Spacebot depends on three outdated packages: `@lobehub/icons` 4.12.0 (frontend), `rig-core` 0.33 (Rust LLM framework), and `arrow-array/schema` 57 (pinned by lancedb). The rig-core upgrade is the most significant — it redesigns how conversation history flows between the caller and the agent loop.

Current state: the project uses rig's `with_history(&mut Vec<Message>)` pattern where rig mutates the caller's history vec in place. Three functions in `src/hooks/spacebot.rs` pass history this way. A fourth function (`prompt_once_streaming`, 265 lines) bypasses `with_history` entirely and manages its own streaming loop.

## Goals / Non-Goals

**Goals:**
- Upgrade `@lobehub/icons` to 5.4.0 (zero-effort)
- Upgrade `rig-core` to 0.35 with correct history reconstruction
- Document the arrow upgrade block for future action

**Non-Goals:**
- Upgrading lancedb or arrow (blocked upstream)
- Refactoring `prompt_once_streaming` to use rig's built-in streaming (separate effort)
- Adding the hook type parameter `P` to function signatures (default `()` is correct)

## Decisions

**D1: Use `.extended_details()` for history reconstruction**

Rig 0.35 offers two return modes from `prompt()`: `String` (default, no history) and `PromptResponse` (via `.extended_details()`, includes `.messages`). We use `.extended_details()` at the `prompt_once` and `prompt_with_tool_nudge_retry` call sites to get the updated history. Alternative considered: manually pushing assistant messages after each call — rejected because rig's internal loop may produce multiple messages (tool calls, tool results, assistant responses) and manually tracking all of them duplicates rig's work.

**D2: Leave `prompt_once_streaming` unchanged**

This function already manages its own `chat_history` vec and writes it back on success at line 709. It doesn't use `with_history` and calls `stream_completion` directly. Since `stream_completion` now accepts `impl IntoIterator` and `Vec<Message>` implements that trait, no changes are needed. Alternative considered: rewriting to use rig's streaming API — rejected as out of scope and high risk.

**D3: Handle PromptError boxing uncertainty at compile time**

Two research agents gave contradictory answers on whether `chat_history` fields in `PromptError` are boxed or unboxed in 0.35. Rather than resolving this before implementation, we bump the version and let the compiler tell us. The plan covers both paths. This avoids acting on unverified information.

**D4: Use a git worktree for the rig-core upgrade (per using-git-worktrees skill)**

The rig-core upgrade touches 5+ files with behavioral changes. A worktree isolates the work from main and the in-progress `feat/integrate-spaceui` branch. Pre-verified: `.worktrees/` exists at project root and is gitignored. Worktree setup requires a clean baseline test pass before any changes begin. After completion, use the finishing-a-development-branch skill for merge and cleanup.

## Risks / Trade-offs

**[Silent history staleness]** If `.extended_details()` is not added to `prompt_once`, history stops being updated. Code compiles, tests that don't check history content pass, but channels lose conversation state at runtime. → Mitigation: Add explicit history length assertions in `channel_history.rs` tests. Verify history growth after `prompt_once` calls.

**[PromptResponse.messages content ambiguity]** Unclear whether `messages` includes the user prompt or only assistant/tool messages. If it includes the prompt, extending history would duplicate it. → Mitigation: Inspect the actual content after first successful compile. Add a debug assertion checking for duplicate user prompts.

**[prompt_with_tool_nudge_retry loop interaction]** The retry loop modifies history between iterations (injecting context at L321, pruning at L356). After upgrade, `with_history` provides a snapshot, not a live reference. The loop's mutations to `history` are only visible to rig on the next iteration via the next `with_history` call. → Mitigation: This actually works correctly — each loop iteration passes the latest `&*history`, so rig sees all mutations. Verify via existing nudge retry tests.

**[Agent type parameter mismatch]** If any code path passes a hook-constructed agent (`Agent<M, CortexHook>`) to a function expecting `Agent<M>` (`= Agent<M, ()>`), it won't compile. → Mitigation: Verified that cortex agents (the only ones with `.hook()` at build time) never pass through our hook wrapper functions. Channel/branch/worker agents use default `P = ()`.
