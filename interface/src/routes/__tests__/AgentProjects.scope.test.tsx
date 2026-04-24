// AgentProjects visibility-chip + filter-wiring test.
// Four cases mirroring the Wiki / AgentMemories / AgentCron scope-test
// template: chip-per-row, filter triggers new fetch, null-visibility
// renders no chip (no-auto-broadening), 500 renders error panel.
// Uses the `[data-testid="visibility-chip"]` selector (Phase 7 PR 4
// convention) and a fail-loud setupMocks default (D109).
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

import { AgentProjects } from "../AgentProjects";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function projectsPayload() {
	return {
		projects: [
			{
				id: "p1",
				name: "Ops Runbook",
				description: "runbook",
				icon: "📁",
				tags: [],
				root_path: "/tmp/ops-runbook",
				settings: {},
				status: "active",
				sort_order: 0,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				visibility: "personal",
				team_name: null,
			},
			{
				id: "p2",
				name: "Platform Monorepo",
				description: "mono",
				icon: "📁",
				tags: [],
				root_path: "/tmp/platform-monorepo",
				settings: {},
				status: "active",
				sort_order: 1,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				visibility: "team",
				team_name: "Platform",
			},
		],
	};
}

describe("AgentProjects with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// Per-URL mock: /api/teams for the Share modal, /api/agents/projects
		// for list data. Any unmocked URL throws so a future handler that
		// calls a new endpoint surfaces loudly instead of silently
		// returning an empty 200 — D109 fail-loud default.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/api/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				if (url.includes("/api/agents/projects")) {
					return new Response(JSON.stringify(projectsPayload()), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				throw new Error(
					`unmocked fetch in AgentProjects scope test: ${url}`,
				);
			},
		);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("renders a visibility chip per project card", async () => {
		const { container } = renderWithProviders(<AgentProjects />);
		await waitFor(() =>
			expect(screen.getByText("Ops Runbook")).toBeInTheDocument(),
		);
		const chips = container.querySelectorAll(
			'[data-testid="visibility-chip"]',
		);
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Personal");
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<AgentProjects />);
		await waitFor(() =>
			expect(screen.getByText("Ops Runbook")).toBeInTheDocument(),
		);
		const callsBefore = vi.mocked(globalThis.fetch).mock.calls.length;
		const teamRadio = screen.getByRole("radio", { name: /Team$/ });
		fireEvent.click(teamRadio);
		await waitFor(() => {
			expect(
				vi.mocked(globalThis.fetch).mock.calls.length,
			).toBeGreaterThan(callsBefore);
		});
	});

	it("renders no chip for a project with null visibility (no-auto-broadening)", async () => {
		// No-auto-broadening policy: an unowned project must show no chip
		// rather than defaulting to a Personal label. Pairs with the
		// backend invariant in
		// `resources.rs::enrich_missing_ownership_row_returns_none_fields_not_personal_default`.
		vi.mocked(globalThis.fetch).mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/api/teams")) {
					return new Response(JSON.stringify([]), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				if (url.includes("/api/agents/projects")) {
					return new Response(
						JSON.stringify({
							projects: [
								{
									id: "p-orphan",
									name: "Orphan Project",
									description: "",
									icon: "",
									tags: [],
									root_path: "/tmp/orphan",
									settings: {},
									status: "active",
									sort_order: 0,
									created_at: "2026-04-23T00:00:00Z",
									updated_at: "2026-04-23T00:00:00Z",
									visibility: null,
									team_name: null,
								},
							],
						}),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				throw new Error(
					`unmocked fetch in AgentProjects scope test: ${url}`,
				);
			},
		);
		const { container } = renderWithProviders(<AgentProjects />);
		await waitFor(() =>
			expect(screen.getByText("Orphan Project")).toBeInTheDocument(),
		);
		expect(
			container.querySelectorAll('[data-testid="visibility-chip"]'),
		).toHaveLength(0);
	});

	it("renders the error panel when the list endpoint returns 500", async () => {
		vi.mocked(globalThis.fetch).mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/api/teams")) {
					return new Response(JSON.stringify([]), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				if (url.includes("/api/agents/projects")) {
					return new Response(
						JSON.stringify({ error: "database unavailable" }),
						{
							status: 500,
							headers: { "content-type": "application/json" },
						},
					);
				}
				throw new Error(
					`unmocked fetch in AgentProjects scope test: ${url}`,
				);
			},
		);
		renderWithProviders(<AgentProjects />);
		await waitFor(() =>
			expect(screen.getByText(/Failed to load projects/i)).toBeInTheDocument(),
		);
	});
});
