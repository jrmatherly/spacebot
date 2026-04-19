# Desktop Sidecar

> **Status:** Implemented. Sidecar naming convention established in PR #58 (`7e8e293`, 2026-04-17) with the APFS case-collision fix landed at commit `4d07189`. Structural enforcement hardened in PR #60 (Tier 2 guard + CI + UX) and PR #58 (Tier 1 follow-ups). This document captures the architectural rationale; operator-facing rules live in `desktop/CLAUDE.md` Common Pitfalls.

Research and rationale for the Tauri sidecar wiring in `desktop/` — how the Spacebot daemon becomes a bundled resource inside the native desktop app, why it must be named `spacebot-daemon-<triple>` rather than `spacebot-<triple>`, and how four separate reference sites stay in agreement via a build-time guard.

## Scope

**In scope.** The sidecar packaging and launch contract between `desktop/src-tauri/` (Tauri host) and the Spacebot daemon binary. The APFS case-insensitivity constraint and why it mandates the `-daemon-` infix. The four reference sites that must agree on the name. The `scripts/bundle-sidecar.sh` build flow and `scripts/check-sidecar-naming.sh` guard. The CI matrix that smoke-tests the sidecar spawn path.

**Out of scope.** Tauri plugin configuration, window-management policy, code-signing (`Info.plist`), the `interface/` web UI itself, and the details of the Spacebot daemon's command-line interface (`src/main.rs` CLI). The desktop app's user-facing behavior ("Start Local Server" button) is mentioned only where it interacts with sidecar naming.

## Ground truth

| Fact | Source |
|---|---|
| Tauri version | 2.x, declared in `desktop/src-tauri/Cargo.toml` |
| Host binary name | `Spacebot` (capital S), from `desktop/src-tauri/Cargo.toml` `[[bin]] name = "Spacebot"` |
| Sidecar binary name | `spacebot-daemon-<triple>[.exe]` |
| Sidecar source | Spacebot's own root-crate build (`cargo build [--release]`) produces `target/<mode>/spacebot`, which is copied and renamed into `desktop/src-tauri/binaries/` |
| Tauri's sidecar convention | `binaries/<name>-<target-triple>[.exe]`, resolved at runtime via `externalBin` in `tauri.conf.json` |
| APFS case-sensitivity | Default macOS filesystem is case-insensitive. `Spacebot` and `spacebot` resolve to the same inode in a single directory. |
| NTFS case-sensitivity | Same default — case-insensitive. |
| Bundle script | `scripts/bundle-sidecar.sh` |
| Guard script | `scripts/check-sidecar-naming.sh` (wired into `scripts/gate-pr.sh`) |
| Reference sites today | `scripts/bundle-sidecar.sh`, `desktop/src-tauri/tauri.conf.json` `externalBin`, `desktop/src-tauri/capabilities/default.json` `shell:allow-spawn` name, `interface/src/components/ConnectionScreen.tsx` `spawnBundledProcess` argument, `docs/content/docs/(getting-started)/desktop.mdx`, `.github/workflows/desktop-ci.yml` |

## The APFS collision

The root `Cargo.toml` builds a binary called `spacebot`. The desktop crate's `Cargo.toml` builds a binary called `Spacebot` (capital S, Tauri convention for app names). During a desktop development build, both binaries land under the same `target/` directory tree.

On APFS (macOS default) and NTFS (Windows default), filenames are compared case-insensitively at the filesystem level. `target/debug/spacebot` and `target/debug/Spacebot` resolve to the same inode. The filename that wins is whichever was written last.

Tauri's sidecar lookup at runtime takes the basename from `externalBin` in `tauri.conf.json` (plus the target triple) and opens the file under `binaries/`. If the basename matches the host binary's basename case-insensitively, the lookup succeeds but points at the desktop host's own executable. The result: clicking "Start Local Server" spawns a second copy of the Tauri host app instead of starting the daemon.

The fix is structural: name the sidecar something that **cannot** collide case-insensitively with the host. `spacebot-daemon-<triple>` accomplishes this. `Spacebot` vs `spacebot-daemon-...` differs by more than case.

This was a real user-facing bug. The original naming `spacebot-<triple>` worked on Linux (case-sensitive ext4) and broke silently on macOS with behavior that looked like "the button does nothing" until closer inspection revealed a second host-app window behind the first.

## Why a `-daemon-` infix specifically

Four alternatives were considered:

- **`spacebot_daemon-<triple>` (underscore).** Tauri convention is hyphenated. Mixing conventions invites later confusion.
- **`spacebot-cli-<triple>`.** Less accurate. The sidecar is not a CLI, it is the full server daemon.
- **`sbd-<triple>` (abbreviation).** Unsearchable. `grep sbd` across the repo returns noise.
- **`spacebot-daemon-<triple>`.** Matches Tauri hyphen convention, accurately describes the binary, greps cleanly.

The last option won. The name appears in user-visible error messages ("Cannot spawn spacebot-daemon-x86_64-apple-darwin"), so accuracy matters.

Renaming the **host** binary to avoid the collision was also considered. Rejected: the host binary name is what macOS shows as the application name in the dock, the menu bar, and `About`. Renaming the host from `Spacebot` to anything else would require re-doing branding surfaces and code-signing metadata for no structural gain.

## Structural enforcement

Documentation alone is not enough. Four reference sites need to agree on `spacebot-daemon` as a literal string:

1. `scripts/bundle-sidecar.sh` — writes the binary with that name.
2. `desktop/src-tauri/tauri.conf.json` — declares `externalBin` for Tauri's packaging.
3. `desktop/src-tauri/capabilities/default.json` — whitelists the binary under `shell:allow-spawn`.
4. `interface/src/components/ConnectionScreen.tsx` — calls `spawnBundledProcess` with the literal name.

Two more supporting sites reference the name:

5. `docs/content/docs/(getting-started)/desktop.mdx` — user-facing docs.
6. `.github/workflows/desktop-ci.yml` — smoke-test assertion.

A contributor who renames the sidecar in one place and forgets the others reintroduces the collision symptom silently. `scripts/check-sidecar-naming.sh` enforces two invariants:

- **Sync invariant.** The sidecar basename derived from `scripts/bundle-sidecar.sh` must appear verbatim at every enumerated reference site. The script also runs a grep cross-check across the repo: any unregistered reference to `spacebot-daemon` is a guard failure, which forces new reference sites to be added to the `KNOWN_SITES` list intentionally.
- **Collision invariant.** `lowercase(sidecar_basename)` must differ from `lowercase(host_bin_name)` extracted from `desktop/src-tauri/Cargo.toml`'s `[[bin]] name = "..."` stanza. A future rename that accidentally re-collides trips the guard at build time.

The guard runs from `scripts/gate-pr.sh`, so every PR passes through it before the rest of the gate. Contributors who work on tooling caches (`.serena/`, `.code-review-graph/`, `binaries/`) used to hit false-positive failures because those gitignored directories contain `spacebot-daemon` strings. PR #61's hardening expanded the `--exclude-dir` list to 17 entries covering the full set of tooling caches. See `CHANGELOG.md` under the PR #61 Fixed entry.

## Why the host binary and the sidecar share a directory

A cleaner architecture would isolate the sidecar build output from the desktop host's. The current setup places both under `target/` because:

- Spacebot's root `cargo build` writes to `target/` by design.
- Tauri's `cargo tauri build` also writes to `target/` by convention.
- Tauri's `beforeBuildCommand` runs `scripts/bundle-sidecar.sh`, which copies the daemon binary out of `target/` and into `desktop/src-tauri/binaries/`. At bundle time the sidecar is no longer under `target/`.

The collision is only a problem during **dev** mode, when Tauri looks up the sidecar from a location that could still contain both binaries. The `-daemon-` infix solves the collision at the name level, which is more robust than enforcing a directory separation that the Tauri toolchain would fight.

## The CI matrix

Prior to PR #60, no GitHub workflow built or smoke-tested the desktop app on any platform. The bug that motivated the rename reached a user because nothing exercised the sidecar spawn path in CI.

`.github/workflows/desktop-ci.yml` now covers:

- Build the sidecar via `scripts/bundle-sidecar.sh` on macOS (APFS), Linux (ext4), and Windows (NTFS) targets.
- Run `scripts/check-sidecar-naming.sh` on every platform so the guard itself stays portable.
- Smoke-test that the daemon binary exists at the expected path and is executable.

The matrix does not yet drive the Tauri app end-to-end ("Start Local Server" → daemon boots → `/api/health` responds). That is follow-up work; the current matrix catches the rename-drift class of bug, which was the demonstrated failure mode.

## Relationship to the cross-platform fork

The desktop app runs on three targets (macOS, Linux, Windows) via one shared Tauri binary description. The sidecar itself is also cross-platform because Spacebot's root binary is. The target-triple suffix (`-x86_64-apple-darwin`, `-aarch64-apple-darwin`, `-x86_64-unknown-linux-gnu`, etc.) is produced by `scripts/bundle-sidecar.sh` using `rustc -vV | awk '/^host:/ {print $2}'` and honored by Tauri's sidecar lookup.

Windows MSVC builds currently do not compile as of PR #60 and are carved out as a separate openspec change (see the 2026-04-18 audit cycle's W6 track). The sidecar naming convention is ready for Windows once the build is restored; no additional design change is expected.

## Relationship to the Spacedrive integration

The desktop app is independent of Spacedrive. Nothing in `desktop/` or `scripts/bundle-sidecar.sh` references Spacedrive. If the desktop app ever grows a "Start Local Spacedrive" button, a similar sidecar-wiring problem surfaces: the Spacedrive `sd-server` binary needs its own `externalBin` registration, its own capabilities allow-list entry, and its own guard coverage. That work is deferred until demand surfaces; the naming convention for a hypothetical second sidecar would be `sd-server-<triple>`, which does not collide with either `Spacebot` or `spacebot-daemon`.

## Future work not in scope here

- **End-to-end CI smoke test.** The current matrix builds the sidecar and checks naming. Running the Tauri app headlessly and asserting the daemon responds to `/api/health` within 30 seconds is a natural next step.
- **Stderr plumbing.** The sidecar's stderr today goes to Tauri's stdout capture; the review in `.scratchpad/2026-04-18-desktop-sidecar-review-findings.md` P-1 noted uncertainty about whether the frontend sees readiness signals. A dedicated stderr channel would harden the "daemon is ready" UX signal.
- **Orphan-on-quit.** If the Tauri host crashes mid-session, the sidecar can survive as an orphan. Proper cleanup requires the host to track the child PID and send SIGTERM on exit.
- **Startup timeout + log surfacing.** Today the "Start Local Server" button has no timeout visible to the user. If the sidecar fails to come up within a reasonable window, the UI should surface logs instead of waiting indefinitely.
