// AgentTasks visibility-chip + filter-wiring test.
// Mirrors `AgentMemories.scope.test.tsx`. The chip lives in the detail
// panel (not per-row) because `@spacedrive/ai`'s TaskList has no
// per-row render slot; the filter is toolbar-level and is piped into
// the queryKey so filter changes force a refetch.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

// `@spacedrive/ai` barrel re-exports `InlineWorkerCard`, whose dep
// `react-loader-spinner` touches framer-motion at module-load and
// crashes jsdom with "be.div is not a function". AgentTasks never
// renders InlineWorkerCard, so stubbing the whole module with only
// the symbols AgentTasks actually uses avoids the load-time crash.
// Same pattern as the `MemoryGraph` stub in
// `AgentMemories.scope.test.tsx`: stub heavyweight jsdom-hostile
// deps at module-load.
vi.mock("@spacedrive/ai", async () => {
	const TASK_STATUS_ORDER = [
		"in_progress",
		"ready",
		"pending_approval",
		"backlog",
		"done",
	] as const;
	return {
		TaskList: ({
			tasks,
			onTaskClick,
		}: {
			tasks: Array<{ id: string; title: string }>;
			onTaskClick: (task: { id: string; title: string }) => void;
		}) => (
			<div>
				{tasks.map((t) => (
					<button
						type="button"
						key={t.id}
						onClick={() => onTaskClick(t)}
					>
						{t.title}
					</button>
				))}
			</div>
		),
		TaskDetail: ({ task }: { task: { title: string } }) => (
			<div data-testid="task-detail">{task.title}</div>
		),
		TaskCreateForm: () => null,
		TASK_STATUS_ORDER,
	};
});

import { AgentTasks } from "../AgentTasks";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function tasksPayload() {
	return {
		tasks: [
			{
				id: "t1",
				task_number: 1,
				title: "first task",
				description: null,
				status: "in_progress",
				priority: "medium",
				owner_agent_id: "agent-1",
				assigned_agent_id: "agent-1",
				subtasks: [],
				metadata: {},
				worker_id: null,
				created_by: "alice",
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				completed_at: null,
				visibility: "personal",
				team_name: null,
			},
			{
				id: "t2",
				task_number: 2,
				title: "second task",
				description: null,
				status: "ready",
				priority: "medium",
				owner_agent_id: "agent-1",
				assigned_agent_id: "agent-1",
				subtasks: [],
				metadata: {},
				worker_id: null,
				created_by: "alice",
				created_at: "2026-04-23T00:00:00Z",
				updated_at: "2026-04-23T00:00:00Z",
				completed_at: null,
				visibility: "team",
				team_name: "Platform",
			},
		],
	};
}

describe("AgentTasks with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// Per-URL mock: /api/teams returns the team directory,
		// everything else returns the tasks payload. A single tasks-only
		// mock would crash useTeams' JSON.parse path.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				return new Response(JSON.stringify(tasksPayload()), {
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

	it("renders a visibility chip in the detail panel when a task is selected", async () => {
		const { container } = renderWithProviders(
			<AgentTasks agentId="agent-1" />,
		);
		await waitFor(() =>
			expect(screen.getByText("second task")).toBeInTheDocument(),
		);
		// Click the team-scoped task's row to open the detail panel.
		fireEvent.click(screen.getByText("second task"));
		await waitFor(() => {
			const chips = container.querySelectorAll(".visibility-chip");
			expect(chips.length).toBeGreaterThan(0);
		});
		const chips = container.querySelectorAll(".visibility-chip");
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<AgentTasks agentId="agent-1" />);
		await waitFor(() =>
			expect(screen.getByText("first task")).toBeInTheDocument(),
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

	it("renders no chip for a task with null visibility", async () => {
		// No-auto-broadening policy: unowned tasks show no chip.
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
						tasks: [
							{
								id: "t-orphan",
								task_number: 99,
								title: "orphan task",
								description: null,
								status: "backlog",
								priority: "medium",
								owner_agent_id: "agent-1",
								assigned_agent_id: "agent-1",
								subtasks: [],
								metadata: {},
								worker_id: null,
								created_by: "alice",
								created_at: "2026-04-23T00:00:00Z",
								updated_at: "2026-04-23T00:00:00Z",
								completed_at: null,
								visibility: null,
								team_name: null,
							},
						],
					}),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			},
		);
		const { container } = renderWithProviders(
			<AgentTasks agentId="agent-1" />,
		);
		await waitFor(() =>
			expect(screen.getByText("orphan task")).toBeInTheDocument(),
		);
		fireEvent.click(screen.getByText("orphan task"));
		// The detail panel opens, but no chip should render.
		expect(container.querySelectorAll(".visibility-chip")).toHaveLength(0);
	});

	it("renders the error panel when the list endpoint returns 500", async () => {
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
					JSON.stringify({ error: "database unavailable" }),
					{
						status: 500,
						headers: { "content-type": "application/json" },
					},
				);
			},
		);
		renderWithProviders(<AgentTasks agentId="agent-1" />);
		await waitFor(() =>
			expect(screen.getByText(/Failed to load tasks/i)).toBeInTheDocument(),
		);
	});
});
