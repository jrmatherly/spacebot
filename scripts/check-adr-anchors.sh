#!/usr/bin/env bash
# Sanity-check the path:line anchors used in Spacedrive integration ADRs.
# If an anchor has drifted, the ADR must be updated.
#
# Exit non-zero if any anchor fails to find its expected symbol.

set -euo pipefail

check() {
    local desc="$1"
    local file="$2"
    local pattern="$3"
    if ! grep -q "$pattern" "$file" 2>/dev/null; then
        echo "FAIL: $desc"
        echo "  expected pattern: $pattern"
        echo "  in: $file"
        return 1
    fi
    return 0
}

failed=0

# Anchors claimed by docs/design-docs/spacedrive-integration-pairing.md:
check "Spacebot bearer-auth middleware" \
    "src/api/server.rs" \
    'strip_prefix("Bearer ")' || failed=1

check "Spacebot secrets-store module" \
    "src/secrets/store.rs" \
    "Credential storage" || failed=1

check "Spacedrive SpacebotConfig struct" \
    "spacedrive/core/src/config/app_config.rs" \
    "pub struct SpacebotConfig" || failed=1

check "Spacedrive SD_AUTH env var" \
    "spacedrive/apps/server/src/main.rs" \
    'env = "SD_AUTH"' || failed=1

check "Spacedrive /rpc route" \
    "spacedrive/apps/server/src/main.rs" \
    '"/rpc"' || failed=1

check "Spacedrive-side update op" \
    "spacedrive/core/src/ops/config/app/update.rs" \
    "spacebot_enabled" || failed=1

if [ "$failed" -gt 0 ]; then
    echo ""
    echo "One or more ADR anchors are stale. Update the ADR or the code to restore them."
    exit 1
fi

# ---------------------------------------------------------------------------
# Anchors activated in Track A Phase 3 (2026-04-17).
# ---------------------------------------------------------------------------

check "Spacedrive envelope helper" \
    "src/spacedrive/envelope.rs" \
    "pub fn wrap_spacedrive_response" || failed=1

echo "OK — all ADR anchors resolve."
