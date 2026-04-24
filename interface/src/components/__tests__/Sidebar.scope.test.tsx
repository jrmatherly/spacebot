// Sidebar agent-nav scope-partition tests. Added in PR #115 review
// remediation to pin the "Mine wins over Team wins over Org" invariant
// and the error-surfacing behavior introduced alongside it.
//
// Four cases:
// 1. Mine wins over Team wins over Org (agent classification collapses
//    to the narrowest scope that claims it).
// 2. Empty groups hide their headers.
// 3. A scoped query's isError surfaces a warning indicator AND keeps
//    the group header rendered so the user sees "something went wrong"
//    instead of silently-empty.
// 4. Fail-loud setupMocks default (D109): any unmocked URL throws.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

// `@/components/WorkersPanel` pulls in `@/lib/providerIcons`, whose
// `@lobehub/icons` barrel imports `@lobehub/ui` which fails to resolve
// `@base-ui/react/merge-props` under jsdom. Same tree the PortalPanel
// scope test stubs via `@/routes/AgentWorkers`. Sidebar only renders
// the `WorkersPanelButton`, so a narrow stub that exposes the named
// export is enough to let the module load.
vi.mock("@/components/WorkersPanel", () => ({
	WorkersPanelButton: () => null,
}));

import { Sidebar } from "../Sidebar";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function agentInfo(id: string) {
	return {
		id,
		display_name: id,
		role: null,
		gradient_start: null,
		gradient_end: null,
		workspace: "/tmp/test",
		context_window: 200_000,
		max_turns: 5,
		max_concurrent_branches: 2,
		max_concurrent_workers: 2,
	};
}

function agentsPayload(ids: string[]) {
	return { agents: ids.map(agentInfo) };
}

function globalSettingsPayload() {
	return {
		company_name: "Test Co",
		vacation: false,
		default_model: null,
		home_display_mode: "calendar",
		home_agent_ids: [],
		worker_idle_threshold_secs: 60,
		worker_idle_log_tail_lines: 50,
		opencode_worker_model: null,
		opencode_worker_api_base: null,
		opencode_worker_api_key_ref: null,
	};
}

type MockOpts = {
	/** Full list of agents (what the unscoped `/api/agents` returns). */
	allAgents: string[];
	/** Subset visible under scope=mine. */
	mineAgents?: string[];
	/** Subset visible under scope=team. */
	teamAgents?: string[];
	/** When true, scope=team returns 500 instead of a payload. */
	teamErrors?: boolean;
};

function setupMocks(opts: MockOpts) {
	const mine = opts.mineAgents ?? [];
	const team = opts.teamAgents ?? [];
	vi.spyOn(globalThis, "fetch").mockImplementation(
		async (input: RequestInfo | URL) => {
			const url = typeof input === "string" ? input : String(input);
			if (url.includes("/api/agents?scope=mine")) {
				return new Response(JSON.stringify(agentsPayload(mine)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/agents?scope=team")) {
				if (opts.teamErrors) {
					return new Response(
						JSON.stringify({ error: "scope query failed" }),
						{
							status: 500,
							headers: { "content-type": "application/json" },
						},
					);
				}
				return new Response(JSON.stringify(agentsPayload(team)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/agents?scope=org")) {
				return new Response(JSON.stringify(agentsPayload(opts.allAgents)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.endsWith("/api/agents") || url.includes("/api/agents?")) {
				return new Response(JSON.stringify(agentsPayload(opts.allAgents)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/providers")) {
				return new Response(JSON.stringify({ has_any: false }), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/agents/projects")) {
				return new Response(JSON.stringify({ projects: [] }), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/api/global-settings")) {
				return new Response(JSON.stringify(globalSettingsPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			throw new Error(`unmocked fetch in Sidebar scope test: ${url}`);
		},
	);
}

describe("Sidebar agent-nav scope partition", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("classifies each agent into exactly one scope group (Mine wins over Team wins over Org)", async () => {
		// agent-a owned by caller — goes to Mine.
		// agent-b shared to caller's team — goes to Team.
		// agent-c neither owned nor team-shared — falls through to Org.
		// agent-d would be both Mine AND Team (owned + team-shared),
		//   but the partition hoists it to Mine (narrower scope wins).
		setupMocks({
			allAgents: ["agent-a", "agent-b", "agent-c", "agent-d"],
			mineAgents: ["agent-a", "agent-d"],
			teamAgents: ["agent-b", "agent-d"],
		});
		renderWithProviders(<Sidebar liveStates={{}} />);
		await waitFor(() =>
			expect(screen.getByTestId("sidebar-agents-group-mine")).toBeInTheDocument(),
		);
		expect(screen.getByTestId("sidebar-agents-group-team")).toBeInTheDocument();
		expect(screen.getByTestId("sidebar-agents-group-org")).toBeInTheDocument();

		// Mine group: agent-a + agent-d (agent-d hoisted here, not Team).
		// Team group: agent-b only (agent-d NOT duplicated).
		// Org group: agent-c only (catch-all).
		// `SortableAgentItem` renders multiple links per agent (root +
		// sub-routes for channels, memories, etc.), so collect the set
		// of distinct agent ids from each group rather than counting
		// raw links.
		const collectDistinctAgentIds = (groupTestId: string): string[] => {
			const links = screen
				.getByTestId(groupTestId)
				.parentElement!.querySelectorAll('a[href*="/agents/"]');
			const ids = new Set<string>();
			for (const a of Array.from(links)) {
				const href = a.getAttribute("href") ?? "";
				const slug = href.split("/agents/")[1]?.split("/")[0];
				if (slug) ids.add(slug);
			}
			return Array.from(ids).sort();
		};
		expect(collectDistinctAgentIds("sidebar-agents-group-mine")).toEqual([
			"agent-a",
			"agent-d",
		]);
		expect(collectDistinctAgentIds("sidebar-agents-group-team")).toEqual([
			"agent-b",
		]);
		expect(collectDistinctAgentIds("sidebar-agents-group-org")).toEqual([
			"agent-c",
		]);
	});

	it("hides empty group headers when all agents classify into a single scope", async () => {
		// All agents are owned by the caller; Team and Org groups are
		// empty and must not render their headers.
		setupMocks({
			allAgents: ["agent-a", "agent-b"],
			mineAgents: ["agent-a", "agent-b"],
			teamAgents: [],
		});
		renderWithProviders(<Sidebar liveStates={{}} />);
		await waitFor(() =>
			expect(screen.getByTestId("sidebar-agents-group-mine")).toBeInTheDocument(),
		);
		expect(screen.queryByTestId("sidebar-agents-group-team")).toBeNull();
		expect(screen.queryByTestId("sidebar-agents-group-org")).toBeNull();
	});

	it("shows a warning indicator AND keeps the header when a scoped query errors", async () => {
		// scope=team returns 500. The Team header must render (with
		// warning icon) so the user sees something is off — rather than
		// silently dropping the group and having agent-b reclassify to
		// Org. The error-testid lets the test assert the indicator is
		// there. Pre-remediation, this case rendered the agent under
		// Org with no warning.
		setupMocks({
			allAgents: ["agent-a", "agent-b"],
			mineAgents: ["agent-a"],
			teamAgents: [],
			teamErrors: true,
		});
		renderWithProviders(<Sidebar liveStates={{}} />);
		await waitFor(() =>
			expect(screen.getByTestId("sidebar-agents-group-mine")).toBeInTheDocument(),
		);
		// Team group header renders (because isError: true), even
		// though the team id list is empty.
		const teamHeader = await screen.findByTestId("sidebar-agents-group-team");
		expect(teamHeader).toBeInTheDocument();
		// Warning indicator is rendered inside the Team header.
		expect(
			screen.getByTestId("sidebar-agents-group-team-error"),
		).toBeInTheDocument();
	});
});
