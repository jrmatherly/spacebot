// Tauri-mode replacement for `@azure/msal-browser`'s
// `PublicClientApplication`. Returns the access token acquired via the
// system-browser loopback flow instead of running MSAL's own cache and
// redirect dance.
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
// Option-C precedent: `mockMsal.ts` ships a structural duck-type PCA
// without a `MsalProvider` crash. This shim is the same pattern, with
// a strict superset of mockMsal's surface.
//
// State machine:
//   uninitialized → initialized (cached?)
//                ↳ initialized (no cache) → loginRedirect → signed-in
//                ↳ initialized (cached)   → signed-in (cold start happy path)
//   signed-in → logoutRedirect → uninitialized

import {
	InteractionRequiredAuthError,
	type AccountInfo,
	type AuthenticationResult,
	type PublicClientApplication,
} from "@azure/msal-browser";
import {
	clearDesktopTokens,
	getCachedAccessToken,
	signInWithEntraDesktop,
} from "./tauriBridge";
import type { AuthConfigResponse } from "@spacebot/api-client/types";

/**
 * Subset of JWT claims the shim reads. All fields are optional because
 * the shim never validates the token (the daemon already did before
 * persisting), and a malformed payload should degrade to "no claims" not
 * a parse exception. Production Entra v2 tokens always carry `tid`,
 * `oid`, and at least one of `preferred_username`/`upn`; absence of any
 * of these from a daemon-supplied token is treated as a fail-closed
 * condition by the callers below.
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
 * Best-effort JWT decode. Returns null on any structural failure.
 * Callers must treat null as fail-closed (clear cached state, show
 * sign-in) rather than synthesizing a placeholder identity.
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

function makeAccountFromClaims(claims: MinimalJwtClaims): AccountInfo | null {
	// Refuse to synthesize an identity from missing claims. Returning
	// null forces the caller to either clear cached state (cold start)
	// or surface a sign-in error (loginRedirect post-mint). Producing
	// a placeholder `tauri-account@unknown` would let `<UserMenu>`
	// display a fake identity for a real but corrupted token.
	const oid = claims.oid ?? claims.sub;
	const tid = claims.tid;
	if (!oid || !tid) return null;
	const username = claims.preferred_username ?? claims.upn ?? `${oid}@unknown`;
	return {
		homeAccountId: `${oid}.${tid}`,
		environment: "login.microsoftonline.com",
		tenantId: tid,
		username,
		localAccountId: oid,
		name: claims.name ?? username,
	};
}

function tokenPreview(token: string): string {
	if (token.length <= 12) return token.slice(0, 4);
	return `${token.slice(0, 8)}...${token.slice(-4)}`;
}

/**
 * Build the structural duck-type that satisfies MsalProvider's
 * mount-time surface. Cast to PublicClientApplication at the boundary
 * the same way mockMsal does.
 *
 * The serverUrl is captured at construction time. AuthGate's bootstrap
 * effect re-runs when serverReady flips, which rebuilds the shim with
 * a fresh URL.
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
	// On hit with valid claims, seed activeAccount so getAllAccounts()
	// returns non-empty and MsalProvider lights up
	// <AuthenticatedTemplate>. On miss or claims-decode failure, fall
	// through; the SPA will render <UnauthenticatedTemplate>.
	const initialToken = await getCachedAccessToken(serverUrl);
	if (initialToken) {
		const claims = decodeJwtClaims(initialToken);
		const account = claims ? makeAccountFromClaims(claims) : null;
		if (account) {
			activeAccount = account;
			cachedToken = initialToken;
		} else {
			console.warn(
				`[tauriMsalShim] cold-start token failed to decode (preview=${tokenPreview(initialToken)}); falling back to sign-in`,
			);
		}
	}

	const instance = {
		// Lifecycle no-ops MsalProvider calls on mount.
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
			auth: {
				clientId,
				authority: `https://login.microsoftonline.com/${tenantId}`,
			},
		}),

		// Account state.
		getAllAccounts: (): AccountInfo[] =>
			activeAccount ? [activeAccount] : [],
		getActiveAccount: (): AccountInfo | null => activeAccount,
		setActiveAccount: (account: AccountInfo | null) => {
			activeAccount = account;
		},

		// Sign-in: route through the system-browser loopback flow.
		// MsalProvider's AuthenticatedTemplate calls this when
		// SignInPrompt fires. Throws if the daemon-returned token cannot
		// be decoded — better than seeding a placeholder account that
		// would put a fake identity in the UI.
		loginRedirect: async (_request?: unknown): Promise<void> => {
			const result = await signInWithEntraDesktop({
				serverUrl,
				tenantId,
				clientId,
				scopes,
			});
			const claims = decodeJwtClaims(result.access_token);
			const account = claims ? makeAccountFromClaims(claims) : null;
			if (!account) {
				const preview = tokenPreview(result.access_token);
				throw new Error(
					`sign_in_with_entra returned a token whose claims could not be decoded (preview=${preview}); refusing to seed a placeholder identity`,
				);
			}
			cachedToken = result.access_token;
			activeAccount = account;
		},

		// Token acquisition. Silent path returns the cached token if
		// present; throws the real `InteractionRequiredAuthError` if not
		// so makeTokenProvider's `instanceof` check passes and the
		// redirect branch fires.
		acquireTokenSilent: async (): Promise<AuthenticationResult> => {
			if (!cachedToken || !activeAccount) {
				throw new InteractionRequiredAuthError(
					"interaction_required",
					"tauri shim has no cached token; full sign-in required",
				);
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

		// Interactive token: route through the same sign-in flow. Real
		// MSAL.js returns void and triggers a navigation; the Tauri flow
		// opens the system browser, awaits the loopback callback,
		// persists tokens, and resolves.
		acquireTokenRedirect: async (_request?: unknown): Promise<void> => {
			await instance.loginRedirect();
		},

		// Sign-out: wipe the daemon-side cache, then forget local state
		// AND navigate, regardless of whether the daemon delete
		// succeeded. A failed daemon delete must never leave the user
		// believing they signed out while the local UI keeps showing
		// them as signed in — that is a SOC 2 / shared-device hazard.
		// Surface the failure via console; a future toast can pick it up.
		logoutRedirect: async (opts?: { postLogoutRedirectUri?: string }) => {
			try {
				await clearDesktopTokens(serverUrl);
			} catch (err) {
				const message = err instanceof Error ? err.message : String(err);
				console.error(
					`[tauriMsalShim] sign-out: daemon delete failed (${message}); wiping local state and navigating anyway`,
				);
			} finally {
				activeAccount = null;
				cachedToken = null;
				window.location.href = opts?.postLogoutRedirectUri ?? "/";
			}
		},
	};

	return instance as unknown as PublicClientApplication;
}
