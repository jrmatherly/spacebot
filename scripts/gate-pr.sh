#!/usr/bin/env bash
set -euo pipefail

usage() {
	cat <<'EOF'
Usage: scripts/gate-pr.sh [--ci] [--fast] [--nextest]

Options:
  --ci       CI mode (enables CI-oriented migration base resolution).
  --fast     Skip clippy and integration test compile for quicker local iteration.
  --nextest  Run unit tests via `cargo nextest run --lib` instead of `cargo test --lib`.
             Requires cargo-nextest installed (`cargo install cargo-nextest`).
             Equivalent env var: GATE_PR_NEXTEST=1
EOF
}

is_ci=false
fast_mode=false
# Nextest opt-in: CLI flag wins, env var is the fallback. Default off because
# nextest's process-per-test isolation may surface latent shared-state assumptions
# in tests that today silently pass under cargo test's shared-process model.
# Promote to default after a sustained period of nextest runs landing green.
use_nextest="${GATE_PR_NEXTEST:-}"
[[ "$use_nextest" == "1" || "$use_nextest" == "true" ]] && use_nextest=true || use_nextest=false

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
		use_nextest=true
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
run_step "cargo fmt --all -- --check" cargo fmt --all -- --check

# `cargo check` was previously run here. Clippy is a strict superset (invokes
# rustc with the full lint set) so running both caused ~30-50s of redundant
# work. Documented anti-pattern in .claude/rules/rust-iteration-loop.md:99.
# For a no-clippy escape hatch during debugging, run `just check-all`.
if $fast_mode; then
	log "fast mode enabled: skipping clippy and integration test compile"
	run_step "cargo check --all-targets" cargo check --all-targets
else
	run_step "RUSTFLAGS=\"-Dwarnings\" cargo clippy --all-targets" env RUSTFLAGS="-Dwarnings" cargo clippy --all-targets
fi

if $use_nextest; then
	if ! command -v cargo-nextest >/dev/null 2>&1; then
		fail "--nextest flag set but cargo-nextest not installed (run: cargo install cargo-nextest)"
	fi
	run_step "cargo nextest run --lib" cargo nextest run --lib
else
	run_step "cargo test --lib" cargo test --lib
fi

if ! $fast_mode; then
	run_step "cargo test --tests --no-run" cargo test --tests --no-run
fi

log "all gate checks passed"
