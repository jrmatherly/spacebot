// Phase 8 Task 8.B.2.5 — Vitest coverage for the Tauri MSAL shim.
//
// The shim is a structural duck-type PCA. These tests pin its method
// surface so a future MsalProvider call site that touches a method we
// did not stub fails at the unit-test boundary instead of crashing in
// the desktop app at sign-in time.
//
// The mocks below stand in for `tauriBridge` so the tests don't need a
// running Tauri host. The bridge mocks return canned values for the
// three command names: sign_in_with_entra, get_cached_access_token,
// clear_auth_tokens.

import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock @/platform first: shim → tauriBridge → @/platform. We need
// IS_DESKTOP=true so the bridge calls invoke() instead of returning
// undefined immediately. The invoke mock dispatches by command name.
const invokeMock = vi.fn();
vi.mock("@/platform", () => ({
	IS_DESKTOP: true,
	invoke: (cmd: string, args?: Record<string, unknown>) =>
		invokeMock(cmd, args),
}));

// Re-import the shim AFTER the mock is in place so the import chain
// picks up the mocked module.
const importShim = async () => {
	const mod = await import("../tauriMsalShim");
	return mod.getTauriMsalInstance;
};

const SERVER_URL = "http://localhost:19898";
const CLIENT_ID = "spa-client";
const TENANT_ID = "tenant-1";
const SCOPES = ["api://spacebot/api.access"];

// Build a base64url-encoded JWT body so decodeJwtClaims sees real
// claims. Header and signature are placeholder strings; the shim
// only parses the middle segment.
function makeFakeJwt(claims: Record<string, unknown>): string {
	const json = JSON.stringify(claims);
	const b64 = btoa(json)
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/, "");
	return `header.${b64}.sig`;
}

describe("tauriMsalShim", () => {
	beforeEach(() => {
		invokeMock.mockReset();
	});

	it("exposes every method MsalProvider touches on mount", async () => {
		invokeMock.mockResolvedValueOnce(null); // get_cached_access_token
		const factory = await importShim();
		const instance = await factory(
			{ entra_enabled: true } as never,
			SERVER_URL,
			SCOPES,
			TENANT_ID,
			CLIENT_ID,
		);
		// Lifecycle methods MsalProvider calls at mount.
		const surface = [
			"initialize",
			"handleRedirectPromise",
			"addEventCallback",
			"removeEventCallback",
			"enableAccountStorageEvents",
			"disableAccountStorageEvents",
			"initializeWrapperLibrary",
			"getLogger",
			"getConfiguration",
			"getAllAccounts",
			"getActiveAccount",
			"setActiveAccount",
			"loginRedirect",
			"acquireTokenSilent",
			"acquireTokenRedirect",
			"logoutRedirect",
		];
		for (const method of surface) {
			expect(
				typeof (instance as unknown as Record<string, unknown>)[method],
			).toBe("function");
		}
	});

	it("cold start without cached token surfaces empty getAllAccounts", async () => {
		invokeMock.mockResolvedValueOnce(null);
		const factory = await importShim();
		const instance = await factory(
			{ entra_enabled: true } as never,
			SERVER_URL,
			SCOPES,
			TENANT_ID,
			CLIENT_ID,
		);
		expect(instance.getAllAccounts()).toEqual([]);
		expect(instance.getActiveAccount()).toBeNull();
	});

	it("cold start WITH cached token seeds a synthetic AccountInfo", async () => {
		const jwt = makeFakeJwt({
			tid: "tenant-real",
			oid: "alice",
			preferred_username: "alice@example.com",
			name: "Alice Example",
		});
		invokeMock.mockResolvedValueOnce(jwt);
		const factory = await importShim();
		const instance = await factory(
			{ entra_enabled: true } as never,
			SERVER_URL,
			SCOPES,
			TENANT_ID,
			CLIENT_ID,
		);
		const accounts = instance.getAllAccounts();
		expect(accounts).toHaveLength(1);
		expect(accounts[0].localAccountId).toBe("alice");
		expect(accounts[0].tenantId).toBe("tenant-real");
		expect(accounts[0].username).toBe("alice@example.com");
		expect(accounts[0].name).toBe("Alice Example");
	});

	it("loginRedirect calls signInWithEntraDesktop with config values", async () => {
		invokeMock.mockResolvedValueOnce(null); // initial cache miss
		const newJwt = makeFakeJwt({ tid: TENANT_ID, oid: "bob" });
		invokeMock.mockResolvedValueOnce({
			access_token: newJwt,
			expires_in: 3600,
		}); // sign_in_with_entra
		const factory = await importShim();
		const instance = await factory(
			{ entra_enabled: true } as never,
			SERVER_URL,
			SCOPES,
			TENANT_ID,
			CLIENT_ID,
		);
		await instance.loginRedirect();
		// Second call to invokeMock is sign_in_with_entra with all four args.
		expect(invokeMock).toHaveBeenLastCalledWith("sign_in_with_entra", {
			serverUrl: SERVER_URL,
			tenantId: TENANT_ID,
			clientId: CLIENT_ID,
			scopes: SCOPES,
		});
		// activeAccount populated from the new token's claims.
		const active = instance.getActiveAccount();
		expect(active?.localAccountId).toBe("bob");
	});

	it("acquireTokenSilent throws before sign-in, returns token after", async () => {
		invokeMock.mockResolvedValueOnce(null); // cold start: no cache
		const factory = await importShim();
		const instance = await factory(
			{ entra_enabled: true } as never,
			SERVER_URL,
			SCOPES,
			TENANT_ID,
			CLIENT_ID,
		);
		// Before sign-in: throws InteractionRequiredAuthError-shape.
		await expect(instance.acquireTokenSilent({} as never)).rejects.toThrow(
			"interaction_required",
		);
		// After sign-in: returns the cached token.
		const newJwt = makeFakeJwt({ tid: TENANT_ID, oid: "carol" });
		invokeMock.mockResolvedValueOnce({
			access_token: newJwt,
			expires_in: 3600,
		});
		await instance.loginRedirect();
		const result = await instance.acquireTokenSilent({} as never);
		expect(result.accessToken).toBe(newJwt);
		expect(result.account.localAccountId).toBe("carol");
	});
});
