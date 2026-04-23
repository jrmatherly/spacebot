// Test harness for full-page route components that depend on React Query
// + TanStack Router context. PR 1's hook-only tests used inline
// QueryClientProvider; route components (AgentMemories, AgentTasks, Wiki,
// etc.) blow up without router context, so Phase 7 PRs 2-5 route tests go
// through this helper.
import { type ReactNode } from "react";
import { render, type RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
	Outlet,
	RouterProvider,
} from "@tanstack/react-router";

export function renderWithProviders(
	ui: ReactNode,
	opts: { initialPath?: string } = {},
): RenderResult {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});

	const rootRoute = createRootRoute({ component: () => <Outlet /> });
	const indexRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/",
		component: () => <>{ui}</>,
	});
	const router = createRouter({
		routeTree: rootRoute.addChildren([indexRoute]),
		history: createMemoryHistory({
			initialEntries: [opts.initialPath ?? "/"],
		}),
	});

	return render(
		<QueryClientProvider client={client}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);
}
