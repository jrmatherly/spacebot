// Vitest for `api.setResourceVisibility`. PR #111 review I2 remediation.
//
// Exercises the three branches of the SetResourceVisibilityArgs
// discriminated union + the non-ok throw path. The helper translates
// the camelCase React-side arg shape (mirrors PR 1's ShareSubmitArgs)
// to the snake_case wire payload expected by PUT /api/resources/.../
// visibility. A regression that breaks the translation would ship
// through CI and surface as confusing 400s in PR 2's consumers.
// These tests pin the wire contract so the regression is caught at
// unit level.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { api, setServerUrl, setAuthTokenProvider } from "../client";

describe("api.setResourceVisibility", () => {
	beforeEach(() => {
		// authedFetch calls getApiBase() which composes {_serverUrl}/api
		// when a server URL is set. Pin to a stable literal so URL
		// assertions do not depend on BASE_PATH (which defaults to "").
		setServerUrl("http://test.invalid");
		// No auth token needed for the mocked fetch. authedFetch skips
		// the Authorization header when the provider is null.
		setAuthTokenProvider(null);
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it("translates team branch to snake_case with shared_with_team_id", async () => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("", { status: 200 }));

		await api.setResourceVisibility("memory", "m-1", {
			visibility: "team",
			sharedWithTeamId: "team-alpha",
		});

		expect(fetchSpy).toHaveBeenCalledTimes(1);
		const call = fetchSpy.mock.calls[0];
		const url = call[0] as string;
		const init = call[1] as RequestInit;
		expect(url).toBe(
			"http://test.invalid/api/resources/memory/m-1/visibility",
		);
		expect(init.method).toBe("PUT");
		const body = JSON.parse(init.body as string);
		expect(body).toEqual({
			visibility: "team",
			shared_with_team_id: "team-alpha",
		});
	});

	it("translates personal branch with explicit shared_with_team_id: null", async () => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("", { status: 200 }));

		await api.setResourceVisibility("task", "t-1", {
			visibility: "personal",
		});

		const body = JSON.parse(
			(fetchSpy.mock.calls[0][1] as RequestInit).body as string,
		);
		expect(body).toEqual({
			visibility: "personal",
			shared_with_team_id: null,
		});
	});

	it("translates org branch with explicit shared_with_team_id: null", async () => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("", { status: 200 }));

		await api.setResourceVisibility("cron", "c-1", {
			visibility: "org",
		});

		const body = JSON.parse(
			(fetchSpy.mock.calls[0][1] as RequestInit).body as string,
		);
		expect(body).toEqual({
			visibility: "org",
			shared_with_team_id: null,
		});
	});

	it("throws `API error <status>: <path>` on !ok response", async () => {
		vi.spyOn(globalThis, "fetch").mockResolvedValue(
			new Response("", { status: 403 }),
		);

		await expect(
			api.setResourceVisibility("memory", "m-1", { visibility: "org" }),
		).rejects.toThrow("API error 403: /resources/memory/m-1/visibility");
	});

	it("url-encodes resource_type + resource_id path segments", async () => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("", { status: 200 }));

		await api.setResourceVisibility("mem ory", "m 1/slash", {
			visibility: "personal",
		});

		const url = fetchSpy.mock.calls[0][0] as string;
		expect(url).toBe(
			"http://test.invalid/api/resources/mem%20ory/m%201%2Fslash/visibility",
		);
	});
});
