import { useState } from "react";
import type { Visibility } from "./VisibilityChip";

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
	currentVisibility: Visibility;
	teams: { id: string; name: string }[];
	onSubmit: (args: {
		visibility: Visibility;
		sharedWithTeamId: string | null;
	}) => Promise<void>;
	onClose: () => void;
}) {
	const [visibility, setVisibility] = useState<Visibility>(currentVisibility);
	const [teamId, setTeamId] = useState<string>("");
	const [submitting, setSubmitting] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const descriptionId = `share-description-${resourceType}-${resourceId}`;

	const onConfirm = async () => {
		if (visibility === "team" && !teamId) {
			setError("Select a team.");
			return;
		}
		setSubmitting(true);
		try {
			await onSubmit({
				visibility,
				sharedWithTeamId: visibility === "team" ? teamId : null,
			});
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
				Change the visibility of {resourceType} {resourceId}.
			</p>
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
