---
name: writing-guide-scan
description: Use this skill to scan staged or working-tree files for writing-guide violations before commit. Catches em-dashes in prose, semicolons in prose, phase-number drift, and common AI-generated phrasing patterns ("Not X. Not Y.", "This isn't X, it's Y") across markdown AND code comments (`//!`, `///`, `--`). The inline pre-commit grep patterns in .claude/rules/writing-guide.md only cover markdown; this skill extends to Rust and SQL comment styles where violations repeatedly landed during Phase 2.
---

# Writing-Guide Scan

## When to Use

Invoke before any commit that touches:
- Documentation (`*.md`, `*.mdx`)
- Rust files with new `//!` module docs or `///` item docs
- SQL migrations with `--` comments
- `CHANGELOG.md` entries
- Design docs in `docs/design-docs/`

Specifically invoke BEFORE `git commit` in session-sync, docs-audit, and phase-wrap workflows where ad-hoc scans have missed violations.

## Invocation

User-only. Not Claude-invokable because the scan is advisory — it shouldn't auto-interrupt an Edit flow. Run manually:

```
/writing-guide-scan
```

Or scope to specific files:

```
/writing-guide-scan CHANGELOG.md docs/design-docs/entra-backfill-strategy.md
```

## The Canonical Patterns

### 1. Em-dash in prose

Pattern: `[a-z)"0-9]\s*—\s*[a-z]`

Applies to: markdown prose, `//!` / `///` / `//` doc comments, SQL `--` comments.

**Allowed exceptions (DO NOT flag):**
- Bullet-point labels: `- **Shell** — run arbitrary commands`
- Numbered-step labels: `1. **Claim** — verbatim quote`
- Table cells: `| Foo | path/to/foo — purpose |`
- Section headers: `## Scale — Targets and Limits`
- Prompt templates under `prompts/**/*.md.j2`

**Forbidden patterns (DO flag):**
- Inside a complete sentence as a substitute for period/comma/colon
- Joining two independent clauses
- Adding an aside or afterthought in prose
- Glueing a trailing fragment to a bullet that is itself a full sentence

### 2. Semicolon-in-prose

Pattern: `[a-z][a-zA-Z)0-9]\s*;\s*[a-zA-Z]`

Applies to the same comment styles as em-dash. Use periods instead.

### 3. Phase-number drift

Pattern: `Phase [0-9]+['']?s\s+[a-z]` (matches `Phase 3's reconciliation`, `Phase 4's helper`, etc.)

This catches forward-pointing phase references that rot when plans are renumbered. Replace with rename-proof language: "the Graph reconciliation loop," "the authz helpers." Exception: phase references inside `.scratchpad/plans/entraid-auth/` itself are intentional and should NOT be flagged.

### 4. "Not X. Not Y." opener

Pattern: `(?m)^Not [a-z].*\.\s*Not [a-z]`

### 5. "This isn't X, it's Y" construction

Pattern: `This isn't .* it's`

### 6. "The result is..." / "The through-line:"

Pattern: `The (result is|through-line:)`

### 7. "No X. No Y." closer

Pattern: `(?m)^No [a-z].*\.\s*No [a-z]`

### 8. Semicolon in prose (strict mode — optional for strict writing contexts)

The skill has two modes:
- **Default:** scan patterns 1-3 (highest-signal drift)
- **`--strict` mode:** adds patterns 4-7 for user-facing README / CHANGELOG

## Scope Rules

Default scope: staged files (`git diff --cached --name-only`).

If no staged files, fall back to the working tree's modified files (`git status --short`).

Target file types only:
- `*.md`, `*.mdx` (markdown)
- `*.rs` (Rust — scan `//!`, `///`, `//` comments only, not string literals or code)
- `*.sql` (SQL — scan `--` comments only)

Skip:
- `prompts/**/*.md.j2` (explicitly exempt per `.claude/rules/writing-guide.md`)
- `vendor/**` (third-party code)
- `target/`, `node_modules/`, `dist/`
- `.scratchpad/plans/entraid-auth/` (phase-number references are intentional there)

## Implementation

### Step 1: Determine scope

```bash
# Prefer staged; fall back to modified
FILES=$(git diff --cached --name-only --diff-filter=ACM | grep -E '\.(md|mdx|rs|sql)$' || true)
if [ -z "$FILES" ]; then
  FILES=$(git diff --name-only --diff-filter=ACM | grep -E '\.(md|mdx|rs|sql)$' || true)
fi
if [ -z "$FILES" ]; then
  echo "No markdown/Rust/SQL files staged or modified — nothing to scan."
  exit 0
fi
```

Or take explicit paths from the user's invocation.

### Step 2: Filter out exempt paths

```bash
FILES=$(echo "$FILES" | grep -vE '^(prompts/.*\.md\.j2$|vendor/|target/|node_modules/|dist/|\.scratchpad/plans/entraid-auth/)')
```

### Step 3: Run the patterns

For each pattern, `grep -nE "<pattern>" $FILES`, collect hits, print grouped by file with line numbers.

### Step 4: Report

If zero violations: print "✅ Writing-guide scan clean across N files" and exit 0.

If violations: print each hit with file:line context, grouped by pattern. Exit 1 (advisory — lets the user decide whether to fix before commit).

### Reporting template

```
🔴 Writing-guide violations found

## Em-dash in prose (N hits)
  path/to/file.md:LL: full line text
  ...

## Semicolon in prose (N hits)
  ...

## Phase-number drift (N hits)
  ...

Run `/writing-guide-scan --strict` for additional AI-phrasing checks.
Reference: .claude/rules/writing-guide.md
```

## Composition

- Run before `/commit-all` or `/commit-commands:commit`
- Run during `/session-sync` before staging doc drift fixes
- Run during `/docs-audit` before applying recommendations
- Pair with the PostToolUse hook (auto-scans on Edit/Write; this skill is manual + broader scope)

## Known Limitations

- Grep-based; won't catch semantic violations ("you can't say it this way") — only mechanical patterns
- Doesn't distinguish a prose em-dash from a valid bullet-label em-dash when both appear on the same line. The allowed-list above is context-dependent; the skill errs toward flagging, user decides.
- Rust comment detection is line-based; won't catch a `//!` that spans multiple lines and violates mid-block. Usually not an issue for short docblocks.

## What this skill does NOT do

- It does not auto-fix. The user decides per-finding whether to edit.
- It does not modify files.
- It does not check external links or verify file-path references (that's docs-audit's job).
- It does not check code quality (that's code-reviewer).
