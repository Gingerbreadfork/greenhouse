import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import TrackerPacks from './TrackerPacks.svelte';
import { api } from '../api';
import { store } from '../store.svelte';

beforeAll(() => store.init());

describe('TrackerPacks', () => {
  it('checks a pack, removes what did not answer and drops stale results', async () => {
    render(TrackerPacks);
    const pack = store.settings!.packs[0];
    const before = pack.trackers;

    await fireEvent.click(screen.getAllByRole('button', { name: 'Check trackers' })[0]);
    expect(screen.getByRole('button', { name: 'Checking…' })).toHaveProperty('disabled', true);

    const remove = await screen.findByRole(
      'button',
      { name: 'Remove 2 that did not answer' },
      { timeout: 3000 }
    );
    expect(screen.getByRole('status').textContent).toContain('8 answered, 2 did not');
    expect(screen.getAllByText('no reply')).toHaveLength(1);

    await fireEvent.click(remove);
    await waitFor(() => expect(store.settings!.packs[0].trackers).toHaveLength(8));
    const kept = store.settings!.packs[0].trackers;
    expect(before.filter((url) => !kept.includes(url))).toEqual([before[2], before[6]]);
    expect(screen.getByRole('status').textContent).toContain('All 8 answered');
    expect(screen.queryByRole('button', { name: /did not answer/ })).toBeNull();
    expect(store.toasts.at(-1)?.action?.label).toBe('Undo');

    // A list that has changed since the check has no results to show.
    await store.patchSettings({
      packs: store.settings!.packs.map((p) =>
        p.id === pack.id ? { ...p, trackers: kept.slice(1) } : p
      )
    });
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });

  it('offers nothing for removal when no tracker answered', async () => {
    vi.spyOn(api, 'checkTrackers').mockImplementationOnce(async (trackers) =>
      trackers.map((url) => ({ url, status: 'silent', ms: null, reason: 'could not connect' }))
    );
    render(TrackerPacks);

    await fireEvent.click(screen.getAllByRole('button', { name: 'Check trackers' })[0]);

    expect((await screen.findByRole('status')).textContent).toContain('None answered');
    expect(screen.getAllByText('could not connect').length).toBeGreaterThan(0);
    expect(screen.queryByRole('button', { name: /did not answer/ })).toBeNull();
  });
});
