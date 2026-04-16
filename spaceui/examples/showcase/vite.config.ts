import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // Force a single React copy. Bun's hoister keeps two majors (18 + 19) in
    // node_modules because Storybook 8.6 satisfies its `^19.0.0-beta` peer with
    // 19.x while showcase pins React 18. Without dedupe, Radix loads its own
    // React copy and `useRef` returns null. Same fix pattern as interface/.
    dedupe: ['react', 'react-dom'],
    alias: {
      '@spacedrive/tokens': path.resolve(__dirname, '../../packages/tokens/src'),
      '@spacedrive/primitives': path.resolve(__dirname, '../../packages/primitives/src'),
      '@spacedrive/forms': path.resolve(__dirname, '../../packages/forms/src'),
      '@spacedrive/ai': path.resolve(__dirname, '../../packages/ai/src'),
      '@spacedrive/explorer': path.resolve(__dirname, '../../packages/explorer/src'),
    },
  },
  server: {
    port: 19850,
  },
})
