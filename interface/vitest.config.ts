/// <reference types="vitest" />
import path from "node:path";
import { createRequire } from "node:module";
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Resolve absolute paths to interface's React installation. Used by
// the singleton-react plugin below to force-resolve every `react` and
// `react-dom` import to the SAME on-disk module, no matter which
// directory the importer lives in.
const interfaceRequire = createRequire(import.meta.url);
const REACT_PATH = interfaceRequire.resolve("react");
const REACT_DOM_PATH = interfaceRequire.resolve("react-dom");
const REACT_DOM_CLIENT_PATH = interfaceRequire.resolve("react-dom/client");
const REACT_JSX_RUNTIME_PATH = interfaceRequire.resolve("react/jsx-runtime");
const REACT_JSX_DEV_RUNTIME_PATH = interfaceRequire.resolve(
	"react/jsx-dev-runtime",
);

// Vite plugin that intercepts every React import (including imports
// from inside spaceui-installed @radix-ui/* packages). Vite's
// `resolve.alias` and `dedupe` aren't sufficient here because Node's
// module resolution finds spaceui's React copy BEFORE the alias gets
// a chance to redirect — the alias only fires for bare specifiers
// resolving from the project root, not from deeply-nested workspace
// packages whose own ancestor `node_modules` contains React.
//
// Background: spaceui has its own bun.lock that pulls React via
// @storybook/react@10. spaceui/node_modules/.bun/react@19.2.5 is real
// and Node-resolvable. Without this plugin, useMemo/useRef in
// @radix-ui/* (transitive of @spacedrive/primitives) come from
// spaceui's React copy, while react-dom comes from interface's copy
// → currentDispatcher null → null deref on every hook.
const reactSingletonPlugin = {
	name: "vitest-react-singleton",
	enforce: "pre" as const,
	resolveId(id: string) {
		if (id === "react") return REACT_PATH;
		if (id === "react-dom") return REACT_DOM_PATH;
		if (id === "react-dom/client") return REACT_DOM_CLIENT_PATH;
		if (id === "react/jsx-runtime") return REACT_JSX_RUNTIME_PATH;
		if (id === "react/jsx-dev-runtime") return REACT_JSX_DEV_RUNTIME_PATH;
		return null;
	},
};

// Vitest config for SPA tests. jsdom environment for DOM-backed hooks
// (useMsal, useMe) and route components (AgentMemories, AgentTasks,
// Wiki, etc.), globals: true so vitest `expect` resolves without
// per-file imports, setup file wires `@testing-library/jest-dom`
// matchers plus the jsdom shims route-level tests need.
//
// The `@` alias must mirror vite.config.ts so route tests (which import
// route components that use `@/components/*` paths) resolve identically
// in vite and vitest.
//
export default defineConfig({
	plugins: [reactSingletonPlugin, react()],
	resolve: {
		alias: {
			"@": path.resolve(__dirname, "src"),
		},
		dedupe: ["react", "react-dom"],
	},
	test: {
		environment: "jsdom",
		globals: true,
		setupFiles: ["./src/test/setup.ts"],
		// vitest 4 + Vite 7: imports inside transitive deps under
		// `spaceui/node_modules/` are loaded via Node.js (not Vite), so
		// `resolve.alias` doesn't intercept them. `test.alias` IS applied
		// at the vitest module-runner level, after Node's resolver picks
		// the wrong React. These aliases redirect every React-shaped
		// specifier to interface's React copy.
		alias: [
			{ find: /^react$/, replacement: REACT_PATH },
			{ find: /^react-dom$/, replacement: REACT_DOM_PATH },
			{ find: /^react-dom\/client$/, replacement: REACT_DOM_CLIENT_PATH },
			{ find: /^react\/jsx-runtime$/, replacement: REACT_JSX_RUNTIME_PATH },
			{
				find: /^react\/jsx-dev-runtime$/,
				replacement: REACT_JSX_DEV_RUNTIME_PATH,
			},
		],
		// Per vitest 4 docs (server.deps.inline): "If true, every
		// dependency will be inlined." This is required (not optional)
		// for our case because @radix-ui/* lives in spaceui/node_modules
		// and imports React via Node's resolver (which finds spaceui's
		// React copy, NOT interface's). Inlining puts ALL deps through
		// Vite's module graph where the reactSingletonPlugin above
		// intercepts every `react` / `react-dom` import.
		server: {
			deps: {
				inline: true,
			},
		},
		// vitest 4 renamed `deps.optimizer.web` → `deps.optimizer.client`.
		// Pre-bundles React + ReactDOM with esbuild so even imports that
		// somehow escape the singleton plugin still hit a single bundled
		// React. Belt-and-suspenders for the singleton invariant.
		deps: {
			optimizer: {
				client: {
					include: [
						"react",
						"react-dom",
						"react-dom/client",
						"react/jsx-runtime",
						"react/jsx-dev-runtime",
					],
				},
			},
		},
	},
});
