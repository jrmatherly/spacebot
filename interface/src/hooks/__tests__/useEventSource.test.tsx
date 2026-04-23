// Tests the transport swap from native EventSource to
// @microsoft/fetch-event-source. Asserts that the hook's public
// contract (`handlers: Record<string, EventHandler>`) survives the
// refactor, and that the wrapper's `fetch` option is the authedFetch
// identity (not a look-alike function).
//
// The library call receives the event-type name in `ev.event`; we
// route through `handlers[ev.event]?.(JSON.parse(ev.data))`.

import { describe, it, expect, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { authedFetch } from "@spacebot/api-client/authedFetch";
import { useEventSource } from "../useEventSource";

// Stub @microsoft/fetch-event-source. We're testing OUR wiring,
// not theirs. The library's EventSourceMessage carries `event`,
// `data`, `id`, `retry`; we emit one with `event: "tick"` so the
// handlers-map dispatch can be asserted.
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
	it("routes inbound events to the correct handler by event type", async () => {
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
		const { fetchEventSource } = await import("@microsoft/fetch-event-source");
		renderHook(() =>
			useEventSource("http://api/events", { handlers: {} }),
		);
		await waitFor(() => {
			const calls = (fetchEventSource as unknown as { mock: { calls: unknown[][] } })
				.mock.calls;
			const call = calls[calls.length - 1];
			expect(call).toBeTruthy();
			// Identity check, not `toBeTypeOf("function")`, so a refactor
			// that swaps in the raw browser fetch fails this test.
			expect(call[1]).toHaveProperty("fetch", authedFetch);
		});
	});
});
