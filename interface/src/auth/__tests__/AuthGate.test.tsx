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
// getMsalInstance returns a discriminated `MsalInstanceResult` as of
// PR #107 review I5 remediation; the default `{ok: false, reason: "disabled"}`
// matches the `entra_enabled: false` config branch.
vi.mock("../msalConfig", () => ({
	loadAuthConfig: vi.fn(async () => ({ entra_enabled: false })),
	getMsalInstance: vi.fn(async () => ({ ok: false, reason: "disabled" })),
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

import * as msalConfig from "../msalConfig";
import { AuthGate } from "../AuthGate";

describe("AuthGate", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Restore the default `entra_disabled` shape after each test since
		// individual tests override via `vi.mocked(...).mockResolvedValueOnce`.
		vi.mocked(msalConfig.loadAuthConfig).mockResolvedValue({
			entra_enabled: false,
		});
		vi.mocked(msalConfig.getMsalInstance).mockResolvedValue({
			ok: false,
			reason: "disabled",
		});
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

	/// PR #107 review I4/I5 remediation: when /api/auth/config reports
	/// `entra_enabled: true` but omits identifiers, AuthGate renders an
	/// operator-visible error banner instead of fail-open to
	/// `entra_disabled` (which previously masked daemon config bugs
	/// behind a 401-loop UI).
	it("renders an error banner when entra is configured but malformed", async () => {
		vi.mocked(msalConfig.loadAuthConfig).mockResolvedValueOnce({
			entra_enabled: true,
			client_id: undefined,
			authority: undefined,
		});
		vi.mocked(msalConfig.getMsalInstance).mockResolvedValueOnce({
			ok: false,
			reason: "malformed",
			missing: ["client_id", "authority"],
		});
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		const banner = await screen.findByTestId("auth-gate-error");
		expect(banner).toBeInTheDocument();
		expect(banner).toHaveTextContent(/client_id/);
		expect(banner).toHaveTextContent(/authority/);
		// Children MUST NOT render when the error state is active — an
		// app that renders while Entra is broken will 401 on every API
		// call.
		expect(screen.queryByText("child-content")).not.toBeInTheDocument();
	});

	/// PR #107 review I4 remediation: loadAuthConfig failure (e.g., 500
	/// from /api/auth/config, or a network error) now surfaces as a
	/// diagnostic banner, not a stuck spinner or a silent fail-open.
	it("renders an error banner when loadAuthConfig throws", async () => {
		vi.mocked(msalConfig.loadAuthConfig).mockRejectedValueOnce(
			new Error("auth-config fetch failed: 500 Internal Server Error"),
		);
		// Silence the expected console.error from AuthGate's catch.
		const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		const banner = await screen.findByTestId("auth-gate-error");
		expect(banner).toBeInTheDocument();
		expect(banner).toHaveTextContent(/500/);
		expect(screen.queryByText("child-content")).not.toBeInTheDocument();
		errSpy.mockRestore();
	});
});
