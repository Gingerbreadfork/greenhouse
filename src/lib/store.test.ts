import { beforeAll, describe, expect, it } from 'vitest';
import { store } from './store.svelte';
import { restarts } from '../../preview/mocks/process';
import { installed, offer } from '../../preview/mocks/updater';

beforeAll(() => store.init());

describe('store', () => {
  it('takes the app facts from the bootstrap payload', () => {
    expect(store.ready).toBe(true);
    expect(store.platform).toBe('linux');
    expect(store.version).toBe('0.3.1');
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

describe('updates', () => {
  it('offers a newer release and installs it on request', async () => {
    offer('9.9.9');
    await store.checkForUpdate();
    expect(store.update?.version).toBe('9.9.9');
    expect(store.toasts.at(-1)?.text).toBe('Greenhouse 9.9.9 is available');

    await store.installUpdate();
    expect(installed).toEqual(['9.9.9']);
    expect(restarts.count).toBe(1);
  });

  it('installs on its own when asked to', async () => {
    await store.patchSettings({ install_updates: true });
    offer('9.9.10');
    await store.checkForUpdate();
    expect(installed).toEqual(['9.9.9', '9.9.10']);
    expect(restarts.count).toBe(2);
  });

  it('says so when a check by hand finds nothing', async () => {
    offer(null);
    await store.checkForUpdate(true);
    expect(store.update).toBeNull();
    expect(store.toasts.at(-1)?.text).toBe('Greenhouse is up to date');
  });
});
