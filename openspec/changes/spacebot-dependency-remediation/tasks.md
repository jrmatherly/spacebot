## 1. Vite Upgrade in Spaceui

- [ ] 1.1 Identify the minimum vite version that exits every range declared in GHSA-4w7w-66w2-5vf9 (target 6.4.2 or later; confirm via `gh api repos/jrmatherly/spacebot/dependabot/alerts/35 -q '.security_advisory.vulnerabilities[]'`)
- [ ] 1.2 Update `vite` version specifier in `spaceui/package.json` to allow resolution to 6.4.2+
- [ ] 1.3 Update `vite` version specifier in `spaceui/.storybook/package.json` to allow resolution to 6.4.2+
- [ ] 1.4 Update `vite` version specifier in `spaceui/examples/showcase/package.json` to allow resolution to 6.4.2+
- [ ] 1.5 Run `bun install` in `spaceui/` (the single workspace lockfile at `spaceui/bun.lock` covers `.storybook/` and `examples/showcase/` workspace members)
- [ ] 1.6 Verify `spaceui/bun.lock` resolves vite to a version outside `<= 6.4.1`, `[7.0.0, 7.3.1]`, and `[8.0.0, 8.0.4]`
- [ ] 1.7 Verify storybook compatibility: storybook 8.6.18 may not fully support vite 6.x. Before running storybook, check the storybook release notes or changelog for vite 6 support. If incompatible, bump storybook to 9.x (or the latest vite-6-compatible release) in `spaceui/package.json` and `spaceui/.storybook/package.json` as part of this task
- [ ] 1.8 Start spaceui storybook via `cd spaceui && bun run storybook` (which `spaceui/package.json` defines as `cd .storybook && bun run dev`); confirm it loads without fatal errors. If it fails after the storybook bump, document the blocker in `docs/security/deferred-advisories.md` and leave the vite alerts open — do NOT dismiss
- [ ] 1.9 Build the spaceui showcase (`cd spaceui/examples/showcase && bun run build`); confirm it exits 0 with a valid bundle output
- [ ] 1.10 Commit the updated `package.json` files and lockfile changes with a descriptive message referencing GHSA-4w7w-66w2-5vf9

## 2. Glib Upgrade in Desktop

- [ ] 2.1 Run `cd desktop/src-tauri && cargo tree -i glib` to identify the transitive chain and any tauri plugins pinning `glib ^0.18`
- [ ] 2.2 Attempt `cd desktop/src-tauri && cargo update -p glib --precise 0.20.X` (substituting latest 0.20.x patch version); record whether the resolver accepts or refuses
- [ ] 2.3 If resolver refuses: bump the blocking tauri plugin's version in `desktop/src-tauri/Cargo.toml` to a release that depends on `glib` 0.20+; re-run step 2.2
- [ ] 2.4 Run `cargo tree -p glib` in `desktop/src-tauri/`; confirm the output shows `glib v0.20.x` or later and no `glib v0.18.x` entries
- [ ] 2.5 Run `just desktop-build` on macOS; confirm it exits 0 and produces a valid binary
- [ ] 2.6 Trigger the Linux CI matrix (or run an equivalent local Linux build if available) to confirm the webkit2gtk path still compiles
- [ ] 2.7 Commit the updated `desktop/src-tauri/Cargo.lock` and `desktop/src-tauri/Cargo.toml` (if plugin bumped) with a descriptive message referencing GHSA-wrw7-89jp-8q8g

## 3. Dependabot Config Expansion

- [x] 3.1 Edit `.github/dependabot.yml` to add a top-of-file YAML comment explaining that this file controls update-PR scoping only, NOT security-alert visibility
- [x] 3.2 Add `package-ecosystem: "cargo"` entry with `directory: "/desktop/src-tauri"`, weekly schedule, commit-message prefix `deps(desktop)`, and `open-pull-requests-limit: 5`
- [x] 3.3 Add `package-ecosystem: "npm"` entry with `directory: "/packages/api-client"`, weekly schedule, commit-message prefix `deps(api-client)`, and `open-pull-requests-limit: 5`
- [x] 3.4 Add `package-ecosystem: "npm"` entry with `directory: "/spaceui"`, weekly schedule, commit-message prefix `deps(spaceui)`, and `open-pull-requests-limit: 5`
- [x] 3.5 Add `package-ecosystem: "npm"` entry with `directory: "/spaceui/.storybook"`, weekly schedule, commit-message prefix `deps(spaceui-storybook)`, and `open-pull-requests-limit: 3`
- [x] 3.6 Add `package-ecosystem: "npm"` entry with `directory: "/spaceui/examples/showcase"`, weekly schedule, commit-message prefix `deps(spaceui-showcase)`, and `open-pull-requests-limit: 3`
- [x] 3.7 Add `package-ecosystem: "npm"` entry with `directory: "/docs"`, weekly schedule, commit-message prefix `deps(docs)`, and `open-pull-requests-limit: 5`
- [x] 3.8 Verify no entry has a `directory:` value starting with `/spacedrive`
- [x] 3.9 Validate the YAML locally (e.g., `python -c 'import yaml; yaml.safe_load(open(".github/dependabot.yml"))'` or equivalent)
- [x] 3.10 Commit the updated `.github/dependabot.yml` with a descriptive message

## 4. Deferred-Advisories Documentation

- [ ] 4.1 Create `docs/security/deferred-advisories.md` with a header explaining the doc's purpose (in-repo tracking of upstream-blocked Dependabot alerts)
- [ ] 4.2 Add an entry for alert #1 (lexical-core 0.7.6, GHSA-2326-pfpj-vx3h): severity low, current 0.7.6, patched 1.0.0, blocker `imap` 3.x migration (async-only API change), unblock trigger = `imap` crate publishes a stable 3.x
- [ ] 4.3 Add an entry for alert #3 (lru 0.12.5 in root `Cargo.lock`, GHSA-rhfx-m35p-ff5j): severity low, current 0.12.5, patched 0.16.3, blocker `lancedb`→`tantivy` 0.24→0.25+ bump, unblock trigger = lancedb releases a version depending on tantivy 0.25+
- [ ] 4.4 Add an entry for alert #15 (rand 0.8.5 in root `Cargo.lock`, GHSA-cq8v-f236-94qc): severity low, current 0.8.5, patched 0.9.3, blocker `rig-core` and `lancedb` transitive pins, unblock trigger = either crate releases using rand 0.9
- [ ] 4.5 Add an entry for alert #18 (rand 0.8.5 in `desktop/src-tauri/Cargo.lock`, GHSA-cq8v-f236-94qc): severity low, blocker tauri transitive chain, unblock trigger = tauri releases using rand 0.9
- [ ] 4.6 Add a footer note explaining the non-dismissal policy: these alerts remain open as tracking signal; do NOT dismiss via GitHub API
- [ ] 4.7 Add a reference to `docs/security/deferred-advisories.md` in `CONTRIBUTING.md` under a security/dependency section (create the section if absent)
- [ ] 4.8 Commit the new doc and `CONTRIBUTING.md` update together

## 5. Verify and Handoff

- [ ] 5.1 After merge, wait for Dependabot rescan to complete (typically within 1 hour of push to main)
- [ ] 5.2 Query `gh api repos/jrmatherly/spacebot/dependabot/alerts/17` (glib) and confirm `state: fixed`; if still `open`, do NOT dismiss — open a follow-up to investigate
- [ ] 5.3 Query `gh api repos/jrmatherly/spacebot/dependabot/alerts/35` and `/36` (vite ×2) and confirm `state: fixed`; if still `open`, do NOT dismiss — open a follow-up
- [ ] 5.4 Confirm total open-alert count equals 52 (55 − 3 fixed): `gh api repos/jrmatherly/spacebot/dependabot/alerts --paginate -q '[.[] | select(.state=="open")] | length'`
- [ ] 5.5 Confirm no alerts have been dismissed as part of this change: `gh api repos/jrmatherly/spacebot/dependabot/alerts --paginate -q '[.[] | select(.dismissed_at != null and (.dismissed_at | fromdate) > (now - 604800))] | length'` should return 0 (no dismissals in the past 7 days)
- [ ] 5.6 Run `cargo audit --ignore RUSTSEC-2023-0071` from repo root; confirm exit 0
- [ ] 5.7 Run `just gate-pr` and confirm it passes
- [ ] 5.8 Open a follow-up issue if any step 5.2 or 5.3 alert did not auto-close, titled "Investigate: Dependabot alert #N did not close after remediation"

## 6. Out of Scope (explicit non-tasks)

The following are deliberately NOT part of this change:

- No `gh api -X PATCH .../dependabot/alerts/{n}` calls for any alert
- No action on the 51 `spacedrive/**` Dependabot alerts
- No CodeQL changes (handled separately by `.scratchpad/codeql-security-findings.md`)
- No `[patch.crates-io]` additions unless specifically documented as interim in step 2.3's failure path
- No test infrastructure added to `packages/api-client/`
- No CI lint recipe for `dependabot.yml` manifest coverage (future hygiene change)
