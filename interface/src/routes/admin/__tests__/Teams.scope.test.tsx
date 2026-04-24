// Admin Teams page scope + role-gate tests. Four cases:
// - non-admin role renders the access-denied panel (no list fetch)
// - admin role renders the team list
// - selecting a team fires /api/admin/teams/:id/members
// - 500 on the list endpoint renders the error panel
//
// Uses fail-loud setupMocks default (D109) so any unmocked URL throws.
// `useRole("SpacebotAdmin")` reads from /api/me, so the mock drives the
// gate via that payload rather than a vi.mock of the auth module.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../../test/renderWithProviders";

import { AdminTeams } from "../Teams";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function mePayload(roles: string[]) {
	return {
		principal_key: "t1:oid-alice",
		tid: "t1",
		oid: "oid-alice",
		display_name: "Alice",
		display_email: "alice@example.com",
		photo_url: null,
		roles,
		groups: [],
		photo_initials: "A",
	};
}

function teamsPayload() {
	return {
		teams: [
			{
				id: "team-1",
				display_name: "Platform",
				status: "active",
				member_count: 2,
				last_sync_at: "2026-04-24T00:00:00Z",
			},
			{
				id: "team-2",
				display_name: "Data",
				status: "active",
				member_count: 0,
				last_sync_at: null,
			},
		],
	};
}

function membersPayload() {
	return {
		members: [
			{
				principal_key: "t1:oid-alice",
				display_name: "Alice",
				display_email: "alice@example.com",
				observed_at: "2026-04-24T00:00:00Z",
				source: "token_claim",
			},
		],
	};
}

function setupMocks(opts: { roles: string[]; teamsStatus?: number } = { roles: [] }) {
	vi.spyOn(globalThis, "fetch").mockImplementation(
		async (input: RequestInfo | URL) => {
			const url = typeof input === "string" ? input : String(input);
			if (url.includes("/api/me")) {
				return new Response(JSON.stringify(mePayload(opts.roles)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.match(/\/api\/admin\/teams\/[^/]+\/members/)) {
				return new Response(JSON.stringify(membersPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/admin/teams")) {
				const status = opts.teamsStatus ?? 200;
				const body =
					status === 200
						? JSON.stringify(teamsPayload())
						: JSON.stringify({ error: "database unavailable" });
				return new Response(body, {
					status,
					headers: { "content-type": "application/json" },
				});
			}
			throw new Error(`unmocked fetch in AdminTeams scope test: ${url}`);
		},
	);
}

describe("AdminTeams role gate + list/member rendering", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("renders an access-denied panel for a non-admin caller", async () => {
		setupMocks({ roles: ["SpacebotUser"] });
		renderWithProviders(<AdminTeams />);
		await waitFor(() =>
			expect(
				screen.getByText(/requires the SpacebotAdmin role/i),
			).toBeInTheDocument(),
		);
		// The list must NOT have been fetched: no `/api/admin/teams` call
		// should appear among the network requests because the guard
		// short-circuits before the useQuery fires.
		const adminCalls = vi
			.mocked(globalThis.fetch)
			.mock.calls.filter(([input]) => {
				const url = typeof input === "string" ? input : String(input);
				return url.includes("/api/admin/teams");
			});
		expect(adminCalls).toHaveLength(0);
	});

	it("renders the team list for an admin caller", async () => {
		setupMocks({ roles: ["SpacebotAdmin"] });
		renderWithProviders(<AdminTeams />);
		await waitFor(() =>
			expect(screen.getByText("Platform")).toBeInTheDocument(),
		);
		expect(screen.getByText("Data")).toBeInTheDocument();
		// "0 members" displays without a pluralized trailing "s"
		expect(screen.getByText(/0 members/)).toBeInTheDocument();
	});

	it("selecting a team triggers a members fetch", async () => {
		setupMocks({ roles: ["SpacebotAdmin"] });
		renderWithProviders(<AdminTeams />);
		await waitFor(() =>
			expect(screen.getByText("Platform")).toBeInTheDocument(),
		);
		const callsBefore = vi.mocked(globalThis.fetch).mock.calls.length;
		fireEvent.click(screen.getByText("Platform"));
		await waitFor(() => {
			const callsAfter = vi.mocked(globalThis.fetch).mock.calls;
			const memberCalls = callsAfter.filter(([input]) => {
				const url = typeof input === "string" ? input : String(input);
				return url.match(/\/api\/admin\/teams\/[^/]+\/members/);
			});
			expect(memberCalls.length).toBeGreaterThan(0);
			expect(callsAfter.length).toBeGreaterThan(callsBefore);
		});
		// Member row rendered.
		await waitFor(() =>
			expect(screen.getByText("Alice")).toBeInTheDocument(),
		);
	});

	it("renders the error panel when /api/admin/teams returns 500", async () => {
		setupMocks({ roles: ["SpacebotAdmin"], teamsStatus: 500 });
		renderWithProviders(<AdminTeams />);
		await waitFor(() =>
			expect(screen.getByText(/Failed to load teams/i)).toBeInTheDocument(),
		);
	});
});
