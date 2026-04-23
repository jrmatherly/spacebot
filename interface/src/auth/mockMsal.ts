// Mock PublicClientApplication for local dev / CI (VITE_AUTH_MOCK=1).
//
// Mints a base64url-encoded JSON token compatible with the daemon's
// MockValidator in `src/auth/testing.rs`. Claims come from Vite env
// vars so CI can set different roles without recompiling:
//
//   VITE_MOCK_TID=tenant-1
//   VITE_MOCK_OID=alice
//   VITE_MOCK_ROLES=SpacebotUser,SpacebotAdmin
//
// `AuthConfigResponse` imports from `@spacebot/api-client/types` (the
// canonical schema path); `msalConfig.ts` imports the same name
// internally but does not re-export it.

import type { AccountInfo } from "@azure/msal-browser";
import type { AuthConfigResponse } from "@spacebot/api-client/types";

interface MockAccount {
	tid: string;
	oid: string;
	name: string;
	username: string;
	roles: string[];
}

// URL-safe base64 alphabet. Exported for direct unit testing of the
// encoding contract with the daemon's MockValidator.
export function base64UrlEncode(bytes: Uint8Array): string {
	let binary = "";
	for (const byte of bytes) {
		binary += String.fromCharCode(byte);
	}
	return btoa(binary)
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/, "");
}

/**
 * Returns a PublicClientApplication-shaped stub. Callers in
 * msalConfig.ts cast to PublicClientApplication after await; the
 * function returns a structural duck-type rather than a real MSAL
 * instance so local dev does not need an Entra tenant.
 */
export async function getMockMsalInstance(_cfg: AuthConfigResponse) {
	const tid =
		(import.meta.env.VITE_MOCK_TID as string | undefined) ?? "tenant-mock";
	const oid = (import.meta.env.VITE_MOCK_OID as string | undefined) ?? "alice";
	const rolesRaw =
		(import.meta.env.VITE_MOCK_ROLES as string | undefined) ?? "SpacebotUser";
	const roles = rolesRaw
		.split(",")
		.map((s) => s.trim())
		.filter(Boolean);

	const account: MockAccount = {
		tid,
		oid,
		name: `Mock ${oid}`,
		username: `${oid}@example.com`,
		roles,
	};

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
		return base64UrlEncode(json);
	};

	return {
		initialize: async () => {},
		handleRedirectPromise: async () => null,
		getAllAccounts: () => [account],
		// Typed contract: match the real MSAL signature rather than
		// `unknown`, so a caller passing the wrong argument type fails
		// at the TS boundary instead of silently at runtime.
		setActiveAccount: (_a: AccountInfo | null) => {},
		acquireTokenSilent: async () => ({ accessToken: mintToken() }),
		acquireTokenRedirect: async () => {},
		loginRedirect: async () => {},
		logoutRedirect: async (opts?: { postLogoutRedirectUri?: string }) => {
			// Honor the caller's postLogoutRedirectUri so SPAs served
			// under a subpath don't land at the wrong root after mock sign-out.
			window.location.href = opts?.postLogoutRedirectUri ?? "/";
		},
	};
}
