## Context

Dependabot reports 6 open alerts (5 Rust transitive deps + 1 false positive). CodeQL reports 23 open alerts across 4 categories: hard-coded crypto values (6), cleartext logging (7), cleartext transmission (5), identity replacement (1), polynomial ReDoS (1), plus 3 vendor/test alerts.

Investigation traced each alert to its source code and determined: most are false positives from CodeQL pattern matching not following data flow (e.g., `[0u8; 32]` flagged as hard-coded key when it's immediately filled by `rand::rng().fill_bytes()`).

## Goals / Non-Goals

**Goals:**
- Fix 2 genuine code issues found by CodeQL
- Dismiss all verified false positives with documented reasons
- Clean the GitHub Security dashboard to make real findings visible
- Document deferred items for future monitoring

**Non-Goals:**
- Upgrading transitive dependencies blocked on upstream (serenity, lancedb, imap)
- Rewriting the secrets store cryptographic implementation (it's correct)
- Adding TLS to localhost OpenCode subprocess communication

## Decisions

1. **Remove the no-op replacement** rather than change it to strip `claude-` prefix. The function's purpose is stripping date suffixes (`-20250514`, `-202X`), not provider prefixes. The `claude-` prefix is part of the display name.

2. **Keep the ReDoS regex** with a guard rather than replacing with a loop. The input is developer-set via `setServerUrl()`, not user-controlled. A simple `endsWith` check before the regex avoids the slow path while keeping the code readable. Alternatively, just dismiss the alert since the risk is near-zero.

3. **Dismiss alerts via GitHub API** rather than inline `// codeql-ignore` comments. The false positives are in patterns CodeQL will always flag (buffer init, function naming) — suppressing at the API level is cleaner than littering source with suppression comments.

4. **Leave 4 Dependabot alerts open** as tracking items rather than dismissing. They represent real vulnerabilities in transitive dependencies that will auto-resolve when upstream crates update.

## Risks / Trade-offs

- Dismissed CodeQL alerts won't re-trigger if the same pattern appears in new code. Acceptable since the patterns are correct (crypto buffer init, localhost comms).
- The `api-client/client.ts` ReDoS fix is cosmetic — the regex only runs on developer-set URLs. If dismissed instead of fixed, the alert count stays at 1 open.
- Deferred Dependabot alerts remain visible on the security dashboard. This is intentional — they serve as reminders to check upstream updates.
