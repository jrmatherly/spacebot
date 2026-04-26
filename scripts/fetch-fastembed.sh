#!/usr/bin/env bash
# Pre-stage the fastembed model cache so the 4 memory::search integration
# tests can hit a local cache instead of downloading at test time.
#
# Why this exists: src/memory/search.rs:577-661 (4 tests) construct
# `EmbeddingModel::new(tempfile::tempdir().path())` per run. fastembed
# 5.13.x then tries to download the default `BGESmallENV15` model from
# `https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/...` —
# 5 files totaling ~127 MB. On macOS this download path intermittently
# fails inside Rust's `ureq` + `native-tls` stack (the LFS CDN redirect
# target uses a cert chain that native-tls rejects, even though `curl`
# against `huggingface.co` works fine). Pre-staging the cache and
# exporting `HF_HOME` makes fastembed take the cache-hit path, skipping
# the network entirely.
#
# Layout this script produces (matches hf-hub 0.5.0's
# `Cache::repo().get(filename)` lookup logic at lib.rs:179-189):
#
#   <cache>/models--Xenova--bge-small-en-v1.5/
#     refs/main           ← exact 40-char commit hash, NO trailing newline
#     snapshots/<commit>/
#       config.json
#       special_tokens_map.json
#       tokenizer.json
#       tokenizer_config.json
#       onnx/model.onnx   ← 127 MB
#
# The trailing-newline detail is load-bearing: hf-hub does
# `std::fs::read_to_string(refs/main)` which preserves trailing newlines,
# then constructs `snapshots/<hash>\n/onnx/model.onnx` — a path that
# obviously doesn't exist. `printf "%s"` (NOT `echo`) is required.
#
# Usage:
#   ./scripts/fetch-fastembed.sh           # fetch into ~/.cache/fastembed
#   FASTEMBED_CACHE=/tmp/fe ./scripts/...  # override cache location
#
# After running, export the cache path so tests pick it up:
#   export HF_HOME="$(./scripts/fetch-fastembed.sh --print-cache-dir)"
#   cargo test --lib
#
# `just fetch-fastembed` wraps this and prints the export line at the end.

set -euo pipefail

CACHE_DIR="${FASTEMBED_CACHE:-${HOME}/.cache/fastembed}"
REPO="Xenova/bge-small-en-v1.5"
REPO_DIR_NAME="models--Xenova--bge-small-en-v1.5"
HF_ENDPOINT="${HF_ENDPOINT:-https://huggingface.co}"

# `--print-cache-dir` short-circuit: emit the cache path and exit. Lets
# `just` recipes compute the export value without running the full fetch.
if [ "${1:-}" = "--print-cache-dir" ]; then
    printf "%s\n" "${CACHE_DIR}"
    exit 0
fi

REPO_DIR="${CACHE_DIR}/${REPO_DIR_NAME}"

# Resolve the commit hash for `main` from the HF API. Pinning to a commit
# hash (rather than the literal "main" branch ref) is what hf-hub does
# internally. Its `refs/main` file holds a commit hash, and snapshots
# are looked up by that hash. Any tool with `python3 -m json.tool` works
# here; using `python3 -c` keeps the dep surface zero (no `jq` required).
resolve_commit() {
    local sha
    sha=$(curl -sf "${HF_ENDPOINT}/api/models/${REPO}" \
        | python3 -c "import json,sys; print(json.load(sys.stdin)['sha'])")
    if [ -z "${sha}" ] || [ "${#sha}" -ne 40 ]; then
        echo "ERROR: could not resolve commit sha for ${REPO} (got: '${sha}')" >&2
        exit 1
    fi
    printf "%s" "${sha}"
}

# Skip-if-fresh: if refs/main already points at a snapshot dir with all
# 5 expected files, treat the cache as ready and exit. Idempotent re-runs
# return in <1s.
is_cache_complete() {
    [ -f "${REPO_DIR}/refs/main" ] || return 1
    local commit
    commit=$(cat "${REPO_DIR}/refs/main")
    [ -d "${REPO_DIR}/snapshots/${commit}" ] || return 1
    local snap="${REPO_DIR}/snapshots/${commit}"
    for f in config.json special_tokens_map.json tokenizer.json tokenizer_config.json onnx/model.onnx; do
        [ -f "${snap}/${f}" ] || return 1
        # onnx/model.onnx must be the actual 127 MB ONNX model, not a tiny
        # LFS pointer file that some redirects can produce.
        if [ "${f}" = "onnx/model.onnx" ]; then
            local size
            size=$(stat -f%z "${snap}/${f}" 2>/dev/null || stat -c%s "${snap}/${f}")
            [ "${size}" -gt 100000000 ] || return 1
        fi
    done
    return 0
}

if is_cache_complete; then
    echo "✓ fastembed cache already complete at ${REPO_DIR}"
    echo "  Set HF_HOME=${CACHE_DIR} before running cargo test."
    exit 0
fi

echo "→ Resolving current commit for ${REPO}…"
COMMIT=$(resolve_commit)
echo "  commit: ${COMMIT}"

SNAP_DIR="${REPO_DIR}/snapshots/${COMMIT}"
mkdir -p "${REPO_DIR}/refs" "${SNAP_DIR}/onnx"

# Write commit hash WITHOUT trailing newline. `echo "$x" > file` would add
# a `\n` that hf-hub then includes in the constructed snapshot path,
# breaking cache lookup silently. printf is the only safe option here.
printf "%s" "${COMMIT}" > "${REPO_DIR}/refs/main"

download() {
    local file="$1"
    local target="$2"
    if [ -f "${target}" ]; then
        echo "  ✓ ${file} (already present)"
        return 0
    fi
    echo "  → ${file}"
    # Use --fail-with-body so a 4xx/5xx surfaces as a script failure
    # instead of silently writing an HTML error page to ${target}.
    if ! curl -sfL "${HF_ENDPOINT}/${REPO}/resolve/${COMMIT}/${file}" -o "${target}"; then
        echo "ERROR: failed to download ${file} from ${HF_ENDPOINT}" >&2
        rm -f "${target}"
        exit 1
    fi
}

echo "→ Fetching tokenizer + config files into ${SNAP_DIR}…"
for f in config.json special_tokens_map.json tokenizer.json tokenizer_config.json; do
    download "${f}" "${SNAP_DIR}/${f}"
done

echo "→ Fetching ONNX model (~127 MB) into ${SNAP_DIR}/onnx/…"
download "onnx/model.onnx" "${SNAP_DIR}/onnx/model.onnx"

echo ""
echo "✓ fastembed cache ready at ${REPO_DIR}"
echo ""
echo "  Run tests with:"
echo "    export HF_HOME=${CACHE_DIR}"
echo "    cargo test --lib"
