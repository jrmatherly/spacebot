// Settings page admin-gate tests. Settings.tsx uses
// `useRole("SpacebotAdmin")` to gate the Providers / Secrets / Channels /
// API-Keys sections (ADMIN_ONLY_SECTIONS at Settings.tsx:27). A non-admin
// caller sees a trimmed section list + a safe default active section
// (appearance); an admin sees the full list and lands on `providers`.
//
// Uses a local router that registers `/settings` because Settings calls
// `useSearch({from: "/settings"})` (Settings.tsx:81). The shared
// `renderWithProviders` helper only registers `/` so it cannot host
// Settings directly.
//
// Mocks /api/me to drive `useRole` — the source of truth for role
// state. setupMocks is fail-loud (D109): unmocked URLs throw.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	Outlet,
	RouterProvider,
} from "@tanstack/react-router";

// Stub the section-body components. The gating logic under test is
// `isAdmin ? SECTIONS : SECTIONS.filter(not-admin-only)` + the nav
// render — neither depends on the bodies. Stubbing also sidesteps an
// ESM resolution issue in `@lobehub/ui` that Settings sections pull
// in transitively. SECTIONS + PROVIDERS + CHATGPT_OAUTH_DEFAULT_MODEL
// + SectionId stay real (they live in lightweight constants.ts /
// types.ts files with no UI deps). vi.importActual on the barrel
// would re-trigger the @lobehub resolution; re-export from the
// leaf files instead.
vi.mock("@/components/settings", async () => {
	const constants = await vi.importActual<
		typeof import("@/components/settings/constants")
	>("@/components/settings/constants");
	const types = await vi.importActual<
		typeof import("@/components/settings/types")
	>("@/components/settings/types");
	const stub = (name: string) => () => <div data-testid={`section-${name}`}>{name}</div>;
	return {
		...constants,
		...types,
		InstanceSection: stub("instance"),
		AppearanceSection: stub("appearance"),
		ChannelsSection: stub("channels"),
		SecretsSection: stub("secrets"),
		ApiKeysSection: stub("api-keys"),
		ServerSection: stub("server"),
		WorkerLogsSection: stub("worker-logs"),
		OpenCodeSection: stub("opencode"),
		UpdatesSection: stub("updates"),
		ChangelogSection: stub("changelog"),
		ConfigFileSection: stub("config-file"),
		ProviderCard: () => <div data-testid="provider-card" />,
		ChatGptOAuthDialog: () => null,
	};
});

import { Settings } from "../Settings";
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

function setupMocks(opts: { roles: string[] }) {
	vi.spyOn(globalThis, "fetch").mockImplementation(
		async (input: RequestInfo | URL) => {
			const url = typeof input === "string" ? input : String(input);
			if (url.includes("/api/me")) {
				return new Response(JSON.stringify(mePayload(opts.roles)), {
					status: 200,
					headers: { "content-type": "application/json" },
				});
			}
			// Providers + settings endpoints that fire from admin branches.
			// Kept permissive: return empty shapes so the admin path can
			// hydrate without blowing up.
			if (url.includes("/api/providers")) {
				return new Response(
					JSON.stringify({ providers: [], has_any: false }),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			}
			if (url.includes("/api/settings")) {
				return new Response(
					JSON.stringify({ litellm: { base_url: "", api_key_set: false } }),
					{ status: 200, headers: { "content-type": "application/json" } },
				);
			}
			throw new Error(`unmocked fetch in Settings.admin scope test: ${url}`);
		},
	);
}

function renderSettings(opts: { initialPath?: string } = {}) {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const rootRoute = createRootRoute({ component: () => <Outlet /> });
	const settingsRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/settings",
		component: Settings,
		validateSearch: (search: Record<string, unknown>) => ({
			tab: typeof search.tab === "string" ? search.tab : undefined,
		}),
	});
	const router = createRouter({
		routeTree: rootRoute.addChildren([settingsRoute]),
		history: createMemoryHistory({
			initialEntries: [opts.initialPath ?? "/settings"],
		}),
	});
	return render(
		<QueryClientProvider client={client}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}

describe("Settings admin-only section gating", () => {
	beforeEach(() => {
		setAuthTokenProvider(async () => "mock-token");
	});

	afterEach(() => {
		vi.restoreAllMocks();
		setAuthTokenProvider(null);
	});

	it("hides admin-only sections from a non-admin caller", async () => {
		setupMocks({ roles: ["SpacebotUser"] });
		renderSettings();
		// Non-admin lands on `appearance` per Settings.tsx:84-86; the
		// stub renders <div data-testid="section-appearance">.
		await waitFor(() =>
			expect(screen.getByTestId("section-appearance")).toBeInTheDocument(),
		);
		// Admin-only sections are filtered out of the section nav. Their
		// nav entries should not appear.
		expect(screen.queryByRole("button", { name: /^providers$/i })).toBeNull();
		expect(screen.queryByRole("button", { name: /^secrets$/i })).toBeNull();
		expect(screen.queryByRole("button", { name: /^channels$/i })).toBeNull();
		expect(screen.queryByRole("button", { name: /^api keys$/i })).toBeNull();
	});

	it("shows admin-only sections to an admin caller", async () => {
		setupMocks({ roles: ["SpacebotAdmin"] });
		renderSettings();
		// Admin lands on providers by default; wait for the Providers nav.
		await waitFor(() =>
			expect(screen.getByRole("button", { name: /^providers$/i })).toBeInTheDocument(),
		);
		expect(screen.getByRole("button", { name: /^secrets$/i })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /^channels$/i })).toBeInTheDocument();
		expect(screen.getByRole("button", { name: /^api keys$/i })).toBeInTheDocument();
	});

	it("shows a lockout message when a non-admin deep-links to an admin section", async () => {
		setupMocks({ roles: ["SpacebotUser"] });
		renderSettings({ initialPath: "/settings?tab=secrets" });
		// The lockout branch (Settings.tsx:90 isLockedOut) renders a
		// role-required message rather than the section body.
		await waitFor(() =>
			expect(
				screen.getByText(/SpacebotAdmin/i),
			).toBeInTheDocument(),
		);
	});
});
