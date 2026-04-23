import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { VisibilityChip, type Visibility } from "../VisibilityChip";

describe("VisibilityChip", () => {
	it("renders 'Personal' label", () => {
		render(<VisibilityChip visibility="personal" />);
		expect(screen.getByText(/personal/i)).toBeInTheDocument();
	});

	it("renders team name when team-scoped", () => {
		render(<VisibilityChip visibility="team" teamName="Platform" />);
		expect(screen.getByText(/platform/i)).toBeInTheDocument();
	});

	it("renders 'Org' for org visibility", () => {
		render(<VisibilityChip visibility="org" />);
		expect(screen.getByText(/org/i)).toBeInTheDocument();
	});

	it("handles unknown visibility gracefully", () => {
		render(<VisibilityChip visibility={"mystery" as Visibility} />);
		expect(screen.getByText(/unknown/i)).toBeInTheDocument();
	});
});
