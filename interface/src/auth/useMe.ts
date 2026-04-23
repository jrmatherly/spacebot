// Phase 6 PR C Task 6.C.6 Step 4 — consolidated identity hook.
//
// Reads from GET /api/me (A-18 single endpoint). The response carries
// everything the SPA needs about the signed-in principal: identity,
// roles, groups (overage-resolved when groups_overage is true),
// display name/email, and either a photo data URL or initials.
//
// Phase 7's useRole(role) reads from this hook, so we cache for 5
// minutes (matches the daemon's group_cache_ttl_secs default to
// avoid UI-stale vs daemon-stale interleaving).
//
// D11 correction (2026-04-23 PR C audit): authedFetch imported from
// the sibling exports entry @spacebot/api-client/authedFetch (PR B
// commit C ships this). getApiBase stays at @spacebot/api-client/client.
//
// D17 correction (2026-04-23 PR C audit): throw-on-!ok message matches
// PR B commit B's fetchJson convention — `API error <status>: <path>`
// so operator/Sentry breadcrumbs identify the failing endpoint.

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
				throw new Error(`API error ${res.status}: ${path}`);
			}
			return res.json();
		},
		staleTime: 5 * 60_000,
	});
}

/**
 * Phase 7 preview: `useRole("SpacebotAdmin")` becomes the canonical
 * gate for admin-only UI surfaces. Reads from the same `/api/me`
 * cache so there is one source of truth for the principal's roles.
 */
export function useRole(role: string): boolean {
	const { data } = useMe();
	return Boolean(data?.roles.includes(role));
}
