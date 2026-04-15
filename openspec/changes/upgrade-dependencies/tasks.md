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

- [ ] 3.1 Run `cargo build` to identify which PromptError fields changed (boxed vs unboxed)
- [ ] 3.2 If `chat_history` fields unboxed: remove `Box::new()` at `src/hooks/spacebot.rs` lines 477, 493, 532, 555, 604, 643, 659
- [ ] 3.3 If `chat_history` fields unboxed: remove `Box::new()` at `src/agent/channel_history.rs` lines 559, 588, 634, 670, 708, 743, 810, 832, 866
- [ ] 3.4 If `chat_history` fields unboxed: remove `Box::new()` at `src/agent/ingestion.rs` line 683
- [ ] 3.5 If `prompt` field unboxed: remove `Box::new()` at `src/hooks/spacebot.rs` line 478

## 4. rig-core: History API migration

- [ ] 4.1 Update `prompt_once` (L427-443) to use `.extended_details()`, receive `PromptResponse`, extend `history` with `response.messages`, return `response.output`
- [ ] 4.2 Update `prompt_with_tool_nudge_retry` (L267-373) to use `.extended_details()` and merge `PromptResponse.messages` into history after successful iterations
- [ ] 4.3 Update `with_history` call at `src/agent/cortex_chat.rs:731` from `&mut history` to `&history`
- [ ] 4.4 Verify `prompt_once_streaming` (L446-711) compiles unchanged — it manages its own history and uses `stream_completion` directly
- [ ] 4.5 Inspect `PromptResponse.messages` content at runtime — check whether it includes the user prompt (would cause duplication if naively extended)

## 5. rig-core: Compile and verify

- [ ] 5.1 Run `cargo build` and fix any remaining type errors
- [ ] 5.2 Verify `agent.tool_server_handle` field access at `src/hooks/spacebot.rs:570` still compiles
- [ ] 5.3 Verify `src/llm/model.rs` has no diff (CompletionModel trait unchanged)
- [ ] 5.4 Run `cargo test --lib` — all tests pass
- [ ] 5.5 Run `cargo clippy --all-targets` — no new warnings
- [ ] 5.6 Run `just gate-pr` — full gate passes
- [ ] 5.7 Verify: `grep -rn "with_history.*&mut" src/` returns zero matches
- [ ] 5.8 Verify: `grep -rn "ToolServerError::" src/` returns zero matches

## 6. rig-core: Worktree cleanup (per finishing-a-development-branch skill)

- [ ] 6.1 Merge `feat/upgrade-rig` into `main` (or create PR depending on preference)
- [ ] 6.2 Remove worktree: `git worktree remove .worktrees/upgrade-rig`
- [ ] 6.3 Delete branch if merged: `git branch -d feat/upgrade-rig`

## 7. Arrow monitoring (no action — blocked upstream)

- [ ] 7.1 Confirm `Cargo.toml` still has `arrow-array = "57"` and `arrow-schema = "57"`
- [ ] 7.2 Document upstream tracking: watch lance-format/lance#6496 for arrow 58 merge
