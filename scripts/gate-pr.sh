#!/usr/bin/env bash
set -euo pipefail

usage() {
	cat <<'EOF'
Usage: scripts/gate-pr.sh [--ci] [--fast] [--no-nextest]

Options:
  --ci          CI mode (enables CI-oriented migration base resolution).
  --fast        Skip clippy and integration test compile for quicker local iteration.
  --no-nextest  Run unit tests via `cargo test --lib` instead of `cargo nextest run --lib`.
                Default since 2026-04-22 is nextest; this flag is the escape hatch for
                environments without cargo-nextest installed, or for A/B-ing runner behavior.
                Equivalent env var: GATE_PR_NEXTEST=0
  --nextest     Accepted for backward compatibility; nextest is the default.
EOF
}

is_ci=false
fast_mode=false
# Nextest is now the default. Promotion rationale: phases 3, 4 (PR 1 + PR 2)
# landed green on nextest without surfacing shared-state regressions, clearing
# the "sustained period of nextest runs landing green" bar recorded here pre-flip.
# Escape hatches: `--no-nextest` CLI flag OR `GATE_PR_NEXTEST=0` env var falls back
# to `cargo test --lib`.
use_nextest="${GATE_PR_NEXTEST:-1}"
[[ "$use_nextest" == "0" || "$use_nextest" == "false" ]] && use_nextest=false || use_nextest=true

while (($# > 0)); do
	case "$1" in
	--ci)
		is_ci=true
		shift
		;;
	--fast)
		fast_mode=true
		shift
		;;
	--nextest)
		# Accepted for backward compatibility with pre-2026-04-22 callers
		# (e.g., `just gate-pr-nextest`); nextest is now the default.
		use_nextest=true
		shift
		;;
	--no-nextest)
		use_nextest=false
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "[gate-pr] ERROR: unknown argument: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

log() {
	echo "[gate-pr] $*"
}

fail() {
	echo "[gate-pr] ERROR: $*" >&2
	exit 1
}

run_step() {
	local label="$1"
	shift
	log "running: $label"
	"$@"
}

repository_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
[[ -n "$repository_root" ]] || fail "not inside a git worktree"
cd "$repository_root"

if $is_ci; then
	log "CI mode enabled"
fi

# Migration safety enforcement is disabled as of 2026-04-16. The prior
# implementation now lives in scripts/_disabled/check-migration-safety.sh
# and can be reactivated from there. Note: editing migrations whose
# checksums are already stored in deployed SQLx `_sqlx_migrations` tables
# will cause checksum mismatches and break startup on those databases;
# coordinate a full redeploy or migration-checksum repair when landing
# migration reformatting changes.
run_step "check-sidecar-naming" ./scripts/check-sidecar-naming.sh

# Frontend invariant guards: workspace-protocol (tracked package.json deps
# must use workspace:*), vite-dedupe (interface dedupe matches spaceui
# shared deps), and ADR anchors (spacedrive integration path:line anchors
# still resolve). Combined overhead is ~300ms post the Task 8 speedup of
# check-workspace-protocol.sh. Local-scope only: .github/workflows/ci.yml
# does not invoke `just gate-pr` — CI enforcement for check-workspace-protocol
# lives in .github/workflows/spaceui.yml, while check-vite-dedupe and
# check-adr-anchors currently have no CI coverage (tracked as a follow-up).
run_step "check-workspace-protocol" ./scripts/check-workspace-protocol.sh
run_step "check-vite-dedupe" ./scripts/check-vite-dedupe.sh
run_step "check-adr-anchors" ./scripts/check-adr-anchors.sh

run_step "cargo fmt --all -- --check" cargo fmt --all -- --check

# `cargo check` was previously run here. Clippy is a strict superset (invokes
# rustc with the full lint set) so running both caused ~30-50s of redundant
# work. Documented anti-pattern in .claude/rules/rust-iteration-loop.md:99.
# For a no-clippy escape hatch during debugging, run `just check-all`.
if $fast_mode; then
	log "fast mode enabled: skipping clippy and integration test compile"
	# NOTE: `cargo check` below does NOT propagate RUSTFLAGS="-Dwarnings".
	# Fast-mode green is not a guarantee of full-gate green — a warning
	# introduced during fast-mode iteration will only surface in the full
	# gate below (non-fast branch). See CONTRIBUTING.md for details.
	run_step "cargo check --all-targets" cargo check --all-targets
else
	run_step "RUSTFLAGS=\"-Dwarnings\" cargo clippy --all-targets" env RUSTFLAGS="-Dwarnings" cargo clippy --all-targets
fi

if $use_nextest; then
	if ! command -v cargo-nextest >/dev/null 2>&1; then
		fail "nextest is the default unit-test runner but cargo-nextest is not installed (run: cargo install cargo-nextest), or pass --no-nextest / set GATE_PR_NEXTEST=0 to use cargo test"
	fi
	run_step "cargo nextest run --lib" cargo nextest run --lib
else
	run_step "cargo test --lib" cargo test --lib
fi

if ! $fast_mode; then
	run_step "cargo test --tests --no-run" cargo test --tests --no-run
fi

log "all gate checks passed"
