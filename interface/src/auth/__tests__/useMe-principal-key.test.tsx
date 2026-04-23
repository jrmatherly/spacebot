// useMyPrincipalKey test. Co-located with useMe per Phase 7 plan
// D27 correction (see `.scratchpad/plans/entraid-auth/
// phase-7-ui-surfaces.md`). Follows the same harness as useMe.test.tsx
// (QueryClientProvider + fetch spy) rather than the mock-of-useMe
// pattern, because the plan's mock pattern bypasses module-scope
// lookups and leaves the real useQuery call un-mocked, which triggers
// "No QueryClient set".
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
		// No fetch spy plus no await: the hook runs in its initial
		// "loading" state where useMe().data is still undefined. A-18
		// pattern: the helper returns null so callers can render a neutral
		// placeholder instead of throwing or showing "undefined".
		vi.spyOn(globalThis, "fetch").mockImplementation(
			() => new Promise(() => {}),
		);

		const { result } = renderHook(() => useMyPrincipalKey(), { wrapper });
		expect(result.current).toBeNull();
	});

	it("returns the raw empty string when /api/me sends principal_key: ''", async () => {
		// S4 (pr-test-analyzer + silent-failure-hunter): pin the current
		// contract that an empty principal_key from the server passes
		// through as "" (not null) so callers using `if (!key)` as a
		// signed-in gate still fail closed (empty string is falsy). If
		// this ever changes to null-coercion, this test forces an explicit
		// documented decision rather than a silent behavior flip.
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response(
				JSON.stringify({
					principal_key: "",
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
		await waitFor(() => expect(result.current).toBe(""));
		// Document the falsy-gate invariant that callers rely on.
		expect(Boolean(result.current)).toBe(false);
	});
});
