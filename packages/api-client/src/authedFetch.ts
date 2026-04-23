// Phase 6 PR B Task 6.B.2 — central fetch wrapper for all API calls.
//
// Attaches Authorization when the token provider yields a token.
// Preserves FormData + Blob browser-managed Content-Type boundary.
// Retries once on 401 after forcing a fresh token (auth cap = 1).
// Retries up to 3 times on 202 Accepted (A-10, Graph-sync race).
// Hard safety cap at 5 total attempts for pathological 401↔202 loops.
//
// D4/O1 correction: lives in a sibling module so Task 6.B.3's sed pass
// against client.ts cannot produce recursion. This file contains
// exactly one `fetch(` call by design: the delegation to the browser
// primitive at line ~60.

import { getAuthToken } from "./client";

// Budget tracker threaded through retry recursion. O3 correction: kept
// module-private so callers can't misuse it (e.g., pre-setting
// `totalAttempts: 4` to neuter retries on a specific call).
type RetryState = {
	authAttempts: number; // 401 → token-refresh retries (cap: 1)
	syncAttempts: number; // 202 → permissions-sync retries (cap: 3, A-10)
	totalAttempts: number; // hard safety cap against 401↔202 loops (cap: 5)
};

const INITIAL_STATE: RetryState = {
	authAttempts: 0,
	syncAttempts: 0,
	totalAttempts: 0,
};

// Derived from Phase 3's A-10 race handling: `group_cache_ttl_secs`
// default 300s → Retry-After: 2s → 3 retries buys 6s of wait before the
// daemon has populated `team_memberships`. Auth refresh is deliberately
// capped at 1 because a second consecutive 401 with a fresh token is an
// operator problem (bad `aud`, wrong tenant, clock-skew) that no amount
// of client retry will fix.
const AUTH_CAP = 1;
const SYNC_CAP = 3;
const TOTAL_CAP = 5;

// Public API. Drop-in replacement for `fetch()` with Entra-aware auth
// behavior. `state` is intentionally NOT a parameter: callers should
// not tune retry budgets per-call.
export async function authedFetch(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	return authedFetchInner(input, init ?? {}, { ...INITIAL_STATE });
}

async function authedFetchInner(
	input: RequestInfo | URL,
	init: RequestInit,
	state: RetryState,
): Promise<Response> {
	const headers = new Headers(init.headers ?? {});

	// D5 correction: route through getAuthToken() to inherit PR A's
	// error-swallow fence. Null returns are normal (no provider, or
	// provider threw and got caught).
	const token = await getAuthToken();
	if (token) headers.set("Authorization", `Bearer ${token}`);

	// Preserve browser-managed Content-Type for body types where the
	// browser computes the boundary or MIME type. FormData + Blob both
	// fall into this class.
	if (init.body instanceof FormData || init.body instanceof Blob) {
		headers.delete("Content-Type");
	}

	const res = await fetch(input, { ...init, headers });

	// Overall safety cap. Terminates any pathological 401↔202 loop that
	// evades the per-dimension budgets.
	if (state.totalAttempts >= TOTAL_CAP) return res;

	// 401 → force refresh + retry, subject to the auth cap.
	if (res.status === 401 && state.authAttempts < AUTH_CAP) {
		// D3 correction: check that the token provider CAN still yield a
		// token. If the retry would produce null (provider unset, or
		// provider threw and got caught), the retry cannot change the
		// outcome. Return the 401 to the caller instead of entering a
		// no-header retry loop.
		const retryToken = await getAuthToken();
		if (retryToken === null) {
			// O2 correction: surface for observability. Downstream can wire
			// Sentry / ReactQuery invalidation / toast banner via
			// addEventListener.
			if (typeof window !== "undefined") {
				window.dispatchEvent(
					new CustomEvent("spacebot:auth-exhausted", {
						detail: { url: String(input), reason: "no_token_on_retry" },
					}),
				);
			}
			return res;
		}
		return authedFetchInner(input, init, {
			...state,
			authAttempts: state.authAttempts + 1,
			totalAttempts: state.totalAttempts + 1,
		});
	}

	// O2 correction: surface final 401 after refresh as well. If we
	// reach this branch with authAttempts >= AUTH_CAP and status 401,
	// the refresh did not help.
	if (res.status === 401 && state.authAttempts >= AUTH_CAP) {
		if (typeof window !== "undefined") {
			window.dispatchEvent(
				new CustomEvent("spacebot:auth-exhausted", {
					detail: { url: String(input), reason: "refresh_failed" },
				}),
			);
		}
	}

	// A-10: 202 Accepted + Retry-After during the first-request race
	// against Phase 3's group-sync fire-and-forget spawn.
	if (res.status === 202 && state.syncAttempts < SYNC_CAP) {
		const waitMs = parseRetryAfterMs(res.headers.get("retry-after"));
		await new Promise((resolve) => setTimeout(resolve, waitMs));
		return authedFetchInner(input, init, {
			...state,
			syncAttempts: state.syncAttempts + 1,
			totalAttempts: state.totalAttempts + 1,
		});
	}

	return res;
}

// D9 correction: the daemon ALWAYS emits `Retry-After: 2` when returning
// 202 from the first-request race handler (see
// `src/auth/middleware.rs:309-317`). A 202 without `Retry-After` is a
// server contract violation; default to 1000ms so the client stays
// responsive while the daemon is fixed. Previously this was 2000ms with
// no documented rationale. Clamped to 10s to prevent a malicious proxy
// from locking the SPA in a long sleep.
function parseRetryAfterMs(header: string | null): number {
	if (!header) return 1000;
	const parsed = Number.parseInt(header, 10);
	if (Number.isNaN(parsed) || parsed < 0) return 1000;
	return Math.min(parsed * 1000, 10_000);
}
