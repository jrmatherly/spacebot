// AuthGate state machine tests.
//
// Covers the gate's branches that are reachable without a real MSAL
// tenant: entra_disabled, loading, and the malformed + fetch-failure
// error states. The unauthenticated + authenticated branches are
// exercised by the mockMsal-backed integration tests.

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

	/// When /api/auth/config reports `entra_enabled: true` but omits
	/// identifiers, AuthGate renders an operator-visible error banner
	/// instead of falling open to `entra_disabled` (which previously
	/// masked daemon config bugs behind a 401-loop UI).
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

	/// loadAuthConfig failure (e.g., 500 from /api/auth/config, or a
	/// network error) surfaces as a diagnostic banner, not a stuck
	/// spinner or a silent fail-open.
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

	// Phase 6 PR C: authedFetch (PR B) dispatches spacebot:auth-exhausted
	// on 401 refresh-exhaustion. SSE via fetchEventSource inherits the
	// same dispatch. AuthGate's global listener surfaces both to
	// console.warn. Phase 7 upgrades to a toast banner; until then, this
	// test pins the plumbing so a refactor that removes the listener is
	// caught immediately.
	it("logs a warn when spacebot:auth-exhausted fires on the window", async () => {
		const warnSpy = vi
			.spyOn(console, "warn")
			.mockImplementation(() => {});
		render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		// Wait for AuthGate to finish async init and attach the listener.
		await waitFor(() => {
			expect(screen.getByText("child-content")).toBeInTheDocument();
		});

		window.dispatchEvent(
			new CustomEvent("spacebot:auth-exhausted", {
				detail: {
					url: "http://api/some-endpoint",
					reason: "refresh_failed",
				},
			}),
		);

		expect(warnSpy).toHaveBeenCalledWith(
			expect.stringContaining("http://api/some-endpoint"),
		);
		expect(warnSpy).toHaveBeenCalledWith(
			expect.stringContaining("refresh_failed"),
		);
		warnSpy.mockRestore();
	});

	// Unmount must remove the listener or future dispatches log to
	// nowhere while still consuming test resources. Catches a regression
	// where the useEffect returns the wrong cleanup (e.g., returns the
	// handler itself instead of a remove-call).
	it("removes the spacebot:auth-exhausted listener on unmount", async () => {
		const warnSpy = vi
			.spyOn(console, "warn")
			.mockImplementation(() => {});
		const { unmount } = render(
			<AuthGate>
				<div>child-content</div>
			</AuthGate>,
		);
		await waitFor(() => {
			expect(screen.getByText("child-content")).toBeInTheDocument();
		});
		unmount();
		warnSpy.mockClear();

		window.dispatchEvent(
			new CustomEvent("spacebot:auth-exhausted", {
				detail: { url: "http://api/after-unmount", reason: "refresh_failed" },
			}),
		);

		expect(warnSpy).not.toHaveBeenCalled();
		warnSpy.mockRestore();
	});
});
