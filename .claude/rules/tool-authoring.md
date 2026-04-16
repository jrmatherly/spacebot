---
paths:
  - "src/tools/**/*.rs"
  - "prompts/en/tools/**/*.md.j2"
---

# Tool Authoring

Tools are the agent's interface to side effects. Every LLM-callable capability lives in `src/tools/` paired with a prompt description in `prompts/en/tools/`. The structure is rigid because the factory wires tools into the right ToolServer topology (Channel, Branch, Worker, Compactor, Cortex) based on where they're declared.

## Paired Files

Every tool has **two** files:

1. `src/tools/<name>.rs` — the Rust implementation
2. `prompts/en/tools/<name>_description.md.j2` — the description the LLM sees (Jinja2 template, 49 tool descriptions currently exist)

If one is present and the other isn't, something is wrong.

## Rust Implementation Shape

```rust
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct <Name>Tool {
    // Shared handles first (store: Arc<X>, manager: Arc<Y>)
    // Then per-instance config
    // Then optional mutable builder state
}

impl <Name>Tool {
    pub fn new(...) -> Self { ... }
}

impl Tool for <Name>Tool {
    const NAME: &'static str = "<name>";
    type Error = ToolError;    // or a tool-specific error enum that converts
    type Args = <Name>Args;    // Deserialize + JsonSchema
    type Output = <Name>Output; // Serialize

    async fn definition(&self, _prompt: String) -> ToolDefinition { ... }
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> { ... }
}
```

- `#[derive(Debug)]` manually if `Arc<dyn X>` fields block the derive.
- Never `panic!` or `.unwrap()` inside `call`. Return a `ToolError` variant; the Rig loop surfaces it to the model.
- Clone `Arc` handles into async blocks rather than capturing `&self`.

## Prompt Description Shape

The `.md.j2` file is the first thing the LLM reads about the tool. It shapes tool-call behavior more than the Rust code does.

- **First line = one-sentence purpose.** Concrete, action-oriented. "Manage scheduled tasks (cron jobs). Actions: `create`, `list`, `delete`."
- **Parameter semantics next.** Anything the JSON schema alone doesn't convey — units, defaults, legacy fields, delivery formats.
- **Worked examples.** At least one, often several. Agents pattern-match on these more than on the schema.
- **Failure modes.** When will this tool refuse? What errors can the model recover from?

Jinja2 variables (`{{ channel_id }}`, `{{ agent_name }}`) must match the `PromptContext` struct used by the renderer. Adding a new var = update the struct and the renderer or the prompt fails to compile at render time.

## Registration

1. Add `pub mod <name>;` to `src/tools.rs` alphabetically within its section.
2. Wire it into the appropriate factory function in `src/tools.rs` based on which ToolServer topology it belongs to (Channel / Branch / Worker / Cortex / Cortex Chat). The module-level doc comment at the top of `src/tools.rs` explains the topology.
3. If the tool holds per-channel state (like `reply`, `branch`, `route`), register it via `add_channel_tools()` / `remove_channel_tools()` per conversation turn, not at ToolServer construction.

## Sandbox Discipline

If the tool reads or writes files on disk, route through `src/sandbox/`. Never `tokio::fs::read(path)` directly — the sandbox enforces the permission model and resolves paths relative to the agent's data directory.

## Cross-Cutting Rules That Apply Here

- Async / state safety: see `.claude/rules/async-state-safety.md` — applies because `src/tools/**/*.rs` interacts with agent state.
- Error handling + `tracing` + naming: see `.claude/rules/rust-essentials.md` and `rust-patterns.md`.

## Verification Before Calling It Done

- `cargo check --all-targets` — the `Tool` trait impl is strict about associated types
- `cargo test --lib` — any tests in the tool module pass
- Grep `src/tools.rs` to confirm the module is declared
- Open the `.md.j2` file and read the first 20 lines out loud — if the purpose isn't obvious to you, it won't be obvious to the LLM
