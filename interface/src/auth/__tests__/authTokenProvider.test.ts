// Phase 6 PR #107 review I6 remediation — tests for the
// `setAuthTokenProvider` / `getAuthToken` primitives in
// `@spacebot/api-client/client`.
//
// The primitives ship as a module-level closure slot fed by AuthGate
// (Task 6.A.5). Placed here (not in packages/api-client/) to reuse the
// vitest infrastructure Task 6.A.3 installed in interface/; the tests
// import via the workspace-symlinked @spacebot/api-client entry point,
// so they exercise the real module state.
//
// Covers four states the behavior can be in:
//   1. unset → returns null
//   2. set, provider returns a token → returns the token
//   3. set, provider returns null → returns null
//   4. set, provider throws → logs + returns null (not rethrown)

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	getAuthToken,
	setAuthTokenProvider,
} from "@spacebot/api-client/client";

describe("authTokenProvider primitives", () => {
	beforeEach(() => {
		// Reset the module-global slot so each test starts from a known
		// state. The setter accepts null to clear, matching its documented
		// contract.
		setAuthTokenProvider(null);
	});

	afterEach(() => {
		setAuthTokenProvider(null);
	});

	it("returns null when no provider is set", async () => {
		const token = await getAuthToken();
		expect(token).toBeNull();
	});

	it("returns the token the provider yields", async () => {
		setAuthTokenProvider(async () => "bearer-token-abc");
		const token = await getAuthToken();
		expect(token).toBe("bearer-token-abc");
	});

	it("returns null (not an error) when the provider yields null", async () => {
		// Scopes-empty + other "no token available" cases intentionally
		// surface as null. getAuthToken must not coerce null→empty-string
		// or vice versa.
		setAuthTokenProvider(async () => null);
		const token = await getAuthToken();
		expect(token).toBeNull();
	});

	it("swallows provider errors and returns null", async () => {
		// AuthGate's token-provider closure can throw on non-interaction
		// MSAL errors (interaction errors are handled separately via
		// acquireTokenRedirect + never-resolving promise). The contract
		// is: callers see null, not an uncaught rejection.
		const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
		setAuthTokenProvider(async () => {
			throw new Error("msal: silent acquisition failed");
		});
		const token = await getAuthToken();
		expect(token).toBeNull();
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});

	it("allows subsequent setAuthTokenProvider calls to replace the provider", async () => {
		setAuthTokenProvider(async () => "first-token");
		expect(await getAuthToken()).toBe("first-token");
		setAuthTokenProvider(async () => "second-token");
		expect(await getAuthToken()).toBe("second-token");
		setAuthTokenProvider(null);
		expect(await getAuthToken()).toBeNull();
	});
});
