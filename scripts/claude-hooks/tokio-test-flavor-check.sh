#!/usr/bin/env bash
# PostToolUse hook: warn if a .rs file has a bare #[tokio::test] alongside
# background-task-spawning APIs that can deadlock on the current-thread
# test runtime. Matches CLAUDE.md:21 and the test-runtime-patterns skill.
#
# Non-blocking: prints to stderr, exit 0 so the edit still lands.
# The author should read the warning and either add the multi_thread flavor
# or confirm the test does not actually need it.

set -eu

# Extract the file path from CLAUDE_TOOL_INPUT (JSON).
F=$(echo "${CLAUDE_TOOL_INPUT:-}" | sed -nE 's/.*"file_path"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)

# Only scan .rs files.
case "$F" in
    *.rs) ;;
    *) exit 0 ;;
esac

# File must exist (Write on a new file is fine too — grep on non-existent fails cleanly).
[ -f "$F" ] || exit 0

# Look for:
#   1. A bare #[tokio::test] line (not the multi_thread form)
#   2. AND at least one deadlock-class API in the same file
#   3. AND no 'multi_thread' mention anywhere in the file
if grep -qE '^[[:space:]]*#\[tokio::test\][[:space:]]*$' "$F" \
    && grep -qE 'tokio::spawn|LanceDB|OTLP|otlp|spawn_blocking|Compactor|cortex::' "$F" \
    && ! grep -q 'multi_thread' "$F"; then

    cat >&2 <<EOF
⚠️  tokio-test flavor check: $F

A bare #[tokio::test] appears alongside background-task spawning
(tokio::spawn / LanceDB / OTLP / spawn_blocking / compactor / cortex).
The current-thread test runtime can deadlock on these — the test
hangs forever instead of failing.

Fix: change to
    #[tokio::test(flavor = "multi_thread")]
    async fn my_test() { ... }

Reference: CLAUDE.md:21 and .claude/skills/test-runtime-patterns/SKILL.md
EOF
fi

exit 0
