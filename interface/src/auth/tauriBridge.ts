// Phase 8 Task 8.B.1 — typed wrappers for the three Tauri commands the
// MSAL shim needs (sign-in, cached-token read, sign-out clear).
//
// All three route through `platform.invoke` — the existing host-IPC
// primitive at platform.ts:146 — to honor the module-level rule that
// `@tauri-apps/api` may only be imported from `platform.ts`. Browser
// mode short-circuits cleanly: invoke returns `undefined`, the
// helpers below translate that into the absent-token shape so the
// SPA can fall back to interactive sign-in.
//
// Daemon contract (Phase 8 Task 8.B.0):
//   sign_in_with_entra(server_url, tenant_id, client_id, scopes)
//     → JSON { access_token, expires_in } on success
//     → string error on failure (locked store, browser, etc.)
//   get_cached_access_token(server_url) → Option<String>
//   clear_auth_tokens(server_url) → Result<(), String>

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
 */
export async function getCachedAccessToken(
	serverUrl: string,
): Promise<string | null> {
	if (!IS_DESKTOP) return null;
	const token = await invoke<string | null>("get_cached_access_token", {
		serverUrl,
	});
	return token ?? null;
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
