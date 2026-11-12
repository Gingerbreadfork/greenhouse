import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import TorrentRow from './TorrentRow.svelte';
import { torrents } from '../../../preview/fixtures';
import type { TorrentRow as Row } from '../types';

const show = (torrent: Row) =>
  render(TorrentRow, {
    torrent,
    selected: false,
    isCursor: false,
    bulkMode: false,
    onselect() {},
    oncontext() {},
    onaction() {},
    ontrackers() {}
  });

const first = (state: Row['state']) => torrents.find((t) => t.state === state)!;

describe('TorrentRow', () => {
  it('offers to resume a torrent that stopped on an error', () => {
    const failed = first('error');
    show(failed);
    expect(screen.queryByLabelText(`Resume ${failed.name}`)).not.toBeNull();
    expect(screen.queryByLabelText(`Pause ${failed.name}`)).toBeNull();
  });

  it('offers to resume a file check that is being held', () => {
    const checking = first('checking');
    const running = show(checking);
    expect(screen.queryByLabelText(`Pause ${checking.name}`)).not.toBeNull();
    running.unmount();

    show({ ...checking, stopped: true });
    expect(screen.queryByLabelText(`Resume ${checking.name}`)).not.toBeNull();
  });

  it('has nothing to press while its files are moved', () => {
    const { container } = show({ ...first('seeding'), state: 'moving', output_folder: '/films' });
    expect(container.querySelector<HTMLElement>('.actions')?.hidden).toBe(true);
    expect(container.textContent).toContain('Moving files');
    expect(container.textContent).toContain('to /films');
  });
});
