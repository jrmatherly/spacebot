// Vitest setup. Runs once per test worker before any test file.
// Registers `@testing-library/jest-dom` matchers onto vitest's `expect`,
// so assertions like `expect(el).toBeInTheDocument()` resolve.

import "@testing-library/jest-dom/vitest";

// jsdom ships without ResizeObserver, but @tanstack/react-virtual and
// several SpaceUI primitives observe their scroll container to compute
// virtual row sizes. The shim fires one synchronous entry with a
// desktop-sized rect so the virtualizer sees a non-zero container and
// produces its first page of virtual items. Real layout is still not
// measured (virtual items size from `estimateSize` instead); the
// observer only needs to fire once so the virtualizer wakes up from
// its initial "no measurement received" state.
if (typeof globalThis.ResizeObserver === "undefined") {
	globalThis.ResizeObserver = class {
		callback: ResizeObserverCallback;
		constructor(cb: ResizeObserverCallback) {
			this.callback = cb;
		}
		observe(target: Element): void {
			const entry = {
				target,
				contentRect: target.getBoundingClientRect(),
				borderBoxSize: [
					{ inlineSize: 1024, blockSize: 768 } as ResizeObserverSize,
				],
				contentBoxSize: [
					{ inlineSize: 1024, blockSize: 768 } as ResizeObserverSize,
				],
				devicePixelContentBoxSize: [
					{ inlineSize: 1024, blockSize: 768 } as ResizeObserverSize,
				],
			} as unknown as ResizeObserverEntry;
			this.callback([entry], this as unknown as ResizeObserver);
		}
		unobserve(): void {}
		disconnect(): void {}
	} as unknown as typeof ResizeObserver;
}

// window.scrollTo is called by TanStack Router on every route transition
// (see @tanstack/router-core/scroll-restoration.ts). jsdom ships a
// "not-implemented" stub that logs a noisy error on every call; replace
// it with a proper no-op so test output stays clean.
if (typeof window !== "undefined") {
	window.scrollTo = (): void => {};
	window.HTMLElement.prototype.scrollTo = function scrollTo(): void {};
}

// @tanstack/react-virtual reads `getBoundingClientRect` off the scroll
// container. jsdom returns a 0x0 rect for every element, so the
// virtualizer produces `getVirtualItems() === []` and no rows render.
// Shim returns a desktop-sized rect; real layout is still not measured
// (jsdom cannot) but the virtualizer produces enough items for list
// assertions to see the first page of data.
Object.defineProperty(Element.prototype, "getBoundingClientRect", {
	configurable: true,
	value: function getBoundingClientRect() {
		return {
			width: 1024,
			height: 768,
			top: 0,
			left: 0,
			right: 1024,
			bottom: 768,
			x: 0,
			y: 0,
			toJSON: () => ({}),
		};
	},
});
