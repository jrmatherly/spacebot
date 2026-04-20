# Spacebot Control UI

Vite + React + TypeScript web app served by the Rust daemon via an embedded Axum HTTP server. Has its own `bun.lock`. Declares `../spaceui/packages/*` and `../packages/*` as workspace members so `@spacedrive/*` and `@spacebot/*` packages resolve to local source via symlink.

## Package Manager

**`bun` only.** Third reinforcement of the root rule because this is where the drift keeps happening. If `package.json` has a new dep: `bun add <pkg>`, never `npm install`.

## UI Components

Consumed from the `spaceui/` packages under the workspace protocol: `@spacedrive/{ai,explorer,forms,primitives,tokens}` at `workspace:*`. `bun install` creates symlinks from `interface/node_modules/@spacedrive/*` into `../spaceui/packages/*`, so changes to spaceui source appear here without a publish step. Import from `@spacedrive/primitives`, not from a local `ui/` tree. The inline tree was retired during the spaceui migration. Only `SettingSidebarButton.tsx` remains as an interface-specific widget.

Before `bunx tsc --noEmit` in this directory, run `cd ../spaceui && bun install && bun run build`. Each spaceui subpackage points its `types` field at `./dist/index.d.ts`, which tsc requires to resolve the symlinked imports. Vite dev/build does not need the prebuild because Rolldown resolves `.tsx` source directly.

For styling and component patterns, invoke `/spaceui-dev`.

## API Client (OpenAPI → TypeScript)

The API client lives at `packages/api-client/` as the workspace-resolved package `@spacebot/api-client`. Import types from the package (e.g., `import { api } from "@spacebot/api-client/client"`) rather than a local `api/` module. The old `interface/src/api/` directory is gone.

The OpenAPI spec is **generated from Rust code**, not hand-edited. Run `just typegen` to regenerate `packages/api-client/src/schema.d.ts`. Never hand-edit that file.

A PreToolUse hook in `.claude/settings.json` blocks Edit/Write attempts on `schema.d.ts` — the file is generated output, not source.

Under the hood `just typegen` runs two steps:

1. `cargo run --bin openapi-spec > /tmp/spacebot-openapi.json`, which emits the spec from `utoipa` annotations on handlers in `src/api/*.rs`.
2. `bunx openapi-typescript <spec> -o packages/api-client/src/schema.d.ts`, which produces the typed client schema.

Before claiming types are correct, run `just check-typegen`. It regenerates the spec and schema to a temp location and diffs against the committed `schema.d.ts`. CI fails if they differ.

When adding a new route:
- Annotate the handler with `#[utoipa::path(...)]` and register it in `src/api/server.rs`
- Run `just typegen` to regenerate `packages/api-client/src/schema.d.ts`
- Commit both the Rust change and the updated `schema.d.ts`

## Build & Serve

- `bun run dev`: Vite on port 3000, proxies `/api` to the daemon on port 19898
- `bun run build`: writes `interface/dist/`
- The Rust daemon uses `rust-embed` to bake `interface/dist/` into the release binary at compile time. If UI changes don't appear after `cargo build --release`, you forgot to run `bun run build` first.

## Desktop Integration

`desktop/` consumes this interface via Tauri. When `desktop/` is dev/built, it runs `bun install && bun run build` in `interface/` automatically via `beforeBuildCommand`. See `desktop/CLAUDE.md` for the Tauri side.
