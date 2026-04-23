import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ShareResourceModal } from "../ShareResourceModal";

describe("ShareResourceModal", () => {
	let consoleErrorSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		// Silence the intentional console.error calls in onSubmit's catch
		// (see C1/C2 remediation) so test output stays clean. We still assert
		// the spy was invoked in the rejection test below.
		consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
	});

	afterEach(() => {
		consoleErrorSpy.mockRestore();
	});

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
		fireEvent.click(screen.getByRole("button", { name: /confirm/i }));
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
		const confirm = screen.getByRole("button", { name: /confirm/i });
		fireEvent.click(confirm);
		expect(onSubmit).not.toHaveBeenCalled();
	});

	it("shows error + keeps modal open + re-enables Confirm when onSubmit rejects with an API error", async () => {
		const onClose = vi.fn();
		const onSubmit = vi.fn(async () => {
			throw new Error("API error 409: /api/resources/memory/m-1/visibility");
		});
		render(
			<ShareResourceModal
				resourceType="memory"
				resourceId="m-1"
				currentVisibility="personal"
				teams={[{ id: "t1", name: "Platform" }]}
				onSubmit={onSubmit}
				onClose={onClose}
			/>,
		);
		fireEvent.click(screen.getByLabelText(/team/i));
		fireEvent.change(screen.getByRole("combobox"), {
			target: { value: "t1" },
		});
		const confirm = screen.getByRole("button", { name: /confirm/i }) as HTMLButtonElement;
		fireEvent.click(confirm);

		// Error message surfaces in the role="alert" region.
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toMatch(/API error 409/);

		// Modal stayed open (onClose not called on rejection path).
		expect(onClose).not.toHaveBeenCalled();

		// Confirm button re-enabled in the finally block.
		expect(confirm.disabled).toBe(false);

		// console.error was called so operators have a stack trace.
		expect(consoleErrorSpy).toHaveBeenCalled();
	});

	it("renders unowned-state hint and refuses submit until a visibility is chosen (currentVisibility=null)", () => {
		const onSubmit = vi.fn();
		render(
			<ShareResourceModal
				resourceType="memory"
				resourceId="m-1"
				currentVisibility={null}
				teams={[{ id: "t1", name: "Platform" }]}
				onSubmit={onSubmit}
				onClose={() => {}}
			/>,
		);
		// Unowned-state hint rendered in a role="note" region so screen
		// readers announce the reason the form is blank.
		const note = screen.getByRole("note");
		expect(note.textContent).toMatch(/no visibility recorded/i);
		// No radio is pre-selected; clicking Confirm without choosing one
		// surfaces the validation message and does not invoke onSubmit.
		fireEvent.click(screen.getByRole("button", { name: /confirm/i }));
		const alert = screen.getByRole("alert");
		expect(alert.textContent).toMatch(/choose a visibility/i);
		expect(onSubmit).not.toHaveBeenCalled();
	});

	it("does not render an alert + does not close on non-API programmer errors (it rethrows so the error boundary handles them)", async () => {
		const onClose = vi.fn();
		// Simulate a programmer-error path: a TypeError from a caller bug.
		// The component's catch narrows to API errors and rethrows others
		// after logging via console.error. In jsdom the rethrow surfaces as
		// an unhandled promise rejection at the vitest test-runner level;
		// we can't observe that rejection directly through a window listener
		// (jsdom batches them post-microtask), so we verify the observable
		// effects: (1) console.error was called, (2) no alert rendered,
		// (3) onClose was not called.
		const onSubmit = vi.fn(async () => {
			throw new TypeError("cannot read properties of undefined");
		});

		// Absorb the expected unhandled rejection so vitest's test-runner
		// catcher does not fail the suite. In jsdom, unhandled promise
		// rejections surface through node's `process` because vitest runs
		// jsdom inside a node worker. `globalThis` is typed by @types/node
		// (via vitest's global augmentation) to carry `process`, so no
		// extra import is needed; we just reach for it here with a narrow
		// assertion to avoid polluting the file with a node/ types import.
		interface NodeLikeProcess {
			on(event: "unhandledRejection", handler: (reason: unknown) => void): void;
			off(event: "unhandledRejection", handler: (reason: unknown) => void): void;
		}
		const proc = (
			globalThis as unknown as { process?: NodeLikeProcess }
		).process;
		const unhandled = vi.fn();
		proc?.on("unhandledRejection", unhandled);

		try {
			render(
				<ShareResourceModal
					resourceType="memory"
					resourceId="m-1"
					currentVisibility="personal"
					teams={[{ id: "t1", name: "Platform" }]}
					onSubmit={onSubmit}
					onClose={onClose}
				/>,
			);
			fireEvent.click(screen.getByLabelText(/team/i));
			fireEvent.change(screen.getByRole("combobox"), {
				target: { value: "t1" },
			});
			fireEvent.click(screen.getByRole("button", { name: /confirm/i }));

			// Wait for the component's log-before-rethrow to land.
			await vi.waitFor(() => expect(consoleErrorSpy).toHaveBeenCalled());

			// The dialog must NOT render an alert for programmer errors —
			// only API errors get the user-facing banner.
			expect(screen.queryByRole("alert")).toBeNull();
			expect(onClose).not.toHaveBeenCalled();

			// The log line is for a non-API error (check the first arg).
			const errorArg = consoleErrorSpy.mock.calls.find((call) =>
				(call[0] as string).includes("non-API error"),
			);
			expect(errorArg).toBeDefined();
		} finally {
			proc?.off("unhandledRejection", unhandled);
		}
	});
});
