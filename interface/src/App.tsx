import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { RouterProvider } from "@tanstack/react-router";
import { AuthGate } from "@/auth/AuthGate";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { ConnectionScreen } from "@/components/ConnectionScreen";
import { UserMenu } from "@/components/UserMenu";
import { LiveContextProvider } from "@/hooks/useLiveContext";
import { ServerProvider, useServer } from "@/hooks/useServer";
import { router } from "@/router";

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			staleTime: 30_000,
			retry: 1,
			refetchOnWindowFocus: true,
		},
	},
});

/**
 * Inner shell: shows the connection screen until the server is
 * reachable and initial data has loaded, then renders the main app.
 */
function AppShell() {
	const { state, hasBootstrapped, onBootstrapped } = useServer();

	// Show connection screen until we've both connected AND loaded
	// initial data. This prevents flashing the main shell before
	// LiveContextProvider's bootstrap queries complete.
	if (state !== "connected" && !hasBootstrapped) {
		return <ConnectionScreen />;
	}

	return (
		<LiveContextProvider onBootstrapped={onBootstrapped}>
			<header
				style={{
					display: "flex",
					justifyContent: "flex-end",
					borderBottom: "1px solid var(--color-border, #e5e7eb)",
				}}
			>
				<UserMenu />
			</header>
			<RouterProvider router={router} />
		</LiveContextProvider>
	);
}

export function App() {
	// Provider layering (outermost → innermost):
	//   ErrorBoundary      — catches any descendant render/effect throw
	//   QueryClientProvider — shared react-query state
	//   ServerProvider     — resolves the daemon URL FIRST so AuthGate's
	//                        loadAuthConfig() fetch hits the right host.
	//                        Required for Tauri cold-start: in desktop
	//                        mode the URL is async (Tauri command); the
	//                        SPA cannot bootstrap MSAL against an
	//                        unresolved or stale URL.
	//   AuthGate           — bootstraps MSAL once useServer() reports a
	//                        usable URL; gates children on
	//                        authenticated state.
	//   AppShell           — ConnectionScreen | LiveContextProvider+router.
	//                        Uses useServer() — still inside ServerProvider.
	return (
		<ErrorBoundary>
			<QueryClientProvider client={queryClient}>
				<ServerProvider>
					<AuthGate>
						<AppShell />
					</AuthGate>
				</ServerProvider>
				{import.meta.env.DEV && (
					<ReactQueryDevtools initialIsOpen={false} buttonPosition="bottom-right" />
				)}
			</QueryClientProvider>
		</ErrorBoundary>
	);
}
