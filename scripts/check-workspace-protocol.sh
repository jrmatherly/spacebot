#!/usr/bin/env bash
# Enforce that every @spacedrive/* dependency in any package.json under the
# repo uses the `workspace:*` protocol. Prevents silent npm fallbacks if a
# `package.json` is edited incorrectly.
#
# Why this exists: our spaceui/packages/*/package.json files declare names
# like `@spacedrive/primitives`, which is also the upstream scope on npm.
# `workspace:*` makes bun resolve locally. Any non-workspace spec (e.g., a
# semver range) would silently resolve to the public registry.
#
# Usage: run via `just spaceui-check-workspace` or as an interface/ preinstall.

set -euo pipefail

# Any package.json inside the repo, excluding dependencies we don't control.
mapfile -t PACKAGE_JSONS < <(
    find . \
        -name package.json \
        -not -path '*/node_modules/*' \
        -not -path '*/target/*' \
        -not -path '*/.git/*' \
        -not -path './spacedrive/*'
)

violations=0

for pj in "${PACKAGE_JSONS[@]}"; do
    # Skip files with no @spacedrive/* entries at all.
    if ! grep -q '"@spacedrive/' "$pj"; then
        continue
    fi
    # Look for @spacedrive/* entries whose value does not start with "workspace:".
    # Multi-line JSON means a simple grep works when each dep is on its own line,
    # which is the convention we use.
    bad=$(grep -E '"@spacedrive/[^"]+":\s*"(?!workspace:)' "$pj" 2>/dev/null || true)
    # Fallback for grep without PCRE (BSD grep on macOS):
    if [ -z "$bad" ]; then
        bad=$(grep -E '"@spacedrive/[^"]+":[[:space:]]*"[^w]' "$pj" || true)
    fi
    if [ -n "$bad" ]; then
        echo "ERROR: non-workspace @spacedrive/* dep in $pj:"
        echo "$bad" | sed 's/^/  /'
        violations=$((violations + 1))
    fi
done

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "Found $violations package.json file(s) with @spacedrive/* deps that"
    echo "do not use the workspace:* protocol. Fix by changing each value to"
    echo "\"workspace:*\"."
    exit 1
fi

echo "OK — all @spacedrive/* deps use workspace:* protocol."
