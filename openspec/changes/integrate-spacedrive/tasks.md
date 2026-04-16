## 1. Pre-flight

- [ ] 1.1 Confirm clean git state: `git status` shows clean working tree
- [ ] 1.2 Confirm Spacedrive source exists: `ls .scratchpad/spacedrive/Cargo.toml .scratchpad/spacedrive/core/Cargo.toml`
- [ ] 1.3 Confirm Spacebot builds independently: `cargo check`
- [ ] 1.4 Verify Spacebot's Cargo.toml has no `[workspace]` members section: `grep '\[workspace\]' Cargo.toml`

## 2. Copy Spacedrive to Project Root

- [ ] 2.1 Create feature branch: `git checkout -b feat/integrate-spacedrive`
- [ ] 2.2 Copy Spacedrive excluding .git, .github, target, node_modules, .next, dist: `rsync -a --exclude '.git' --exclude '.github' --exclude 'target' --exclude 'node_modules' --exclude '.next' --exclude 'dist' .scratchpad/spacedrive/ spacedrive/`
- [ ] 2.3 Verify copy: `ls spacedrive/Cargo.toml spacedrive/core/Cargo.toml` and `du -sh spacedrive/` (~53 MB)
- [ ] 2.4 Verify rust-toolchain.toml preserved: `cat spacedrive/rust-toolchain.toml` should show channel = "stable"
- [ ] 2.5 Verify .rustfmt.toml preserved: `cat spacedrive/.rustfmt.toml` should show hard_tabs = true

## 3. Add Workspace Exclude Guard

- [ ] 3.1 Add `[workspace]` section with `exclude = ["spacedrive"]` to Spacebot's Cargo.toml before the `[lints.clippy]` section (line 170). Only `spacedrive/` needs exclusion: `spaceui/` has no Cargo.toml (pure Bun workspace), and `desktop/src-tauri/Cargo.toml` is nested two levels deep so auto-discovery would not reach it.
- [ ] 3.2 Verify compilation: `cargo check`
- [ ] 3.3 Verify workspace membership: `cargo metadata --format-version=1 --no-deps | jq -r '.workspace_members[]'` shows only Spacebot's package (one line)
- [ ] 3.4 Verify Spacebot bin targets still build: `cargo build --bins`

## 4. Update .gitignore

- [ ] 4.1 Add Spacedrive exclusions to `.gitignore` after the SpaceUI section: `spacedrive/target/`, `spacedrive/node_modules/`, `spacedrive/apps/*/node_modules/`, `spacedrive/packages/*/node_modules/`, `spacedrive/.next/`

## 5. Update .dockerignore

- [ ] 5.1 Add `spacedrive/` to `.dockerignore` (entire directory excluded — not needed for Spacebot Docker builds)

## 6. Update CODEOWNERS

- [ ] 6.1 Add `spacedrive/ @jrmatherly` to `.github/CODEOWNERS` after the `spaceui/` entry and before the `migrations/` entry (preserves Frontend grouping)

## 7. Update Documentation

- [ ] 7.1 Update CLAUDE.md: add `spacedrive/` to Key Directories section after `spaceui/` or `desktop/`
- [ ] 7.2 Update CONTRIBUTING.md: add Spacedrive section after the Frontend section (after line 134) explaining: (a) independent Cargo workspace — always `cd spacedrive` before running `cargo` commands, (b) separate Rust toolchain (`stable` vs root `1.94.1`) resolved per directory via `rust-toolchain.toml`, (c) separate Bun workspace — `cd` into the target subdir (`spaceui/`, `interface/`, `docs/`, or `spacedrive/`) before running `bun` commands, (d) formatter divergence (hard tabs inside `spacedrive/`), (e) HTTP API communication at runtime on port 19898
- [ ] 7.3 Update README.md: add co-location note to existing "Spacebot + Spacedrive" section (around line 226) — preserve existing subsections

## 8. Commit and Verify

- [ ] 8.1 Run `openspec validate integrate-spacedrive` to verify change artifact structure
- [ ] 8.2 Run `cargo fmt --all -- --check` to verify no formatting drift (Spacedrive's `.rustfmt.toml` must not apply at root)
- [ ] 8.3 Run `cargo check` to verify Spacebot still compiles with workspace exclude
- [ ] 8.4 Verify Docker context excludes Spacedrive: `docker build --target builder -t spacebot-test . 2>&1 | tee /tmp/docker-build.log && ! grep -qi 'spacedrive' /tmp/docker-build.log` (should produce no matches)
- [ ] 8.5 Stage all changes: `git add spacedrive/ Cargo.toml .gitignore .dockerignore .github/CODEOWNERS CLAUDE.md CONTRIBUTING.md README.md`
- [ ] 8.6 Review staged diff: `git diff --cached --stat`
- [ ] 8.7 Commit: `feat: add Spacedrive platform to repository`
- [ ] 8.8 Verify clean state: `git status`
- [ ] 8.9 (Optional cleanup) Remove duplicate source: `rm -rf .scratchpad/spacedrive` once in-tree copy is verified working
