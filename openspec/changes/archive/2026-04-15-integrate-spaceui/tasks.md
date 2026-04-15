## 1. Worktree Setup and Pre-flight

- [x] 1.1 Create a git worktree on a feature branch: `git worktree add .worktrees/integrate-spaceui -b feat/integrate-spaceui`
- [x] 1.2 In the worktree, verify clean git state and confirm baseline builds: interface dev server (`bun run dev`), Docker (`docker build --target builder .`), Nix (`nix build .#frontend`)
- [x] 1.3 Verify SpaceUI builds standalone (run from the main working tree, not the worktree): `cd /Users/jason/dev/spacebot/.scratchpad/spaceui && bun install && bun run build`

## 2. Move Directory and Rewire Core Paths

- [x] 2.1 Copy SpaceUI into the worktree. Since `.scratchpad/` lives in the main working tree (not the worktree), use the absolute path: `cp -R /Users/jason/dev/spacebot/.scratchpad/spaceui/ spaceui/` — exclude `node_modules/` with `rsync -a --exclude node_modules --exclude '*/dist' /Users/jason/dev/spacebot/.scratchpad/spaceui/ spaceui/` to keep the copy clean
- [x] 2.2 Update `interface/vite.config.ts` line 6: change `../../spaceui/packages` to `../spaceui/packages`
- [x] 2.3 Update `interface/vite.config.ts` line 12: add `framer-motion`, `sonner`, `clsx`, `class-variance-authority` to the `dedupe` array
- [x] 2.4 Update `interface/vite.config.ts`: remove line 97 (`path.resolve(__dirname, "../../spaceui")`) from `server.fs.allow` — it becomes redundant because line 96 (`path.resolve(__dirname, "..")`) already grants access to the project root which contains `spaceui/`
- [x] 2.5 Update `interface/src/styles.css` lines 25-28: change `../../../spaceui/` to `../../spaceui/` in all four `@source` directives
- [x] 2.6 Replace `justfile` lines 27-48: update `spaceui-link` and `spaceui-unlink` recipes to reference `spaceui/` (in-tree) instead of `../spaceui/` (sibling)
- [x] 2.7 Verify: `cd spaceui && bun install && bun run build` succeeds in the worktree
- [x] 2.8 Verify: `cd interface && bun run build` succeeds in the worktree
- [x] 2.9 Verify: `cd interface && bun run dev` starts and UI loads at localhost:19840

## 3. Build System Integration

- [x] 3.1 Update `Dockerfile`: insert COPY commands for SpaceUI package source between lines 46-47 (before `COPY interface/ interface/`)
- [x] 3.2 Update `.dockerignore`: add `spaceui/node_modules/`, `spaceui/packages/*/dist/`, `spaceui/examples/`, `spaceui/.storybook/`, `spaceui/.changeset/`, `spaceui/scripts/` after line 9
- [x] 3.3 Update `flake.nix`: add SpaceUI package source files and config to `frontendSrc` fileset unions after line 72
- [x] 3.4 Update `nix/default.nix`: change frontend derivation `src` from `"${frontendSrc}/interface"` to `frontendSrc`, add `cd interface` to buildPhase, update installPhase to copy from `interface/dist/*`
- [x] 3.5 Update `build.rs`: add `println!("cargo:rerun-if-changed=spaceui/packages/");` after line 12
- [x] 3.6 Verify: `docker build --target builder -t spacebot-test .` succeeds
- [x] 3.7 Verify: `just update-frontend-hash && nix build .#frontend` succeeds
- [x] 3.8 Verify: `cargo build` completes (with or without frontend, depending on bun availability)

## 4. CI and Ownership

- [x] 4.1 Update `.github/workflows/interface-ci.yml`: add `"spaceui/packages/**"` to both push and pull_request path triggers (lines 7 and 12)
- [x] 4.2 Update `.github/CODEOWNERS`: add `spaceui/ @jrmatherly` after line 10

## 5. Documentation

- [x] 5.1 Update `CLAUDE.md`: add `spaceui/` entry to Key Directories section (after line 80) and SpaceUI entry to Package Managers section (after line 39)
- [x] 5.2 Update `CONTRIBUTING.md`: replace lines 110-133 (SpaceUI Packages section) with in-tree workflow instructions
- [x] 5.3 Update `README.md`: replace lines 363-365 (SpaceUI section) to reference in-tree location
- [x] 5.4 Update `.gitignore`: add SpaceUI build artifact exclusions after line 13

## 6. Commit and Verify

- [x] 6.1 Run `cargo fmt --all -- --check` to verify no formatting drift
- [x] 6.2 Stage all changes: `git add spaceui/` then stage all 14 modified files individually
- [x] 6.3 Review staged diff: `git diff --cached --stat` and spot-check key files
- [x] 6.4 Commit with message: `feat: integrate SpaceUI design system into repository`
- [x] 6.5 Run full end-to-end verification: interface dev, interface build, Docker build, Nix build, `just gate-pr`
- [x] 6.6 Clean up worktree if desired: `git worktree remove .worktrees/integrate-spaceui` (after merging)
