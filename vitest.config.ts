import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig, mergeConfig } from 'vitest/config';
import preview from './vite.preview.config';

// The interface is tested against the same mocks the browser preview uses.
export default mergeConfig(
  preview,
  defineConfig({
    plugins: [svelteTesting()],
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.ts'],
      setupFiles: ['./src/test-setup.ts']
    }
  })
);
