# Spacebot Control UI — Original Bootstrap Plan (Historical)

> **Status:** This document captures the original Phase 1–5 bootstrap plan for `interface/`. Phases 1–3 are complete: the axum HTTP server, Vite + React + TypeScript scaffolding, and the UI component set now live in the tree (the UI set migrated from an inline `src/ui/` tree to `@spacedrive/primitives` + siblings from the `spaceui/` workspace). Tailwind is on v4, not v3 as this document originally specified. Use it for historical context only; for current-state conventions, see `CLAUDE.md`, `spaceui/README.md`, and `spaceui/INTEGRATION.md`.

A self-contained React web app living at `interface/` in the Spacebot repo, served by the Rust daemon via an embedded axum HTTP server. Copies Spacedrive's UI component library and color system. Uses spec-first OpenAPI with `openapi-fetch` + `openapi-typescript` for the data layer, TanStack Query for caching, and `rust-embed` to bake the built assets into the single binary.

## Directory Structure

```
interface/
├── package.json
├── bunfig.toml
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── tailwind.config.ts
├── postcss.config.js
├── index.html
├── openapi.yaml
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/
│   │   ├── schema.d.ts       # generated from openapi.yaml via openapi-typescript
│   │   └── client.ts         # openapi-fetch client instance
│   ├── ui/                   # copied + adapted from @sd/ui
│   │   ├── index.ts
│   │   ├── style/
│   │   │   ├── colors.scss
│   │   │   └── style.scss
│   │   ├── Button.tsx
│   │   ├── Input.tsx
│   │   ├── Dialog.tsx
│   │   ├── Toast.tsx
│   │   ├── Select.tsx
│   │   ├── Switch.tsx
│   │   ├── Tabs.tsx
│   │   ├── Tooltip.tsx
│   │   ├── ContextMenu.tsx
│   │   ├── DropdownMenu.tsx
│   │   ├── Dropdown.tsx
│   │   ├── Popover.tsx
│   │   ├── Slider.tsx
│   │   ├── CheckBox.tsx
│   │   ├── RadioGroup.tsx
│   │   ├── ProgressBar.tsx
│   │   ├── CircularProgress.tsx
│   │   ├── Loader.tsx
│   │   ├── Divider.tsx
│   │   ├── Typography.tsx
│   │   ├── Shortcut.tsx
│   │   ├── Resizable.tsx
│   │   ├── Layout.tsx
│   │   ├── InfoBanner.tsx
│   │   ├── SearchBar.tsx
│   │   ├── TopBarButton.tsx
│   │   ├── TopBarButtonGroup.tsx
│   │   ├── ShinyButton.tsx
│   │   ├── ShinyToggle.tsx
│   │   ├── utils.tsx
│   │   └── forms/
│   │       ├── index.ts
│   │       ├── Form.tsx
│   │       ├── FormField.tsx
│   │       ├── CheckBoxField.tsx
│   │       ├── InputField.tsx
│   │       ├── SwitchField.tsx
│   │       ├── SelectField.tsx
│   │       ├── TextAreaField.tsx
│   │       └── RadioGroupField.tsx
│   ├── hooks/
│   ├── routes/
│   └── components/
```

## Phases

### Phase 1 — Rust HTTP Server

Add axum + tower-http + rust-embed to the daemon. Wire up a new TCP listener alongside the existing Unix socket IPC in the main event loop.

**Changes:**
- `Cargo.toml` — add `axum`, `tower-http` (cors, static files), `rust-embed`
- `src/api.rs` (new module root) — axum Router, state, middleware
  - `src/api/server.rs` — `start_http_server()` returns a shutdown handle, serves on configurable port (default `19898`)
  - `src/api/static_files.rs` — serve embedded frontend assets with SPA fallback (all non-API routes return `index.html`)
- `src/main.rs` — add HTTP server startup between IPC server start and the main event loop. Add it to the `tokio::select!` shutdown group.
- `src/config.rs` — add `[api]` section with `port`, `bind`, `enabled` fields
- Initial route: `GET /api/health` returning `{"status": "ok"}`

The embedded assets come from `interface/dist/` — the Vite build output. During development, you run Vite's dev server separately and it proxies `/api` to the Rust daemon (same pattern as Spacedrive's web app).

### Phase 2 — Interface Scaffolding

Bootstrap the `interface/` directory with bun, Vite, React, TanStack Query, Tailwind.

**Steps:**
1. `bun init` in `interface/`
2. Install core deps: `react`, `react-dom`, `@tanstack/react-query`, `@tanstack/react-router` (or `react-router-dom`), `openapi-fetch`, `openapi-typescript`
3. Install UI deps (matching Spacedrive): `@radix-ui/*`, `@react-spring/web`, `class-variance-authority`, `clsx`, `sonner`, `@phosphor-icons/react`, `react-hook-form`, `@hookform/resolvers`, `zod`, `framer-motion`, `valtio`, `use-debounce`, `react-resizable-layout`, `react-loading-icons`, `rooks`, `@fontsource/ibm-plex-sans`
4. Install dev deps: `tailwindcss@3`, `postcss`, `autoprefixer`, `@tailwindcss/forms`, `@tailwindcss/typography`, `tailwindcss-animate`, `tailwindcss-radix`, `sass`, `typescript`, `@types/react`, `@types/react-dom`, `@vitejs/plugin-react`, `vite`
5. Create config files: `vite.config.ts`, `tailwind.config.ts`, `postcss.config.js`, `tsconfig.json`, `index.html`
6. Vite dev server configured to proxy `/api` to `localhost:19898`

### Phase 3 — Copy UI Components

Copy the full `@sd/ui` component set from Spacedrive into `interface/src/ui/`. Adapt imports — remove `@sd/` workspace references, flatten everything into the local `ui/` module.

**Key adaptations:**
- Remove `react-router-dom` dependency from `Button.tsx` (`ButtonLink`) — either adapt to whatever router we pick or drop it
- Remove any references to `@sd/ts-client` or `@sd/interface` types
- Copy `colors.scss` and `style.scss` verbatim — the color system is self-contained CSS variables
- Copy the `tailwind.js` config factory, convert to a static `tailwind.config.ts`
- Copy the font imports (`@fontsource/ibm-plex-sans`)
- The `tw()` utility, `cva`/`cx` re-exports, and all Radix-based components should work as-is with import path changes

### Phase 4 — OpenAPI Spec + API Layer

Write the initial OpenAPI spec and wire up the TypeScript client.

**OpenAPI spec (`interface/openapi.yaml`):**
- `GET /api/health` — health check
- `GET /api/agents` — list agents with status
- `GET /api/agents/{id}` — agent detail (config, identity, memory stats)
- `GET /api/agents/{id}/conversations` — recent conversations
- `GET /api/agents/{id}/memories` — memory search/browse
- `GET /api/agents/{id}/cron` — cron job list
- `POST /api/agents/{id}/cron` — create/update cron job
- `GET /api/status` — daemon status (uptime, active channels, worker counts)

**Rust side (`src/api/`):**
- `src/api/routes.rs` — handler functions matching the spec
- `src/api/state.rs` — `ApiState` holding `Arc<HashMap<AgentId, Agent>>` references and shared deps
- Use `utoipa` for spec validation (optional — the YAML is the source of truth, but utoipa can validate that handlers match)

**TypeScript side:**
- `bun run generate` script runs `openapi-typescript openapi.yaml -o src/api/schema.d.ts`
- `src/api/client.ts` creates the `openapi-fetch` client pointed at `/api`
- Custom hooks wrapping `@tanstack/react-query` with the typed client

### Phase 5 — Shell UI

Build the actual control panel pages.

- Dashboard: daemon status, agent overview cards, active channel count, memory stats
- Agent detail: identity files (read-only initially), memory browser, conversation list, cron management
- Sidebar navigation, theme switcher (leveraging the copied Spacedrive theme system)

This phase is iterative and doesn't need full spec upfront.

## Build Integration

- `.gitignore` — add `interface/node_modules/`, `interface/dist/`
- `rust-embed` points at `interface/dist/` — if the directory doesn't exist at compile time, the binary builds without a frontend (API-only mode)
- `scripts/build.sh` — runs `bun install && bun run build` in `interface/`, then `cargo build --release`
- In dev: run `bun run dev` in `interface/` (Vite on port 3000, proxying to Rust on 19898) + `cargo run -- start -f -d` separately

## Dependencies

### Rust (new)

| Crate | Purpose |
|-------|---------|
| `axum` | HTTP framework |
| `tower-http` | CORS, compression, static files |
| `rust-embed` | Embed frontend assets in binary |

### TypeScript

| Package | Purpose |
|---------|---------|
| `react` + `react-dom` | UI framework |
| `@tanstack/react-query` | Data fetching + caching |
| `openapi-fetch` | Type-safe API client |
| `openapi-typescript` | Generate TS types from OpenAPI spec |
| `@radix-ui/*` | Headless UI primitives |
| `class-variance-authority` + `clsx` | Variant styling |
| `tailwindcss@3` + plugins | Utility CSS |
| `sonner` | Toast notifications |
| `@phosphor-icons/react` | Icons |
| `vite` + `@vitejs/plugin-react` | Build tooling |
| `react-hook-form` + `zod` | Form handling |
