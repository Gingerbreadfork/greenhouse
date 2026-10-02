import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';
import pkg from './package.json';

const mock = (name: string) => fileURLToPath(new URL(`./preview/mocks/${name}`, import.meta.url));

// Builds the real UI against fixtures, for design review in a browser.
export default defineConfig({
  plugins: [svelte()],
  define: { __APP_DESCRIPTION__: JSON.stringify(pkg.description) },
  resolve: {
    alias: {
      '@tauri-apps/api/core': mock('core.ts'),
      '@tauri-apps/api/event': mock('event.ts'),
      '@tauri-apps/api/window': mock('window.ts'),
      '@tauri-apps/plugin-dialog': mock('dialog.ts'),
      '@tauri-apps/plugin-opener': mock('opener.ts')
    }
  },
  build: { outDir: 'preview-dist', target: 'esnext', emptyOutDir: true },
  server: { port: 4173, strictPort: true }
});
