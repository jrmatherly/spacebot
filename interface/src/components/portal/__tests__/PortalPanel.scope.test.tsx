// PortalPanel visibility-chip + filter-wiring test.
// Portal renders conversations inside a popover over the History
// button in PortalHeader, so each test opens the popover before
// asserting chip presence. PortalPanel fetches multiple endpoints
// (portal-conversations, conversation-defaults, agents, projects,
// teams on modal open) and the per-URL mock routes each of them.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../../test/renderWithProviders";

// `@/routes/AgentWorkers` pulls in `@/lib/providerIcons`, whose
// `@lobehub/icons` barrel imports `@lobehub/ui` which fails to resolve
// `@base-ui/react/merge-props` under jsdom. Stub the tree at the
// `AgentWorkers` boundary: `WorkersPanel` only reaches it for a type
// and a nested detail view that never mounts under this test's
// zero-live-workers mock. Mirrors the `@spacedrive/ai` stub pattern
// established by the AgentTasks scope test.
vi.mock("@/routes/AgentWorkers", () => ({
	WorkerDetail: () => null,
}));

// `@spacedrive/ai` barrel re-exports `InlineWorkerCard` which pulls
// `react-loader-spinner` → `framer-motion` and crashes jsdom at
// module-load with "be.div is not a function". Narrow stub
// that exposes only the symbols PortalComposer / PortalTimeline /
// PortalWorkerCard consume.
vi.mock("@spacedrive/ai", () => ({
	ChatComposer: () => null,
	InlineBranchCard: () => null,
	MessageBubble: () => null,
	InlineWorkerCard: () => null,
}));

import { PortalPanel } from "../PortalPanel";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function portalListPayload() {
	return {
		conversations: [
			{
				id: "portal:chat:a-1:aaa",
				agent_id: "a-1",
				title: "Personal chat",
				title_source: "user",
				archived: false,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				last_message_at: null,
				last_message_preview: null,
				last_message_role: null,
				message_count: 0,
				settings: null,
				visibility: "personal",
				team_name: null,
			},
			{
				id: "portal:chat:a-1:bbb",
				agent_id: "a-1",
				title: "Team chat",
				title_source: "user",
				archived: false,
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				last_message_at: null,
				last_message_preview: null,
				last_message_role: null,
				message_count: 0,
				settings: null,
				visibility: "team",
				team_name: "Platform",
			},
		],
	};
}

function agentsPayload() {
	return { agents: [{ id: "a-1", display_name: "Agent 1" }] };
}

function defaultsPayload() {
	return {
		model: "claude-sonnet-4-5",
		memory: "ambient",
		delegation: "standard",
		worker_context: { history: "summary", memory: "ambient" },
		available_models: [
			{
				id: "claude-sonnet-4-5",
				name: "Claude Sonnet 4.5",
				provider: "anthropic",
				context_window: 200000,
				supports_tools: true,
				supports_thinking: false,
			},
		],
		memory_modes: ["full", "ambient", "off"],
		delegation_modes: ["standard", "direct"],
		worker_history_modes: ["none", "summary", "recent", "full"],
		worker_memory_modes: ["none", "ambient", "tools", "full"],
	};
}

function projectsPayload() {
	return { projects: [] };
}

function setupMocks(
	conversationsBody: unknown | Response = portalListPayload(),
) {
	vi.spyOn(globalThis, "fetch").mockImplementation(
		async (input: RequestInfo | URL) => {
			const url = typeof input === "string" ? input : String(input);
			if (url.includes("/teams")) {
				return new Response(
					JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			}
			if (url.includes("/conversation-defaults")) {
				return new Response(JSON.stringify(defaultsPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/projects")) {
				return new Response(JSON.stringify(projectsPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/portal/conversations")) {
				if (conversationsBody instanceof Response) return conversationsBody;
				return new Response(JSON.stringify(conversationsBody), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			if (url.includes("/agents")) {
				return new Response(JSON.stringify(agentsPayload()), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			// Fail loudly on unmatched URLs rather than returning an
			// empty 200 payload: a typo in the substring routing above
			// would otherwise silently satisfy the wrong endpoint and
			// yield a false pass.
			throw new Error(`unmocked fetch in PortalPanel scope test: ${url}`);
		},
	);
}

describe("PortalPanel with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		setupMocks();
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("renders a visibility chip per conversation in the history popover", async () => {
		renderWithProviders(<PortalPanel agentId="a-1" />);
		// Wait for the conversation list to load.
		await waitFor(() =>
			expect(vi.mocked(globalThis.fetch).mock.calls.length).toBeGreaterThan(0),
		);
		// Open the history popover (button title is "History"). Radix
		// Popover renders its content in a portal attached to
		// document.body, so query chips from there rather than from the
		// render container.
		const historyButton = await screen.findByTitle("History");
		fireEvent.click(historyButton);
		await waitFor(() =>
			expect(screen.getByText("Team chat")).toBeInTheDocument(),
		);
		const chips = document.body.querySelectorAll('[data-testid="visibility-chip"]');
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Personal");
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<PortalPanel agentId="a-1" />);
		const historyButton = await screen.findByTitle("History");
		fireEvent.click(historyButton);
		await waitFor(() =>
			expect(screen.getByText("Team chat")).toBeInTheDocument(),
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

	it("renders no chip for a conversation with null visibility (no-auto-broadening)", async () => {
		setupMocks({
			conversations: [
				{
					id: "portal:chat:a-1:orphan",
					agent_id: "a-1",
					title: "Orphan chat",
					title_source: "user",
					archived: false,
					created_at: "2026-04-23T00:00:00Z",
					updated_at: "2026-04-23T00:00:00Z",
					last_message_at: null,
					last_message_preview: null,
					last_message_role: null,
					message_count: 0,
					settings: null,
					visibility: null,
					team_name: null,
				},
			],
		});
		renderWithProviders(<PortalPanel agentId="a-1" />);
		const historyButton = await screen.findByTitle("History");
		fireEvent.click(historyButton);
		await waitFor(() =>
			expect(screen.getByText("Orphan chat")).toBeInTheDocument(),
		);
		expect(document.body.querySelectorAll('[data-testid="visibility-chip"]')).toHaveLength(0);
	});

	it("renders no conversations when the list endpoint returns 500", async () => {
		setupMocks(
			new Response(JSON.stringify({ error: "database unavailable" }), {
				status: 500,
				headers: { "content-type": "application/json" },
			}),
		);
		renderWithProviders(<PortalPanel agentId="a-1" />);
		const historyButton = await screen.findByTitle("History");
		fireEvent.click(historyButton);
		await waitFor(() =>
			expect(screen.getByText(/No conversations yet/i)).toBeInTheDocument(),
		);
	});
});
