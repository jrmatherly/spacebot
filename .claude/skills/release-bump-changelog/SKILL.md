---
name: release-bump-changelog
description: Use this skill when preparing a release bump or updating release notes. It writes a launch-style release story from the actual change set, then runs `cargo bump` so the generated GitHub notes and the marketing copy land together in `CHANGELOG.md`.
---

# Release Bump + Changelog

## Goal

Create a version bump commit where each release section includes both:

- a launch-style narrative (marketing copy)
- the exact GitHub-generated release notes

## Workflow

1. Ensure the working tree is clean (except allowed release files).
2. Draft release story markdown from real changes (PR titles, release-note bullets, and diff themes).
   - Target style: similar to the `v0.5.0` narrative (clear positioning + concrete highlights).
   - Keep it factual and specific to the release.
   - Write to a markdown file **inside repo root** under the gitignored `.scratchpad/release/` directory. `scripts/release-tag.sh` enforces inside-repo paths — `$(mktemp)` produces `/var/folders/...` (or `/tmp/...`) on macOS/Linux which fails the path check. Use:
     - `mkdir -p .scratchpad/release`
     - `marketing_file=".scratchpad/release/v<X.Y.Z>-marketing.md"`
     - write markdown content to `$marketing_file`
     - `.scratchpad/` is gitignored (see `.gitignore` line 52), so the file won't appear in the release commit.
   - Writing-guide compliance is mandatory before invoking cargo bump. The release-tag.sh script writes the file's content verbatim into `CHANGELOG.md`, which then triggers the writing-guide PostToolUse hook on the resulting Edit. Self-check before bumping:
     - `grep -nE '[a-z)"0-9]\s*—\s*[a-z]' "$marketing_file" | grep -v '^[0-9]*:- \*\*'` — should return zero (em-dashes inside prose are violations; bullet labels using ` — ` after `**Foo**` are exempt).
     - `grep -nE ';' "$marketing_file"` — should return zero in prose lines (writing-guide replaces all prose semicolons with periods).
3. Run `cargo bump <patch|minor|major|X.Y.Z>` with marketing copy input:
   - `SPACEBOT_RELEASE_MARKETING_COPY_FILE="$marketing_file" cargo bump <...>`
   - This invokes `scripts/release-tag.sh`.
   - The script generates GitHub-native notes (`gh api .../releases/generate-notes`).
   - The script upserts `CHANGELOG.md` with:
     - `### Release Story` (from your marketing file)
     - GitHub-generated notes body
   - The script includes `CHANGELOG.md` in the release commit.
4. Verify results:
   - `git show --name-only --stat`
   - Confirm commit contains `Cargo.toml`, `Cargo.lock` (if present), and `CHANGELOG.md`.
   - Confirm tag was created (`git tag --list "v*" --sort=-v:refname | head -n 5`).

## Requirements

- `gh` CLI installed and authenticated (`gh auth status`).
- `origin` remote points to GitHub, or set `SPACEBOT_RELEASE_REPO=<owner/repo>`.
- Marketing copy is required unless explicitly bypassed with `SPACEBOT_SKIP_MARKETING_COPY=1`.

## Release Story Format

Use markdown only (no outer `## vX.Y.Z` heading; script adds it). Recommended structure:

1. One strong opening paragraph (why this release matters)
2. One paragraph on major technical shifts
3. Optional short highlight bullets for standout additions/fixes

Avoid vague hype. Tie claims to concrete shipped changes.

## Notes

- Do not use a standalone changelog sync script.
- `CHANGELOG.md` is seeded from historical releases and then maintained by the release bump workflow.
