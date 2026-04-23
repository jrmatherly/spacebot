/// <reference types="vitest" />
import { defineConfig } from "vitest/config";

// Minimal vitest config for @spacebot/api-client. jsdom env because
// client.ts evaluates `window.__SPACEBOT_BASE_PATH` at module load
// (line 7): in node env that reference throws. The tests themselves
// exercise pure fetch wrappers so the DOM surface area is minimal;
// jsdom just provides a working `window` stub.
export default defineConfig({
	test: {
		environment: "jsdom",
		globals: true,
	},
});
