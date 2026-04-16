## 1. Pre-flight

- [x] 1.1 Confirm clean git state: `git status` shows clean working tree
- [x] 1.2 Confirm Spacedrive source exists: `ls .scratchpad/spacedrive/Cargo.toml .scratchpad/spacedrive/core/Cargo.toml`
- [x] 1.3 Confirm Spacebot builds independently: `cargo check`
- [x] 1.4 Verify Spacebot's Cargo.toml has no `[workspace]` members section: `grep '\[workspace\]' Cargo.toml`

## 2. Copy Spacedrive to Project Root

- [~] 2.1 ~~Create feature branch~~ — N/A: implementing in `.worktrees/integrate-spacedrive` worktree, branch `integrate-spacedrive` already isolates the work.
- [x] 2.2 Copy Spacedrive excluding .git, .github, target, node_modules, .next, dist. Used absolute source path because `.scratchpad/` is a main-worktree-only artifact: `rsync -a --exclude '.git' --exclude '.github' --exclude 'target' --exclude 'node_modules' --exclude '.next' --exclude 'dist' /Users/jason/dev/spacebot/.scratchpad/spacedrive/ spacedrive/`
- [x] 2.3 Verify copy: `ls spacedrive/Cargo.toml spacedrive/core/Cargo.toml` and `du -sh spacedrive/` (50 MB after exclusions, slightly under 53 MB estimate)
- [x] 2.4 Verify rust-toolchain.toml preserved: `cat spacedrive/rust-toolchain.toml` should show channel = "stable"
- [x] 2.5 Verify .rustfmt.toml preserved: `cat spacedrive/.rustfmt.toml` should show hard_tabs = true

## 3. Add Workspace Exclude Guard

- [x] 3.1 Add `[workspace]` section with `exclude = ["spacedrive"]` to Spacebot's Cargo.toml before the `[lints.clippy]` section (line 170). Only `spacedrive/` needs exclusion: `spaceui/` has no Cargo.toml (pure Bun workspace), and `desktop/src-tauri/Cargo.toml` is nested two levels deep so auto-discovery would not reach it.
- [x] 3.2 Verify compilation: `cargo check`
- [x] 3.3 Verify workspace membership: `cargo metadata --format-version=1 --no-deps | jq -r '.workspace_members[]'` shows only Spacebot's package (one line)
- [ ] 3.4 Verify Spacebot bin targets still build: `cargo build --bins` — deferred to Checkpoint 1 review; `cargo check` already validates type/import surface. Skip per `.claude/rules/rust-iteration-loop.md` (narrowest tool first).

## 4. Update .gitignore

- [x] 4.1 Add Spacedrive exclusions to `.gitignore` after the SpaceUI section: `spacedrive/target/`, `spacedrive/node_modules/`, `spacedrive/apps/*/node_modules/`, `spacedrive/packages/*/node_modules/`, `spacedrive/.next/`

## 5. Update .dockerignore

- [x] 5.1 Add `spacedrive/` to `.dockerignore` (entire directory excluded — not needed for Spacebot Docker builds)

## 6. Update CODEOWNERS

- [x] 6.1 Add `spacedrive/ @jrmatherly` to `.github/CODEOWNERS` after the `spaceui/` entry and before the `migrations/` entry (preserves Frontend grouping)

## 7. Update Documentation

- [x] 7.1 Update CLAUDE.md: add `spacedrive/` to Key Directories section after `spaceui/` or `desktop/`
- [x] 7.2 Update CONTRIBUTING.md: add Spacedrive section after the Frontend section (after line 134) explaining: (a) independent Cargo workspace — always `cd spacedrive` before running `cargo` commands, (b) separate Rust toolchain (`stable` vs root `1.94.1`) resolved per directory via `rust-toolchain.toml`, (c) separate Bun workspace — `cd` into the target subdir (`spaceui/`, `interface/`, `docs/`, or `spacedrive/`) before running `bun` commands, (d) formatter divergence (hard tabs inside `spacedrive/`), (e) HTTP API communication at runtime on port 19898
- [x] 7.3 Update README.md: add co-location note to existing "Spacebot + Spacedrive" section (around line 226) — preserve existing subsections

## 8. Commit and Verify

- [x] 8.1 Run `openspec validate integrate-spacedrive` to verify change artifact structure → "Change 'integrate-spacedrive' is valid"
- [x] 8.2 Run `cargo fmt --all -- --check` to verify no formatting drift (Spacedrive's `.rustfmt.toml` must not apply at root) → exit 0
- [x] 8.3 Run `cargo check` to verify Spacebot still compiles with workspace exclude → exit 0
- [x] 8.4 Verify Docker context excludes Spacedrive: build started, log grepped, **0 matches** for `spacedrive` in build log. Build was stopped after exclusion was confirmed (the test is exclusion of context, not a full Docker build of Spacebot which is already covered by CI).
- [~] 8.5 ~~Single staged commit~~ — implementation produced 3 reviewable commits instead of one monolith: `chore: vendor Spacedrive platform under spacedrive/` (the 50MB copy), `chore(cargo): add workspace exclude guard for spacedrive/`, and `chore: wire spacedrive/ into ignores, codeowners, and docs`. Bisect-friendly.
- [~] 8.6 ~~Review staged diff~~ — superseded by per-commit `git log --shortstat` review.
- [~] 8.7 ~~Single feat commit~~ — see 8.5.
- [ ] 8.8 Verify clean state: `git status` (will run after final tasks.md update)
- [ ] 8.9 (Optional cleanup) Remove duplicate source: `rm -rf .scratchpad/spacedrive` once in-tree copy is verified working — deferred to a follow-up; touches the main worktree, not this branch's responsibility.
