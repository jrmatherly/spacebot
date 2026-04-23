// useTeams test. Mirrors useMe-principal-key.test.tsx harness (fetch spy
// + QueryClientProvider) so the hook exercises the real useQuery path
// through authedFetch. Covers: happy path, empty array, and surfacing
// authedFetch rejections to the caller.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import React from "react";
import { setAuthTokenProvider } from "@spacebot/api-client/client";
import { useTeams } from "../useMe";

function wrapper({ children }: { children: ReactNode }) {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return React.createElement(QueryClientProvider, { client }, children);
}

describe("useTeams", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		vi.restoreAllMocks();
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("returns the list of active teams from /api/teams", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response(
				JSON.stringify([
					{ id: "team-1", display_name: "Platform" },
					{ id: "team-2", display_name: "Research" },
				]),
				{ status: 200, headers: { "content-type": "application/json" } },
			),
		);

		const { result } = renderHook(() => useTeams(), { wrapper });
		await waitFor(() => expect(result.current.data).toBeDefined());
		expect(result.current.data).toEqual([
			{ id: "team-1", display_name: "Platform" },
			{ id: "team-2", display_name: "Research" },
		]);
	});

	it("returns an empty array when no active teams exist", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response(JSON.stringify([]), {
				status: 200,
				headers: { "content-type": "application/json" },
			}),
		);
		const { result } = renderHook(() => useTeams(), { wrapper });
		await waitFor(() => expect(result.current.data).toBeDefined());
		expect(result.current.data).toEqual([]);
	});

	it("surfaces a non-OK response as a query error", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response("", { status: 500 }),
		);
		const { result } = renderHook(() => useTeams(), { wrapper });
		await waitFor(() => expect(result.current.isError).toBe(true));
		expect(result.current.error?.message).toContain("500");
		expect(result.current.error?.message).toContain("/teams");
	});

	it("surfaces a 401 exhaustion with the documented prefix", async () => {
		// Locks the `API error 401: /teams` contract that downstream
		// listeners (sign-out flow, auth-exhausted toast) narrow on.
		// authedFetch's one-shot retry is itself 401-returning in this
		// mock, so the refresh-exhausted path surfaces straight through.
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response("", { status: 401 }),
		);
		const { result } = renderHook(() => useTeams(), { wrapper });
		await waitFor(() => expect(result.current.isError).toBe(true));
		expect(result.current.error?.message.startsWith("API error 401")).toBe(
			true,
		);
		expect(result.current.error?.message).toContain("/teams");
	});
});
