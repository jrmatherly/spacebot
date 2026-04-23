// Phase 6 PR C Task 6.C.4 — header user menu with sign-out.
//
// Renders the signed-in principal's name + a Sign out button that
// calls msal.logoutRedirect to clear both the MSAL cache and the
// browser session at the Entra endpoint. Post-logout returns to the
// SPA root (window.location.origin) where AuthGate re-enters the
// "unauthenticated" state and shows the sign-in prompt.
//
// Only renders when there is an active account; in entra_disabled
// mode the MsalProvider is absent so useMsal returns an empty
// accounts array and this component returns null.

import { useMsal } from "@azure/msal-react";

export function UserMenu() {
	const { instance, accounts } = useMsal();
	const account = accounts[0];
	if (!account) return null;

	const onSignOut = async () => {
		await instance.logoutRedirect({
			account,
			postLogoutRedirectUri: window.location.origin,
		});
	};

	return (
		<div
			className="user-menu"
			style={{
				display: "flex",
				alignItems: "center",
				gap: "0.75rem",
				padding: "0.5rem 0.75rem",
			}}
		>
			<span aria-label="Signed in as">
				{account.name ?? account.username}
			</span>
			<button type="button" onClick={onSignOut}>
				Sign out
			</button>
		</div>
	);
}
