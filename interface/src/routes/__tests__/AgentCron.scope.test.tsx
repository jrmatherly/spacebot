// AgentCron visibility-chip + filter-wiring test.
// Mirrors Wiki.scope.test.tsx. Cron renders per-row chips + a Share
// button inside each CronJobCard, and lifts the filter state to the URL
// search params via useSearch / useNavigate.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../../test/renderWithProviders";

import { AgentCron } from "../AgentCron";
import { setAuthTokenProvider } from "@spacebot/api-client/client";

function cronListPayload() {
	return {
		timezone: "UTC",
		jobs: [
			{
				id: "daily-summary",
				prompt: "summarize yesterday",
				cron_expr: "0 9 * * *",
				interval_secs: 0,
				delivery_target: "discord:123456789",
				enabled: true,
				run_once: false,
				active_hours: null,
				timeout_secs: null,
				execution_success_count: 10,
				execution_failure_count: 0,
				delivery_success_count: 10,
				delivery_failure_count: 0,
				delivery_skipped_count: 0,
				last_executed_at: "2026-04-23T09:00:00Z",
				visibility: "personal",
				team_name: null,
			},
			{
				id: "platform-digest",
				prompt: "post weekly engineering digest",
				cron_expr: "0 9 * * 1",
				interval_secs: 0,
				delivery_target: "discord:987654321",
				enabled: true,
				run_once: false,
				active_hours: null,
				timeout_secs: null,
				execution_success_count: 5,
				execution_failure_count: 0,
				delivery_success_count: 5,
				delivery_failure_count: 0,
				delivery_skipped_count: 0,
				last_executed_at: "2026-04-22T09:00:00Z",
				visibility: "team",
				team_name: "Platform",
			},
		],
	};
}

function channelsPayload() {
	return {
		channels: [
			{
				id: "123456789",
				agent_id: "a-1",
				platform: "discord",
				display_name: "#general",
			},
		],
	};
}

describe("AgentCron with visibility", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
		// Per-URL mock: /api/teams for the Share modal, /api/channels
		// for delivery-target options, /api/agents/cron for the list.
		vi.spyOn(globalThis, "fetch").mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(
						JSON.stringify([{ id: "team-1", display_name: "Platform" }]),
						{ status: 200, headers: { "content-type": "application/json" } },
					);
				}
				if (url.includes("/channels")) {
					return new Response(JSON.stringify(channelsPayload()), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				return new Response(JSON.stringify(cronListPayload()), {
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

	it("renders a visibility chip per cron row", async () => {
		const { container } = renderWithProviders(<AgentCron agentId="a-1" />);
		await waitFor(() =>
			expect(screen.getByText("daily-summary")).toBeInTheDocument(),
		);
		const chips = container.querySelectorAll('[data-testid="visibility-chip"]');
		const chipLabels = Array.from(chips).map((n) => n.textContent);
		expect(chipLabels).toContain("Personal");
		expect(chipLabels).toContain("Team: Platform");
	});

	it("piping visibility filter into the query key triggers a new fetch", async () => {
		renderWithProviders(<AgentCron agentId="a-1" />);
		await waitFor(() =>
			expect(screen.getByText("daily-summary")).toBeInTheDocument(),
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

	it("renders no chip for a job with null visibility (no-auto-broadening)", async () => {
		// No-auto-broadening policy: an unowned cron must show no chip
		// rather than defaulting to a Personal label. Pairs with the
		// backend invariant in
		// `resources.rs::enrich_missing_ownership_row_returns_none_fields_not_personal_default`.
		vi.mocked(globalThis.fetch).mockImplementation(
			async (input: RequestInfo | URL) => {
				const url = typeof input === "string" ? input : String(input);
				if (url.includes("/teams")) {
					return new Response(JSON.stringify([]), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				if (url.includes("/channels")) {
					return new Response(JSON.stringify(channelsPayload()), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				return new Response(
					JSON.stringify({
						timezone: "UTC",
						jobs: [
							{
								id: "orphan-cron",
								prompt: "pre-entra cron without an ownership row",
								cron_expr: "0 0 * * *",
								interval_secs: 0,
								delivery_target: "discord:123456789",
								enabled: true,
								run_once: false,
								active_hours: null,
								timeout_secs: null,
								execution_success_count: 0,
								execution_failure_count: 0,
								delivery_success_count: 0,
								delivery_failure_count: 0,
								delivery_skipped_count: 0,
								last_executed_at: null,
								visibility: null,
								team_name: null,
							},
						],
					}),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			},
		);
		const { container } = renderWithProviders(<AgentCron agentId="a-1" />);
		await waitFor(() =>
			expect(screen.getByText("orphan-cron")).toBeInTheDocument(),
		);
		expect(container.querySelectorAll('[data-testid="visibility-chip"]')).toHaveLength(0);
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
				if (url.includes("/channels")) {
					return new Response(JSON.stringify(channelsPayload()), {
						status: 200,
						headers: { "content-type": "application/json" },
					});
				}
				return new Response(
					JSON.stringify({ error: "database unavailable" }),
					{ status: 500, headers: { "content-type": "application/json" } },
				);
			},
		);
		renderWithProviders(<AgentCron agentId="a-1" />);
		await waitFor(() =>
			expect(screen.getByText(/Failed to load cron jobs/i)).toBeInTheDocument(),
		);
	});
});
