import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig, type Plugin } from 'vite';

/** A module script, or one marked crossorigin, does not run from file://, so the page gets a classic deferred one. */
const classicScript = (): Plugin => ({
  name: 'rostery-classic-script',
  enforce: 'post',
  transformIndexHtml: (html) => html.replace(/<script type="module" crossorigin/g, '<script defer').replace(/ crossorigin/g, ''),
});

// The Share Site: exactly index.html, assets/share.js and assets/share.css, with relative paths, so it works
// from a folder on disk, a subpath or a domain root.
export default defineConfig({
  root: 'web/share',
  base: './',
  plugins: [svelte({ configFile: `${import.meta.dirname}/svelte.config.js` }), classicScript()],
  build: {
    outDir: '../../dist/share',
    emptyOutDir: true,
    modulePreload: false,
    cssCodeSplit: false,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        format: 'iife',
        inlineDynamicImports: true,
        entryFileNames: 'assets/share.js',
        assetFileNames: 'assets/share[extname]',
      },
    },
  },
});
