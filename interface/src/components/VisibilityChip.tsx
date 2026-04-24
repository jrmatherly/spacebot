export type Visibility = "personal" | "team" | "org";

export function VisibilityChip({
	visibility,
	teamName,
}: {
	visibility: Visibility;
	teamName?: string;
}) {
	const label =
		visibility === "personal"
			? "Personal"
			: visibility === "team"
				? `Team${teamName ? `: ${teamName}` : ""}`
				: visibility === "org"
					? "Org"
					: "Unknown";

	const tone =
		visibility === "personal"
			? "neutral"
			: visibility === "team"
				? "info"
				: visibility === "org"
					? "success"
					: "warning";

	return (
		<span
			data-tone={tone}
			data-testid="visibility-chip"
			className="visibility-chip"
		>
			{label}
		</span>
	);
}
