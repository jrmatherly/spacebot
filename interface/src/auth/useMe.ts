// Consolidated identity hook. Reads from GET /api/me (one payload
// carrying principal_key, tid/oid, roles, groups, display name/email,
// and either a photo data URL or initials fallback).
//
// Phase 7's useRole(role) reads from this hook. 5-minute staleTime
// matches the daemon's group_cache_ttl_secs default so UI-stale and
// daemon-stale windows don't interleave.

import { useQuery } from "@tanstack/react-query";
import { getApiBase } from "@spacebot/api-client/client";
import { authedFetch } from "@spacebot/api-client/authedFetch";
import type { components } from "@spacebot/api-client/schema";

export type MeResponse = components["schemas"]["MeResponse"];

export function useMe() {
	return useQuery({
		queryKey: ["me"],
		queryFn: async (): Promise<MeResponse> => {
			const path = "/me";
			const res = await authedFetch(`${getApiBase()}${path}`);
			if (!res.ok) {
				// Match fetchJson's convention: status + path so Sentry
				// breadcrumbs identify the failing endpoint.
				throw new Error(`API error ${res.status}: ${path}`);
			}
			try {
				return (await res.json()) as MeResponse;
			} catch {
				// Distinguish malformed daemon response from network
				// failure. React Query otherwise wraps the SyntaxError
				// identically to a fetch rejection.
				throw new Error(`API error: malformed JSON from ${path}`);
			}
		},
		staleTime: 5 * 60_000,
	});
}

// Known built-in roles get autocomplete + typo protection. The
// `(string & {})` intersection preserves the opt-out for ad-hoc roles
// a future plugin / custom deployment might introduce.
type KnownRole = "SpacebotAdmin" | "SpacebotUser" | "SpacebotService";
type RoleLike = KnownRole | (string & {});

/**
 * Phase 7: the canonical gate for admin-only UI surfaces. Reads from
 * the same /api/me cache so there is one source of truth for the
 * principal's roles.
 */
export function useRole(role: RoleLike): boolean {
	const { data } = useMe();
	return Boolean(data?.roles.includes(role));
}

/**
 * Consolidated principal-key helper. Draws from the same /api/me cache
 * as useMe + useRole, so a sign-out invalidates all three together.
 */
export function useMyPrincipalKey(): string | null {
	const { data } = useMe();
	return data?.principal_key ?? null;
}

export type TeamSummary = components["schemas"]["TeamSummary"];

/**
 * Active teams for the ShareResourceModal selector. Authenticated-only;
 * the backend filters archived rows at the SQL layer so the UI does not
 * have to. 5-minute staleTime matches useMe: team membership changes
 * propagate through the same Graph-sync cadence as role claims.
 */
export function useTeams() {
	return useQuery({
		queryKey: ["teams"],
		queryFn: async (): Promise<TeamSummary[]> => {
			const path = "/teams";
			const res = await authedFetch(`${getApiBase()}${path}`);
			if (!res.ok) {
				throw new Error(`API error ${res.status}: ${path}`);
			}
			try {
				return (await res.json()) as TeamSummary[];
			} catch {
				throw new Error(`API error: malformed JSON from ${path}`);
			}
		},
		staleTime: 5 * 60_000,
	});
}
