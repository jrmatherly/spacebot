// Phase 6 Task 6.A.5 — AuthGate state machine tests.
//
// Covers the two happy-path branches of the gate:
//   1. entra_disabled → children render directly (static-token deployments)
//   2. loading → spinner visible before async init resolves
//
// The unauthenticated + authenticated branches are exercised by
// Task 6.C.5's mockMsal-backed integration tests; at this stage we only
// assert the pre-MSAL states because msalConfig is mocked here.

import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock msalConfig before importing AuthGate so the component sees the
// mocked implementations during module initialization. `entra_disabled`
// is the default return to keep tests minimal; individual tests override
// via `vi.mocked(...)` when they need authenticated/unauthenticated shape.
vi.mock("../msalConfig", () => ({
	loadAuthConfig: vi.fn(async () => ({ entra_enabled: false })),
	getMsalInstance: vi.fn(async () => null),
	getActiveScopes: vi.fn(async () => []),
}));

// setAuthTokenProvider is called by AuthGate when a user authenticates.
// Mock it so tests don't mutate module-global state in the real client.
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

describe("AuthGate", () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it("renders children when entra is disabled", async () => {
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		await waitFor(() => {
			expect(screen.getByText("child-content")).toBeInTheDocument();
		});
	});

	it("shows loading spinner while initializing", async () => {
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		// Before the useEffect async load resolves, the loading state is
		// in the DOM. queryByTestId (not getByTestId) so the assertion
		// semantics are "present", not "throws if absent".
		expect(screen.queryByTestId("auth-gate-loading")).toBeInTheDocument();
		// waitFor the async state update so useEffect's setState lands
		// inside the test scope — silences React's `act(...)` warning about
		// unwrapped state updates during teardown.
		await waitFor(() => {
			expect(screen.getByText("child-content")).toBeInTheDocument();
		});
	});
});
