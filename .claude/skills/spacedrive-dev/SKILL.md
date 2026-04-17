---
name: spacedrive-dev
description: Specialized Spacedrive development skill for AI coding assistants working on Spacedrive integration. Covers the VDFS architecture, CQRS operations, library system, hybrid indexing, P2P sync (HLC/CRDT), Iroh networking, extension system (WASM), virtual sidecars, CLI commands, React hooks, semantic color system, and the Spacebot-Spacedrive integration contract (connection modes, remote execution, file system intelligence, ContextNode). Use when working on Spacedrive integration, the vendored `spacedrive/` subtree at the repo root, P2P networking, file indexing, sync systems, or the Spacebot-Spacedrive contract.
---

# Spacedrive Development Guide

Spacedrive is a distributed, local-first Virtual Distributed File System (VDFS) built in Rust. Data stays on user devices. No cloud servers control files. This skill covers the architecture and integration points relevant to working on Spacedrive and its integration with Spacebot.

## Architecture Overview

A headless Rust daemon manages the VDFS core, library database, background jobs, and RPC interface. Clients (desktop, mobile, CLI) connect to the daemon and contain no business logic.

**CQRS over RPC**: Every operation is a Command (Preview -> Commit -> Verify lifecycle) or a Query (read-only). Code lives in `core/src/ops/` (per-domain handlers) and `core/src/service/` (long-running services).

**Key services**: NetworkService (P2P), SyncService (metadata sync), JobManager (durable background tasks).

**Mobile**: iOS and Android embed the entire Rust core in the app binary. Phones are complete Spacedrive nodes.

**Startup**: Daemon Launch -> Library Load (SQLite) -> Service Activation (network announce) -> Client Connection (RPC socket).

## Data Model

**Database**: SQLite with SeaORM. WAL mode, NORMAL synchronous, 10000 cache_size. Each library embeds its own SQLite DB.

**Two modeling layers**: Domain Models (rich, computed fields) and Database Entity Models (direct table mapping).

### Core Entities

**`SdPath`** (universal addressing):
```rust
pub enum SdPath {
    Physical { device_slug: String, path: PathBuf },
    Cloud { service: CloudServiceType, identifier: String, path: String },
    Content { content_id: Uuid },
    Sidecar { content_id: Uuid, kind: String, variant: String, format: String },
}
```
URI forms: `local://{device-slug}/{path}`, `s3://{bucket}/{path}`, `gdrive://`, `content://{uuid}`

**`Entry`**: File/directory. Fields: `id` (int PK), `uuid`, `name`, `kind` (0=File, 1=Dir, 2=Symlink), `extension`, `parent_id`, `metadata_id`, `content_id`, `volume_id`, `size`, `aggregate_size`, `child_count`, `file_count`, `permissions`, `inode`, timestamps.

**`ContentIdentity`**: Unique file content for dedup. Two-stage BLAKE3 hashing: `content_hash` (fast sampled ~58KB) and `integrity_hash` (full). Deterministic v5 UUID from content_hash.

**`UserMetadata`**: Organization data. Fields: `notes`, `favorite`, `hidden`, `custom_data` (JSON). Scoped to entry or content identity.

**`Tag`**: Graph-based semantic tags with `canonical_name`, `display_name`, `namespace`, `tag_type`, `color`, `icon`, `is_organizational_anchor`, `search_weight`, `attributes` (JSON), `composition_rules` (JSON).

**`TagRelationship`**: `parent_tag_id`, `child_tag_id`, `relationship_type` ("parent_child"|"synonym"|"related"), `strength` (0.0-1.0).

**`Location`**: Monitored directories. `device_id`, `entry_id` (root), `index_mode` ("shallow"|"content"|"deep"), `scan_state`, `job_policies` (JSON).

**`Device`**: Machines. `uuid`, `name`, `slug`, `os`, `hardware_model`, `network_addresses` (JSON), `is_online`, `capabilities` (JSON), `sync_enabled`.

**`Volume`**: Physical drives. `device_id`, `fingerprint` (stable across mounts), capacities, read/write speed, `is_removable`, `cloud_identifier`.

**`Sidecar`**: Generated content. Links to ContentIdentity (not Entry). `kind` ("thumbnail"|"preview"|"metadata"), `variant`, `format`, `status` ("pending"|"processing"|"ready"|"error").

**Hierarchy**: Closure table (`EntryClosure { ancestor_id, descendant_id, depth }`) for O(1) queries. `directory_paths` table caches full absolute paths.

**Ownership chain**: Device -> Volume -> Location/Entry. Entries inherit sync ownership from their volume's device.

## Library System

A library is a self-contained `.sdlibrary` directory:
- `library.json` (config: version, id, name, settings, statistics)
- `database.db` (SQLite)
- `thumbnails/` (two-level sharded, content-addressed)
- `.sdlibrary.lock` (prevents concurrent access)

Copy the entire folder for a complete backup. Zero configuration after copying.

## File Indexing

### Hybrid Indexing Engine

**Ephemeral Layer** ("File Manager" mode): Memory-resident, ~50 bytes/entry, custom slab allocators (`NodeArena`), string interning (`NameCache`). Millions of files in RAM, zero DB I/O.

**Persistent Layer** ("Library" mode): Full pipeline to SQLite with content analysis, dedup, closure tables.

**Seamless State Promotion**: Ephemeral folder added as location preserves UUIDs, UI doesn't flicker, indexer resumes.

### Five Phases
1. **Discovery** — Parallel async walk, work-stealing, `IndexerRuler` (NO_HIDDEN, NO_DEV_DIRS, .gitignore), batches of 1000
2. **Processing** — Topology-sorted batch inserts, `ChangeDetector` (New/Modified/Moved/Deleted)
3. **Aggregation** — Bottom-up `aggregate_size`, `child_count` via closure table
4. **Content Identification** — BLAKE3 hashing, deterministic v5 UUIDs, `FileTypeRegistry`
5. **Finalizing** — Post-processing, thumbnail generation dispatch

**Index modes**: Shallow (<500ms), Content (~1K files/sec), Deep (~100 files/sec)

**State persistence**: `IndexerState` serialized as MessagePack. Checkpoints every 5000 items or 30 seconds.

## Sync System

Leaderless P2P. Every device is equal. No central server.

### Two Protocols

**Device-owned** (locations, files, volumes): Owner broadcasts `StateChange` in real-time. Pull-based backfill. No conflicts possible.

**Shared resources** (tags, collections, metadata, content IDs): Any device modifies. HLC-ordered log. Default LWW resolution.

### Hybrid Logical Clock (HLC)
`{ timestamp: u64, counter: u64, device_id: Uuid }`. String format: `{timestamp:016x}-{counter:016x}-{device_id}` (lexicographically sortable). Maintains causal ordering without clock sync.

### Sync State Machine
`Uninitialized` -> `Backfilling { peer, progress }` -> `CatchingUp { buffered_count }` -> `Ready` <-> `Paused`. Buffer queue: 100K updates max, priority-sorted by timestamp.

### Database Architecture
- `database.db`: All library data
- `sync.db`: `shared_changes` table (HLC log), `peer_acks`, watermarks, backfill checkpoints, event log

### Syncable Trait
`SYNC_MODEL`, `sync_id()`, `version()`, `exclude_fields()`, `sync_depends_on()`, `foreign_key_mappings()`. Registered via `register_syncable_shared!` / `register_syncable_device_owned!`.

**19 models syncing**: 3 device-owned (Volume, Location, Entry), 16 shared (Device, Tag, TagRelationship, Collection, CollectionEntry, ContentIdentity, UserMetadata, and more).

## Networking

**Transport**: Iroh (P2P library on QUIC). Built-in TLS 1.3, multiplexed streams, NAT traversal (90%+ success), relay fallback.

**Protocol system**: ALPN-based routing. Protocols: `pairing/1.0`, `sync/1.0`, `transfer/1.0`.

**Pairing**: BIP39 mnemonic code, device info + public key exchange, challenge-response, ECDH shared secret. 5-minute timeout.

**File transfer**: Resumable, 256KB encrypted chunks, BLAKE3 verification, parallel transfers, optional gzip.

**Security**: TLS 1.3 transport + application-level session key encryption + per-file encryption. Ed25519 identity, ECDH key exchange, forward secrecy.

## Jobs System

Resumable background tasks. Library-scoped. State persists to survive crashes.

**Lifecycle**: Queued -> Running -> Paused/Completed. `#[typetag::serde]` for polymorphic serialization.

**Job trait**: `NAME`, `VERSION`, `IS_RESUMABLE` + `JobHandler::run()`. Progress types: count, percentage, bytes, custom.

**Database**: Dedicated `jobs.db` with `jobs`, `job_history`, `job_checkpoints` tables.

## Events System

Three generic events replace ~40 specific variants:
- `ResourceChanged { resource_type, resource, metadata }`
- `ResourceChangedBatch`
- `ResourceDeleted { resource_type, resource_id }`

Resource types: "file", "tag", "collection", "location", "device", "volume", "sidecar", "user_metadata", "content_identity".

Automatic emission after successful commits via TransactionManager.

## Extension System (WASM)

Five building blocks:
1. **`#[extension]`** — Definition with id, name, version, permissions
2. **`#[model]`** — Creates SQL tables prefixed `ext_{id}_`, supports `#[indexed]`, `#[metadata]`, `#[foreign_key]`
3. **`#[job]` / `#[task]`** — Durable background work with retries and timeouts
4. **`#[agent]`** — Observe-Orient-Act loop with persistent memory
5. **`#[action]`** — Preview-commit-verify flow for safe operations

**Data storage**: DB tables via `#[model]`, files in user-controlled locations, sidecars in library directory.

**Sync**: Models auto-implement `Syncable`. Strategies: `device_owned` or `shared` (HLC).

**Security**: SQL validated, only own tables (prefixed `ext_{id}_`), sandboxed connection with authorizer.

## Virtual Sidecar System

Content-scoped derivative data. Non-destructive. Portable (in `.sdlibrary/sidecars/`).

Path: `content/{h0}/{h1}/{content_uuid}/{kind}/{variant}.{format}`

Types: Managed (thumbnails .webp, video proxies .mp4, OCR .json) and Reference (Live Photo videos, RAW+JPEG pairs).

## Key Manager

`redb` at `<data_dir>/secrets.redb`. XChaCha20-Poly1305 encryption. Key hierarchy: Device Key (OS keychain) -> Library Keys -> Paired Device Data -> Arbitrary Secrets.

## API System

Pipeline: Application -> SessionContext -> ApiDispatcher -> PermissionLayer -> Operation.

**Actions** (write): `validate()` -> `resolve_confirmation()` -> `execute()`. Can return `RequiresConfirmation`.

**Queries** (read): `from_input()` + `execute()`.

**Registration**: `register_core_action!(MyAction, "category.name")`, `register_library_query!(MyQuery, "name")`.

**Wire protocol**: `action:{category}.{operation}.input.v{version}`, `query:{scope}.{operation}.v{version}`.

Auto-generates TypeScript and Swift clients from Rust via Specta.

## CLI Commands

Binary: `sd-cli` (aliased `sd`). Daemon-client model over Unix socket JSON-RPC.

### Essential Commands
```
sd start [--foreground]              # Start daemon
sd stop                              # Stop daemon
sd status                            # Check status
sd library create "Name"             # Create library
sd library list / switch / current   # Manage libraries
sd location add ~/path [--mode deep] # Add location
sd location rescan <id>              # Rescan location
sd index quick-scan /path            # Quick index
sd index verify <location>           # Verify index integrity
sd job list / monitor / pause / cancel  # Manage jobs
sd network devices / pair / spacedrop   # Networking
```

**Index modes**: `shallow` (metadata), `content` (+ hashing), `deep` (+ media metadata)
**Index scopes**: `current` (single dir), `recursive` (all subdirs)

## React Integration

### Hooks (auto-generated types from Rust via Specta)
- `useCoreQuery({ type, input })` — Core-scoped: `libraries.list`, `node.info`
- `useLibraryQuery({ type, input })` — Library-scoped: `files.directory_listing`, `locations.list`, `files.search`
- `useCoreMutation(type)` — `libraries.create`, `node.update_config`
- `useLibraryMutation(type)` — `locations.create`, `files.delete`, `tags.apply`
- `useNormalizedQuery` — Real-time event-driven cache with server-side filtering
- `useEvent(eventType, handler)` — Subscribe to events
- `useSpacedriveClient()` — Direct client access

### Semantic Color System
Never use raw Tailwind colors. Use semantic tokens:
- **Text**: `text-ink`, `text-ink-dull`, `text-ink-faint`
- **Backgrounds**: `bg-app`, `bg-app-box`, `bg-app-input`, `bg-sidebar`
- **Borders**: `border-app-line`, `border-sidebar-line`
- **Interactive**: `bg-app-hover`, `bg-app-selected`, `bg-sidebar-selected`

### UI Primitives (`@spacedrive/primitives`)
Button (8 variants, 5 sizes), Input/SearchInput/PasswordInput/TextArea, Switch, Select, Dialog, Popover, Tooltip, Tabs, DropdownMenu, ContextMenu, Loader, ProgressBar, ShinyToggle, Resizable.

### Icons
Phosphor Icons (`@phosphor-icons/react`). Sizes: 16/20/24/32/48. Weights: regular/fill/bold.

## Cloud Integration

Cloud volumes function identically to local. Connected via OpenDAL (40+ services: S3, GDrive, Dropbox, OneDrive, Azure, GCS, MinIO, and more).

Same BLAKE3 sampling for content identification. Ranged reads for cloud indexing. Metadata cached 5 min.

## Spacebot-Spacedrive Integration

### Connection Modes
1. **Managed Local** (`ManagedLocal`) — Spacedrive launches/supervises Spacebot child process (recommended)
2. **External Local** (`ExternalLocal`) — Connect to existing localhost Spacebot at `http://127.0.0.1:19898`
3. **Library** (`Library`) — Routed via P2P library layer to remote Spacebot instance

### Architecture Boundary
Integration is HTTP + SSE. Spacedrive is the device graph and permission layer. Spacebot is the agent runtime and scheduler.

### Spacebot Endpoints Used by Spacedrive
`GET /api/health`, `GET /api/status`, `GET /api/idle`, `GET /api/agents/warmup`, `POST /api/webchat/send`, `GET /api/webchat/history`, `GET /api/events` (SSE)

### Config (Spacedrive side)
```rust
SpacebotConfig {
    enabled: bool,
    mode: SpacebotConnectionMode, // ManagedLocal | ExternalLocal | Library
    base_url: String,
    auth_token: Option<String>,
    binary_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    instance_dir: Option<PathBuf>,
    auto_start: bool,
    default_agent_id: String,
    default_sender_name: String,
}
```

### Config (Spacebot side, in config.toml)
```rust
SpacedriveIntegrationConfig {
    enabled: bool,              // Master switch, default false
    api_url: Option<String>,    // Default "http://127.0.0.1:7872"
    api_key: Option<String>,
    library_id: Option<String>,
    device_id: Option<String>,
}
```

### `spacebot_host` Capability
Boolean flag in device `capabilities` JSON. Set on exactly one device. Syncs automatically. Enables other devices to know Spacebot exists, where to route, and host's online status.

### P2P Proxy
`SpacebotProxy` on host device accepts HTTP-over-P2P, forwards to `localhost:19898`, returns response verbatim. SSE relay fans out. Spacebot API evolves freely without proxy changes. Mobile reaches Spacebot through same proxy.

### Remote Execution

Workers bind to an `execution_target` (device slug/UUID). Tools become target-aware. Proxy sends requests through paired Spacedrive node.

**Policy layers**:
1. Device Access Policy — which devices Spacebot may access
2. Location/Subtree Policy — which paths (allow/deny per path, read-only)
3. Operation Policy — list, read, search, write, move, delete, shell, computer_use
4. Confirmation Policy — which actions require live user confirmation

**Principal model**: `AgentPrincipal { id, library_id, kind, paired_device_id, display_name, status }`

**Audit model**: Every remote operation logged with request_id, principal, origin/target devices, operation, policy decision, result.

### File System Intelligence

Three pillars:
1. **File Intelligence** — Per-file derived knowledge (metadata, OCR, transcripts, classifications)
2. **Directory Intelligence** — Contextual knowledge on directories ("active projects", "archive")
3. **Access Intelligence** — Universal permissions (agent read/write, deletion policy, sensitivity)

**Core primitive — `ContextNode`**:
```
id, library_id, target_kind (file|directory|subtree|volume|cloud_location),
target_id, scope (exact|inherited), node_kind (fact|summary|policy|note|tag),
title, content, structured_payload, source_kind (user|agent|job|system),
source_id, confidence, visibility (user_only|agent_visible|private|synced),
created_at, updated_at, supersedes_id, archived_at
```

**Agent experience**: When navigating via Spacedrive, agent receives: directory listing + inherited context + local context + permissions/policy + subtree summaries + recent changes.

### Archive System

Indexes external data beyond the filesystem: emails, notes, messages, bookmarks, calendar events.

Standalone crate: `crates/archive/` (`sd-archive`). Per-source folders with `data.db`, `embeddings.lance/`, `schema.toml`.

Hybrid search: FTS5 + LanceDB/FastEmbed via RRF. Prompt Guard 2 safety screening.

11 built-in adapters: Gmail, Obsidian, Chrome Bookmarks/History, Safari History, Apple Notes, OpenCode, Slack, macOS Contacts/Calendar, GitHub.

### Implementation Order (Spacebot-Spacedrive)

**Spacebot side**: 1) Flags + Config, 2) Direct Connection, 3) Device graph awareness, 4) Remote execution, 5) File System Intelligence, 6) Proxy chat

**Spacedrive side**: 1) Config + Discovery, 2) Managed Local, 3) Embedded Chat, 4) P2P Proxy, 5) Mobile Chat, 6) Remote Execution

## Writing Conventions

From Spacedrive's WRITING_GUIDE: Clear, simple language. Short sentences. Active voice. Direct ("you"/"your"). Same banned words as Spacebot's writing guide. No em dashes in prose. No ASCII/Mermaid diagrams.

Code comments explain WHY, not WHAT. Module docs (`//!`) with title, prose, runnable examples. Function docs (`///`) with brief summary, rationale, error handling.

## Performance Targets

- 8,500 files/sec indexing
- ~55ms search on 1M entries
- ~150MB memory for 1M files
- 110 MB/s P2P transfer
- Open 100 `.memory` docs <100ms
- Search 500 embeddings in 20-50ms
