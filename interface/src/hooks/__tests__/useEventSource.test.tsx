// Tests the transport swap from native EventSource to
// @microsoft/fetch-event-source. Asserts that the hook's public
// contract (`handlers: Record<string, EventHandler>`) survives the
// refactor, and that the wrapper's `fetch` option is the authedFetch
// identity (not a look-alike function). Covers the onerror backoff
// sequence, the `lagged` → onReconnect resync path, and the unmount
// abort cleanup.
//
// The library call receives the event-type name in `ev.event`; we
// route through `handlers[ev.event]?.(JSON.parse(ev.data))`.

import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { authedFetch } from "@spacebot/api-client/authedFetch";
import { useEventSource } from "../useEventSource";

// Stub @microsoft/fetch-event-source. Default mock: simple open →
// single tick message. Individual tests override via
// `vi.mocked(fetchEventSource).mockImplementationOnce(...)` to drive
// onerror / onclose / lagged / unmount paths.
vi.mock("@microsoft/fetch-event-source", () => ({
	fetchEventSource: vi.fn(async (_url, options) => {
		await options.onopen?.(new Response(null, { status: 200 }));
		await options.onmessage?.({
			event: "tick",
			data: JSON.stringify({ count: 1 }),
			id: "",
			retry: undefined,
		});
	}),
}));

describe("useEventSource", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("routes inbound events to the correct handler by event type", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		vi.mocked(fetchEventSource).mockImplementationOnce(
			async (_url, options) => {
				await options?.onopen?.(new Response(null, { status: 200 }));
				await options?.onmessage?.({
					event: "tick",
					data: JSON.stringify({ count: 1 }),
					id: "",
					retry: undefined,
				});
			},
		);
		const tickCalls: Array<unknown> = [];
		const otherCalls: Array<unknown> = [];
		renderHook(() =>
			useEventSource("http://api/events", {
				handlers: {
					tick: (data) => tickCalls.push(data),
					other: (data) => otherCalls.push(data),
				},
			}),
		);
		await waitFor(() => expect(tickCalls.length).toBeGreaterThan(0));
		expect(tickCalls[0]).toEqual({ count: 1 });
		expect(otherCalls.length).toBe(0);
	});

	it("passes authedFetch as the fetch option (identity check)", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		renderHook(() =>
			useEventSource("http://api/events", { handlers: {} }),
		);
		await waitFor(() => {
			const calls = (
				fetchEventSource as unknown as { mock: { calls: unknown[][] } }
			).mock.calls;
			const call = calls[calls.length - 1];
			expect(call).toBeTruthy();
			// Identity check, not `toBeTypeOf("function")`, so a refactor
			// that swaps in the raw browser fetch fails this test.
			expect(call[1]).toHaveProperty("fetch", authedFetch);
		});
	});

	// Exponential backoff: onerror returns the current delay, then
	// advances. Sequence: 1000 → 2000 → 4000 → 8000 → 16000 → 30000 →
	// 30000 (clamped). A regression that throws from onerror (aborts
	// reconnect) or forgets Math.min(..., MAX_RETRY_MS) is invisible in
	// the UI until production SSE breaks.
	it("returns the current backoff delay from onerror and advances per call", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		const delays: number[] = [];
		vi.mocked(fetchEventSource).mockImplementationOnce(
			async (_url, options) => {
				// Don't call onopen — we're testing the initial-connection
				// error path. Call onerror repeatedly and capture the
				// returned delays.
				if (!options?.onerror) return;
				for (let i = 0; i < 7; i++) {
					const delay = options.onerror(new Error(`attempt-${i}`));
					delays.push(delay as number);
				}
			},
		);
		renderHook(() =>
			useEventSource("http://api/events", { handlers: {} }),
		);
		await waitFor(() => expect(delays).toHaveLength(7));
		// INITIAL_RETRY_MS=1000, BACKOFF_MULTIPLIER=2, MAX_RETRY_MS=30_000.
		// Sequence: 1000, 2000, 4000, 8000, 16000, 30000 (clamped from
		// 32000), 30000 (stays at cap).
		expect(delays).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000]);
	});

	// The `lagged` event is how the daemon tells the SPA "you missed
	// messages; full resync". Handler is NOT invoked directly from
	// handlers map; instead it triggers onReconnect to flush caches. A
	// regression that routes `lagged` through handlers["lagged"] (absent
	// → no-op) would silently lose messages.
	it("triggers onReconnect on lagged event, not the handlers map", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		vi.mocked(fetchEventSource).mockImplementationOnce(
			async (_url, options) => {
				await options?.onopen?.(new Response(null, { status: 200 }));
				await options?.onmessage?.({
					event: "lagged",
					data: JSON.stringify({ skipped: 42 }),
					id: "",
					retry: undefined,
				});
			},
		);
		const handlerCalls: Array<unknown> = [];
		const onReconnectCalls: Array<void> = [];
		const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

		renderHook(() =>
			useEventSource("http://api/events", {
				handlers: {
					lagged: () => handlerCalls.push("should-not-fire"),
				},
				onReconnect: () => onReconnectCalls.push(undefined),
			}),
		);

		await waitFor(() => expect(onReconnectCalls.length).toBe(1));
		// handlers["lagged"] must NOT be consulted for the lagged event
		// because the hook's lagged branch short-circuits before the
		// handlers-map lookup.
		expect(handlerCalls.length).toBe(0);
		// The skipped count is surfaced to console.warn for operator
		// triage.
		expect(warnSpy).toHaveBeenCalledWith(
			expect.stringContaining("42"),
		);
		warnSpy.mockRestore();
	});

	// Unmount must abort the fetch-event-source signal so the library's
	// retry loop terminates. A regression that forgets the cleanup
	// function return keeps the loop alive past component teardown.
	it("aborts the AbortController on unmount", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		// The library's `signal` field is typed `AbortSignal | null | undefined`
		// even though useEventSource always provides an AbortController;
		// widen to match so strict-mode tsc (CI) accepts the assignment.
		let capturedSignal: AbortSignal | null | undefined;
		vi.mocked(fetchEventSource).mockImplementationOnce(
			async (_url, options) => {
				capturedSignal = options?.signal;
				await options?.onopen?.(new Response(null, { status: 200 }));
			},
		);
		const { unmount } = renderHook(() =>
			useEventSource("http://api/events", { handlers: {} }),
		);
		await waitFor(() => expect(capturedSignal).toBeDefined());
		expect(capturedSignal?.aborted).toBe(false);
		unmount();
		expect(capturedSignal?.aborted).toBe(true);
	});

	// enabled: false short-circuits without calling fetchEventSource at
	// all. Catches a regression where `enabled` is dropped from the
	// options or the condition inverts.
	it("does not call fetchEventSource when enabled is false", async () => {
		const { fetchEventSource } = await import(
			"@microsoft/fetch-event-source"
		);
		renderHook(() =>
			useEventSource("http://api/events", {
				handlers: {},
				enabled: false,
			}),
		);
		// Wait a tick for any effects to run.
		await new Promise((r) => setTimeout(r, 10));
		expect(fetchEventSource).not.toHaveBeenCalled();
	});
});
