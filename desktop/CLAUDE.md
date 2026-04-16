# Spacebot Desktop (Tauri)

Tauri 2 app that wraps the Spacebot daemon + `interface/` web UI into a native desktop application for macOS, Linux, and Windows.

## Workspace Isolation

`desktop/src-tauri/` is a **separate Cargo crate** with its own `Cargo.lock` and `Cargo.toml`. It is **not** a member of the root workspace.

- Never add `desktop` or `desktop/src-tauri` to the `members` list in the root `Cargo.toml`.
- The root `Cargo.toml` only carries `[workspace] exclude = ["spacedrive"]`. The desktop crate is isolated because `desktop/src-tauri/Cargo.toml` declares its own empty `[workspace]` table, which prevents Cargo from attaching it to the root workspace.
- `cargo build` at the repo root does NOT build the Tauri app.

## Build & Run

Use the just recipes. They wire up the sidecar daemon and the Vite frontend correctly:

```bash
just desktop-dev     # build sidecar + run `tauri dev`
just desktop-build   # build sidecar (release) + run `tauri build`
just bundle-sidecar  # just the sidecar step (builds the spacebot daemon and copies it into Tauri resources)
```

Under the hood:
- `desktop/package.json` scripts `tauri:dev` and `tauri:build` call `../scripts/bundle-sidecar.sh` first, then `tauri dev`/`tauri build`
- `tauri.conf.json` has `beforeBuildCommand` that runs `cd ../interface && bun install --frozen-lockfile && bun run build`, so the UI is always fresh when the Tauri build kicks off
- `frontendDist` points at `../../interface/dist/`

## Package Manager

- **`bun` for JS side** (`desktop/package.json`): `bun install`, never `npm`
- **`cargo tauri` for Rust side**: not `cargo build`. The Tauri CLI handles bundling, code signing, and resource copying.

## macOS Specifics

`tauri.conf.json` declares `identifier: "sh.spacebot.desktop"` and uses `macOSPrivateApi: true` with window transparency + sidebar effects. If code-signing fails on macOS, check `Info.plist` and the Tauri signing docs. Don't `--no-verify` around it.

## Common Pitfalls

- "cargo build fails in desktop" → you're probably in the root. `cd desktop/src-tauri` or use `just desktop-*`.
- "UI changes don't show in the app" → the `beforeBuildCommand` rebuilds `interface/dist/`, but in dev the Vite server must be running. Use `just desktop-dev`, not manual orchestration.
- "Tauri complains about missing sidecar binary" → run `just bundle-sidecar` first. It copies the built daemon into the Tauri resource path.
