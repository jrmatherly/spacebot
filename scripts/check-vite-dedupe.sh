#!/usr/bin/env bash
# Verify interface/vite.config.ts dedupe list covers every dep shared between
# interface/ and any spaceui/packages/*.
#
# Missing dedupe entries cause multi-copy bugs (two Reacts, two framer-motions)
# that are hard to diagnose.

set -euo pipefail

INTERFACE_DEPS=$(jq -r '.dependencies // {} | keys[]' interface/package.json | sort -u)
DEDUPE_LIST=$(
    awk '/dedupe: *\[/,/\]/' interface/vite.config.ts \
    | grep -oE '"[^"]+"' \
    | tr -d '"' \
    | sort -u
)

# Find every dep declared in any spaceui package.json.
SPACEUI_DEPS=$(
    find spaceui/packages -mindepth 2 -maxdepth 2 -name package.json -print0 \
    | xargs -0 -n1 jq -r '(.dependencies // {}) + (.peerDependencies // {}) | keys[]' \
    | sort -u
)

# Shared deps: appear in both interface/ and some spaceui package.
SHARED=$(comm -12 <(echo "$INTERFACE_DEPS") <(echo "$SPACEUI_DEPS"))

missing=0
for dep in $SHARED; do
    # React is handled via alias, not dedupe; skip.
    if [ "$dep" = "react" ] || [ "$dep" = "react-dom" ]; then
        continue
    fi
    # @spacedrive/* packages are resolved through the bun workspace protocol
    # (single symlinked copy); vite dedupe is redundant here.
    case "$dep" in
        @spacedrive/*) continue ;;
    esac
    if ! echo "$DEDUPE_LIST" | grep -qx "$dep"; then
        echo "WARN: dep '$dep' is shared between interface/ and spaceui but not in vite dedupe list"
        missing=$((missing + 1))
    fi
done

if [ "$missing" -gt 0 ]; then
    echo ""
    echo "$missing shared dep(s) missing from interface/vite.config.ts dedupe. Fix:"
    echo "  add them to the dedupe array, or add them to the known-exception list in this script."
    exit 1
fi

echo "OK — all shared deps are in the dedupe list."
