// Phase 6 Task 6.A.4 — MSAL.js v5 loader + `PublicClientApplication` factory.
//
// Flow at SPA boot:
//   1. `loadAuthConfig()` fetches `/api/auth/config` (unprotected — no bearer
//      token required; see src/api/auth_config.rs). Response is cached in a
//      module-level closure for the tab lifetime.
//   2. `getMsalInstance()` constructs a `PublicClientApplication` using the
//      fetched identifiers. Also cached; subsequent calls return the same
//      instance so every consumer (AuthGate, authedFetch, UserMenu) works
//      against one MSAL state.
//   3. If `entra_enabled` is `false` or the bootstrap fields are missing,
//      `getMsalInstance()` returns `null` so callers can branch to static-
//      token mode without null-pointer surprises.
//
// Amendments applied:
//   - A-16: `cacheLocation: "memoryStorage"` is the canonical default.
//     MSAL types accept the string "memory" but silently fall back to
//     `sessionStorage` at runtime — do NOT use it.
//   - A-17: "Trust this device" opt-in flips cache to `localStorage`
//     (MSAL v4+ AES-GCM-encrypts the cache blob, so this is safe). Default
//     (checkbox unchecked) is memoryStorage, which means the user re-auths
//     on every tab close. Acceptable XSS-mitigation trade-off per
//     research §12 S-C4.

import { PublicClientApplication, type Configuration } from "@azure/msal-browser";
import { getApiBase } from "@spacebot/api-client/client";
import type { AuthConfigResponse } from "@spacebot/api-client/types";

/// localStorage key read by `getMsalInstance()` to decide between
/// `memoryStorage` (default) and `localStorage` caching. Written by the
/// sign-in UI's "stay signed in on this device" checkbox (Task 6.A.6).
const TRUST_DEVICE_KEY = "spacebot.auth.trust_device";

let cachedConfig: AuthConfigResponse | null = null;
let cachedInstance: PublicClientApplication | null = null;
let inflightConfig: Promise<AuthConfigResponse> | null = null;
let inflightInstance: Promise<PublicClientApplication | null> | null = null;

/// Fetches `/api/auth/config` and caches the result for the page lifetime.
/// Concurrent callers share one in-flight request (prevents thundering
/// herd on app boot when multiple components race to know the config).
export async function loadAuthConfig(): Promise<AuthConfigResponse> {
	if (cachedConfig) return cachedConfig;
	if (inflightConfig) return inflightConfig;

	inflightConfig = (async () => {
		const res = await fetch(`${getApiBase()}/auth/config`);
		if (!res.ok) {
			throw new Error(
				`auth-config fetch failed: ${res.status} ${res.statusText}`,
			);
		}
		const data = (await res.json()) as AuthConfigResponse;
		cachedConfig = data;
		return data;
	})();

	try {
		return await inflightConfig;
	} finally {
		inflightConfig = null;
	}
}

/// Returns the singleton `PublicClientApplication`, constructing it on
/// first call. Returns `null` when Entra is not configured on the daemon
/// (static-token deployments) — callers must branch accordingly.
///
/// `await instance.initialize()` is mandatory in MSAL v4+ before any other
/// method call; the caller receives an already-initialized instance.
export async function getMsalInstance(): Promise<PublicClientApplication | null> {
	if (cachedInstance) return cachedInstance;
	if (inflightInstance) return inflightInstance;

	inflightInstance = (async () => {
		const cfg = await loadAuthConfig();
		if (!cfg.entra_enabled || !cfg.client_id || !cfg.authority) {
			return null;
		}

		// Mock mode for local dev / CI. `mockMsal.ts` ships in Task 6.C.5;
		// this branch only fires when VITE_AUTH_MOCK=1, which is set
		// explicitly in dev env vars and never in production builds.
		//
		// The dynamic-import target is intentionally typed as `any` via
		// the ts-expect-error pragma: `./mockMsal` does not exist yet, and
		// writing a fallback .d.ts stub solely to satisfy tsc would leak
		// Task 6.C.5's API shape up-stack. Error narrowing happens at
		// runtime in Task 6.C.5's tests.
		if (import.meta.env.VITE_AUTH_MOCK === "1") {
			// @ts-expect-error Task 6.C.5 creates ./mockMsal; guarded by runtime env flag
			const mod = await import(/* @vite-ignore */ "./mockMsal");
			cachedInstance = (await mod.getMockMsalInstance(
				cfg,
			)) as unknown as PublicClientApplication;
			return cachedInstance;
		}

		const trustThisDevice =
			window.localStorage.getItem(TRUST_DEVICE_KEY) === "true";

		const msalConfig: Configuration = {
			auth: {
				clientId: cfg.client_id,
				authority: cfg.authority,
				redirectUri: `${window.location.origin}/auth/callback`,
				postLogoutRedirectUri: `${window.location.origin}/`,
			},
			cache: {
				// A-16: canonical string is `memoryStorage`. Do not shorten to
				// `memory` — that typechecks but degrades to sessionStorage
				// silently at runtime.
				cacheLocation: trustThisDevice ? "localStorage" : "memoryStorage",
				// MSAL v5 dropped `storeAuthStateInCookie` from CacheOptions
				// (it was a v2/v3 IE-compat legacy path). Nothing to set here.
			},
			system: {
				// MSAL v5 renamed `allowNativeBroker` → `allowPlatformBroker`.
				// Keep it disabled: WAM/MacBroker adds a UX divergence we
				// don't want until we've shipped an explicit native-broker
				// test matrix.
				allowPlatformBroker: false,
			},
		};

		const instance = new PublicClientApplication(msalConfig);
		await instance.initialize();
		cachedInstance = instance;
		return instance;
	})();

	try {
		return await inflightInstance;
	} finally {
		inflightInstance = null;
	}
}

/// Returns the delegated scopes the SPA should request at sign-in (from the
/// daemon's bootstrap config). Empty array when Entra is disabled or
/// `scopes` is missing; callers should treat empty-scopes as "skip MSAL
/// flows" rather than requesting `[]` which MSAL rejects.
export async function getActiveScopes(): Promise<string[]> {
	const cfg = await loadAuthConfig();
	return cfg.scopes ?? [];
}

/// Test-only reset. Drops cached config + instance + in-flight promises
/// so a subsequent `loadAuthConfig()` / `getMsalInstance()` re-fetches.
/// Exported for vitest teardown in Tasks 6.A.5 / 6.C.1.
///
/// NOT exported from `interface/src/auth/index.ts` (when created) — this
/// is deliberately off the happy-path import surface.
export function __resetMsalCaches(): void {
	cachedConfig = null;
	cachedInstance = null;
	inflightConfig = null;
	inflightInstance = null;
}
