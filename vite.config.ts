import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  root: 'web/app',
  plugins: [svelte({ configFile: `${import.meta.dirname}/svelte.config.js` })],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  preview: { port: 4173, strictPort: true },
  // The largest chunk is three.js, which only the Universe screen loads.
  build: { outDir: '../../dist/app', emptyOutDir: true, chunkSizeWarningLimit: 600 },
  test: { dir: `${import.meta.dirname}/web`, include: ['**/*.test.ts'], passWithNoTests: true },
});
