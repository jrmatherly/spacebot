# Spacebot - Project Overview

**Version:** 0.4.1
**Language:** Rust (edition 2024), ~130K lines of code
**Purpose:** An agentic system where every LLM process has a dedicated role. Replaces the monolithic session model with specialized processes (Channel, Branch, Worker, Compactor, Cortex).

## Tech Stack
- **Runtime:** Tokio async
- **HTTP Framework:** Axum 0.8
- **Database:** SQLite via sqlx 0.8 (42 migrations)
- **Vector DB:** LanceDB 0.26 (embedded vector + FTS for memory)
- **Key-Value Store:** redb (embedded)
- **LLM Framework:** Rig v0.30.0 (agentic loop framework)
- **CLI:** clap 4.5
- **Serialization:** serde/serde_json 1.0
- **Logging:** tracing 0.1
- **Error Handling:** thiserror + anyhow
- **WebSocket:** tokio-tungstenite 0.28
- **Metrics:** Prometheus (feature-gated behind `metrics` feature)
- **macOS Keychain:** security-framework 3

## Frontend
- **Web UI:** Vite + React + TypeScript (`interface/`)
- **JS Package Manager:** bun (NEVER npm/pnpm/yarn)
- **Documentation Site:** Next.js + Fumadocs (`docs/`)
- **Desktop App:** Tauri (`desktop/`)

## Deployment
- Docker → Fly.io (region: iad, port 19898)
- Single binary, no server dependencies
- All data in embedded databases in a local data directory

## Architecture
Five process types, each a Rig `Agent<SpacebotModel, SpacebotHook>`:
1. **Channel** — User-facing conversation process. Delegates everything. Never blocked.
2. **Branch** — Fork of channel context for independent thinking. Short-lived.
3. **Worker** — Background task executor. Has shell, file, and memory tools.
4. **Compactor** — Context management and summarization.
5. **Cortex** — System-level intelligence.

## Key Identifiers
- `AgentId = Arc<str>`
- `ChannelId = Arc<str>`
- `WorkerId = uuid::Uuid`
- `BranchId = uuid::Uuid`
