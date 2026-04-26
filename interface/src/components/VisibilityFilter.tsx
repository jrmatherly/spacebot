import type { Visibility } from "./VisibilityChip";

// Derived union: the three `Visibility` values plus an "all" catch-all
// for the list filter. Deriving from `Visibility` means a future schema
// addition (e.g., "system") propagates here automatically once the Rust
// enum changes land via `just typegen`. Avoids the hand-duplicated-
// literal drift class PR #110 review finding S1 (type-design-analyzer)
// called out.
export type VisibilityFilterValue = "all" | Visibility;

export function VisibilityFilter({
	value,
	onChange,
}: {
	value: VisibilityFilterValue;
	onChange: (v: VisibilityFilterValue) => void;
}) {
	const options: VisibilityFilterValue[] = ["all", "personal", "team", "org"];
	return (
		<div
			role="radiogroup"
			aria-label="Visibility filter"
			className="inline-flex items-center gap-1 rounded-md border border-app-line bg-app-input p-0.5 text-xs"
		>
			{options.map((v) => {
				const checked = value === v;
				return (
					<label
						key={v}
						className={`cursor-pointer select-none rounded px-2 py-1 transition-colors ${
							checked
								? "bg-app-selected text-ink"
								: "text-ink-faint hover:bg-app-hover hover:text-ink"
						}`}
					>
						<input
							type="radio"
							name="visibility-filter"
							value={v}
							checked={checked}
							onChange={() => onChange(v)}
							className="sr-only"
						/>
						{v[0].toUpperCase() + v.slice(1)}
					</label>
				);
			})}
		</div>
	);
}
