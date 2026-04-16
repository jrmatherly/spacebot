# Spacebot Control UI

Vite + React + TypeScript web app served by the Rust daemon via an embedded Axum HTTP server. Has its own `bun.lock` — independent of the `spaceui/` workspace even though it consumes `@spacedrive/*` packages.

## Package Manager

**`bun` only.** Third reinforcement of the root rule because this is where the drift keeps happening. If `package.json` has a new dep: `bun add <pkg>`, never `npm install`.

## UI Components

Consumed from the `spaceui/` workspace: `@spacedrive/{ai,explorer,forms,primitives,tokens}` at `^0.2.3`. Import from `@spacedrive/primitives`, not from a local `ui/` tree — the inline tree was retired during the spaceui migration (only `SettingSidebarButton.tsx` and `Toggle.tsx` remain as interface-specific widgets).

For styling and component patterns, invoke `/spaceui-dev`.

## API Client (OpenAPI → TypeScript)

The OpenAPI spec is **generated from Rust code**, not hand-edited. Two files are generated in sequence:

1. `cargo run --bin openapi-spec > /tmp/spacebot-openapi.json` — emits the spec from `utoipa` annotations on handlers in `src/api/*.rs`.
2. `bunx openapi-typescript <spec> -o src/api/schema.d.ts` — produces the typed client schema.

**Never hand-edit `interface/src/api/schema.d.ts`.** Run `just typegen` to regenerate.

Before claiming types are correct: `just check-typegen` — regenerates the spec+schema to a temp location and diffs against the committed `schema.d.ts`. CI fails if they differ.

When adding a new route:
- Annotate the handler with `#[utoipa::path(...)]` and register it in `src/api/server.rs`
- Run `just typegen` to regenerate `schema.d.ts`
- Commit both the Rust change and the updated `schema.d.ts`

## Build & Serve

- `bun run dev` — Vite on port 3000, proxies `/api` to the daemon on port 19898
- `bun run build` — writes `interface/dist/`
- The Rust daemon uses `rust-embed` to bake `interface/dist/` into the release binary at compile time. If UI changes don't appear after `cargo build --release`, you forgot to run `bun run build` first.

## Desktop Integration

`desktop/` consumes this interface via Tauri. When `desktop/` is dev/built, it runs `bun install && bun run build` in `interface/` automatically via `beforeBuildCommand`. See `desktop/CLAUDE.md` for the Tauri side.
