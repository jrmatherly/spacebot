// React wrapper that gates the app shell behind Entra sign-in. Exposes
// a six-state machine:
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
//   error              — bootstrap failed; show a diagnostic banner
//                        instead of fail-open to entra_disabled.
//
// Responsibilities:
//   - handleRedirectPromise for return-from-Entra redirect flows
//   - setActiveAccount so MsalProvider has a stable account throughout the tree
//   - setAuthTokenProvider wires a silent-acquisition closure into the
//     api-client so every outbound call can attach a Bearer token
//   - SignInPrompt child renders the interactive sign-in button (and a
//     "stay signed in on this device" opt-in checkbox in browser mode)

import {
	InteractionRequiredAuthError,
	type AccountInfo,
	type PublicClientApplication,
} from "@azure/msal-browser";
import { MsalProvider } from "@azure/msal-react";
import { Button, CheckBox } from "@spacedrive/primitives";
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
	// notifications surface lands.
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
			<AuthGateStatus testid="auth-gate-waiting-server">
				Connecting to Spacebot…
			</AuthGateStatus>
		);
	}
	if (state.kind === "loading") {
		return (
			<AuthGateStatus testid="auth-gate-loading">
				Signing in…
			</AuthGateStatus>
		);
	}
	if (state.kind === "error") {
		return (
			<div
				data-testid="auth-gate-error"
				role="alert"
				aria-live="assertive"
				className="flex h-screen w-full flex-col items-center justify-center bg-app overflow-hidden"
			>
				<div className="flex w-full max-w-lg flex-col gap-4 rounded-lg border border-red-500/40 bg-red-950/40 p-6 mx-6">
					<h2 className="font-plex text-lg font-semibold text-red-200 m-0">
						Sign-in is unavailable
					</h2>
					<p className="text-sm text-red-100/90 m-0">
						{state.message}
					</p>
				</div>
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
	if (state.kind === "authenticated") {
		return <MsalProvider instance={state.msal}>{children}</MsalProvider>;
	}
	// Exhaustiveness guard: a new GateState variant added without
	// updating this render chain becomes a TypeScript compile error
	// (assignment to `never`), not a silent default-case render.
	const _exhaustive: never = state;
	return _exhaustive;
}

/// Builds the closure that `api-client/client.ts` calls on every request
/// that needs a Bearer token. Silent acquisition is the happy path; if
/// MSAL says "user must interact," we kick off a full-page redirect and
/// return a never-resolving Promise so authedFetch suspends gracefully
/// until the redirect completes and the tab reloads.
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
				// Kick off the redirect. Real MSAL navigates the page;
				// the Tauri shim opens the system browser. Both resolve
				// to a "page is gone or about to be" state, so we
				// return a never-resolving Promise and let authedFetch
				// suspend. If the redirect itself fails (locked store,
				// daemon unreachable in Tauri mode), surface it via
				// `spacebot:auth-exhausted` so the listener at the top
				// of AuthGate logs it and a future toast can pick it up
				// — never silently leave authedFetch hanging.
				msal.acquireTokenRedirect({ scopes, account }).catch((redirectErr) => {
					const message =
						redirectErr instanceof Error
							? redirectErr.message
							: String(redirectErr);
					console.error(
						`[AuthGate] acquireTokenRedirect failed: ${message}`,
					);
					window.dispatchEvent(
						new CustomEvent<AuthExhaustedDetail>(
							"spacebot:auth-exhausted",
							{
								detail: {
									url: "(internal:acquireTokenRedirect)",
									reason: "refresh_failed",
								},
							},
						),
					);
				});
				return new Promise<string | null>(() => {
					/* never resolves; page navigates away or the
					 * auth-exhausted dispatch above is the user's
					 * recovery path. */
				});
			}
			throw err;
		}
	};
}

function SignInPrompt({ msal }: { msal: PublicClientApplication }) {
	// "Stay signed in on this device" opt-in. Default off; reading the
	// localStorage key here (not from msalConfig) because the checkbox
	// state must reflect the value BEFORE the next MSAL init reads it
	// on reload. The opt-in is browser-only; in Tauri the daemon's
	// secret store handles persistence and localStorage is not the
	// cache MSAL would use anyway.
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
		<div
			data-testid="auth-gate-signin"
			role="status"
			className="flex h-screen w-full flex-col items-center justify-center bg-app overflow-hidden"
		>
			<div className="flex w-full max-w-md flex-col items-center gap-8 px-6">
				<div className="flex flex-col items-center gap-3">
					<div className="relative h-[160px] w-[160px]">
						<img
							src="/ball.png"
							alt="Spacebot"
							className="h-full w-full object-contain"
						/>
					</div>
					<h1 className="font-plex text-xl font-semibold text-ink">
						Sign in to Spacebot
					</h1>
					<p className="text-center text-sm text-ink-dull">
						This Spacebot instance is protected by Microsoft Entra
						ID. Sign in with your work or school account to
						continue.
					</p>
				</div>

				<div className="flex w-full flex-col gap-4">
					<Button
						type="button"
						onClick={onSignIn}
						aria-label={ariaLabel}
						data-testid="auth-gate-signin-button"
						size="lg"
						variant="accent"
						className="w-full bg-[hsl(282,70%,57%)] text-white shadow hover:bg-[hsl(282,70%,50%)] hover:text-white border-transparent"
					>
						{buttonLabel}
					</Button>

					{!IS_DESKTOP && (
						<label className="flex items-start gap-2 text-sm text-ink-dull cursor-pointer select-none">
							<CheckBox
								checked={trust}
								onChange={(e) =>
									onTrustToggle(e.target.checked)
								}
								className="mt-0.5 mr-0! float-none!"
							/>
							<span>
								Stay signed in on this device
								<span className="block text-xs text-ink-faint">
									Uses encrypted local storage. Leave
									unchecked on shared devices.
								</span>
							</span>
						</label>
					)}

					{IS_DESKTOP && (
						<p className="text-sm text-ink-faint">
							Sign-in completes in your default browser. Tokens
							are stored in the daemon's encrypted secret store,
							not in this window.
						</p>
					)}
				</div>

				<p className="text-center text-xs text-ink-faint">
					Tokens are issued by your tenant and scoped to this Spacebot
					instance only.
				</p>
			</div>
		</div>
	);
}

/// Centered full-screen status card for the transitional auth states
/// (waiting_for_server, loading). Mirrors the visual frame of
/// `SignInPrompt` so users see one continuous gate surface across
/// state transitions, not a flash of unstyled text.
function AuthGateStatus({
	children,
	testid,
}: {
	children: ReactNode;
	testid: string;
}) {
	return (
		<div
			data-testid={testid}
			role="status"
			aria-live="polite"
			className="flex h-screen w-full flex-col items-center justify-center bg-app overflow-hidden"
		>
			<div className="flex w-full max-w-md flex-col items-center gap-6 px-6">
				<div className="relative h-[120px] w-[120px]">
					<img
						src="/ball.png"
						alt="Spacebot"
						className="h-full w-full object-contain opacity-80"
					/>
				</div>
				<div className="flex items-center gap-3 text-ink-dull">
					<span
						aria-hidden="true"
						className="size-3 rounded-full bg-accent animate-pulse"
					/>
					<span className="text-sm">{children}</span>
				</div>
			</div>
		</div>
	);
}
