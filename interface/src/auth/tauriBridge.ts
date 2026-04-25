// Typed wrappers for the three Tauri commands the MSAL shim needs:
// sign-in, cached-token read, sign-out clear.
//
// All three route through `platform.invoke` (the host-IPC primitive
// re-exported from `@/platform`) to honor the module-level rule that
// `@tauri-apps/api` may only be imported from `platform.ts`. Browser
// mode short-circuits cleanly via the `IS_DESKTOP` guards.
//
// Daemon command contract:
//   sign_in_with_entra(serverUrl, tenantId, clientId, scopes)
//     → JSON { access_token, expires_in } on success (snake_case fields
//       mirror the Rust struct's serde shape)
//     → string error on failure (locked store, browser failure, etc.)
//   get_cached_access_token(serverUrl) → Option<String>
//   clear_auth_tokens(serverUrl) → Result<(), String>
//
// Naming convention: TS-side IPC arg interfaces use camelCase (idiomatic
// JS); the result shape uses snake_case to match the Rust struct's
// serde fields verbatim. The wrapper at the call site is the single
// translation point — do not "fix" one side in isolation.

import { IS_DESKTOP, invoke } from "@/platform";

export interface SignInArgs {
	serverUrl: string;
	tenantId: string;
	clientId: string;
	scopes: string[];
}

export interface SignInResult {
	access_token: string;
	expires_in: number;
}

/**
 * Drive the system-browser SSO flow through the Tauri host and persist
 * the resulting tokens via the daemon's loopback-gated secret store.
 * Throws if not running under Tauri.
 */
export async function signInWithEntraDesktop(
	args: SignInArgs,
): Promise<SignInResult> {
	if (!IS_DESKTOP) {
		throw new Error(
			"signInWithEntraDesktop called in browser mode; check IS_DESKTOP before invoking",
		);
	}
	const result = await invoke<SignInResult>("sign_in_with_entra", {
		serverUrl: args.serverUrl,
		tenantId: args.tenantId,
		clientId: args.clientId,
		scopes: args.scopes,
	});
	if (!result) {
		throw new Error(
			"sign_in_with_entra returned undefined; Tauri command may not be registered",
		);
	}
	return result;
}

/**
 * Read the cached access token persisted by an earlier sign-in via the
 * daemon's `GET /api/desktop/tokens` endpoint. Returns `null` whenever
 * the SPA should fall back to interactive sign-in: not in Tauri, no
 * cached token, daemon unreachable, daemon locked, parse failure.
 *
 * `null` and `undefined` are NOT interchangeable. Under Tauri,
 * `invoke` resolving to `undefined` means the command itself was not
 * registered — a deployment bug, not a "no token" condition. We log
 * loudly and return null to keep the SPA from looping silently, but
 * an operator looking at the console sees the deployment regression.
 */
export async function getCachedAccessToken(
	serverUrl: string,
): Promise<string | null> {
	if (!IS_DESKTOP) return null;
	const token = await invoke<string | null>("get_cached_access_token", {
		serverUrl,
	});
	if (token === undefined) {
		console.error(
			"[tauriBridge] get_cached_access_token returned undefined under Tauri; the command may not be registered in src-tauri/main.rs",
		);
		return null;
	}
	return token;
}

/**
 * Wipe both `entra_access_token` and `entra_refresh_token` from the
 * daemon's secret store via `DELETE /api/desktop/tokens`. Used on
 * sign-out. Throws on failure so the SPA can show an error; locked-
 * store surfaces with a user-actionable message.
 */
export async function clearDesktopTokens(serverUrl: string): Promise<void> {
	if (!IS_DESKTOP) return;
	await invoke<void>("clear_auth_tokens", { serverUrl });
}
