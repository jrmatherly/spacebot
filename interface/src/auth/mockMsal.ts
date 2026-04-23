// Phase 6 PR C Task 6.C.5 — mock PublicClientApplication for local dev / CI.
//
// Activated by VITE_AUTH_MOCK=1. Mints a base64url JSON token
// compatible with the daemon's MockValidator (src/auth/testing.rs,
// Phase 4 PR 1). Claims pulled from Vite env vars so CI can set
// different roles without recompiling:
//
//   VITE_MOCK_TID=tenant-1
//   VITE_MOCK_OID=alice
//   VITE_MOCK_ROLES=SpacebotUser,SpacebotAdmin
//
// D16 correction (2026-04-23 PR C audit): AuthConfigResponse is
// imported from @spacebot/api-client/types (the canonical schema
// path), NOT from ./msalConfig (which imports it internally but does
// not re-export).

import type { AuthConfigResponse } from "@spacebot/api-client/types";

interface MockAccount {
	tid: string;
	oid: string;
	name: string;
	username: string;
	roles: string[];
}

/**
 * Returns a PublicClientApplication-shaped stub. Callers in
 * msalConfig.ts cast to PublicClientApplication after await; this
 * function intentionally returns a structural duck-type rather than
 * a real MSAL instance so local dev does not need an Entra tenant.
 */
export async function getMockMsalInstance(_cfg: AuthConfigResponse) {
	const tid = (import.meta.env.VITE_MOCK_TID as string | undefined) ?? "tenant-mock";
	const oid = (import.meta.env.VITE_MOCK_OID as string | undefined) ?? "alice";
	const rolesRaw =
		(import.meta.env.VITE_MOCK_ROLES as string | undefined) ?? "SpacebotUser";
	const roles = rolesRaw.split(",").map((s) => s.trim()).filter(Boolean);

	const account: MockAccount = {
		tid,
		oid,
		name: `Mock ${oid}`,
		username: `${oid}@example.com`,
		roles,
	};

	// Produce a JSON-base64url token compatible with the daemon's
	// MockValidator. No signature — MockValidator parses the body
	// directly after base64url-decoding.
	const mintToken = (): string => {
		const mintable = {
			principal_type: "user",
			tid,
			oid,
			roles,
			groups: [] as string[],
			display_email: account.username,
			display_name: account.name,
		};
		const json = new TextEncoder().encode(JSON.stringify(mintable));
		// btoa over a plain char string (String.fromCharCode on the
		// bytes), then URL-safe transform: + → -, / → _, strip =.
		let binary = "";
		for (const byte of json) {
			binary += String.fromCharCode(byte);
		}
		return btoa(binary)
			.replace(/\+/g, "-")
			.replace(/\//g, "_")
			.replace(/=+$/, "");
	};

	return {
		initialize: async () => {},
		handleRedirectPromise: async () => null,
		getAllAccounts: () => [account],
		setActiveAccount: (_a: unknown) => {},
		acquireTokenSilent: async () => ({ accessToken: mintToken() }),
		acquireTokenRedirect: async () => {},
		loginRedirect: async () => {},
		logoutRedirect: async () => {
			window.location.href = "/";
		},
	};
}
