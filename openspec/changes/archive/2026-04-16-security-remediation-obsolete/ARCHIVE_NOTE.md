# Archived as Obsolete — 2026-04-16

This change was drafted 2026-04-15 with a triage snapshot of 6 open Dependabot
alerts and 23 open CodeQL alerts. Within ~10 hours the Dependabot count had
grown to 55 open alerts (3 critical, 16 high) as GitHub continued scanning
transitive dependencies. Several scope and factual problems were identified
during audit:

- Dependabot scope mismatch: the change planned to handle 6 alerts; live count is 55.
- Task 2.1 bundled vendored-crate alerts (imap-proto) with app-code alerts (secrets/store.rs) under one dismissal rationale.
- `fix-codeql-findings/spec.md` scenario for `formatModelName` contained
  contradictory phrasing about provider-prefix removal.
- design.md cleartext-logging count (7) did not match tasks.md or live count (10).
- Design decision #2 (keep ReDoS regex with guard) conflicts with tasks.md 1.2
  (replace regex with safe alternative).
- Proposal's "Dependabot open alert count is 5" post-state is no longer
  reachable by executing the tasks.

Work replaced by fresh triage documents in `.scratchpad/`:

- `.scratchpad/codeql-security-findings.md`
- `.scratchpad/dependabot-security-findings.md`

Each includes a draft OpenSpec proposal ready to promote to
`openspec/changes/` when scope is confirmed.

The two code fixes identified in this change (AgentDetail.tsx no-op replace,
client.ts ReDoS regex) remain valid and are carried forward in the new
CodeQL findings doc.
