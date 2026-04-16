import clsx from "clsx";
import type { ReactNode } from "react";

export interface LoadingDotProps {
	/**
	 * Label rendered next to the dot. Most call sites say "Loading..." or
	 * similar; pass the verbatim copy to preserve the exact user-visible text.
	 */
	children?: ReactNode;
	/**
	 * Extra classes merged onto the wrapper. Use to adjust typography
	 * (`text-sm`, `text-ink-faint`, etc.) without rebuilding the component.
	 */
	className?: string;
	/**
	 * Extra classes merged onto the dot element itself. Useful for tuning
	 * color (`bg-status-warning`) or size for a specific surface.
	 */
	dotClassName?: string;
}

/**
 * Accent-colored pulsing dot with an inline label. The canonical Spacebot
 * "loading..." indicator, consolidated from 26 inline copies across the
 * interface.
 */
export function LoadingDot({ children, className, dotClassName }: LoadingDotProps) {
	return (
		<div className={clsx("flex items-center gap-2 text-ink-dull", className)}>
			<div
				className={clsx("h-2 w-2 animate-pulse rounded-full bg-accent", dotClassName)}
			/>
			{children}
		</div>
	);
}
