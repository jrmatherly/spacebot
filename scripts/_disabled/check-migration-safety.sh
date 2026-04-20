#!/usr/bin/env bash
# shellcheck shell=bash
# check-migration-safety.sh — DISABLED as of 2026-04-16.
#
# Historical policy enforced that migration files in migrations/ were
# immutable: once committed, never edited. The rationale was that SQLx
# stores migration checksums in _sqlx_migrations at apply time, so editing
# an already-applied migration causes startup failures on that database
# until the checksum is repaired or the DB is reset.
#
# Spacebot now owns its migration tree (we do not track an upstream) and
# has chosen to allow historical reformatting. This file preserves the
# enforcement logic so the policy can be re-enabled by:
#   1. Sourcing this file from scripts/gate-pr.sh, and
#   2. Calling check_migration_safety inside the gate.
#
# When sourcing from gate-pr.sh, delete (or guard) the top-level `is_ci`
# parse block below so the standalone-mode arg parsing does not clobber
# the caller's $is_ci variable.
#
# When reactivating, re-test the diff-range resolution cascade against
# your current CI environment — the logic predates several workflow
# redesigns and may need updates.
#
# NOT WIRED INTO ANY GATE. Do not assume this runs automatically.

set -euo pipefail

is_ci=false
if [[ "${1:-}" == "--ci" ]]; then
	is_ci=true
fi

log() {
	echo "[check-migration-safety] $*"
}

fail() {
	echo "[check-migration-safety] ERROR: $*" >&2
	exit 1
}

resolve_migration_diff_range() {
	local base_ref=""
	local head_sha=""
	head_sha="$(git rev-parse HEAD)"

	if $is_ci \
		&& [[ -n "${GITHUB_EVENT_BEFORE:-}" && "${GITHUB_EVENT_BEFORE}" != "0000000000000000000000000000000000000000" ]] \
		&& git rev-parse --verify --quiet "${GITHUB_EVENT_BEFORE}^{commit}" >/dev/null 2>&1; then
		echo "${GITHUB_EVENT_BEFORE}..HEAD"
		return
	fi

	if [[ -n "${PR_GATE_BASE_REF:-}" ]]; then
		base_ref="${PR_GATE_BASE_REF}"
	elif [[ -n "${GITHUB_BASE_REF:-}" ]]; then
		base_ref="origin/${GITHUB_BASE_REF}"
	elif git symbolic-ref --quiet --short refs/remotes/origin/HEAD >/dev/null 2>&1; then
		base_ref="$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD)"
	elif git rev-parse --verify --quiet origin/main >/dev/null 2>&1; then
		base_ref="origin/main"
	fi

	if [[ -n "$base_ref" ]] && git rev-parse --verify --quiet "$base_ref" >/dev/null 2>&1; then
		local merge_base=""
		merge_base="$(git merge-base HEAD "$base_ref" 2>/dev/null || true)"
		if [[ -n "$merge_base" && "$merge_base" != "$head_sha" ]]; then
			echo "${merge_base}..HEAD"
			return
		fi
	fi

	if git rev-parse --verify --quiet HEAD~1 >/dev/null 2>&1; then
		echo "HEAD~1..HEAD"
	fi
}

check_migration_safety() {
	log "checking migration safety"

	local diff_range=""
	diff_range="$(resolve_migration_diff_range)"
	if [[ -n "$diff_range" ]]; then
		log "migration diff range: $diff_range"
	else
		log "migration diff range: working tree only (no base ref available)"
	fi

	local -a migration_changes=()
	local migration_change=""
	while IFS= read -r migration_change; do
		migration_changes+=("$migration_change")
	done < <(
		{
			if [[ -n "$diff_range" ]]; then
				git diff --name-status "$diff_range" -- migrations
			fi
			git diff --name-status --cached -- migrations
			git diff --name-status -- migrations
			git ls-files --others --exclude-standard -- migrations | sed $'s/^/A\t/'
		} | sed '/^[[:space:]]*$/d' | sort -u
	)

	if ((${#migration_changes[@]} == 0)); then
		log "migration safety passed (no migration changes detected)"
		return
	fi

	declare -A paths_with_add=()
	for line in "${migration_changes[@]}"; do
		local status=""
		local path=""
		if [[ "$line" == *$'\t'* ]]; then
			status="${line%%$'\t'*}"
			path="${line#*$'\t'}"
		else
			status="${line%% *}"
			path="${line#* }"
			[[ "$path" == "$line" ]] && path=""
		fi
		if [[ -n "$path" && "$status" == A* ]]; then
			paths_with_add["$path"]=1
		fi
	done

	local violations=()
	for line in "${migration_changes[@]}"; do
		local status=""
		local path=""
		if [[ "$line" == *$'\t'* ]]; then
			status="${line%%$'\t'*}"
			path="${line#*$'\t'}"
		else
			status="${line%% *}"
			path="${line#* }"
			[[ "$path" == "$line" ]] && path=""
		fi
		if [[ -n "$path" && "$status" != A* && -z "${paths_with_add[$path]:-}" ]]; then
			violations+=("$line")
		fi
	done

	if ((${#violations[@]} > 0)); then
		echo "[check-migration-safety] ERROR: existing migration files were modified:" >&2
		printf '  %s\n' "${violations[@]}" >&2
		fail "create a new timestamped migration instead of editing migration history"
	fi

	log "migration safety passed (only new migration files detected)"
}

# Run if this file is invoked directly (not sourced). Allows reactivation
# either by sourcing and calling check_migration_safety, or by invoking
# this file directly from a gate.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
	repository_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
	[[ -n "$repository_root" ]] || fail "not inside a git worktree"
	cd "$repository_root"
	check_migration_safety
fi
