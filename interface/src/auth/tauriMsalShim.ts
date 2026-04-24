// Phase 8 Task 8.B.2 — Tauri-mode replacement for `@azure/msal-browser`'s
// `PublicClientApplication`. Returns the access token acquired via the
// system-browser loopback flow (Phase 8 PR A) instead of running MSAL's
// own cache and redirect dance.
//
// Why a shim, not real MSAL:
//   * MSAL.js v5 in a Tauri WebView assumes `localStorage` survives a
//     full-page navigation back from accounts.microsoft.com. The system-
//     browser flow returns to a loopback HTTP server, NOT the WebView,
//     so MSAL's cache never sees the redirect_state and silently fails.
//   * A shim that satisfies `MsalProvider`'s structural contract lets us
//     keep the rest of the SPA (AuthGate, useAccount, AuthenticatedTemplate)
//     unchanged.
//
// Option-C precedent: `mockMsal.ts` has shipped a structural duck-type
// PCA since Phase 6 Task 6.A.5 with no `MsalProvider` crash. This shim
// is the same pattern, with a strict superset of mockMsal's surface so
// any future MSAL method MsalProvider touches at mount survives both
// VITE_AUTH_MOCK=1 and Tauri builds.
//
// State machine:
//   uninitialized → initialized (cached?)
//                ↳ initialized (no cache) → loginRedirect → signed-in
//                ↳ initialized (cached)   → signed-in (cold start happy path)
//   signed-in → logoutRedirect → uninitialized

import type {
	AccountInfo,
	AuthenticationResult,
	PublicClientApplication,
} from "@azure/msal-browser";
import {
	clearDesktopTokens,
	getCachedAccessToken,
	signInWithEntraDesktop,
} from "./tauriBridge";
import type { AuthConfigResponse } from "@spacebot/api-client/types";

/**
 * JWT payload shape we actually use. Tokens minted by Entra carry many
 * more claims; this is the minimum the shim needs to seed an
 * `AccountInfo` for `MsalProvider` consumers.
 */
interface MinimalJwtClaims {
	tid?: string;
	oid?: string;
	preferred_username?: string;
	upn?: string;
	name?: string;
	sub?: string;
}

/**
 * Best-effort JWT decode. Real MSAL parses with full validation; the
 * shim does not validate (the daemon already validated before storing).
 * Returns null on any structural failure so callers can fall back to
 * an empty account.
 */
function decodeJwtClaims(token: string): MinimalJwtClaims | null {
	const parts = token.split(".");
	if (parts.length < 2) return null;
	try {
		const payload = parts[1].replace(/-/g, "+").replace(/_/g, "/");
		const padded = payload + "=".repeat((4 - (payload.length % 4)) % 4);
		return JSON.parse(atob(padded)) as MinimalJwtClaims;
	} catch {
		return null;
	}
}

function makeAccountFromClaims(claims: MinimalJwtClaims): AccountInfo {
	const oid = claims.oid ?? claims.sub ?? "tauri-account";
	const tid = claims.tid ?? "tauri-tenant";
	const username =
		claims.preferred_username ?? claims.upn ?? `${oid}@unknown`;
	return {
		homeAccountId: `${oid}.${tid}`,
		environment: "login.microsoftonline.com",
		tenantId: tid,
		username,
		localAccountId: oid,
		name: claims.name ?? username,
	};
}

/**
 * Build the structural duck-type that satisfies MsalProvider's
 * mount-time surface. Cast to PublicClientApplication at the boundary
 * the same way mockMsal does.
 *
 * The serverUrl is captured at construction time (acquired once at
 * shim build via the Tauri `get_server_url` command in
 * `desktop/src-tauri/src/main.rs:30`). Daemon URL changes after
 * mount are rare enough that re-acquiring per-call is overkill;
 * AuthGate's bootstrap effect re-runs when serverReady flips, which
 * also rebuilds the shim.
 */
export async function getTauriMsalInstance(
	_cfg: AuthConfigResponse,
	serverUrl: string,
	scopes: string[],
	tenantId: string,
	clientId: string,
) {
	let activeAccount: AccountInfo | null = null;
	let cachedToken: string | null = null;

	// Cold-start path: ask the daemon for any persisted access token.
	// On hit, seed activeAccount so getAllAccounts() returns non-empty
	// and MsalProvider lights up <AuthenticatedTemplate>. On miss, fall
	// through; the SPA will render <UnauthenticatedTemplate>.
	const initialToken = await getCachedAccessToken(serverUrl);
	if (initialToken) {
		const claims = decodeJwtClaims(initialToken);
		if (claims) {
			activeAccount = makeAccountFromClaims(claims);
			cachedToken = initialToken;
		}
	}

	const instance = {
		// Lifecycle no-ops — MsalProvider calls these on mount.
		initialize: async () => {},
		handleRedirectPromise: async (): Promise<AuthenticationResult | null> =>
			null,
		addEventCallback: (_cb: unknown) => "",
		removeEventCallback: (_id: string) => {},
		enableAccountStorageEvents: () => {},
		disableAccountStorageEvents: () => {},
		initializeWrapperLibrary: (_name: string, _version: string) => {},
		getLogger: () => ({
			info: () => {},
			warning: () => {},
			error: () => {},
			verbose: () => {},
			trace: () => {},
		}),
		getConfiguration: () => ({
			auth: { clientId, authority: `https://login.microsoftonline.com/${tenantId}` },
		}),

		// Account state.
		getAllAccounts: (): AccountInfo[] =>
			activeAccount ? [activeAccount] : [],
		getActiveAccount: (): AccountInfo | null => activeAccount,
		setActiveAccount: (account: AccountInfo | null) => {
			activeAccount = account;
		},

		// Sign-in: route through the system-browser loopback flow.
		// `loginRedirect` is what MsalProvider's AuthenticatedTemplate
		// calls when the SignInPrompt button fires.
		loginRedirect: async (_request?: unknown): Promise<void> => {
			const result = await signInWithEntraDesktop({
				serverUrl,
				tenantId,
				clientId,
				scopes,
			});
			cachedToken = result.access_token;
			const claims = decodeJwtClaims(result.access_token);
			activeAccount = claims
				? makeAccountFromClaims(claims)
				: makeAccountFromClaims({});
		},

		// Token acquisition. Silent path returns the cached token if
		// present; throws InteractionRequiredAuthError-shape if not so
		// makeTokenProvider() in AuthGate falls into its redirect
		// branch, which calls acquireTokenRedirect → loginRedirect.
		acquireTokenSilent: async (): Promise<AuthenticationResult> => {
			if (!cachedToken || !activeAccount) {
				const error = new Error("interaction_required") as Error & {
					name: string;
				};
				error.name = "InteractionRequiredAuthError";
				throw error;
			}
			return {
				accessToken: cachedToken,
				account: activeAccount,
				scopes,
				idToken: "",
				idTokenClaims: {},
				fromCache: true,
				expiresOn: null,
				tenantId: activeAccount.tenantId,
				uniqueId: activeAccount.localAccountId,
				tokenType: "Bearer",
				correlationId: "",
				authority: `https://login.microsoftonline.com/${tenantId}`,
			} as unknown as AuthenticationResult;
		},

		// Interactive token: route through the same sign-in flow. The
		// real MSAL.js variant returns void and triggers a navigation;
		// the Tauri flow opens the system browser, awaits the loopback
		// callback, persists tokens, and resolves. Same observable
		// effect: by the time the promise resolves, sign-in is done.
		acquireTokenRedirect: async (_request?: unknown): Promise<void> => {
			await (instance.loginRedirect as () => Promise<void>)();
		},

		// Sign-out: wipe the daemon-side cache, then forget local state.
		logoutRedirect: async (opts?: { postLogoutRedirectUri?: string }) => {
			await clearDesktopTokens(serverUrl);
			activeAccount = null;
			cachedToken = null;
			window.location.href = opts?.postLogoutRedirectUri ?? "/";
		},
	};

	return instance as unknown as PublicClientApplication;
}
