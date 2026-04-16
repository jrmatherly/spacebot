import { fileURLToPath } from 'node:url';
import path from 'node:path';
import type { StorybookConfig } from '@storybook/react-vite';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const config: StorybookConfig = {
  stories: [
    '../packages/**/*.mdx',
    '../packages/**/*.stories.@(js|jsx|mjs|ts|tsx)',
  ],
  addons: [
    '@chromatic-com/storybook',
    '@storybook/addon-themes',
  ],
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  viteFinal: async (config) => {
    // Add Tailwind CSS v4 Vite plugin
    const tailwindcss = (await import('@tailwindcss/vite')).default;
    config.plugins = [...(config.plugins || []), tailwindcss()];

    // Pin React/ReactDOM to the storybook workspace's own symlinks AND
    // bypass @spacedrive/tokens' exports map for Tailwind v4.
    //
    // Two reasons we use the regex array form (not the object/record form):
    // 1. Vite's record-form aliases are prefix matches — `'react-dom'` would
    //    overshadow `'react-dom/test-utils'`, sending all subpaths to
    //    react-dom/index.js. Regex `/^react-dom$/` matches only the bare
    //    specifier.
    // 2. The order is honored: more-specific patterns first, bare last.
    //
    // bun's content-addressed `.bun/` store keeps multiple React patches
    // side by side. Storybook's preview chunks live at
    // `.bun/@storybook+react@.../...` with no react symlink in their own
    // node_modules; without these aliases Vite walks up and either can't
    // pick a copy (404 on the chunk) or picks the wrong one (`module
    // .../react/index.js does not provide an export named 'default'`).
    // Same fix pattern as interface/vite.config.ts.
    const reactRoot = path.resolve(__dirname, 'node_modules/react');
    const reactDomRoot = path.resolve(__dirname, 'node_modules/react-dom');
    const tokensRoot = path.resolve(__dirname, '../packages/tokens/src/css');

    config.resolve = config.resolve ?? {};
    const existingAlias = config.resolve.alias;
    const existingAliasArray = Array.isArray(existingAlias)
      ? existingAlias
      : existingAlias
        ? Object.entries(existingAlias).map(([find, replacement]) => ({
            find,
            replacement: replacement as string,
          }))
        : [];

    config.resolve.alias = [
      // React: regex aliases prevent prefix overshadowing.
      { find: /^react$/, replacement: path.join(reactRoot, 'index.js') },
      { find: /^react\/jsx-runtime$/, replacement: path.join(reactRoot, 'jsx-runtime.js') },
      { find: /^react\/jsx-dev-runtime$/, replacement: path.join(reactRoot, 'jsx-dev-runtime.js') },
      { find: /^react-dom$/, replacement: path.join(reactDomRoot, 'index.js') },
      { find: /^react-dom\/client$/, replacement: path.join(reactDomRoot, 'client.js') },
      { find: /^react-dom\/test-utils$/, replacement: path.join(reactDomRoot, 'test-utils.js') },

      // Tokens: bypass the package exports map for Tailwind v4's
      // enhanced-resolve, which doesn't honor it for CSS @import.
      { find: '@spacedrive/tokens/src/css/theme.css', replacement: path.join(tokensRoot, 'theme.css') },
      { find: '@spacedrive/tokens/src/css/base.css', replacement: path.join(tokensRoot, 'base.css') },

      // Preserve any aliases Storybook already set.
      ...existingAliasArray,
    ];

    config.resolve.dedupe = Array.from(
      new Set([...(config.resolve.dedupe ?? []), 'react', 'react-dom']),
    );

    return config;
  },
};

export default config;
