export type VisibilityFilterValue = "all" | "personal" | "team" | "org";

export function VisibilityFilter({
	value,
	onChange,
}: {
	value: VisibilityFilterValue;
	onChange: (v: VisibilityFilterValue) => void;
}) {
	const options: VisibilityFilterValue[] = ["all", "personal", "team", "org"];
	return (
		<div role="radiogroup" aria-label="Visibility filter">
			{options.map((v) => (
				<label key={v}>
					<input
						type="radio"
						name="visibility-filter"
						value={v}
						checked={value === v}
						onChange={() => onChange(v)}
					/>
					{v[0].toUpperCase() + v.slice(1)}
				</label>
			))}
		</div>
	);
}
