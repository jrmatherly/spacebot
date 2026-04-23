// Vitest for useMe / useRole hooks.
//
// Covers the observable behaviors:
//   1. Happy path: /api/me returns 200 with a populated MeResponse
//      shape; useMe's data matches; useRole(role) returns boolean.
//   2. Failure path: /api/me returns 401; useMe's error surfaces with
//      the `API error <status>: <path>` message convention.
//   3. Malformed payload: /api/me returns 200 with non-JSON body;
//      useMe's error message identifies the parse failure.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import React from "react";
import { setAuthTokenProvider } from "@spacebot/api-client/client";
import { useMe, useRole } from "../useMe";

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

describe("useMe", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		vi.restoreAllMocks();
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("returns MeResponse shape from /api/me on 200", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response(
				JSON.stringify({
					principal_key: "tenant-1:alice",
					tid: "tenant-1",
					oid: "alice",
					principal_type: "user",
					display_name: "Alice Example",
					display_email: "alice@example.com",
					display_photo_data_url: null,
					initials: "AE",
					roles: ["SpacebotUser", "SpacebotAdmin"],
					groups: ["engineering"],
					groups_overage: false,
				}),
				{ status: 200, headers: { "content-type": "application/json" } },
			),
		);

		const { result } = renderHook(() => useMe(), { wrapper });
		await waitFor(() => expect(result.current.isSuccess).toBe(true));
		expect(result.current.data?.principal_key).toBe("tenant-1:alice");
		expect(result.current.data?.initials).toBe("AE");
		expect(result.current.data?.roles).toContain("SpacebotAdmin");
	});

	it("surfaces API error <status>: /me on 401", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response("", { status: 401 }),
		);

		const { result } = renderHook(() => useMe(), { wrapper });
		await waitFor(() => expect(result.current.isError).toBe(true));
		expect((result.current.error as Error).message).toBe(
			"API error 401: /me",
		);
	});

	it("distinguishes malformed JSON from network failure", async () => {
		// Daemon returns 200 but the body is non-JSON (e.g., accidentally
		// serving HTML from a misconfigured proxy). Without the explicit
		// JSON-parse catch, React Query would surface the SyntaxError
		// with the same shape as a network rejection.
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response("<html>not json</html>", {
				status: 200,
				headers: { "content-type": "text/html" },
			}),
		);

		const { result } = renderHook(() => useMe(), { wrapper });
		await waitFor(() => expect(result.current.isError).toBe(true));
		// Prefix is fixed; parse-error detail is appended (e.g.,
		// `: Unexpected token '<' ... is not valid JSON`). The prefix
		// is what downstream listeners narrow on.
		expect((result.current.error as Error).message).toMatch(
			/^API error: malformed JSON from \/me/,
		);
	});
});

describe("useRole", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		vi.restoreAllMocks();
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("returns true when /api/me roles contain the queried role", async () => {
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
					roles: ["SpacebotAdmin"],
					groups: [],
					groups_overage: false,
				}),
				{ status: 200, headers: { "content-type": "application/json" } },
			),
		);

		const { result } = renderHook(() => useRole("SpacebotAdmin"), {
			wrapper,
		});
		// useRole returns false before data arrives; re-read after fetch.
		await waitFor(() => expect(result.current).toBe(true));
	});

	it("returns false when /api/me roles do not contain the role", async () => {
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
					roles: ["SpacebotUser"],
					groups: [],
					groups_overage: false,
				}),
				{ status: 200, headers: { "content-type": "application/json" } },
			),
		);

		const { result, rerender } = renderHook(
			() => {
				const me = useMe();
				const role = useRole("SpacebotAdmin");
				return { me, role };
			},
			{ wrapper },
		);
		// Wait for the underlying query to resolve, then assert the role
		// check returns false. Previous fixed-timeout approach was flaky.
		await waitFor(() =>
			expect(result.current.me.isSuccess).toBe(true),
		);
		rerender();
		expect(result.current.role).toBe(false);
	});
});
