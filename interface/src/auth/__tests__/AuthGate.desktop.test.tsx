// AuthGate Tauri-mode tests.
//
// The desktop branch gates the bootstrap effect on serverReady (a
// non-empty serverUrl from useServer() in IS_DESKTOP mode). A regression
// that flips the initial GateState back to "loading" would leave the
// SPA stuck on "Signing in..." forever during cold start before the
// daemon URL is resolved.
//
// `useServer` is the unit of truth for serverUrl; mocking it lets us
// exercise the empty -> non-empty transition without spinning up a real
// ServerProvider.

import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// IS_DESKTOP must be true for the gate to fire. The platform module is
// captured at AuthGate import time, so the mock has to be in place
// before the dynamic `import` below.
vi.mock("@/platform", () => ({
	IS_DESKTOP: true,
	invoke: vi.fn(),
}));

// Default useServer mock: empty serverUrl so the gate starts in
// `waiting_for_server`. Individual tests override per-call.
const useServerMock = vi.fn(() => ({
	serverUrl: "",
	state: "checking" as const,
	setServerUrl: () => {},
	hasConnected: false,
	hasBootstrapped: false,
	onBootstrapped: () => {},
	isDesktopHost: true,
	hasBundledServer: true,
}));
vi.mock("@/hooks/useServer", () => ({
	useServer: () => useServerMock(),
}));

// msalConfig must default to entra_disabled so the bootstrap effect
// resolves cleanly in tests that run past the waiting_for_server gate.
vi.mock("../msalConfig", () => ({
	loadAuthConfig: vi.fn(async () => ({ entra_enabled: false })),
	getMsalInstance: vi.fn(async () => ({ ok: false, reason: "disabled" })),
	getActiveScopes: vi.fn(async () => []),
}));

// setAuthTokenProvider isn't called in the disabled branch; mock it
// anyway so the test never mutates the real api-client.
vi.mock("@spacebot/api-client/client", async (importOriginal) => {
	const actual = await importOriginal<
		typeof import("@spacebot/api-client/client")
	>();
	return {
		...actual,
		setAuthTokenProvider: vi.fn(),
	};
});

import { AuthGate } from "../AuthGate";

describe("AuthGate (desktop / Tauri mode)", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		useServerMock.mockReturnValue({
			serverUrl: "",
			state: "checking" as const,
			setServerUrl: () => {},
			hasConnected: false,
			hasBootstrapped: false,
			onBootstrapped: () => {},
			isDesktopHost: true,
			hasBundledServer: true,
		});
	});

	it("renders waiting_for_server when serverUrl is empty under Tauri", () => {
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		// The gate sits in waiting_for_server; children must not render.
		expect(
			screen.getByTestId("auth-gate-waiting-server"),
		).toBeInTheDocument();
		expect(screen.queryByText("child-content")).not.toBeInTheDocument();
	});

	it("transitions to children when serverUrl flips non-empty", async () => {
		// First render with empty URL; gate is waiting.
		const { rerender } = render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		expect(
			screen.getByTestId("auth-gate-waiting-server"),
		).toBeInTheDocument();

		// Daemon URL resolves: simulate the next useServer() return.
		useServerMock.mockReturnValue({
			serverUrl: "http://localhost:19898",
			state: "connected" as const,
			setServerUrl: () => {},
			hasConnected: true,
			hasBootstrapped: true,
			onBootstrapped: () => {},
			isDesktopHost: true,
			hasBundledServer: true,
		});
		rerender(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		// AuthGate re-runs the bootstrap effect (serverReady flipped),
		// hits entra_disabled (mocked), and renders children.
		await waitFor(() => {
			expect(screen.getByText("child-content")).toBeInTheDocument();
		});
	});
});
