# Frontend API Client

The `@spacebot/api-client` package is the single source of truth for the TypeScript surface of the Spacebot daemon's REST API and its server-sent event stream. It is a workspace-only package (not published to npm) consumed by `interface/` and reserved for future internal consumers (e.g., `desktop/`, embeddable widgets).

This capability defines what the package owns, how it is consumed, and how its generated type surface stays in sync with the Rust API implementation.

## ADDED Requirements

### Requirement: Package identity and namespace

The package MUST declare its name as `@spacebot/api-client`. The `@spacedrive/` namespace is reserved for packages vendored from the upstream Spacedrive project (see `spaceui/`); the api-client package is Spacebot-authored and not subject to upstream sync.

#### Scenario: Inspecting the package manifest

- **WHEN** a developer opens `packages/api-client/package.json`
- **THEN** the `name` field equals `"@spacebot/api-client"`
- **AND** the `private` field equals `true`
- **AND** the `type` field equals `"module"`
- **AND** the `exports` field declares the `.`, `./client`, `./types`, and `./schema` entry points

### Requirement: Package is the single source of truth for API types

The package SHALL own all TypeScript type declarations and client helpers that describe the Spacebot daemon's REST API surface and SSE event stream. Code outside the package MUST NOT maintain a parallel copy of the generated schema, the hand-rolled client, or the SSE event interfaces.

#### Scenario: Interface consuming the client

- **WHEN** a component in `interface/src/` needs to call a Spacebot REST endpoint
- **THEN** it imports the `api` object from `@spacebot/api-client/client`
- **AND** it does NOT import from `@/api/client`, `../api/client`, or any other non-package path

#### Scenario: Interface consuming generated types

- **WHEN** a component in `interface/src/` needs a type derived from the OpenAPI schema
- **THEN** it imports from `@spacebot/api-client/types` (for friendly type aliases) or `@spacebot/api-client/schema` (for raw generated `paths`/`components`/`operations` types)
- **AND** no `interface/src/api/` directory exists

### Requirement: OpenAPI codegen emits into the package

The `just typegen` recipe MUST regenerate the TypeScript schema from the Utoipa-annotated Rust API surface and write the output directly into `packages/api-client/src/schema.d.ts`. The `just check-typegen` recipe MUST validate this path.

#### Scenario: Running typegen

- **WHEN** a developer runs `just typegen`
- **THEN** `cargo run --bin openapi-spec` emits the current OpenAPI spec
- **AND** `bunx openapi-typescript` consumes that spec and writes `packages/api-client/src/schema.d.ts`
- **AND** no file under `interface/src/` is written by the recipe

#### Scenario: Verifying typegen sync

- **WHEN** a developer runs `just check-typegen`
- **THEN** the recipe regenerates the spec and schema to a temp location
- **AND** diffs the temp schema against `packages/api-client/src/schema.d.ts`
- **AND** exits zero if and only if the committed file matches what regeneration would produce

### Requirement: Workspace protocol integration

`interface/package.json` MUST declare `@spacebot/api-client` as a `workspace:*` dependency, and its `workspaces` array MUST include `"../packages/*"` so `bun install` resolves the package via symlink rather than falling back to the public npm registry.

#### Scenario: Installing interface dependencies

- **WHEN** a developer runs `bun install` in `interface/`
- **THEN** the preinstall hook (`scripts/check-workspace-protocol.sh`) passes
- **AND** `interface/node_modules/@spacebot/api-client` is a symlink to `../../packages/api-client`
- **AND** no console output mentions fetching `@spacebot/api-client` from a registry

### Requirement: Dependency alert coverage

`.github/dependabot.yml` MUST continue to track `packages/api-client/` under `package-ecosystem: "npm"` so that any npm dependency of the api-client (e.g., `openapi-fetch` if reintroduced, or typing libraries) surfaces security advisories.

#### Scenario: Dependabot configuration snapshot

- **WHEN** a developer opens `.github/dependabot.yml`
- **THEN** it contains an entry with `package-ecosystem: "npm"` and `directory: "/packages/api-client"`

### Requirement: Package export surface

The package MUST expose the following entry points via its `exports` field. Consumers SHOULD import from subpath exports (`/client`, `/types`, `/schema`) rather than the root to enable tree-shaking and to signal intent.

| Entry point | Source file | Contents |
|---|---|---|
| `.` | `./src/index.ts` | Re-exports from `./client` and `./types` (the default surface). |
| `./client` | `./src/client.ts` | The hand-rolled `api` object with per-endpoint helpers, `fetchJson`, `setServerUrl`, `getApiBase`, and inline SSE event interfaces (`InboundMessageEvent`, `OutboundMessageEvent`, and the full `ApiEvent` union). |
| `./types` | `./src/types.ts` | Friendly type aliases derived from the generated schema (`StatusResponse`, `HealthResponse`, etc.). |
| `./schema` | `./src/schema.d.ts` | Raw generated types (`paths`, `components`, `operations`) from `openapi-typescript`. |

SSE event interfaces MUST be exported from `./client` only, not from a dedicated `./events` subpath. The package does not declare an `./events` export and importing `@spacebot/api-client/events` will fail with a module-not-found error.

#### Scenario: Subpath import

- **WHEN** a consumer writes `import { api } from "@spacebot/api-client/client"`
- **THEN** TypeScript resolves the import to `packages/api-client/src/client.ts`
- **AND** the `api` export is available at runtime after `bun run build`

#### Scenario: Root entry point import

- **WHEN** a consumer writes `import { api } from "@spacebot/api-client"` (root, no subpath)
- **THEN** TypeScript resolves the import to `packages/api-client/src/index.ts`, which re-exports from `./client` and `./types`
- **AND** all exports from `./client` and `./types` are available
- **AND** this form is permitted but subpath imports are preferred for tree-shaking and intent-signaling

#### Scenario: Raw schema import

- **WHEN** a consumer needs a raw `paths`-indexed operation type
- **THEN** they import from `@spacebot/api-client/schema` (not `/types`)

#### Scenario: SSE event type import

- **WHEN** a consumer needs an SSE event interface (`InboundMessageEvent`, `OutboundMessageEvent`, `ApiEvent`, etc.)
- **THEN** they import from `@spacebot/api-client/client` (where the types are inlined)
- **AND** they do NOT import from `@spacebot/api-client/events` (which does not exist as an export)

### Requirement: Backwards-compatibility scope

Because `interface/` is the sole consumer of the package, backwards compatibility with older import paths is NOT required. The `@/api/*` alias MUST be removed from active use (though the `@/` alias itself MAY remain for non-api-related `interface/src/` paths).

#### Scenario: Checking for legacy imports

- **WHEN** a grep runs across `interface/src/` for the pattern `"@/api/"`
- **THEN** zero matches are returned
- **AND** all equivalent imports have been migrated to `"@spacebot/api-client/*"`

### Requirement: Dead-code elimination

The package SHOULD NOT co-resident two client implementations without documented differentiation. If a second client style (e.g., an `openapi-fetch`-based generic wrapper) is added alongside the existing hand-rolled client, the package's README (or a top-of-file doc comment in each client source) MUST document when to use each style and why both exist.

As of this change's initial merge, the package contains exactly one client (`client.ts` — hand-rolled) and no aspirational siblings.

#### Scenario: Inspecting the package source at initial merge

- **WHEN** a developer opens `packages/api-client/src/` immediately after this change merges
- **THEN** it contains exactly: `client.ts`, `types.ts`, `schema.d.ts`, `index.ts`
- **AND** no `client-typed.ts`, `events.ts`, or other speculative files are present

#### Scenario: Adding a second client later

- **WHEN** a future OpenSpec change adds a second client implementation to the package
- **THEN** the change MUST include documentation (package README or file-level doc comment) explaining the purpose of each client and when to prefer one over the other
- **AND** consumers must not be left to guess which to import
