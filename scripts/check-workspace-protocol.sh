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

# Documented escape hatch for build contexts that lack `.git/` AND have
# already had this guard run upstream (e.g., the Docker `RUN bun install`
# stage). The host CI workflow + `just gate-pr` always run this guard
# against a real worktree before any image build, so suppressing inside
# the container is safe. Set SKIP_WORKSPACE_PROTOCOL_CHECK=1 in the
# Dockerfile RUN line, never in dev shells or CI runner steps.
if [ "${SKIP_WORKSPACE_PROTOCOL_CHECK:-0}" = "1" ]; then
	echo "[check-workspace-protocol] SKIPPED via SKIP_WORKSPACE_PROTOCOL_CHECK=1." >&2
	exit 0
fi

# Require a git worktree. The Docker image's interface/ preinstall stage
# runs this script in a context where `.git` is absent (.dockerignore:2
# excludes .git/, Dockerfile COPYs only this script + interface/). Fail
# loudly rather than silently pass-through (empty ls-files output would
# collapse the guard to zero checks).
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
	echo "[check-workspace-protocol] ERROR: requires a git worktree (no .git found)." >&2
	echo "  If invoking from a Docker build stage, set SKIP_WORKSPACE_PROTOCOL_CHECK=1" >&2
	echo "  on the RUN line (the host CI run is authoritative; container is redundant)." >&2
	exit 1
fi

# Enumerate tracked package.json files. Use a `while read -r -d ''` loop
# (not mapfile) so the script runs on macOS's bash 3.2 (mapfile is bash 4+).
# Null-delimited via `git ls-files -z`. The spacedrive/ filter lives in shell
# because Spacedrive uses npm-registry conventions, not workspace:*.
violations=0

# Probe `**/package.json` pathspec support up front. The `:(glob)` syntax
# requires git's builtin globstar; some minimal CI images ship an older git
# without it. Failing here gives a clear diagnostic before the main pass.
if ! git ls-files -z ':(glob)**/package.json' ':(glob)package.json' >/dev/null 2>&1; then
    echo "[check-workspace-protocol] ERROR: git ls-files pathspec failed." >&2
    echo "  Your git version may not support :(glob)**/ syntax." >&2
    exit 1
fi

# Stage tracked package.json paths to a temp file so the main scan loop
# reads from a seekable fd rather than a pipeline. This decouples the
# filter step from the scan step and avoids the bash 3.2 parse error that
# triggered v0.6.0 CI failure: `case` inside an inline `{ while; done; }`
# *pipeline group* (e.g., `cmd | { while ...; do case ...) ;; esac; done; }`)
# fails at parse time on Apple /bin/bash 3.2. Using a function body or a
# plain `while ... done < <(...)` redirect works fine — bash 3.2 only rejects
# `case` when it appears inside an inline brace-grouped pipeline stage.
# Cleanup of the temp file is guaranteed via trap on EXIT.
list_file=$(mktemp -t check-workspace-protocol.XXXXXX)
trap 'rm -f "$list_file"' EXIT

# Filter spacedrive/* paths (npm-registry scope, not workspace:*) into the
# temp file using a bash while-read loop. We cannot use `grep -z` here
# because its null-data semantic differs across implementations:
#   - GNU grep (Linux): `-z` = --null-data (NUL-delimited lines)
#   - BSD grep (macOS CI runner): `-z` = --decompress (zgrep mode)
#   - ugrep (macOS dev): `-z` = decompress; `--null-data` works but not
#     available on stock BSD grep
# A plain while-read-case loop is the only universally portable approach.
while IFS= read -r -d '' pj; do
    case "$pj" in
        spacedrive/*) ;;
        *) printf '%s\0' "$pj" ;;
    esac
done < <(git ls-files -z ':(glob)**/package.json' ':(glob)package.json') \
    > "$list_file" || true

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
done < "$list_file"

if [ "$violations" -gt 0 ]; then
    echo ""
    echo "Found $violations package.json file(s) with @spacedrive/* or @spacebot/* deps that"
    echo "do not use the workspace:* protocol. Fix by changing each value to"
    echo "\"workspace:*\"."
    exit 1
fi

echo "OK — all @spacedrive/* and @spacebot/* deps use workspace:* protocol."
