#!/usr/bin/env bash
# check-sidecar-naming.sh — Enforce that the Tauri sidecar binary name agrees
# across every reference site, and that it does not collide case-insensitively
# with the desktop host binary name (the original APFS bug).
#
# Runs two checks:
#
# 1. Sync invariant: the sidecar basename derived from `scripts/bundle-sidecar.sh`
#    must appear verbatim as `binaries/<name>` in the three configuration files
#    that Tauri and the renderer consume, and as `<name>-<target-triple>` in the
#    user-facing docs. A grep cross-check catches new reference sites that were
#    not added to the enumerated list below.
#
# 2. Collision invariant: lowercase(sidecar_basename) must differ from
#    lowercase(host_bin_name) extracted from desktop/src-tauri/Cargo.toml's
#    `[[bin]] name = "..."` stanza. If they match, `target/debug/<name>` and
#    `target/debug/<Name>` resolve to the same inode on APFS and NTFS,
#    causing Tauri's sidecar lookup to execute the host binary recursively.
#
# Usage: ./scripts/check-sidecar-naming.sh
#        Wired into `scripts/gate-pr.sh`; runs automatically before `cargo fmt`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

log() { echo "[check-sidecar-naming] $*"; }
fail() {
	echo "[check-sidecar-naming] ERROR: $*" >&2
	exit 1
}

BUNDLE_SCRIPT="scripts/bundle-sidecar.sh"
CARGO_MANIFEST="desktop/src-tauri/Cargo.toml"

[[ -f "$BUNDLE_SCRIPT" ]] || fail "missing $BUNDLE_SCRIPT"
[[ -f "$CARGO_MANIFEST" ]] || fail "missing $CARGO_MANIFEST"

# --- Source-of-truth extraction -------------------------------------------

# Pull the sidecar basename from the line that constructs DEST_BIN.
# Matches: DEST_BIN="$BINARIES_DIR/spacebot-daemon-${TARGET_TRIPLE}${SUFFIX}"
SIDECAR_NAME="$(
	grep -E '^DEST_BIN=' "$BUNDLE_SCRIPT" \
		| sed -E 's|.*/([a-zA-Z0-9_-]+)-\$\{TARGET_TRIPLE\}.*|\1|' \
		| head -n 1
)"
[[ -n "$SIDECAR_NAME" ]] || fail "could not extract sidecar basename from $BUNDLE_SCRIPT"

# Pull the host bin name from desktop/src-tauri/Cargo.toml's [[bin]] stanza.
# awk keeps track of the current section; we only grab `name` inside `[[bin]]`.
HOST_BIN="$(
	awk '
		/^\[\[bin\]\]/ { in_bin = 1; next }
		/^\[/         { in_bin = 0 }
		in_bin && /^name[[:space:]]*=/ {
			gsub(/.*=[[:space:]]*"/, "")
			gsub(/".*/, "")
			print
			exit
		}
	' "$CARGO_MANIFEST"
)"
[[ -n "$HOST_BIN" ]] || fail "could not extract [[bin]] name from $CARGO_MANIFEST"

log "sidecar name: $SIDECAR_NAME (from $BUNDLE_SCRIPT)"
log "host bin name: $HOST_BIN (from $CARGO_MANIFEST)"

# --- Collision invariant --------------------------------------------------

# Case-insensitive equality check. If lowercase names match, APFS/NTFS will
# resolve sidecar and host to the same file in target/debug/.
SIDECAR_LC="$(echo "$SIDECAR_NAME" | tr '[:upper:]' '[:lower:]')"
HOST_LC="$(echo "$HOST_BIN" | tr '[:upper:]' '[:lower:]')"

if [[ "$SIDECAR_LC" == "$HOST_LC" ]]; then
	fail "case-insensitive collision: sidecar '$SIDECAR_NAME' and host '$HOST_BIN' " \
		"lowercase to the same name. APFS/NTFS will clobber one with the other " \
		"in target/debug/. Rename one so lowercase basenames differ."
fi

# The sidecar also must not be a case-insensitive prefix of host or vice versa
# that would produce colliding on-disk paths. Today's names ("spacebot-daemon"
# vs "Spacebot") already differ structurally; a future rename to e.g. "Spacebot"
# or "spacebot" would fail the equality check above, so no separate prefix check
# is required here.

log "collision invariant holds"

# --- Sync invariant -------------------------------------------------------

# Each entry: "file:pattern" where pattern is the literal the file must contain.
# The pattern uses the sidecar basename extracted above so this list never drifts
# from the source of truth.
SIDECAR_REF="binaries/$SIDECAR_NAME"
DOCS_REF="$SIDECAR_NAME-<target-triple>"

KNOWN_SITES=(
	"desktop/src-tauri/tauri.conf.json|$SIDECAR_REF"
	"desktop/src-tauri/capabilities/default.json|$SIDECAR_REF"
	"interface/src/components/ConnectionScreen.tsx|$SIDECAR_REF"
	"docs/content/docs/(getting-started)/desktop.mdx|$DOCS_REF"
	"scripts/bundle-sidecar.sh|$DOCS_REF"
	".github/workflows/desktop-ci.yml|$SIDECAR_REF"
)

violations=0
for entry in "${KNOWN_SITES[@]}"; do
	file="${entry%%|*}"
	pattern="${entry#*|}"
	if [[ ! -f "$file" ]]; then
		echo "  MISSING FILE: $file" >&2
		violations=$((violations + 1))
		continue
	fi
	if ! grep -qF "$pattern" "$file"; then
		echo "  MISSING PATTERN: $file does not contain '$pattern'" >&2
		violations=$((violations + 1))
	fi
done

if ((violations > 0)); then
	fail "$violations known sync site(s) failed to match. If you renamed the sidecar, " \
		"update every site; if you added a new site, add it to KNOWN_SITES in this " \
		"script and to the comment block in $BUNDLE_SCRIPT."
fi

log "all $((${#KNOWN_SITES[@]})) known sync sites agree on '$SIDECAR_NAME'"

# --- Grep cross-check for unlisted reference sites ------------------------

# Find any reference to `binaries/$SIDECAR_NAME` across the tree. Every hit
# should map to a known site or an explicitly ignored path (target/, node_modules/,
# generated Tauri artifacts, and the bundle-sidecar.sh grep-hint comment itself).
KNOWN_FILES=()
for entry in "${KNOWN_SITES[@]}"; do
	KNOWN_FILES+=("${entry%%|*}")
done

# Collect all candidate files via grep. Exclude directories up front
# (--exclude-dir) rather than filtering afterward; piping through grep -v
# still forces a full recursive walk of target/ and node_modules/ which
# takes minutes on a warm Cargo build directory.
mapfile -t HIT_FILES < <(
	grep -rlF "$SIDECAR_REF" \
		--exclude-dir=target \
		--exclude-dir=gen \
		--exclude-dir=node_modules \
		--exclude-dir=.git \
		--exclude-dir=spacedrive \
		--exclude-dir=.scratchpad \
		--exclude-dir=dist \
		--exclude-dir=.next \
		. 2>/dev/null \
		| sort -u
)

unexpected=()
for hit in "${HIT_FILES[@]}"; do
	normalized="${hit#./}"
	matched=false
	for known in "${KNOWN_FILES[@]}"; do
		if [[ "$normalized" == "$known" ]]; then
			matched=true
			break
		fi
	done
	# The guard script itself and the bundle-sidecar comment legitimately
	# reference the pattern; they are source-of-truth files.
	case "$normalized" in
	scripts/check-sidecar-naming.sh) matched=true ;;
	esac
	if ! $matched; then
		unexpected+=("$normalized")
	fi
done

if ((${#unexpected[@]} > 0)); then
	echo "[check-sidecar-naming] ERROR: reference sites not in KNOWN_SITES:" >&2
	printf '  %s\n' "${unexpected[@]}" >&2
	fail "add each new site to KNOWN_SITES in $0 and to the comment in $BUNDLE_SCRIPT"
fi

log "grep cross-check clean: no unlisted reference sites"
log "all checks passed"
