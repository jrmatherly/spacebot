#!/usr/bin/env bash
# bundle-sidecar.sh — Build the spacebot binary and copy it into the
# Tauri sidecar binaries directory with the correct target-triple suffix.
#
# Usage:
#   ./scripts/bundle-sidecar.sh [--release]
#
# Tauri expects sidecar binaries at:
#   desktop/src-tauri/binaries/spacebot-daemon-<target-triple>[.exe]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARIES_DIR="$REPO_ROOT/desktop/src-tauri/binaries"

# Determine Rust target triple
HOST_TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
TARGET_TRIPLE="${TAURI_ENV_TARGET_TRIPLE:-$HOST_TRIPLE}"

# Build mode
BUILD_MODE="release"
CARGO_FLAGS="--release"
if [[ "${1:-}" != "--release" ]]; then
    BUILD_MODE="debug"
    CARGO_FLAGS=""
fi

echo "Building spacebot ($BUILD_MODE) for $TARGET_TRIPLE..."
if [[ "$TARGET_TRIPLE" != "$HOST_TRIPLE" ]]; then
    cargo build $CARGO_FLAGS --target "$TARGET_TRIPLE" --manifest-path "$REPO_ROOT/Cargo.toml"
    SRC_BIN="$REPO_ROOT/target/$TARGET_TRIPLE/$BUILD_MODE/spacebot"
else
    cargo build $CARGO_FLAGS --manifest-path "$REPO_ROOT/Cargo.toml"
    SRC_BIN="$REPO_ROOT/target/$BUILD_MODE/spacebot"
fi

# Destination with target triple suffix (Tauri convention)
mkdir -p "$BINARIES_DIR"

SUFFIX=""
case "$TARGET_TRIPLE" in
    *windows*) SUFFIX=".exe" ;;
esac

# Name the sidecar `spacebot-daemon-<triple>` rather than `spacebot-<triple>` because the
# Tauri host binary on macOS is `Spacebot`, and APFS is case-insensitive by default. As a
# result, `target/debug/spacebot` and `target/debug/Spacebot` resolve to the same inode,
# causing Tauri's dev-mode sidecar lookup to execute the desktop host instead of the daemon.
#
# If you rename this, update the three matching references so they stay in sync:
#   - desktop/src-tauri/tauri.conf.json           (externalBin)
#   - desktop/src-tauri/capabilities/default.json (shell:allow-spawn name)
#   - interface/src/components/ConnectionScreen.tsx (spawnBundledProcess arg)
DEST_BIN="$BINARIES_DIR/spacebot-daemon-${TARGET_TRIPLE}${SUFFIX}"

cp "$SRC_BIN${SUFFIX}" "$DEST_BIN"
echo "Copied $SRC_BIN -> $DEST_BIN"
echo "Sidecar binary ready."
