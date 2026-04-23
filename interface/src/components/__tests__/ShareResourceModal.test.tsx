import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ShareResourceModal } from "../ShareResourceModal";

describe("ShareResourceModal", () => {
	it("submits visibility + team when confirmed", async () => {
		const onSubmit = vi.fn(async () => {});
		render(
			<ShareResourceModal
				resourceType="memory"
				resourceId="m-1"
				currentVisibility="personal"
				teams={[
					{ id: "t1", name: "Platform" },
					{ id: "t2", name: "Sec" },
				]}
				onSubmit={onSubmit}
				onClose={() => {}}
			/>,
		);
		fireEvent.click(screen.getByLabelText(/team/i));
		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "t1" },
		});
		fireEvent.click(screen.getByRole("button", { name: /confirm|share/i }));
		await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled());
		expect(onSubmit).toHaveBeenCalledWith({
			visibility: "team",
			sharedWithTeamId: "t1",
		});
	});

	it("prevents confirming team visibility without a team selection", () => {
		const onSubmit = vi.fn();
		render(
			<ShareResourceModal
				resourceType="memory"
				resourceId="m-1"
				currentVisibility="personal"
				teams={[{ id: "t1", name: "Platform" }]}
				onSubmit={onSubmit}
				onClose={() => {}}
			/>,
		);
		fireEvent.click(screen.getByLabelText(/team/i));
		const confirm = screen.getByRole("button", { name: /confirm|share/i });
		fireEvent.click(confirm);
		expect(onSubmit).not.toHaveBeenCalled();
	});
});
