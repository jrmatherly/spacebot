// GlobalTasks visibility-chip + filter-wiring test (Phase 7 PR 3 T7.8, D65).
// GlobalTasks is the second Tasks-list surface and must carry the same
// chip + filter + Share wiring as AgentTasks.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

// See AgentTasks.scope.test.tsx for the rationale. Same stub.
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

import { GlobalTasks } from "../GlobalTasks";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function tasksPayload() {
	return {
		tasks: [
			{
				id: "gt1",
				task_number: 101,
				title: "global first",
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
				id: "gt2",
				task_number: 102,
				title: "global second",
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

function agentsPayload() {
	return {
		agents: [{ id: "agent-1", display_name: "Agent One" }],
	};
}

describe("GlobalTasks with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// GlobalTasks calls /api/agents (picker) + /api/tasks (list) +
		// /api/teams (share modal). Per-URL routing (D61) keeps each
		// JSON.parse consumer fed a valid shape.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				if (url.includes("/agents")) {
					return new Response(JSON.stringify(agentsPayload()), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
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
		const { container } = renderWithProviders(<GlobalTasks />);
		await waitFor(() =>
			expect(screen.getByText("global second")).toBeInTheDocument(),
		);
		fireEvent.click(screen.getByText("global second"));
		await waitFor(() => {
			const chips = container.querySelectorAll(".visibility-chip");
			expect(chips.length).toBeGreaterThan(0);
		});
		const chipLabels = Array.from(
			container.querySelectorAll(".visibility-chip"),
		).map((n) => n.textContent);
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<GlobalTasks />);
		await waitFor(() =>
			expect(screen.getByText("global first")).toBeInTheDocument(),
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
		vi.mocked(globalThis.fetch).mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(JSON.stringify([]), {
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
				return new Response(
					JSON.stringify({
						tasks: [
							{
								id: "gt-orphan",
								task_number: 999,
								title: "orphan global",
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
		const { container } = renderWithProviders(<GlobalTasks />);
		await waitFor(() =>
			expect(screen.getByText("orphan global")).toBeInTheDocument(),
		);
		fireEvent.click(screen.getByText("orphan global"));
		expect(container.querySelectorAll(".visibility-chip")).toHaveLength(0);
	});
});
