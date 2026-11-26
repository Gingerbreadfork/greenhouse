import { render, screen } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import PieceMap from './PieceMap.svelte';
import { api } from '../api';
import { torrents } from '../../../preview/fixtures';
import { detailOf } from '../../../preview/pieces';
import type { TorrentRow } from '../types';

const named = (start: string) => torrents.find((t) => t.name.startsWith(start))!;

function show(torrent: TorrentRow) {
  const detail = detailOf(torrent);
  return render(PieceMap, { torrent, files: detail.files, pieceSize: detail.piece_size });
}

describe('PieceMap', () => {
  beforeEach(() => {
    // jsdom has no canvas to draw on; the words around the map still apply.
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
  });

  it('says how many pieces are here in words', async () => {
    show(named('debian'));
    const map = await screen.findByRole('img', { name: /3,248 of 3,248 pieces downloaded/ });
    expect(map.getAttribute('aria-label')).not.toMatch(/to come|skipped/);
    expect(screen.queryByText('3,248 downloaded')).not.toBeNull();
  });

  it('counts the pieces of skipped files apart from those still to come', async () => {
    show(named('Internet Archive'));
    const map = await screen.findByRole('img', { name: /pieces downloaded/ });
    expect(map.getAttribute('aria-label')).toMatch(/[\d,]+ to come, [\d,]+ skipped\.$/);
    expect(screen.queryByText(/skipped$/)).not.toBeNull();
  });

  it('explains itself while the files are being checked', async () => {
    show(named('archlinux'));
    expect(await screen.findByText(/^Checking files\./)).not.toBeNull();
    expect(screen.getByRole('img').getAttribute('aria-label')).toMatch(/^Piece map\. Checking/);
  });

  it('asks at its own pace, not each time the list refreshes', async () => {
    vi.useFakeTimers();
    try {
      const asked = vi.spyOn(api, 'torrentPieces');
      const paused = named('Internet Archive');
      const view = show(paused);
      // The list hands over a new row object on every refresh.
      for (let refresh = 0; refresh < 4; refresh += 1) {
        await vi.advanceTimersByTimeAsync(900);
        await view.rerender({ torrent: { ...paused } });
      }
      expect(asked).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(400);
      expect(asked).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
