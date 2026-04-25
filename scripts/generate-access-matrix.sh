#!/usr/bin/env bash
# Regenerate docs/design-docs/entra-access-matrix.md from the running
# daemon's /api/admin/access-review output.
#
# Required env:
#   SPACEBOT_DAEMON_URL   - https://<deployment>
#   SPACEBOT_ADMIN_TOKEN  - bearer token for a SpacebotAdmin principal
#
# Usage:
#   SPACEBOT_DAEMON_URL=https://spacebot.example.com \
#   SPACEBOT_ADMIN_TOKEN=$(spacebot entra admin token) \
#       scripts/generate-access-matrix.sh
#
# SOC 2 evidence: this script's output is artifact #3 in the evidence
# index. Run quarterly (or whenever the auditor asks).

set -euo pipefail

: "${SPACEBOT_DAEMON_URL:?set the daemon URL}"
: "${SPACEBOT_ADMIN_TOKEN:?set the admin token}"

out="docs/design-docs/entra-access-matrix.md"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

curl -fsS -H "Authorization: Bearer $SPACEBOT_ADMIN_TOKEN" \
    "${SPACEBOT_DAEMON_URL}/api/admin/access-review?format=csv" > "$tmp"

{
    echo "# User Access Matrix"
    echo
    echo "> Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) by scripts/generate-access-matrix.sh"
    echo "> Source: ${SPACEBOT_DAEMON_URL}/api/admin/access-review?format=csv"
    echo "> File is regenerated; do not hand-edit."
    echo
    echo '```csv'
    cat "$tmp"
    echo '```'
} > "$out"

echo "Wrote $out"
