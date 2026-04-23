/// <reference types="vitest" />
import path from "node:path";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Vitest config for Phase 6 SPA auth tests. Kept minimal: jsdom environment
// for DOM-backed hooks (useMsal, useMe), globals: true so vitest `expect`
// resolves without per-file imports, setup file wires
// `@testing-library/jest-dom` matchers.
//
// The `@` alias must mirror vite.config.ts so route tests (which import
// route components that use `@/components/*` paths) resolve identically in
// vite and vitest.
export default defineConfig({
	plugins: [react()],
	resolve: {
		alias: {
			"@": path.resolve(__dirname, "src"),
		},
	},
	test: {
		environment: "jsdom",
		globals: true,
		setupFiles: ["./src/test/setup.ts"],
	},
});
