#!/usr/bin/env bash
# Enforce that every @spacedrive/* and @spacebot/* dependency in any tracked
# package.json uses the `workspace:*` protocol. Tracked-only is intentional:
# untracked files cannot regress CI, and `git ls-files` is ~80x faster than
# the previous `find` pattern on this tree (~0.1s vs ~9s). Prevents silent
# npm fallbacks if a tracked `package.json` is edited incorrectly.
#
# Why this exists: our spaceui/packages/*/package.json files declare names
# like `@spacedrive/primitives`, which is also the upstream scope on npm.
# `workspace:*` makes bun resolve locally. Any non-workspace spec (e.g., a
# semver range) would silently resolve to the public registry. The same risk
# applies to @spacebot/api-client (no npm package exists, so fallback would
# fail — but with an opaque error rather than a clear guard hit).
#
# Usage: run via `just spaceui-check-workspace`, as an interface/ preinstall
# hook, in .github/workflows/spaceui.yml, or as part of `just gate-pr`.

set -euo pipefail

# Require a git worktree. The Docker image's interface/ preinstall stage
# runs this script in a context where `.git` is absent (.dockerignore:2
# excludes .git/, Dockerfile COPYs only this script + interface/). Fail
# loudly rather than silently pass-through (empty ls-files output would
# collapse the guard to zero checks).
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
	echo "[check-workspace-protocol] ERROR: requires a git worktree (no .git found)." >&2
	echo "  If invoking from a Docker build stage, ensure .git is not excluded" >&2
	echo "  by .dockerignore, OR run this guard outside the container." >&2
	exit 1
fi

# Enumerate tracked package.json files. Use a `while read -r -d ''` loop
# (not mapfile) so the script runs on macOS's bash 3.2 (mapfile is bash 4+).
# Null-delimited via `git ls-files -z`. The spacedrive/ filter lives in shell
# because Spacedrive uses npm-registry conventions, not workspace:*.
violations=0

while IFS= read -r -d '' pj; do
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
done < <(
    # Explicit status check: `**/package.json` pathspec requires git's builtin
    # globstar. If the git binary is too old to understand it (some minimal
    # CI images ship older git), `ls-files` exits non-zero and pipefail fires
    # — but the outer while loop would see EOF with no diagnostic. Probe
    # first to surface a clear error.
    if ! git ls-files -z ':(glob)**/package.json' ':(glob)package.json' >/dev/null 2>&1; then
        echo "[check-workspace-protocol] ERROR: git ls-files pathspec failed." >&2
        echo "  Your git version may not support :(glob)**/ syntax." >&2
        exit 1
    fi
    git ls-files -z ':(glob)**/package.json' ':(glob)package.json' \
        | { while IFS= read -r -d '' pj; do
                case "$pj" in
                    spacedrive/*) continue ;;  # npm-registry scope, not workspace:*
                esac
                printf '%s\0' "$pj"
            done; }
)

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "Found $violations package.json file(s) with @spacedrive/* or @spacebot/* deps that"
    echo "do not use the workspace:* protocol. Fix by changing each value to"
    echo "\"workspace:*\"."
    exit 1
fi

echo "OK — all @spacedrive/* and @spacebot/* deps use workspace:* protocol."
