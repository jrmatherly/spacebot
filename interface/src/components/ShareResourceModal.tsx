import { useState } from "react";
import type { Visibility } from "./VisibilityChip";

/**
 * Discriminated-union payload passed to `onSubmit`. Makes the previously
 * illegal pair `{ visibility: "personal", sharedWithTeamId: "t1" }`
 * unrepresentable at the type level — "team" is the only branch that
 * carries a team id. Replaces the old `{ visibility: Visibility;
 * sharedWithTeamId: string | null }` product type per PR #110 review
 * finding I4 (type-design-analyzer).
 */
export type ShareSubmitArgs =
	| { visibility: "team"; sharedWithTeamId: string }
	| { visibility: "personal" | "org" };

export function ShareResourceModal({
	resourceType,
	resourceId,
	currentVisibility,
	teams,
	onSubmit,
	onClose,
}: {
	resourceType: string;
	resourceId: string;
	/// `null` when the resource has no recorded visibility (the
	/// no-auto-broadening policy emits `null` for unowned rows). The
	/// modal renders a neutral "Choose a visibility" header in that
	/// case with no radio pre-selected, so the user opts in to a
	/// scope rather than having "Personal" pre-chosen on their behalf.
	currentVisibility: Visibility | null;
	teams: { id: string; name: string }[];
	onSubmit: (args: ShareSubmitArgs) => Promise<void>;
	onClose: () => void;
}) {
	// Initial radio selection matches the current visibility; unowned
	// resources start with no radio selected so the user makes a
	// deliberate first choice.
	const [visibility, setVisibility] = useState<Visibility | null>(
		currentVisibility,
	);
	const [teamId, setTeamId] = useState<string>("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const descriptionId = `share-description-${resourceType}-${resourceId}`;

	const onConfirm = async () => {
		// Build the payload first so TypeScript exhaustiveness narrowing
		// handles the personal/org/team split. The old code constructed an
		// object where `sharedWithTeamId: visibility === "team" ? teamId :
		// null` embedded the invariant in a runtime expression; the
		// discriminated union below moves that invariant into the type.
		if (visibility === null) {
			setError("Choose a visibility.");
			return;
		}
		let args: ShareSubmitArgs;
		if (visibility === "team") {
			if (!teamId) {
				setError("Select a team.");
				return;
			}
			args = { visibility: "team", sharedWithTeamId: teamId };
		} else {
			args = { visibility };
		}

		setSubmitting(true);
		try {
			await onSubmit(args);
			onClose();
		} catch (e) {
			// Narrow to API errors (authedFetch throws `Error("API error ...")`).
			// Programmer errors (TypeError, unmount-race, missing provider) are
			// rethrown so the React error boundary sees them. Log the full
			// Error before narrowing so operators have a stack trace in the
			// browser console even when only a human-readable message reaches
			// the dialog.
			const isApiError = e instanceof Error && e.message.startsWith("API error");
			if (!isApiError) {
				console.error("ShareResourceModal: non-API error in onSubmit", e);
				throw e;
			}
			console.error("ShareResourceModal: share submit failed", e);
			setError(e.message);
		} finally {
			setSubmitting(false);
		}
	};

	return (
		<div
			role="dialog"
			aria-labelledby="share-title"
			aria-describedby={descriptionId}
		>
			<h2 id="share-title">Share {resourceType}</h2>
			<p id={descriptionId} className="sr-only">
				{currentVisibility === null
					? `This ${resourceType} has no recorded visibility; choose one to claim ownership.`
					: `Change the visibility of ${resourceType} ${resourceId}.`}
			</p>
			{currentVisibility === null && (
				<p role="note" className="text-tiny text-ink-faint">
					No visibility recorded yet. Choose one to claim ownership.
				</p>
			)}
			<fieldset>
				<legend>Visibility</legend>
				{(["personal", "team", "org"] as Visibility[]).map((v) => (
					<label key={v}>
						<input
							type="radio"
							name="vis"
							value={v}
							checked={visibility === v}
							onChange={() => setVisibility(v)}
						/>
						{v[0].toUpperCase() + v.slice(1)}
					</label>
				))}
			</fieldset>
			{visibility === "team" && (
				<select
					aria-label="Team"
					value={teamId}
					onChange={(e) => setTeamId(e.target.value)}
				>
					<option value="">(select a team)</option>
					{teams.map((t) => (
						<option key={t.id} value={t.id}>
							{t.name}
						</option>
					))}
				</select>
			)}
			{error && <p role="alert">{error}</p>}
			<button type="button" onClick={onClose}>
				Cancel
			</button>
			<button type="button" onClick={onConfirm} disabled={submitting}>
				Confirm
			</button>
		</div>
	);
}
