#!/usr/bin/env bash
# PreToolUse hook: block Edit/Write that introduces a `litellm_master_key`
# assignment anywhere in the tool input. The master key belongs in the
# LiteLLM proxy's environment, not Spacebot config — Spacebot uses the
# virtual `litellm_api_key` instead. See project_overview.md Serena
# memory for the full rationale.

set -eu

INPUT="${CLAUDE_TOOL_INPUT:-}"

# Match `litellm_master_key` followed by any whitespace, then `=` or `:` or `"`.
# Catches TOML (`litellm_master_key = "..."`), JSON (`"litellm_master_key": "..."`),
# and bash exports (`export LITELLM_MASTER_KEY=...` — normalized to lowercase for match).
if echo "$INPUT" | grep -qiE 'litellm_master_key[[:space:]]*[=:"]'; then
    cat <<'EOF'
{"decision": "block", "reason": "litellm_master_key must not land in Spacebot config. Spacebot authenticates to LiteLLM with the virtual `litellm_api_key` (system-category secret, per project_overview.md). The master key is the proxy's admin credential and belongs in the LiteLLM container environment (LITELLM_MASTER_KEY env var set in deploy/docker/.env or the container runtime), never in a committed file. If you meant litellm_api_key, change the identifier."}
EOF
fi

exit 0
