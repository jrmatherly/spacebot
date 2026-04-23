// Vitest setup — runs once per test worker before any test file.
// Registers `@testing-library/jest-dom` matchers onto vitest's `expect`,
// so assertions like `expect(el).toBeInTheDocument()` resolve.

import "@testing-library/jest-dom/vitest";
