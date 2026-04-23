// AgentMemories visibility-chip + filter-wiring test.
// Covers the T7.7 rollout: a chip renders inside each list-view card,
// and the VisibilityFilter's selection is piped into the React Query key
// so filter changes produce a distinct fetch (even if the backend param
// is client-side-only for now).
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

// MemoryGraph depends on sigma, which touches WebGL2RenderingContext at
// module load and that global is not in jsdom. The graph view is out of
// scope for this test (T7.7 tests the list view only per D43) so stub
// the module before AgentMemories is imported.
vi.mock("@/components/MemoryGraph", () => ({
	MemoryGraph: () => null,
}));

import { AgentMemories } from "../AgentMemories";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function memoryPayload() {
	return {
		memories: [
			{
				id: "m1",
				content: "first memory content",
				memory_type: "fact",
				importance: 0.5,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				last_accessed_at: "2026-04-23T00:00:00Z",
				access_count: 0,
				source: null,
				channel_id: null,
				forgotten: false,
				visibility: "personal",
				team_name: null,
			},
			{
				id: "m2",
				content: "second memory content",
				memory_type: "fact",
				importance: 0.5,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				last_accessed_at: "2026-04-23T00:00:00Z",
				access_count: 0,
				source: null,
				channel_id: null,
				forgotten: false,
				visibility: "team",
				team_name: "Platform",
			},
		],
		total: 2,
	};
}

describe("AgentMemories with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// AgentMemories calls two endpoints: /api/agents/memories (list)
		// and /api/teams (ShareResourceModal team selector). Route by URL
		// so each query gets a valid response shape; returning the memory
		// payload for the /teams call would make TS happy but crash the
		// hook's JSON.parse consumer.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				return new Response(JSON.stringify(memoryPayload()), {
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

	it("renders a visibility chip for each memory in list view", async () => {
		const { container } = renderWithProviders(
			<AgentMemories agentId="agent-1" />,
		);
		await waitFor(() =>
			expect(screen.getByText("first memory content")).toBeInTheDocument(),
		);
		// Narrow the Personal match to chips (class `visibility-chip`) so
		// the VisibilityFilter's Personal radio label doesn't collide with
		// the chip label in the assertion.
		const chips = container.querySelectorAll(".visibility-chip");
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Personal");
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<AgentMemories agentId="agent-1" />);
		await waitFor(() =>
			expect(screen.getByText("first memory content")).toBeInTheDocument(),
		);
		const callsBefore = vi.mocked(globalThis.fetch).mock.calls.length;

		// VisibilityFilter ships as a radiogroup; picking "team" changes
		// the queryKey tuple and forces a refetch through React Query.
		const teamRadio = screen.getByRole("radio", { name: /Team$/ });
		fireEvent.click(teamRadio);

		await waitFor(() => {
			expect(
				vi.mocked(globalThis.fetch).mock.calls.length,
			).toBeGreaterThan(callsBefore);
		});
	});
});
