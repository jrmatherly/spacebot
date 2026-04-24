// Phase 6 Task 6.A.5 — React wrapper that gates the app shell behind
// Entra sign-in. Exposes a five-state machine:
//
//   waiting_for_server — Tauri cold-start: daemon URL not resolved yet.
//                        Defers loadAuthConfig() until ServerProvider
//                        publishes a usable URL.
//   loading            — async bootstrap in-flight (loadAuthConfig +
//                        getMsalInstance + handleRedirectPromise).
//   entra_disabled     — daemon is in static-token mode; render children
//                        directly.
//   unauthenticated    — Entra configured but no active account yet.
//   authenticated      — MSAL has an account, token provider is wired.
//
// Responsibilities:
//   - handleRedirectPromise for return-from-Entra redirect flows
//   - setActiveAccount so MsalProvider has a stable account throughout the tree
//   - setAuthTokenProvider wires a silent-acquisition closure into the
//     api-client so every outbound call can attach a Bearer token
//   - SignInPrompt child renders the interactive sign-in button + A-17
//     "stay signed in on this device" opt-in checkbox

import {
	InteractionRequiredAuthError,
	type AccountInfo,
	type PublicClientApplication,
} from "@azure/msal-browser";
import { MsalProvider } from "@azure/msal-react";
import {
	setAuthTokenProvider,
	type AuthExhaustedDetail,
} from "@spacebot/api-client/client";
import { useEffect, useState, type ReactNode } from "react";
import { useServer } from "@/hooks/useServer";
import { IS_DESKTOP } from "@/platform";
import { getActiveScopes, getMsalInstance, loadAuthConfig } from "./msalConfig";

const TRUST_DEVICE_KEY = "spacebot.auth.trust_device";

type GateState =
	| { kind: "waiting_for_server" }
	| { kind: "loading" }
	| { kind: "entra_disabled" }
	| { kind: "unauthenticated"; msal: PublicClientApplication }
	| { kind: "authenticated"; msal: PublicClientApplication }
	| { kind: "error"; message: string };

export function AuthGate({ children }: { children: ReactNode }) {
	// In desktop mode the daemon URL is async (Tauri command +
	// localStorage reconcile in useServer). Until ServerProvider
	// publishes a non-empty URL, loadAuthConfig() would fetch against
	// an unresolved/stale base. Browser mode is same-origin (relative
	// API base) so the URL gate is a no-op there.
	const { serverUrl } = useServer();
	const serverReady = !IS_DESKTOP || serverUrl.length > 0;
	const [state, setState] = useState<GateState>(
		serverReady ? { kind: "loading" } : { kind: "waiting_for_server" },
	);

	// authedFetch dispatches `spacebot:auth-exhausted` on 401
	// refresh-exhaustion. SSE via fetchEventSource(fetch: authedFetch)
	// inherits the same dispatch. A single window-level listener
	// covers both REST and SSE.
	//
	// TODO: replace console.warn with a toast banner + "Re-sign in"
	// CTA wired to acquireTokenRedirect. Trigger: when the
	// notifications surface lands (tracked as a Phase 7 scope item).
	useEffect(() => {
		const handler = (event: Event) => {
			const detail = (event as CustomEvent<AuthExhaustedDetail>).detail;
			console.warn(
				`[authedFetch] session expired at ${detail.url} (${detail.reason})`,
			);
		};
		window.addEventListener("spacebot:auth-exhausted", handler);
		return () =>
			window.removeEventListener("spacebot:auth-exhausted", handler);
	}, []);

	useEffect(() => {
		if (!serverReady) return;
		// Re-entering the bootstrap (e.g. desktop URL just resolved):
		// flip out of waiting_for_server so the user sees "Signing in…"
		// rather than nothing.
		setState((prev) =>
			prev.kind === "waiting_for_server" ? { kind: "loading" } : prev,
		);
		let cancelled = false;

		(async () => {
			const cfg = await loadAuthConfig();
			if (cancelled) return;
			if (!cfg.entra_enabled) {
				setState({ kind: "entra_disabled" });
				return;
			}
			const result = await getMsalInstance();
			if (cancelled) return;
			if (!result.ok) {
				if (result.reason === "disabled") {
					setState({ kind: "entra_disabled" });
					return;
				}
				// "malformed": /api/auth/config reported entra_enabled: true
				// but omitted one or more identifiers (client_id, authority).
				// Surface this to operators instead of fail-open to
				// entra_disabled, which would mask a real daemon config bug
				// behind a UI that 401s on every API call.
				setState({
					kind: "error",
					message: `Auth bootstrap failed: daemon reported entra_enabled but missing ${result.missing.join(", ")}. Check /api/auth/config response and the daemon's [api.auth.entra] config block.`,
				});
				return;
			}
			const msal = result.instance;

			const redirectResult = await msal.handleRedirectPromise();
			if (cancelled) return;
			const accounts = msal.getAllAccounts();
			const active = redirectResult?.account ?? accounts[0] ?? null;
			if (!active) {
				setState({ kind: "unauthenticated", msal });
				return;
			}

			msal.setActiveAccount(active);
			setAuthTokenProvider(makeTokenProvider(msal, active));
			setState({ kind: "authenticated", msal });
		})().catch((err) => {
			console.error("[AuthGate] init failed:", err);
			if (!cancelled) {
				// Surface the error to operators instead of the legacy
				// "fail open to entra_disabled" behavior, which silently
				// masked tenant misconfigurations behind a 401-loop UI.
				// Keeps the SPA from rendering children that would 401 on
				// every request; operators see a diagnostic banner.
				const message =
					err instanceof Error ? err.message : String(err);
				setState({
					kind: "error",
					message: `Auth bootstrap failed: ${message}. Check the browser console and daemon logs.`,
				});
			}
		});

		return () => {
			cancelled = true;
		};
	}, [serverReady]);

	if (state.kind === "waiting_for_server") {
		return (
			<div
				data-testid="auth-gate-waiting-server"
				role="status"
				aria-live="polite"
			>
				Connecting to Spacebot…
			</div>
		);
	}
	if (state.kind === "loading") {
		return (
			<div data-testid="auth-gate-loading" role="status" aria-live="polite">
				Signing in…
			</div>
		);
	}
	if (state.kind === "error") {
		return (
			<div
				data-testid="auth-gate-error"
				role="alert"
				aria-live="assertive"
				style={{
					padding: "1.5rem",
					margin: "2rem auto",
					maxWidth: "600px",
					border: "1px solid var(--color-danger, #dc2626)",
					borderRadius: "0.5rem",
					background: "var(--color-danger-bg, #fef2f2)",
					color: "var(--color-danger-fg, #991b1b)",
				}}
			>
				<h2 style={{ margin: "0 0 0.75rem 0" }}>
					Sign-in is unavailable
				</h2>
				<p style={{ margin: 0 }}>{state.message}</p>
			</div>
		);
	}
	if (state.kind === "entra_disabled") {
		return <>{children}</>;
	}
	if (state.kind === "unauthenticated") {
		return (
			<MsalProvider instance={state.msal}>
				<SignInPrompt msal={state.msal} />
			</MsalProvider>
		);
	}
	return <MsalProvider instance={state.msal}>{children}</MsalProvider>;
}

/// Builds the closure that `api-client/client.ts` calls on every request
/// that needs a Bearer token. Silent acquisition is the happy path; if
/// MSAL says "user must interact," we kick off a full-page redirect and
/// return a never-resolving Promise so authedFetch (Task 6.B.1) suspends
/// gracefully until the redirect completes and the tab reloads.
function makeTokenProvider(
	msal: PublicClientApplication,
	account: AccountInfo,
): () => Promise<string | null> {
	return async () => {
		const scopes = await getActiveScopes();
		if (scopes.length === 0) return null;
		try {
			const result = await msal.acquireTokenSilent({ scopes, account });
			return result.accessToken;
		} catch (err) {
			if (err instanceof InteractionRequiredAuthError) {
				// Full-page navigation; the returned Promise resolves after
				// the page is gone. Using `never` ensures authedFetch's retry
				// loop suspends rather than racing the navigation.
				void msal.acquireTokenRedirect({ scopes, account });
				return new Promise<string | null>(() => {
					/* never resolves; page navigates away */
				});
			}
			throw err;
		}
	};
}

function SignInPrompt({ msal }: { msal: PublicClientApplication }) {
	// A-17: "Stay signed in on this device" opt-in. Default off; reading
	// the localStorage key here (not from msalConfig) because the checkbox
	// state must reflect the value BEFORE the next MSAL init reads it on
	// reload. The opt-in is browser-only; in Tauri the daemon's secret
	// store handles persistence (Phase 8 PR A) and localStorage is not
	// the cache MSAL would use anyway.
	const [trust, setTrust] = useState(
		window.localStorage.getItem(TRUST_DEVICE_KEY) === "true",
	);

	const onTrustToggle = (checked: boolean) => {
		setTrust(checked);
		if (checked) {
			window.localStorage.setItem(TRUST_DEVICE_KEY, "true");
		} else {
			window.localStorage.removeItem(TRUST_DEVICE_KEY);
		}
	};

	const onSignIn = async () => {
		const scopes = await getActiveScopes();
		await msal.loginRedirect({ scopes });
	};

	const buttonLabel = IS_DESKTOP
		? "Sign in with Microsoft (opens system browser)"
		: "Sign in with Microsoft";
	const ariaLabel = IS_DESKTOP
		? "Sign in with Microsoft Entra ID; opens the system browser to complete sign-in"
		: "Sign in with Microsoft Entra ID";

	return (
		<div data-testid="auth-gate-signin" role="status">
			<button type="button" onClick={onSignIn} aria-label={ariaLabel}>
				{buttonLabel}
			</button>
			{!IS_DESKTOP && (
				<label style={{ display: "block", marginTop: "0.75rem" }}>
					<input
						type="checkbox"
						checked={trust}
						onChange={(e) => onTrustToggle(e.target.checked)}
					/>
					Stay signed in on this device (uses encrypted local storage)
				</label>
			)}
			{IS_DESKTOP && (
				<p
					style={{
						display: "block",
						marginTop: "0.75rem",
						fontSize: "0.875rem",
						color: "var(--color-text-muted, #6b7280)",
					}}
				>
					Sign-in completes in your default browser. Tokens are stored
					in the daemon's encrypted secret store, not in this window.
				</p>
			)}
		</div>
	);
}
