import { fireEvent, render, screen } from '@testing-library/svelte';
import { beforeAll, describe, expect, it } from 'vitest';
import TorrentList from './TorrentList.svelte';
import { store } from '../store.svelte';

beforeAll(() => store.init());

describe('TorrentList', () => {
  it('leaves Space and Enter to a focused row button', async () => {
    const actions: string[] = [];
    render(TorrentList, {
      onaction: (_id: number, action: string) => actions.push(action),
      ontrackers() {},
      onbulktrackers() {},
      onbulk() {},
      oncontext() {},
      onpick() {}
    });
    const torrent = store.torrents[0];
    store.selectOnly(torrent.id);
    const inspecting = store.inspectorOpen;

    const remove = screen.getByLabelText(`Remove ${torrent.name}`);
    const untouched = await fireEvent.keyDown(remove, { key: ' ' });
    await fireEvent.keyDown(remove, { key: 'Enter' });

    expect(untouched).toBe(true);
    expect(actions).toEqual([]);
    expect(store.inspectorOpen).toBe(inspecting);

    await fireEvent.keyDown(screen.getByRole('grid'), { key: ' ' });
    expect(actions).toEqual([torrent.stopped ? 'start' : 'pause']);
  });
});
