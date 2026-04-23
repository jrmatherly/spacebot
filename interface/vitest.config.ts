/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Vitest config for Phase 6 SPA auth tests. Kept minimal: jsdom environment
// for DOM-backed hooks (useMsal, useMe), globals: true so vitest `expect`
// resolves without per-file imports, setup file wires
// `@testing-library/jest-dom` matchers.
export default defineConfig({
	plugins: [react()],
	test: {
		environment: "jsdom",
		globals: true,
		setupFiles: ["./src/test/setup.ts"],
	},
});
