// useMyPrincipalKey test — co-located with useMe per D27. Follows the
// same harness as useMe.test.tsx (QueryClientProvider + fetch spy)
// rather than the mock-of-useMe pattern, because the plan's mock
// pattern bypasses module-scope lookups and leaves the real useQuery
// call un-mocked (triggers "No QueryClient set").
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import React from "react";
import { setAuthTokenProvider } from "@spacebot/api-client/client";
import { useMyPrincipalKey } from "../useMe";

function wrapper({ children }: { children: ReactNode }) {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return React.createElement(
		QueryClientProvider,
		{ client },
		children,
	);
}

describe("useMyPrincipalKey", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		vi.restoreAllMocks();
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("returns the principal_key from /api/me", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response(
				JSON.stringify({
					principal_key: "tenant-1:alice",
					tid: "tenant-1",
					oid: "alice",
					principal_type: "user",
					display_name: null,
					display_email: null,
					display_photo_data_url: null,
					initials: "?",
					roles: [],
					groups: [],
					groups_overage: false,
				}),
				{ status: 200, headers: { "content-type": "application/json" } },
			),
		);

		const { result } = renderHook(() => useMyPrincipalKey(), { wrapper });
		await waitFor(() => expect(result.current).toBe("tenant-1:alice"));
	});

	it("returns null before data arrives", () => {
		// No fetch spy + no await — hook runs in its initial "loading" state
		// where useMe().data is still undefined. A-18 pattern: the helper
		// returns null so callers can render a neutral placeholder instead
		// of throwing or showing "undefined".
		vi.spyOn(globalThis, "fetch").mockImplementation(
			() => new Promise(() => {}),
		);

		const { result } = renderHook(() => useMyPrincipalKey(), { wrapper });
		expect(result.current).toBeNull();
	});
});
