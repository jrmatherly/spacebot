import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
	VisibilityFilter,
	type VisibilityFilterValue,
} from "../VisibilityFilter";

describe("VisibilityFilter", () => {
	it("renders all options including 'all'", () => {
		render(<VisibilityFilter value="all" onChange={() => {}} />);
		expect(screen.getByLabelText(/all/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/personal/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/team/i)).toBeInTheDocument();
		expect(screen.getByLabelText(/org/i)).toBeInTheDocument();
	});

	it("invokes onChange with new value when clicked", () => {
		const onChange = vi.fn();
		render(<VisibilityFilter value="all" onChange={onChange} />);
		fireEvent.click(screen.getByLabelText(/personal/i));
		expect(onChange).toHaveBeenCalledWith(
			"personal" satisfies VisibilityFilterValue,
		);
	});
});
