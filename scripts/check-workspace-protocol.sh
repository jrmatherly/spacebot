#!/usr/bin/env bash
# Enforce that every @spacedrive/* and @spacebot/* dependency in any package.json
# under the repo uses the `workspace:*` protocol. Prevents silent npm fallbacks
# if a `package.json` is edited incorrectly.
#
# Why this exists: our spaceui/packages/*/package.json files declare names
# like `@spacedrive/primitives`, which is also the upstream scope on npm.
# `workspace:*` makes bun resolve locally. Any non-workspace spec (e.g., a
# semver range) would silently resolve to the public registry. The same risk
# applies to @spacebot/api-client once activated (no npm package exists, so
# fallback would fail — but with an opaque error rather than a clear guard hit).
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
    # Find every line with a scoped dep entry. Portable ERE that works on
    # both BSD (macOS) and GNU grep. Each dep must live on its own line,
    # which is the repo's package.json formatting convention.
    scoped_lines=$(grep -E '"@(spacedrive|spacebot)/[^"]+":[[:space:]]*"[^"]*"' "$pj" || true)
    [ -z "$scoped_lines" ] && continue

    # Filter out every line where the value literally starts with "workspace:".
    # `grep -v` with a fixed-literal value substring survives on all platforms.
    bad=$(printf '%s\n' "$scoped_lines" | grep -v '"workspace:' || true)

    if [ -n "$bad" ]; then
        echo "ERROR: non-workspace @spacedrive/* or @spacebot/* dep in $pj:"
        echo "$bad" | sed 's/^/  /'
        violations=$((violations + 1))
    fi
done

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "Found $violations package.json file(s) with @spacedrive/* or @spacebot/* deps that"
    echo "do not use the workspace:* protocol. Fix by changing each value to"
    echo "\"workspace:*\"."
    exit 1
fi

echo "OK — all @spacedrive/* and @spacebot/* deps use workspace:* protocol."
