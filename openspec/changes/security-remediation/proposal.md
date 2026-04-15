## Why

GitHub Dependabot and CodeQL identified 29 open alerts across the spacebot codebase (6 Dependabot, 23 CodeQL). Investigation revealed 2 genuine code issues, 22 false positives requiring dismissal, and 5 dependency alerts blocked on upstream crate releases. The code bugs (a no-op string replacement and a ReDoS-susceptible regex) should be fixed, and false positives should be dismissed to maintain a clean security dashboard.

## What Changes

- Fix no-op `.replace(/claude-/, "claude-")` in `interface/src/routes/AgentDetail.tsx:820` that CodeQL flagged as identity replacement
- Fix ReDoS-susceptible regex `/\/+$/` in `packages/api-client/src/client.ts:21` operating on developer-set URL input
- Dismiss 21 CodeQL false positive alerts (hard-coded crypto buffer inits, test logging, localhost-only session IDs)
- Dismiss 1 Dependabot false positive (`glib` not present in Cargo.lock)
- Document 4 deferred Dependabot alerts blocked on upstream: `rustls-webpki` (serenity), `rand` (ecosystem), `lexical-core` (imap), `lru` (lancedb/tantivy)

## Capabilities

### New Capabilities

- `fix-codeql-findings`: Fix 2 genuine CodeQL code findings and dismiss 21 false positives
- `dismiss-dependabot-false-positives`: Dismiss inaccurate Dependabot alert for `glib`

### Modified Capabilities

_None — no spec-level behavior changes._

## Impact

- `interface/src/routes/AgentDetail.tsx` — model name display formatting
- `packages/api-client/src/client.ts` — server URL setter
- GitHub Security dashboard — 22 alerts dismissed, 2 resolved by code fix
- No API changes, no dependency changes, no breaking changes
