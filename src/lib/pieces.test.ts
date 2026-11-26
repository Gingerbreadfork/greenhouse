import { describe, expect, it } from 'vitest';
import {
  binPieces,
  cellAt,
  cellOf,
  cellRange,
  decodeHave,
  fileSpans,
  filesAt,
  hasPiece,
  layoutFor,
  wantedPieces
} from './pieces';

const file = (length: number, included = true) => ({ length, included });

describe('decodeHave', () => {
  it('reads the first piece from the top bit of the first byte', () => {
    const have = decodeHave('eec0');
    expect([...have]).toEqual([0xee, 0xc0]);
    const pieces = Array.from({ length: 10 }, (_, i) => hasPiece(have, i));
    expect(pieces).toEqual([true, true, true, false, true, true, true, false, true, true]);
  });

  it('treats what it cannot read as nothing downloaded', () => {
    expect([...decodeHave('')]).toEqual([]);
    expect([...decodeHave('zz0')]).toEqual([0]);
    expect(hasPiece(decodeHave('ff'), 12)).toBe(false);
  });
});

describe('fileSpans', () => {
  it('gives each file the pieces it touches', () => {
    // Pieces of 100 bytes: 250 + 50 + 0 + 100 bytes of files.
    expect(fileSpans([file(250), file(50), file(0), file(100)], 100, 4)).toEqual([
      { first: 0, last: 2 },
      { first: 2, last: 2 },
      null,
      { first: 3, last: 3 }
    ]);
  });

  it('finds the files under a run of pieces', () => {
    const spans = fileSpans([file(250), file(50), file(0), file(100)], 100, 4);
    expect(filesAt(spans, 0, 1)).toEqual({ from: 0, to: 0 });
    expect(filesAt(spans, 2, 2)).toEqual({ from: 0, to: 1 });
    expect(filesAt(spans, 2, 3)).toEqual({ from: 0, to: 3 });
    expect(filesAt([null], 0, 0)).toBeNull();
  });
});

describe('wantedPieces', () => {
  it('wants a piece when any wanted file touches it', () => {
    // The second file ends inside piece 2, which the wanted third file shares.
    const wanted = wantedPieces([file(100), file(150, false), file(150)], 100, 4);
    expect([...wanted!]).toEqual([1, 0, 1, 1]);
  });

  it('does not want a piece that only skipped files touch', () => {
    const wanted = wantedPieces([file(120), file(280, false)], 100, 4);
    expect([...wanted!]).toEqual([1, 1, 0, 0]);
  });

  it('wants everything when nothing is skipped or nothing is known', () => {
    expect(wantedPieces([file(100), file(300)], 100, 4)).toBeNull();
    expect(wantedPieces([], 100, 4)).toBeNull();
    expect(wantedPieces([file(100, false)], 0, 4)).toBeNull();
  });
});

describe('binPieces', () => {
  it('gives each piece its own cell when there is room', () => {
    const cells = binPieces(decodeHave('a0'), new Uint8Array([1, 1, 1, 0]), 4, 4);
    expect([...cells.size]).toEqual([1, 1, 1, 1]);
    expect([...cells.have]).toEqual([1, 0, 1, 0]);
    expect([...cells.skipped]).toEqual([0, 0, 0, 1]);
    expect(cells.totals).toEqual({ have: 2, skipped: 1 });
  });

  it('shares pieces out evenly and loses none', () => {
    const total = 200_003;
    const have = new Uint8Array(Math.ceil(total / 8)).fill(0xff);
    const cells = binPieces(have, null, total, 476);
    const sizes = [...cells.size];
    expect(sizes.reduce((a, b) => a + b, 0)).toBe(total);
    expect(Math.max(...sizes) - Math.min(...sizes)).toBeLessThanOrEqual(1);
    expect(cells.totals.have).toBe(total);
    expect([...cells.have]).toEqual(sizes);
  });

  it('counts a downloaded piece as downloaded even when it is not wanted', () => {
    const cells = binPieces(decodeHave('c0'), new Uint8Array([0, 1, 0, 1]), 4, 2);
    expect([...cells.have]).toEqual([2, 0]);
    expect([...cells.skipped]).toEqual([0, 1]);
  });

  it('shows nothing as downloaded while there is no record', () => {
    const cells = binPieces(null, null, 9, 3);
    expect([...cells.have]).toEqual([0, 0, 0]);
    expect(cells.totals).toEqual({ have: 0, skipped: 0 });
  });

  it('maps pieces to cells and back', () => {
    for (const [total, count] of [[10, 10], [11, 4], [200_003, 476], [1, 1]]) {
      for (const piece of [0, 1, Math.floor(total / 2), total - 1]) {
        if (piece >= total) continue;
        const [first, last] = cellRange(total, count, cellOf(total, count, piece));
        expect(first <= piece && piece <= last).toBe(true);
      }
    }
  });
});

describe('layoutFor', () => {
  it('draws a single piece as one pane the size of the map', () => {
    const layout = layoutFor(1, 340, 110);
    expect(layout).toMatchObject({ cells: 1, cols: 1, rows: 1, xs: [0], ws: [340], hs: [110] });
  });

  it('gives a dozen pieces a pane each, edge to edge', () => {
    const layout = layoutFor(12, 340, 110);
    expect(layout.cells).toBe(12);
    expect(layout.rows * layout.cols).toBeGreaterThanOrEqual(12);
    const last = layout.cols - 1;
    expect(layout.xs[0]).toBe(0);
    expect(layout.xs[last] + layout.ws[last]).toBe(340);
  });

  it('keeps one pane per piece for as long as they fit', () => {
    for (const total of [28, 150, 300, 408]) {
      const layout = layoutFor(total, 340, 110);
      expect(layout.cells).toBe(total);
      expect(Math.ceil(total / layout.cols)).toBe(layout.rows);
      expect(Math.min(...layout.ws)).toBeGreaterThanOrEqual(3);
    }
  });

  it('shares the densest grid among more pieces than fit', () => {
    const layout = layoutFor(200_000, 340, 110);
    expect(layout.cells).toBe(layout.rows * layout.cols);
    expect(layout.cells).toBe(408);
    expect(layoutFor(409, 340, 110).cells).toBe(408);
    expect(new Set(layout.ws).size).toBe(1);
    const last = layout.cols - 1;
    expect(layout.xs[0]).toBeGreaterThanOrEqual(0);
    expect(layout.xs[last] + layout.ws[last]).toBeLessThanOrEqual(340);
  });

  it('stays on whole pixels at any scale', () => {
    for (const scale of [1, 1.25, 2]) {
      const layout = layoutFor(5000, Math.round(340 * scale), Math.round(110 * scale), scale);
      for (const n of [...layout.xs, ...layout.ws, ...layout.ys, ...layout.hs]) {
        expect(Number.isInteger(n)).toBe(true);
      }
      const bottom = layout.rows - 1;
      expect(layout.ys[bottom] + layout.hs[bottom]).toBe(Math.round(110 * scale));
    }
  });

  it('has nothing to lay out before the piece count is known', () => {
    expect(layoutFor(0, 340, 110).cells).toBe(0);
  });
});

describe('cellAt', () => {
  it('finds the pane under a point and nothing past the last one', () => {
    const layout = layoutFor(28, 340, 110);
    expect(cellAt(layout, 0, 0)).toBe(0);
    expect(cellAt(layout, layout.xs[1], layout.ys[0])).toBe(1);
    expect(cellAt(layout, layout.xs[1] - 1, layout.ys[0])).toBe(0);
    const row = layout.rows - 1;
    expect(cellAt(layout, layout.xs[0], layout.ys[row])).toBe(row * layout.cols);
    const spare = layout.rows * layout.cols - 28;
    if (spare > 0) expect(cellAt(layout, 339, 109)).toBeNull();
    expect(cellAt(layout, -1, 5)).toBeNull();
  });
});
