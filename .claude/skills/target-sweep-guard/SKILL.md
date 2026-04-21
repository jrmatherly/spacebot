---
name: target-sweep-guard
description: Check the size of `target/` and run `just sweep-target` if it exceeds a threshold. Use when disk pressure is a concern, at mid-phase checkpoints, or after a TDD cycle that compiled many test binaries. The canonical storage-budget reference is `.scratchpad/plans/entraid-auth/INDEX.md` § "Cargo discipline — storage + time budget." Reports bytes reclaimed.
---

# Target Sweep Guard

## When to Use

Invoke at natural storage-pressure checkpoints:
- **After ~10 compile cycles** (counted informally — TDD cycle on one file, plus a full lib rebuild, is about one compile-cycle-equivalent)
- **Before `just gate-pr`** if you suspect `target/` is bloated
- **After a phase squash-merges** — reclaim the feature-branch-specific fingerprints
- **When `du -sh target/` reports > 40 GB** (the default threshold in this skill)
- **After a `cargo update`** that invalidated many fingerprints

Do NOT invoke:
- In the middle of a compile (interference)
- During an active `cargo test` run
- Before a cold-cache build where the next run needs every fingerprint (e.g., directly after `cargo clean`)

## What It Does

1. Measure `target/` size
2. If under threshold (default 40 GB), report and exit with status 0 (no action)
3. If over threshold, run `just sweep-target` (which runs `cargo sweep --installed` + `cargo sweep --time 30`)
4. Re-measure; report bytes reclaimed
5. Suggest `just clean-all` as the nuclear next step if `sweep-target` didn't help

## Arguments

Accepts an optional threshold override in GB:

```
/target-sweep-guard          # uses default 40 GB threshold
/target-sweep-guard 30       # threshold is 30 GB
/target-sweep-guard --force  # runs sweep regardless of size
```

## Implementation

```bash
THRESHOLD_GB="${1:-40}"
FORCE=0
if [ "$1" = "--force" ]; then FORCE=1; fi

# Measure before
BEFORE=$(du -sk target 2>/dev/null | awk '{print $1}')
BEFORE_GB=$((BEFORE / 1024 / 1024))

echo "target/ size: ${BEFORE_GB} GB"

if [ "$FORCE" -eq 0 ] && [ "$BEFORE_GB" -lt "$THRESHOLD_GB" ]; then
  echo "✅ Under threshold (${THRESHOLD_GB} GB). No action."
  exit 0
fi

# Sweep
echo "Running just sweep-target..."
just sweep-target

# Measure after
AFTER=$(du -sk target 2>/dev/null | awk '{print $1}')
AFTER_GB=$((AFTER / 1024 / 1024))
RECLAIMED_KB=$((BEFORE - AFTER))
RECLAIMED_GB=$((RECLAIMED_KB / 1024 / 1024))

echo "target/ size after sweep: ${AFTER_GB} GB"
echo "Reclaimed: ${RECLAIMED_GB} GB"

if [ "$AFTER_GB" -ge "$THRESHOLD_GB" ]; then
  echo ""
  echo "⚠️  Still over threshold after sweep. If you can afford a cold rebuild:"
  echo "    just clean-all    # wipes target/, interface/dist, node_modules, .fastembed_cache"
  echo "    # Next cargo build will take ~6 min cold-cache"
fi
```

## Reporting

Always report:
- Size before
- Whether action was taken
- Size after (if action)
- Bytes reclaimed (if action)

## Anti-Patterns

| Anti-pattern | Why it's wrong |
|---|---|
| Running `cargo clean` as first-line response to disk pressure | Forces full rebuild; `just sweep-target` reclaims most of the space without invalidating current state |
| Running this skill inside a tight `cargo test` loop | Sweep interferes with in-flight compilation |
| Setting the threshold to < 15 GB | Legit warm-cache `target/` for this project is ~20-30 GB; sweeping below that wastes rebuild time |
| Invoking on every Edit | Far too aggressive; once per session or at explicit checkpoints is enough |

## Composition

- Pair with the storage-discipline notes in `.scratchpad/plans/entraid-auth/phase-*.md` files
- The `entra-phase-wrap` skill should invoke this as its Step 5-ish cleanup
- The Phase 4, 5, 9, 10 plan files already name sweep checkpoints; those checkpoints should call this skill

## What this skill does NOT do

- It does not clean `node_modules/` (use `just clean-frontend`)
- It does not clean `.fastembed_cache/` (use `just clean-all` if needed)
- It does not clean Docker or Nix artifacts
- It does not modify source files, lockfiles, or workspace config
- It does not run across `desktop/src-tauri/target/` separately — add a second invocation if you need to sweep the Tauri target

## Reference

- `justfile` recipes: `sweep-target`, `clean-all`, `clean-frontend`
- `.claude/rules/rust-iteration-loop.md` — canonical rule for iteration discipline
- `.scratchpad/plans/entraid-auth/INDEX.md` § "Cargo discipline — storage + time budget" — phase-aware prescriptions
