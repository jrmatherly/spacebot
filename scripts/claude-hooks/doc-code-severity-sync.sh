#!/usr/bin/env bash
# PostToolUse.Edit|Write hook: flag when a handler's module-level //! doc
# claims "always-on tracing::warn!" but the file's code uses tracing::error!
# in any pool-None else branch.
#
# Why this exists:
#   The second-pass 6-agent re-review of PR #105 caught 8 handler module
#   docs that still said `always-on tracing::warn!` in their //! preambles
#   after commit f255f3f had escalated all 60+ pool-None sites to
#   tracing::error!. The drift was purely a doc-code severity mismatch that
#   no existing automation caught; comment-analyzer found it via manual
#   Read during review. This hook closes that loop at edit time.
#
# Scope:
#   Fires on any .rs file in src/api/. Cheap string match — no AST walk.
#
# Failure mode:
#   Advisory only. Writes to stderr; exit 0 regardless.
#
# Input: $CLAUDE_TOOL_INPUT is the JSON tool invocation.

set -u

F=$(echo "$CLAUDE_TOOL_INPUT" | sed -nE 's/.*"file_path"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -1)

# Fast exit: not a handler file.
case "$F" in
    */src/api/*.rs) ;;
    *) exit 0 ;;
esac

# File must exist (some tool inputs are speculative).
[ -f "$F" ] || exit 0

# The pattern: module doc claims warn! in the //! block BUT code uses error!.
# Both patterns must match for this to be a real drift.
HAS_WARN_DOC_CLAIM=$(grep -cE '^//!.*always-on .tracing::warn!.' "$F" 2>/dev/null)
HAS_ERROR_CODE=$(grep -cE '^[[:space:]]+tracing::error!' "$F" 2>/dev/null)

if [ "$HAS_WARN_DOC_CLAIM" -gt 0 ] && [ "$HAS_ERROR_CODE" -gt 0 ]; then
    echo "" >&2
    echo "⚠️  Doc-code severity drift in $F:" >&2
    echo "   Module //! doc claims 'always-on tracing::warn!' but code uses" >&2
    echo "   tracing::error!. Likely a stale claim from a prior severity" >&2
    echo "   escalation sweep that didn't update the module doc." >&2
    echo "" >&2
    echo "   Fix: update the //! block to say 'always-on tracing::error!'" >&2
    echo "   to match the code, or restore the warn! level if that was" >&2
    echo "   actually the intended severity." >&2
fi

exit 0
