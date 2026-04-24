# @spacebot/api-client

Internal TypeScript client for the Spacebot REST API. Consumed by `interface/` (React web UI) and `desktop/` (Tauri) through the repo-root workspace symlink. Not published.

## Contents

| File | Role |
|------|------|
| `src/schema.d.ts` | **Generated** OpenAPI-derived types. Hook-blocked from hand edits. |
| `src/types.ts` | Handwritten adapter types + `*ListItem` aliases layered on the schema. |
| `src/client.ts` | Typed `api.*` call helpers + server-URL / token plumbing. |
| `src/authedFetch.ts` | Authenticated fetch wrapper (MSAL.js token refresh, 401 handling, SSE polyfill). |

## Generated-schema precedence

**`src/schema.d.ts` is authoritative for wire shape; handwritten types in `src/types.ts` and `src/client.ts` adapt, never contradict.**

The schema is emitted from `utoipa` annotations on the Rust handler tree under `src/api/**/*.rs`, then converted to TypeScript by `openapi-typescript`. A PreToolUse hook blocks hand edits on `schema.d.ts` and `just check-typegen` fails CI if the committed schema drifts from the current annotations.

In practice, this shapes how handwritten types relate to the schema:

- `types.ts` exports **aliases** over schema components (for example, `ProjectListItem = components["schemas"]["ProjectListItem"]`). The alias gives consumers an ergonomic import path without re-declaring field shapes.
- `types.ts` may **narrow** a schema type (restrict an enum to a known subset, refine a string union) but may not **widen** or **rename** fields. Widening loses the wire contract; renaming forces consumers to maintain a translation layer that will silently drift.
- When a handler changes its Rust response shape, regenerate the schema first (`just typegen`), then update the handwritten adapter to match. The reverse order does not work. Editing `types.ts` to describe a new intended shape and back-filling the Rust handler afterwards will pass tsc but fail the `check-typegen` gate.
- When both a generated and a handwritten definition exist for the same name, prefer the generated one. Delete the handwritten definition and re-export the schema alias instead. Phase 7 PR 5 deleted the handwritten `ProjectListResponse` interface in `client.ts` and re-exported it from `types.ts`; this is the canonical example. `CronListItem` and `PortalConversationListItem` are schema aliases by construction (Phase 7 PR 4) and never had a handwritten predecessor to collapse.

## Regenerating the schema

```bash
just typegen        # regenerates schema.d.ts from Rust utoipa annotations
just check-typegen  # mandatory before committing handler changes
```
