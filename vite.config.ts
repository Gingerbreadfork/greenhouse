import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import pkg from './package.json';

export default defineConfig({
  plugins: [svelte()],
  define: { __APP_DESCRIPTION__: JSON.stringify(pkg.description) },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] }
  },
  build: { target: 'esnext', chunkSizeWarningLimit: 1200 }
});
