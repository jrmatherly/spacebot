// Phase 6 PR B Task 6.B.1 — failing vitest for `authedFetch`, the central
// fetch wrapper introduced by Task 6.B.2 as a sibling module to
// `@spacebot/api-client/client`.
//
// G7 correction (2026-04-23 PR B audit): this file mirrors the import
// pattern established by `authTokenProvider.test.ts` (PR A). The module
// slot `setAuthTokenProvider(null)` in beforeEach/afterEach resets
// closure state between tests; no new mock infrastructure required.
//
// D5 correction: `authedFetch` calls `getAuthToken()` internally, so
// tests drive behavior through `setAuthTokenProvider` rather than
// stubbing `getAuthToken` directly. This exercises the real PR A
// error-swallow fence and keeps the tests coupled to the shipping
// contract, not a mock.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	authedFetch,
	setAuthTokenProvider,
} from "@spacebot/api-client/client";
import type { AuthExhaustedDetail } from "@spacebot/api-client/authedFetch";
import { parseRetryAfterMs } from "@spacebot/api-client/authedFetch";

describe("authedFetch", () => {
	beforeEach(() => {
		setAuthTokenProvider(null);
		vi.restoreAllMocks();
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("attaches Authorization header when provider is set", async () => {
		setAuthTokenProvider(async () => "fake-token-abc");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok", { status: 200 }));
		await authedFetch("http://api/resource");
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		expect(headers.get("authorization")).toBe("Bearer fake-token-abc");
	});

	it("does NOT set Content-Type on FormData requests", async () => {
		setAuthTokenProvider(async () => "t");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok"));
		const fd = new FormData();
		fd.append("file", new Blob(["hi"]), "file.txt");
		await authedFetch("http://api/upload", { method: "POST", body: fd });
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		// If we accidentally set Content-Type, the browser can't set the
		// multipart boundary and the upload corrupts.
		expect(headers.get("content-type")).toBeNull();
	});

	// G1 correction (2026-04-23 PR B audit): portalSendAudio at
	// client.ts sends a raw Blob (not FormData). authedFetch must not
	// force a Content-Type on Blob bodies either; the browser + server
	// negotiate via the Blob's own `type`.
	it("does NOT set Content-Type on Blob body", async () => {
		setAuthTokenProvider(async () => "t");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok"));
		const blob = new Blob(["audio-bytes"], { type: "audio/webm" });
		await authedFetch("http://api/portal/audio", {
			method: "POST",
			body: blob,
		});
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		expect(headers.get("content-type")).toBeNull();
	});

	// Review-B item 4: tightens G1/G2 coverage. The prior FormData/Blob
	// tests only proved Content-Type was absent — they passed even if the
	// `headers.delete("Content-Type")` line were removed, because no
	// caller-supplied Content-Type existed to delete. This test explicitly
	// sets Content-Type alongside FormData, proving the delete branch runs.
	it("deletes caller-supplied Content-Type when body is FormData", async () => {
		setAuthTokenProvider(async () => "t");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok"));
		const fd = new FormData();
		fd.append("file", new Blob(["hi"]), "file.txt");
		// Callers that naively set application/json break multipart uploads
		// because the browser can't insert its own boundary parameter. The
		// wrapper must scrub the caller's header on FormData/Blob bodies.
		await authedFetch("http://api/upload", {
			method: "POST",
			body: fd,
			headers: { "content-type": "application/json" },
		});
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		expect(headers.get("content-type")).toBeNull();
	});

	// G2 correction (2026-04-23 PR B audit): callers may pass a Headers
	// instance instead of a plain object literal. The `new Headers(init.headers ?? {})`
	// idiom in authedFetch accepts both; this test locks the contract.
	it("accepts init.headers as a Headers instance", async () => {
		setAuthTokenProvider(async () => "t");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok"));
		const hdrs = new Headers({ "x-custom": "keep-me" });
		await authedFetch("http://api/foo", { headers: hdrs });
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		expect(headers.get("authorization")).toBe("Bearer t");
		expect(headers.get("x-custom")).toBe("keep-me");
	});

	it("retries once on 401 after forcing a fresh token", async () => {
		let call = 0;
		setAuthTokenProvider(async () => `token-${++call}`);
		const spy = vi.spyOn(globalThis, "fetch").mockImplementation(
			async (_u, init) => {
				const h = new Headers(init?.headers);
				if (h.get("authorization") === "Bearer token-1") {
					return new Response("{}", { status: 401 });
				}
				return new Response("{}", { status: 200 });
			},
		);
		const res = await authedFetch("http://api/foo");
		expect(res.status).toBe(200);
		expect(spy).toHaveBeenCalledTimes(2);
	});

	// D3 correction (2026-04-23 PR B audit): if the provider yields null
	// on the 401-retry attempt (MSAL silent acquisition fails without
	// triggering a redirect), authedFetch must NOT loop forever and must
	// NOT retry with no-Authorization-header. Return the 401 to caller.
	//
	// Also pins the `spacebot:auth-exhausted` observability event with
	// reason=no_token_on_retry, so a refactor that drops or renames the
	// event fails this test.
	it("returns 401 without further retry when provider yields null on retry + dispatches no_token_on_retry", async () => {
		let call = 0;
		setAuthTokenProvider(async () => {
			call++;
			// First call: token issued. Retry: provider unavailable.
			return call === 1 ? "TOKEN" : null;
		});
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("{}", { status: 401 }));
		const dispatchSpy = vi.spyOn(window, "dispatchEvent");
		const res = await authedFetch("http://api/foo");
		expect(res.status).toBe(401);
		// 1 fetch call: authedFetch sees the 401, re-reads the provider,
		// gets null, and returns immediately without a retry. A retry with
		// a null token would set no Authorization header, which the plan
		// explicitly forbids as a silent no-header loop.
		expect(fetchSpy).toHaveBeenCalledTimes(1);
		const authExhaustedCalls = dispatchSpy.mock.calls.filter(
			(c) =>
				c[0] instanceof CustomEvent &&
				c[0].type === "spacebot:auth-exhausted",
		);
		expect(authExhaustedCalls).toHaveLength(1);
		const detail = (authExhaustedCalls[0][0] as CustomEvent<AuthExhaustedDetail>)
			.detail;
		expect(detail.reason).toBe("no_token_on_retry");
		expect(detail.url).toBe("http://api/foo");
	});

	it("passes through when no provider is set (Entra disabled)", async () => {
		setAuthTokenProvider(null);
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("ok"));
		await authedFetch("http://api/foo");
		const init = spy.mock.calls[0][1] as RequestInit | undefined;
		const headers = new Headers(init?.headers);
		expect(headers.get("authorization")).toBeNull();
	});

	// G4 correction (2026-04-23 PR B audit): disabled-mode 401 must NOT
	// trigger retry (can't refresh what doesn't exist). Return the 401 to
	// the caller on the first response.
	it("returns 401 without retry when no provider is set", async () => {
		setAuthTokenProvider(null);
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("{}", { status: 401 }));
		const res = await authedFetch("http://api/foo");
		expect(res.status).toBe(401);
		expect(spy).toHaveBeenCalledTimes(1);
	});

	// Pins the two-counter retry-state design: a 401 refresh must NOT
	// consume the 202 sync budget.
	it("401 → refresh → 202 → 202 → 202 → 200 succeeds (202 budget not consumed by 401)", async () => {
		setAuthTokenProvider(async () => "TOKEN");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValueOnce(new Response("", { status: 401 }))
			.mockResolvedValueOnce(
				new Response("", { status: 202, headers: { "retry-after": "0" } }),
			)
			.mockResolvedValueOnce(
				new Response("", { status: 202, headers: { "retry-after": "0" } }),
			)
			.mockResolvedValueOnce(
				new Response("", { status: 202, headers: { "retry-after": "0" } }),
			)
			.mockResolvedValueOnce(new Response("ok", { status: 200 }));
		const res = await authedFetch("http://api/test");
		expect(res.status).toBe(200);
		expect(spy).toHaveBeenCalledTimes(5);
	});

	it("second consecutive 401 is returned to the caller + dispatches refresh_failed (auth cap = 1)", async () => {
		setAuthTokenProvider(async () => "TOKEN");
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("", { status: 401 }));
		const dispatchSpy = vi.spyOn(window, "dispatchEvent");
		const res = await authedFetch("http://api/test");
		expect(res.status).toBe(401);
		expect(fetchSpy).toHaveBeenCalledTimes(2); // original + one refresh retry
		const authExhaustedCalls = dispatchSpy.mock.calls.filter(
			(c) =>
				c[0] instanceof CustomEvent &&
				c[0].type === "spacebot:auth-exhausted",
		);
		expect(authExhaustedCalls).toHaveLength(1);
		const detail = (authExhaustedCalls[0][0] as CustomEvent<AuthExhaustedDetail>)
			.detail;
		expect(detail.reason).toBe("refresh_failed");
		expect(detail.url).toBe("http://api/test");
	});

	// D8 correction (2026-04-23 PR B audit): the previous "alternating
	// 401/202" test accepted EITHER 401 or 202 as the outcome, which was
	// a weak assertion. Trace the state machine deterministically:
	//   attempt 0 (total=0): 401; authAttempts=0<1 → retry; auth=1, total=1
	//   attempt 1 (total=1): 202; syncAttempts=0<3 → retry; sync=1, total=2
	//   attempt 2 (total=2): 401; authAttempts=1!<1 → return 401 to caller
	// → exactly 3 fetch calls, deterministic 401.
	it("401 → 202 → 401 returns 401 after exactly 3 calls (auth budget exhausted)", async () => {
		setAuthTokenProvider(async () => "TOKEN");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValueOnce(new Response("", { status: 401 }))
			.mockResolvedValueOnce(
				new Response("", { status: 202, headers: { "retry-after": "0" } }),
			)
			.mockResolvedValueOnce(new Response("", { status: 401 }));
		const res = await authedFetch("http://api/test");
		expect(res.status).toBe(401);
		expect(spy).toHaveBeenCalledTimes(3);
	});

	// G3 correction (2026-04-23 PR B audit): the overall `totalAttempts`
	// cap is the safety net for a pathological loop that neither the auth
	// nor the sync budget alone would catch. The sync cap is the tightening
	// constraint in this sequence: 4 fetch calls (original + 3 sync retries)
	// before syncAttempts=3 gates the 4th retry.
	it("202 run: sync-cap fires at attempt 4 and returns the last 202", async () => {
		setAuthTokenProvider(async () => "TOKEN");
		const spy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(
				new Response("", { status: 202, headers: { "retry-after": "0" } }),
			);
		const res = await authedFetch("http://api/test");
		expect(res.status).toBe(202);
		// Original + 3 sync retries = 4 fetch calls. syncAttempts=3 gates
		// the 4th-retry attempt.
		expect(spy.mock.calls.length).toBe(4);
	});
});

// Review-B item 5: direct tests for the Retry-After parser. The four
// branches are all reachable through the authedFetch integration path,
// but driving them via a 202 response with a specific header is slow
// (real setTimeout waits) and indirect. Direct unit tests pin the
// security-adjacent 10s clamp and the malformed-header fallback.
describe("parseRetryAfterMs", () => {
	it("returns 1000ms default when header is null", () => {
		expect(parseRetryAfterMs(null)).toBe(1000);
	});

	it("returns 1000ms fallback when header is non-numeric", () => {
		expect(parseRetryAfterMs("abc")).toBe(1000);
	});

	it("returns 1000ms fallback when header is negative", () => {
		expect(parseRetryAfterMs("-5")).toBe(1000);
	});

	it("multiplies seconds to milliseconds for valid input", () => {
		expect(parseRetryAfterMs("2")).toBe(2000);
		expect(parseRetryAfterMs("5")).toBe(5000);
	});

	it("clamps at 10s to prevent malicious-proxy long-sleep attacks", () => {
		expect(parseRetryAfterMs("11")).toBe(10_000);
		expect(parseRetryAfterMs("999999")).toBe(10_000);
	});

	it("accepts integer boundary value (10s) without clamping further", () => {
		expect(parseRetryAfterMs("10")).toBe(10_000);
	});
});
