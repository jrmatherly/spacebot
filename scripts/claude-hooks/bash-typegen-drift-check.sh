#!/usr/bin/env bash
# PostToolUse.Bash hook: detect in-place shell edits on src/api/*.rs and run
# just check-typegen.
#
# Why this exists:
#   The existing PostToolUse.Edit|Write typegen hook matches Edit/Write tools,
#   but mechanical bulk sweeps done via `perl -i -pe` or `sed -i` bypass that
#   path. Commit 530beb6 (the Phase 4 PR 2 em-dash sweep) was a concrete
#   instance: 79 perl -i replacements touched route doc headers that utoipa
#   ingests into the OpenAPI schema. check-typegen was not run and the PR's
#   CI failed on schema drift.
#
# Scope:
#   Fires only when the Bash command string contains `perl -i` or `sed -i`
#   AND references a path under `src/api/` with a `.rs` suffix. Both
#   conditions must match to avoid noise on the dozens of shell invocations
#   that don't touch handler files.
#
# Failure mode:
#   Advisory only. Writes to stderr; exit 0 regardless of check-typegen
#   result. A failing check still shows the operator the drift so they can
#   run `just typegen` manually before the next commit.
#
# Input: $CLAUDE_TOOL_INPUT is the JSON tool invocation.

set -u

CMD=$(echo "$CLAUDE_TOOL_INPUT" | sed -nE 's/.*"command"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)

# Fast exit: no in-place shell edit.
if ! echo "$CMD" | grep -qE '(perl -i|sed -i)'; then
    exit 0
fi

# Fast exit: no src/api/*.rs reference.
if ! echo "$CMD" | grep -qE 'src/api/[A-Za-z_]*\.rs'; then
    exit 0
fi

# Run the typegen check. Do NOT fail the tool call on drift: the hook is
# advisory, not a gate. The user can fix it at the next commit boundary.
OUT=$(just check-typegen 2>&1)
RC=$?

if [ $RC -ne 0 ]; then
    echo "" >&2
    echo "⚠️  In-place shell edit touched src/api/*.rs; typegen drift detected." >&2
    echo "   Run 'just typegen' and commit packages/api-client/src/schema.d.ts" >&2
    echo "   before pushing. This hook fires on perl -i / sed -i sweeps that" >&2
    echo "   bypass the PostToolUse.Edit|Write typegen check." >&2
    echo "" >&2
    echo "$OUT" | tail -10 >&2
fi

exit 0
