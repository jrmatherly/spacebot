---
name: fetch-fastembed
description: Pre-stage the fastembed BGESmallENV15 ONNX model cache so the 4 memory::search integration tests can run reliably. Call when the user reports `EmbeddingFailed("Failed to retrieve onnx/model.onnx")`, when memory::search tests fail with `native-tls` cert errors, or before a `cargo test --lib` run on a fresh checkout / clean target/.
disable-model-invocation: true
---

# fetch-fastembed

Wraps `just fetch-fastembed` (which calls `scripts/fetch-fastembed.sh`) to pre-stage the HuggingFace cache so the 4 memory tests don't have to download `~/.cache/huggingface/hub/Xenova/bge-small-en-v1.5/...` during the test run.

## Why this exists

`src/memory/search.rs:577-661` (4 tests) construct `EmbeddingModel::new(tempfile::tempdir().path())` per run. fastembed 5.13.x then tries to download the default `BGESmallENV15` model from `https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/...` — 5 files totaling ~127 MB. On macOS the download path intermittently fails inside Rust's `ureq + native-tls` stack: the LFS CDN redirect target uses a cert chain that native-tls rejects, even though `curl` against `huggingface.co` itself works fine.

Pre-staging the cache + setting `HF_HOME` makes fastembed take the cache-hit path, skipping the network entirely. This was the recipe shipped in commit `a1b245f` (2026-04-26) after a 2-hour investigation traced the failure to a trailing-newline gotcha in the `refs/main` file (hf-hub's `read_to_string` preserves the trailing `\n`, then constructs `snapshots/<hash>\n/onnx/model.onnx` — a path that silently doesn't exist).

## Usage

```bash
# One-time cache pre-stage (~127 MB download on first run; <1s on re-runs)
just fetch-fastembed

# Export the cache path so cargo test picks it up
export HF_HOME=$(just fetch-fastembed-cache-dir)

# Now run tests
cargo test --lib

# OR run only the previously-flaky 4 tests
cargo test --lib memory::search::tests::
```

## What the script produces

```
$HOME/.cache/fastembed/models--Xenova--bge-small-en-v1.5/
├── refs/main           ← 40-char commit hash, NO trailing newline (load-bearing)
└── snapshots/<commit>/
    ├── config.json
    ├── special_tokens_map.json
    ├── tokenizer.json
    ├── tokenizer_config.json
    └── onnx/model.onnx ← 127 MB
```

Override the cache location with `FASTEMBED_CACHE=/tmp/fe just fetch-fastembed`.

## Idempotency + safety checks

The script's `is_cache_complete()` function checks all 5 files exist AND that `onnx/model.onnx` is >100 MB (catches the case where an LFS pointer file masquerades as the real model). Re-runs return in <1s.

## Related context

- Commit `a1b245f`: shipped the script + recipe + CLAUDE.md pointer
- Phase 7 polish backlog item #9: refactor `src/memory/search.rs` to share a `OnceCell<EmbeddingModel>` fixture across the 4 tests, dropping per-test runtime from ~11s to <1s. Until that ships, `just fetch-fastembed` is the practical workaround.
- `CLAUDE.md` "Build & Test" section line 22 documents the env-var requirement.
