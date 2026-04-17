# Coding Discipline

Behavioral guardrails for every code change. These cover how to interpret requests, how much to change, and how to verify "done." For code shape and language conventions, see `rust-essentials.md` and `rust-patterns.md`.

Derived from [Andrej Karpathy's observations](https://x.com/karpathy/status/2015883857489522876) on common LLM coding failures. Upstream guidance: [forrestchang/andrej-karpathy-skills](https://github.com/forrestchang/andrej-karpathy-skills). Long-form examples: [EXAMPLES.md](https://github.com/forrestchang/andrej-karpathy-skills/blob/main/EXAMPLES.md).

## 1. Think Before Coding

Surface assumptions. Do not hide confusion.

- State assumptions explicitly before acting on them.
- If a request has multiple plausible interpretations, list them and pick one with reasoning, or ask.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop and name the confusion. Do not guess.

Spacebot already applies this to failures: CLAUDE.md says "if the same command fails twice, stop and debug root cause." Extend the same reflex to ambiguous requests.

## 2. Simplicity First

Minimum code that solves the problem. Nothing speculative.

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that was not requested.
- No error handling for scenarios that cannot happen.
- If a 200-line solution could be 50, rewrite it.

The test: would a senior Rust engineer call this overcomplicated? If yes, simplify.

## 3. Surgical Changes

Touch only what the request requires. Every changed line should trace directly to the user's ask.

- Do not "improve" adjacent code, comments, or formatting.
- Do not refactor things that are not broken.
- Match existing style, even if you would do it differently.
- If you notice unrelated dead code, mention it. Do not delete it.
- Remove imports, variables, or functions that your change orphaned. Do not remove pre-existing dead code unless asked.

Spacebot specifics:

- The PostToolUse hook runs `cargo fmt --all` on every `.rs` edit. Do not fight the formatter on whitespace or import ordering, but also do not let it mask scope creep in comments or APIs.
- Migration files in `migrations/` are immutable. The PreToolUse hook blocks edits. Always create a new timestamped migration for schema changes.

## 4. Goal-Driven Execution

Define success criteria before implementing. Loop until verified.

Transform tasks into verifiable goals:

- "Add validation" becomes "write tests for invalid inputs, then make them pass."
- "Fix the bug" becomes "write a test that reproduces it, then make it pass."
- "Refactor X" becomes "ensure tests pass before and after."

For multi-step tasks, state the plan inline:

```markdown
1. [Step] -> verify: [check]
2. [Step] -> verify: [check]
3. [Step] -> verify: [check]
```

Strong success criteria let you loop independently. Vague criteria force rework.

### TDD default and escape hatches

Default: for bug fixes and new behavior, write a failing test first.

Escape hatches (every exception must be named in the PR summary):

- Docs-only changes (markdown, prompt templates, reference files).
- Pure config changes with no behavioral surface (dependency version bumps, CI tweaks, lints).
- Async or state-path changes where a targeted repro test is genuinely impractical. The standard here is reasoning, not avoidance: the existing `async-state-safety.md` rule applies, so document terminal states, allowed transitions, race windows, and idempotency reasoning in the PR summary.

## Tradeoff

These rules bias toward caution over speed. For trivial edits (typo fixes, obvious one-liners), use judgment. The cost of rigor on a 5-line fix is not worth it. The cost of skipping rigor on a 500-line refactor always is.
