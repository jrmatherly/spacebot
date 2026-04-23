// Wiki visibility-chip + filter-wiring test (Phase 7 PR 3 T7.9).
// Mirrors the AgentMemories pattern. Wiki renders chips inside the
// sidebar page list (not per-row in a virtualized table) so each page
// entry carries its own chip + Share button directly.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

import { Wiki } from "../Wiki";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function wikiListPayload() {
	return {
		pages: [
			{
				id: "w1",
				slug: "runbook-ops",
				title: "Ops Runbook",
				page_type: "reference",
				version: 1,
				updated_at: "2026-04-23T00:00:00Z",
				updated_by: "alice",
				visibility: "personal",
				team_name: null,
			},
			{
				id: "w2",
				slug: "platform-architecture",
				title: "Platform Architecture",
				page_type: "concept",
				version: 3,
				updated_at: "2026-04-23T00:00:00Z",
				updated_by: "alice",
				visibility: "team",
				team_name: "Platform",
			},
		],
		total: 2,
	};
}

describe("Wiki with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// Per-URL mock (D61): /api/teams for the Share modal, /api/wiki
		// (list or search) for page data. The order matters: `/wiki`
		// matches both `/api/wiki` (list) AND `/api/wiki/search`, so the
		// list payload is fine for both since both endpoints share the
		// WikiListResponse shape.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				return new Response(JSON.stringify(wikiListPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			},
		);
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("renders a visibility chip per wiki page in the sidebar", async () => {
		const { container } = renderWithProviders(<Wiki />);
		await waitFor(() =>
			expect(screen.getByText("Ops Runbook")).toBeInTheDocument(),
		);
		const chips = container.querySelectorAll(".visibility-chip");
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Personal");
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<Wiki />);
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

	it("renders no chip for a page with null visibility (no-auto-broadening)", async () => {
		// D54/D68 pin: an unowned wiki page must show no chip rather than
		// defaulting to a Personal label. Pairs with the backend invariant
		// in `resources.rs::enrich_missing_ownership_row_returns_none_fields_not_personal_default`.
		vi.mocked(globalThis.fetch).mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(JSON.stringify([]), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				return new Response(
					JSON.stringify({
						pages: [
							{
								id: "w-orphan",
								slug: "orphan-page",
								title: "Orphan Page",
								page_type: "reference",
								version: 1,
								updated_at: "2026-04-23T00:00:00Z",
								updated_by: "alice",
								visibility: null,
								team_name: null,
							},
						],
						total: 1,
					}),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			},
		);
		const { container } = renderWithProviders(<Wiki />);
		await waitFor(() =>
			expect(screen.getByText("Orphan Page")).toBeInTheDocument(),
		);
		expect(container.querySelectorAll(".visibility-chip")).toHaveLength(0);
	});
});
