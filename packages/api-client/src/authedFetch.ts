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

// Reasons surfaced on the `spacebot:auth-exhausted` CustomEvent.
// Downstream listeners (Sentry, toast banner, React Query invalidation)
// can narrow on this to distinguish "MSAL silent acquisition failed
// without triggering a redirect" (no_token_on_retry) from "fresh token
// did not satisfy the server" (refresh_failed).
export type AuthExhaustedReason = "no_token_on_retry" | "refresh_failed";

export type AuthExhaustedDetail = {
	url: string;
	reason: AuthExhaustedReason;
};

// Budget tracker threaded through retry recursion. O3 correction: kept
// module-private so callers can't misuse it (e.g., pre-setting a
// high `authAttempts` to neuter retries on a specific call).
//
// totalAttempts is derivable as authAttempts + syncAttempts, so it is
// NOT stored on the state. Recomputed at the safety-cap check.
type RetryState = {
	authAttempts: number; // 401 → token-refresh retries (cap: 1)
	syncAttempts: number; // 202 → permissions-sync retries (cap: 3, A-10)
};

const INITIAL_STATE: RetryState = {
	authAttempts: 0,
	syncAttempts: 0,
};

// Derived from Phase 3's A-10 race handling: `group_cache_ttl_secs`
// default 300s → Retry-After: 2s → 3 retries buys 6s of wait before the
// daemon has populated `team_memberships`. Auth refresh is deliberately
// capped at 1 because a second consecutive 401 with a fresh token is an
// operator problem (bad `aud`, wrong tenant, clock-skew) that no amount
// of client retry will fix.
const AUTH_CAP = 1;
const SYNC_CAP = 3;
const TOTAL_CAP = AUTH_CAP + SYNC_CAP + 1; // safety net, structurally unreachable under current caps

// Public API. Drop-in replacement for `fetch()` with Entra-aware auth
// behavior. `state` is intentionally NOT a parameter: callers should
// not tune retry budgets per-call.
export async function authedFetch(
	input: RequestInfo | URL,
	init?: RequestInit,
): Promise<Response> {
	return authedFetchInner(input, init ?? {}, { ...INITIAL_STATE }, undefined);
}

// `preResolvedToken` allows the caller to pass in a token already
// fetched via `getAuthToken()` so the retry path does not call the
// provider twice for the same retry attempt. When undefined, we fetch.
async function authedFetchInner(
	input: RequestInfo | URL,
	init: RequestInit,
	state: RetryState,
	preResolvedToken: string | null | undefined,
): Promise<Response> {
	const headers = new Headers(init.headers ?? {});

	// D5 correction: route through getAuthToken() to inherit PR A's
	// error-swallow fence. Null returns are normal (no provider, or
	// provider threw and got caught).
	const token =
		preResolvedToken !== undefined ? preResolvedToken : await getAuthToken();
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
	if (state.authAttempts + state.syncAttempts >= TOTAL_CAP) return res;

	// 401 → force refresh + retry, subject to the auth cap.
	if (res.status === 401 && state.authAttempts < AUTH_CAP) {
		// D3 correction: check that the token provider CAN still yield a
		// token. If the retry would produce null (provider unset, or
		// provider threw and got caught), the retry cannot change the
		// outcome. Return the 401 to the caller instead of entering a
		// no-header retry loop.
		const retryToken = await getAuthToken();
		if (retryToken === null) {
			dispatchAuthExhausted(String(input), "no_token_on_retry");
			return res;
		}
		// Thread the freshly-resolved token into the recursive call so the
		// inner invocation does not ask the provider a second time.
		return authedFetchInner(
			input,
			init,
			{ ...state, authAttempts: state.authAttempts + 1 },
			retryToken,
		);
	}

	// O2 correction: surface final 401 after refresh as well. If we
	// reach this branch with authAttempts >= AUTH_CAP and status 401,
	// the refresh did not help.
	if (res.status === 401 && state.authAttempts >= AUTH_CAP) {
		dispatchAuthExhausted(String(input), "refresh_failed");
	}

	// A-10: 202 Accepted + Retry-After during the first-request race
	// against Phase 3's group-sync fire-and-forget spawn.
	if (res.status === 202 && state.syncAttempts < SYNC_CAP) {
		const waitMs = parseRetryAfterMs(res.headers.get("retry-after"));
		await new Promise((resolve) => setTimeout(resolve, waitMs));
		// Sync retry uses a fresh provider read. The token may have rotated
		// during the wait.
		return authedFetchInner(
			input,
			init,
			{ ...state, syncAttempts: state.syncAttempts + 1 },
			undefined,
		);
	}

	return res;
}

// SSR / Node-test guard: authedFetch is SPA-only today, but if it is
// ever imported from a Tauri backend or a Node integration harness the
// `window` global is absent. Silent no-op in that case. Revisit the
// observability surface if authedFetch starts being consumed server-side.
function dispatchAuthExhausted(
	url: string,
	reason: AuthExhaustedReason,
): void {
	if (typeof window === "undefined") return;
	const detail: AuthExhaustedDetail = { url, reason };
	window.dispatchEvent(
		new CustomEvent<AuthExhaustedDetail>("spacebot:auth-exhausted", {
			detail,
		}),
	);
}

// D9 correction: the daemon ALWAYS emits `Retry-After: 2` when returning
// 202 from the first-request race handler (see
// `src/auth/middleware.rs:309-317`). A 202 without `Retry-After` is a
// server contract violation; default to 1000ms so the client stays
// responsive while the daemon is fixed. Previously this was 2000ms with
// no documented rationale. Clamped to 10s to prevent a malicious proxy
// from locking the SPA in a long sleep.
//
// Exported for direct unit testing of the NaN / negative / clamp branches.
export function parseRetryAfterMs(header: string | null): number {
	if (!header) return 1000;
	const parsed = Number.parseInt(header, 10);
	if (Number.isNaN(parsed) || parsed < 0) return 1000;
	return Math.min(parsed * 1000, 10_000);
}
