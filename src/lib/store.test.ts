import { beforeAll, describe, expect, it } from 'vitest';
import { store } from './store.svelte';

beforeAll(() => store.init());

describe('store', () => {
  it('takes the app facts from the bootstrap payload', () => {
    expect(store.ready).toBe(true);
    expect(store.platform).toBe('linux');
    expect(store.version).toBe('0.3.0');
    expect(store.homeDir).toBe('/home/you');
    expect(store.settings?.download_dir).toBe('/home/you/Downloads');
    expect(store.torrents.length).toBeGreaterThan(0);
  });

  it('applies a settings patch and the chrome that goes with it', async () => {
    await store.patchSettings({ accent: 'brass', compact: true });
    expect(store.settings?.accent).toBe('brass');
    expect(document.documentElement.dataset.accent).toBe('brass');
    expect(document.documentElement.dataset.density).toBe('compact');
  });
});
