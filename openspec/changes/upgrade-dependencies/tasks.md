## 1. @lobehub/icons 4.12.0 → 5.4.0

- [x] 1.1 Bump `@lobehub/icons` version from `^4.12.0` to `^5.4.0` in `interface/package.json`
- [x] 1.2 Run `bun install` in `interface/`
- [x] 1.3 Run `bun run build` in `interface/` — fixed SpaceUI symlink, build passes (exit 0)
- [x] 1.4 Verify no `antd` or `@lobehub/ui` was added to `package.json`

## 2. rig-core 0.33 → 0.35: Worktree setup (per using-git-worktrees skill)

- [x] 2.1 Verify `.worktrees/` exists and is gitignored: `git check-ignore -q .worktrees`
- [x] 2.2 Create git worktree: `git worktree add .worktrees/upgrade-rig -b feat/upgrade-rig`
- [x] 2.3 Run project setup in worktree: `cd .worktrees/upgrade-rig && cargo build`
- [x] 2.4 Verify clean baseline: 819 tests pass, 0 failures
- [x] 2.5 Bump `rig-core` version from `0.33` to `0.35` in `Cargo.toml` line 20
- [x] 2.6 Run `cargo update -p rig-core` — rig-core 0.33.0 → 0.35.0

## 3. rig-core: PromptError field types

- [x] 3.1 `PromptCancelled.chat_history` unboxed, `MaxTurnsError.chat_history` still boxed (asymmetric). Also found: `Usage` gained `cache_creation_input_tokens` field, `M` needs `'static` bound.
- [x] 3.2 Removed `Box::new()` from `PromptCancelled` at `src/hooks/spacebot.rs` (6 sites), kept `Box::new()` for `MaxTurnsError`
- [x] 3.3 Removed `Box::new()` from `PromptCancelled` at `src/agent/channel_history.rs` (8 sites), kept `Box::new()` for `MaxTurnsError`
- [x] 3.4 `src/agent/ingestion.rs` line 683 is `MaxTurnsError` — no change needed
- [x] 3.5 `prompt` field still boxed — no change needed. Added `cache_creation_input_tokens` to all 5 `Usage` constructions in `model.rs`. Added `M: 'static` bound to `prompt_once` and `prompt_with_tool_nudge_retry`.

## 4. rig-core: History API migration

- [x] 4.1 History assessment: `prompt_once` no longer mutates caller's history. Branch/compactor error paths call `extract_last_assistant_text(&self.history)` which returns `None` (stale), falling back to generic messages. Graceful degradation, not a crash. Worker uses `prompt_with_tool_nudge_retry` which manages its own history. Channel uses `prompt_once_streaming` (unaffected).
- [ ] 4.2 DEFERRED: `.extended_details()` migration for `prompt_once` — tracked as follow-up to restore full history on error paths in branch/compactor. Not blocking since fallback text is acceptable.
- [x] 4.3 Updated `with_history` call at `src/agent/cortex_chat.rs:731` from `&mut history` to `&history`
- [x] 4.4 `prompt_once_streaming` compiles unchanged — manages own history via local `chat_history` vec
- [x] 4.5 All 819 tests pass — no test failures from stale history

## 5. rig-core: Compile and verify

- [x] 5.1 `cargo build` succeeds with zero errors
- [x] 5.2 `agent.tool_server_handle` field access compiles unchanged
- [x] 5.3 `src/llm/model.rs` has changes (Usage field added, Anthropic cache_creation_input_tokens extraction) but CompletionModel trait impl is unchanged
- [x] 5.4 `cargo test --lib` — 819 tests pass, 0 failures
- [x] 5.5 `cargo clippy --all-targets` — zero warnings
- [x] 5.6 `just gate-pr` — all gate checks passed
- [x] 5.7 `grep -rn "with_history.*&mut" src/` — zero matches
- [x] 5.8 `grep -rn "ToolServerError::" src/` — zero matches

## 6. rig-core: Worktree cleanup (per finishing-a-development-branch skill)

- [ ] 6.1 Merge `feat/upgrade-rig` into `main` (or create PR depending on preference)
- [ ] 6.2 Remove worktree: `git worktree remove .worktrees/upgrade-rig`
- [ ] 6.3 Delete branch if merged: `git branch -d feat/upgrade-rig`

## 7. Arrow monitoring (no action — blocked upstream)

- [x] 7.1 Confirmed: `Cargo.toml` has `arrow-array = "57"` and `arrow-schema = "57"` (unchanged)
- [x] 7.2 Upstream tracking documented in proposal.md and design.md: watch lance-format/lance#6496
