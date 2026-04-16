# Rust Iteration Loop

## Goal

Keep the Rust inner loop fast by running the cheapest check that answers the current question. Reserve the full gate for pre-push.

## Core Rule

Run the narrowest tool that can catch the class of bug the current edit introduces. Broader tools are for final verification, not feedback loops.

## Tool Scope

Ordered fastest to slowest on a warm cache. Actual timings vary by machine and cache state; measure once on your setup to calibrate.

| Tool | Canonical invocation | What it catches |
|------|---------------------|-----------------|
| Format check | `just fmt-check` (`cargo fmt --all -- --check`) | Whitespace, import ordering |
| Type check | `just check-all` (`cargo check --all-targets`) | Type errors, syntax, trait resolution |
| Lint | `just clippy-all` (`cargo clippy --all-targets`) | Everything `check` catches, plus lint violations |
| Narrow lint | `cargo clippy --lib` | Same as above, skips test/example/binary targets |
| Single test | `cargo test --lib <name>` | One test's behavior |
| Unit tests | `just test-lib` (`cargo test --lib`) | All unit tests |
| Integration compile | `just test-integration-compile` (`cargo test --tests --no-run`) | Integration test compilation only |
| Full gate | `just gate-pr` | Runs preflight then `scripts/gate-pr.sh` |

## Tool Selection

### Clippy is a superset of check

Never run both in the same step. If clippy passes, check passes.

### Match the check to the change class

| Change class | Minimum check |
|--------------|--------------|
| Pure style (rename, formatting, comment) | `just fmt-check` |
| Lint cleanup (mechanical, clippy-suggested) | `just clippy-all` + warning-count diff |
| Lifetime or type signature changes | `just clippy-all` |
| Control-flow or boolean logic | `cargo test --lib <affected_module>` |
| New feature or bugfix | `just test-lib` |
| Async or state-machine change | `just test-lib` + targeted race tests |

Anything that doesn't fit: the "When to Run the Full Gate" section below applies.

### Defer tests for style-only changes

These cannot change test outcomes. Skip `cargo test` during the edit loop. Run once at the end of the sequence.

- Lint cleanup (auto-deref, lifetime annotation, collapsible-if, type alias)
- Import reordering
- Comment edits
- Doc-string edits
- Renames that compile

### Warning-count delta is the fastest regression signal for lint cleanup

```bash
cargo clippy --all-targets 2>&1 | grep "generated.*warnings" | head -1
```

One run, answers "did my lint fix drop the count as expected?". Don't re-run the full clippy output to re-read the same diagnostics.

**Empty output means 0 warnings.** Clippy only prints the summary line when warnings exist.

## Cadence Checklist

**Per edit (the inner loop):**

- Pick the narrowest check from the table above.
- Run it once.
- Fix what it surfaces.
- Move on.

**Per task (a logical unit that will land as one commit):**

- Run the task-appropriate check once at the end.
- For lint cleanup: one warning-count diff is enough.
- For behavior changes: one `just test-lib` run is enough.

**Per multi-commit series (a stack on one branch):**

- Skip `just gate-pr` between commits in the series.
- Run `just gate-pr` once before push.
- If the last commit is style-only, the gate will pass if earlier commits' tests passed.

## When Expectations Diverge

If a check's output does not match what you expected (wrong warning count, unexpected error, test that should have been unaffected now fails):

- Stop the iteration.
- Read the full output, don't re-run with the same args.
- Identify the actual divergence: is it a stale cache, an unrelated regression, or a scope leak from a prior edit?
- Resolve root cause before continuing. Do not re-run hoping it passes the second time.

## Anti-Patterns

| Anti-pattern | Reason |
|--------------|--------|
| Running `cargo check` and `cargo clippy` back-to-back | Duplicate work; clippy already includes check |
| Running `just test-lib` after a lint fix | Lint fixes are syntactic; tests cannot change |
| Running `just gate-pr` between intermediate commits | Designed for pre-push; don't pay the cost per commit |
| Re-running clippy to re-read the same diagnostics | Pipe to a file once, read the file |

## When to Run the Full Gate

- Before `git push`.
- After a rebase that touched more than five files.
- After a dependency upgrade.
- When diagnosing a test regression that only reproduces on `--all-targets`.

Not:

- Between every commit in a stack.
- After every mechanical lint fix.
- "Just to be safe" without a specific hypothesis.

## Handoff Requirements

When a Rust change is ready for review or merge, the PR summary should include:

- Which checks ran, in what order.
- Which checks were skipped, with the change-class justification.
- The warning-count delta, if lint-relevant.
- The final `gate-pr` outcome.
