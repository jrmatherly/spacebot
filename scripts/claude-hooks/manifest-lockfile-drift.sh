#!/usr/bin/env bash
# Detect drift between a `package.json` spec range and the resolved version
# in the sibling `bun.lock`. Surfaces the class of bug that opened PRs
# #124, #125, #130, #131 on 2026-04-26. The pattern: `bun update` (without
# --latest) bumped the lockfile but left the manifest spec at an older
# semver range, so dependabot kept reopening PRs forever.
#
# Why this exists: `bun update <pkg>` only walks within the existing
# spec range. For `"^3.2.4"`, the lockfile can move to 3.x.x latest but
# the spec stays at `^3.x`. An operator who ran `bun update vitest`
# expecting a 3 → 4 jump would silently get 3.x latest and not notice
# until dependabot's next scan cycle. This hook fires on every
# `Edit|Write` to a `package.json` and warns if the resolved version
# in bun.lock is outside the manifest's declared range.
#
# Trigger pattern (in .claude/settings.json PostToolUse hooks):
#   matcher: "Edit|Write"
#   command: case "$F" in */package.json) bash this-script "$F" ;; esac
#
# Exit codes:
#   0 — no drift (or check skipped because lockfile/python3 missing)
#   0 with stderr warning — drift found; we warn, never block
#
# We intentionally do NOT exit non-zero. The hook is informational —
# blocking on lockfile drift would create false-positives mid-edit when
# the operator is still in the process of bumping a dep.

set -euo pipefail

PKG_JSON="${1:-}"
if [ -z "$PKG_JSON" ] || [ ! -f "$PKG_JSON" ]; then
    exit 0
fi

# Only check workspace-managed package.json files. Skip nested ones inside
# node_modules (vendored deps), inside .scratchpad (gitignored), or any
# spaceui/examples/ stub manifests that don't have their own bun.lock.
case "$PKG_JSON" in
    */node_modules/*|*/.scratchpad/*|*/target/*|*/dist/*|*/.next/*) exit 0 ;;
esac

LOCK_FILE="$(dirname "$PKG_JSON")/bun.lock"
if [ ! -f "$LOCK_FILE" ]; then
    # Workspace member without its own lockfile — drift would surface in
    # the parent workspace's bun.lock, which is checked when the parent's
    # package.json is edited. Skip silently here.
    exit 0
fi

# Need python3 to parse the JSON safely. If unavailable, skip — no
# fallback to `grep -oE '"name": "X"'` because that misses transitive
# resolutions and false-positives on commented-out specs.
if ! command -v python3 >/dev/null 2>&1; then
    exit 0
fi

# Drift detection: for each direct dep declared in package.json, look up
# the resolved version in bun.lock (which uses `"<pkg>@<version>"` keys),
# then compare against the spec range using a minimal semver-range check.
#
# We only flag drift in ONE direction: spec is more restrictive than the
# resolved version (e.g., spec `^3.x` but lock has `4.1.5`). The opposite
# direction (spec allows more than what's resolved) is normal — that's
# what `bun update` would close on the next install.
HITS=$(python3 - "$PKG_JSON" "$LOCK_FILE" <<'PYEOF'
import json
import re
import sys
from pathlib import Path

pkg_path = Path(sys.argv[1])
lock_path = Path(sys.argv[2])

try:
    pkg = json.loads(pkg_path.read_text())
except Exception:
    sys.exit(0)

deps = {}
for section in ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies"):
    for name, spec in (pkg.get(section) or {}).items():
        # Only check semver-style specs. Skip workspace:*, file:, git+, etc.
        if isinstance(spec, str) and re.match(r"^[\^~<>=]?[\d.]+", spec):
            deps[name] = spec

if not deps:
    sys.exit(0)

lock_text = lock_path.read_text()
# bun.lock entries look like: "<pkg>": ["<pkg>@<version>", ...]
# Extract the resolved version per package by regex.
resolved = {}
for name in deps:
    # bun.lock entries look like:
    #   "<pkg>": ["<pkg>@<version>", "", { ... }, "<integrity>"]
    # We anchor on the EXACT JSON key `"<pkg>": [`, not on bare-name
    # substring matches — otherwise `react` would false-positive on
    # `react-redux@5.3.1`. Escape both the leading and trailing
    # delimiters so package names with regex meta-chars (e.g.,
    # `@types/node`, `@scope/pkg`) work.
    key_pattern = r'"' + re.escape(name) + r'":\s*\["' + re.escape(name) + r"@([\d]+\.[\d]+\.[\d]+(?:-[A-Za-z0-9.]+)?)"
    matches = re.findall(key_pattern, lock_text)
    if matches:
        # The JSON key is unambiguous — first match IS the direct-dep
        # resolution. Transitive duplicates (multiple `@types/node`
        # entries from peer-resolution) appear under different parent
        # package keys and don't reach this regex.
        resolved[name] = matches[0]

def parse_version(v):
    parts = re.split(r"[.\-+]", v)
    out = []
    for p in parts[:3]:
        try:
            out.append(int(p))
        except ValueError:
            out.append(0)
    while len(out) < 3:
        out.append(0)
    return tuple(out)

def spec_allows(spec, version):
    """Return True if `spec` allows `version`."""
    v = parse_version(version)
    # Strip leading operator
    op_match = re.match(r"^([\^~<>=]*)([\d].*)$", spec)
    if not op_match:
        return True  # unknown spec shape — don't false-positive
    op = op_match.group(1) or ""
    base = parse_version(op_match.group(2))
    if op == "":
        # Exact pin
        return v == base
    if op == "^":
        # Caret: same major (or minor for 0.x.y, patch for 0.0.x)
        if base[0] != 0:
            return v[0] == base[0] and v >= base
        if base[1] != 0:
            return v[0] == 0 and v[1] == base[1] and v >= base
        return v == base
    if op == "~":
        # Tilde: same minor
        return v[0] == base[0] and v[1] == base[1] and v >= base
    if op == ">=":
        return v >= base
    if op == ">":
        return v > base
    return True

drift = []
for name, spec in deps.items():
    if name not in resolved:
        continue
    if not spec_allows(spec, resolved[name]):
        drift.append(f"  {name}: spec={spec!r} but bun.lock resolved {resolved[name]!r}")

if drift:
    print(f"⚠️  Manifest-lockfile drift in {pkg_path}:")
    print("\n".join(drift))
    print("")
    print("This means bun update bumped the lockfile but the package.json spec")
    print("still pins to an older range. Dependabot will keep reopening PRs.")
    print("")
    print("Fix: bump the spec range in package.json to match the resolved")
    print("version, then `bun install` to confirm. See:")
    print("  /bun-deps-bump for the workflow")
    print("  commit 92ce85c for the precedent (vitest + fumadocs spec sync)")
PYEOF
)

if [ -n "$HITS" ]; then
    printf "%s\n" "$HITS" >&2
fi

exit 0
