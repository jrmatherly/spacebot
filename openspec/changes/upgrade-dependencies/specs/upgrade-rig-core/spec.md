## ADDED Requirements

### Requirement: Upgrade rig-core to 0.35
The system SHALL use `rig-core` version 0.35 in `Cargo.toml`. The upgrade SHALL maintain all existing behavior: agent prompting, history management, tool execution, hook-driven nudging, and error recovery.

#### Scenario: Cargo build succeeds
- **WHEN** `rig-core` is bumped from `0.33` to `0.35` in `Cargo.toml` and all migration edits are applied
- **THEN** `cargo build` succeeds with zero errors

#### Scenario: All unit tests pass
- **WHEN** the upgrade is complete
- **THEN** `cargo test --lib` passes all existing tests

### Requirement: History reconstruction via extended_details
The system SHALL use `.extended_details()` on `PromptRequest` to obtain updated history from the rig agent loop. The `PromptResponse.messages` field SHALL be merged into the caller's history vec after each successful prompt call.

#### Scenario: prompt_once updates history
- **WHEN** `prompt_once` is called and the agent responds successfully
- **THEN** the caller's `history` vec contains all new messages (user prompt, assistant response, any tool calls/results) appended after the call

#### Scenario: prompt_with_tool_nudge_retry updates history across retries
- **WHEN** `prompt_with_tool_nudge_retry` runs through one or more nudge retry iterations
- **THEN** the caller's `history` vec reflects the final state including injected context messages, pruned nudge attempts, and the successful response

#### Scenario: prompt_once_streaming is unaffected
- **WHEN** `prompt_once_streaming` is called
- **THEN** its internal history management (local `chat_history` vec, manual message pushing, write-back at L709) continues to work unchanged

### Requirement: PromptError field construction matches rig 0.35
The system SHALL construct `PromptError::PromptCancelled` and `PromptError::MaxTurnsError` variants with field types matching rig 0.35's definitions. If `chat_history` fields are unboxed in 0.35, all `Box::new()` wrappers SHALL be removed.

#### Scenario: PromptError construction compiles
- **WHEN** all `PromptError` construction sites are updated
- **THEN** `cargo build` succeeds with no type mismatch errors on `chat_history` or `prompt` fields

### Requirement: with_history call sites use immutable references
The system SHALL pass history to `.with_history()` as an immutable reference (`&*history` or `&history`), not a mutable reference. No call site SHALL use `&mut` with `with_history`.

#### Scenario: No mutable history references remain
- **WHEN** the upgrade is complete
- **THEN** `grep -rn "with_history.*&mut" src/` returns zero matches

### Requirement: ToolServer changes are transparent
The system SHALL NOT pattern-match on `ToolServerError` variants. The `tool_server_handle` public field on `Agent` SHALL continue to be accessed directly for `.call_tool()` in `prompt_once_streaming`.

#### Scenario: No ToolServerError variant matching
- **WHEN** the upgrade is complete
- **THEN** `grep -rn "ToolServerError::" src/` returns zero matches (unchanged from current state)

### Requirement: CompletionModel trait impl unchanged
The `SpacebotModel` implementation of `CompletionModel` in `src/llm/model.rs` SHALL NOT be modified. The trait signatures are unchanged between 0.33 and 0.35.

#### Scenario: model.rs untouched
- **WHEN** the upgrade is complete
- **THEN** `src/llm/model.rs` has no diff from the pre-upgrade state

### Requirement: Full gate passes
The system SHALL pass all delivery gates after the upgrade.

#### Scenario: gate-pr succeeds
- **WHEN** the upgrade is complete
- **THEN** `just gate-pr` passes (formatting, compile, clippy, migration safety, tests)
