/** A file's size and whether it is wanted, as the piece map needs them. */
export interface SizedFile {
  length: number;
  included: boolean;
}

/** The pieces a file touches, both ends included. */
export interface FileSpan {
  first: number;
  last: number;
}

/** Unpacks a hex bitfield into bytes. */
export function decodeHave(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length >> 1);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16) || 0;
  }
  return bytes;
}

/** Whether a piece is downloaded. The first piece is the top bit of the first byte. */
export function hasPiece(have: Uint8Array, piece: number): boolean {
  return (have[piece >> 3] & (0x80 >> (piece & 7))) !== 0;
}

/** The pieces each file touches, in file order. An empty file touches none. */
export function fileSpans(
  files: readonly { length: number }[],
  pieceSize: number,
  total: number
): (FileSpan | null)[] {
  let offset = 0;
  return files.map((file) => {
    const start = offset;
    offset += file.length;
    if (file.length <= 0 || pieceSize <= 0 || total <= 0) return null;
    const first = Math.min(total - 1, Math.floor(start / pieceSize));
    const last = Math.min(total - 1, Math.floor((offset - 1) / pieceSize));
    return { first, last };
  });
}

/**
 * One flag per piece, set when a wanted file touches it. A piece shared by a
 * wanted file and a skipped one is still downloaded, so it counts as wanted.
 * Null when there is nothing to go on, which means everything is wanted.
 */
export function wantedPieces(
  files: readonly SizedFile[],
  pieceSize: number,
  total: number
): Uint8Array | null {
  if (files.length === 0 || pieceSize <= 0 || total <= 0) return null;
  if (files.every((file) => file.included)) return null;
  const wanted = new Uint8Array(total);
  fileSpans(files, pieceSize, total).forEach((span, index) => {
    if (span && files[index].included) wanted.fill(1, span.first, span.last + 1);
  });
  return wanted;
}

/** The files that touch any piece from `first` to `last`, as a run of indices. */
export function filesAt(
  spans: readonly (FileSpan | null)[],
  first: number,
  last: number
): { from: number; to: number } | null {
  let from = -1;
  let to = -1;
  spans.forEach((span, index) => {
    if (!span || span.last < first || span.first > last) return;
    if (from < 0) from = index;
    to = index;
  });
  return from < 0 ? null : { from, to };
}

/** The pieces a cell stands for, both ends included. */
export function cellRange(total: number, cells: number, cell: number): [number, number] {
  const first = Math.floor((cell * total) / cells);
  const next = Math.floor(((cell + 1) * total) / cells);
  return [first, Math.max(first, next - 1)];
}

/** The cell a piece is drawn in. */
export function cellOf(total: number, cells: number, piece: number): number {
  let cell = Math.min(cells - 1, Math.floor((piece * cells) / total));
  while (cell > 0 && cellRange(total, cells, cell)[0] > piece) cell -= 1;
  while (cell < cells - 1 && cellRange(total, cells, cell)[1] < piece) cell += 1;
  return cell;
}

/** What each cell of the map holds. */
export interface Cells {
  /** How many pieces each cell stands for. */
  size: Uint32Array;
  /** How many of them are downloaded. */
  have: Uint32Array;
  /** How many are neither downloaded nor wanted. */
  skipped: Uint32Array;
  totals: { have: number; skipped: number };
}

/**
 * Shares the pieces out over `cells` cells of near-equal size and counts what
 * each holds. With as many cells as pieces, every cell is one piece.
 */
export function binPieces(
  have: Uint8Array | null,
  wanted: Uint8Array | null,
  total: number,
  cells: number
): Cells {
  const out: Cells = {
    size: new Uint32Array(cells),
    have: new Uint32Array(cells),
    skipped: new Uint32Array(cells),
    totals: { have: 0, skipped: 0 }
  };
  for (let cell = 0; cell < cells; cell += 1) {
    const [first, last] = cellRange(total, cells, cell);
    out.size[cell] = last - first + 1;
    for (let piece = first; piece <= last; piece += 1) {
      if (have && hasPiece(have, piece)) out.have[cell] += 1;
      else if (wanted && !wanted[piece]) out.skipped[cell] += 1;
    }
    out.totals.have += out.have[cell];
    out.totals.skipped += out.skipped[cell];
  }
  return out;
}

/** Where the cells sit on the canvas, in device pixels. */
export interface Layout {
  cells: number;
  cols: number;
  rows: number;
  /** Left edge and width of each column. */
  xs: number[];
  ws: number[];
  /** Top edge and height of each row. */
  ys: number[];
  hs: number[];
}

/** Panes are drawn about this many times taller than wide. */
const SHAPE = 3.2;
/** The slimmest panes, which many pieces share, are drawn taller still. */
const SLIM_SHAPE = 5;

/** Splits `length` into `count` runs with `gap` between, all whole pixels. */
function runs(length: number, count: number, gap: number): [number[], number[]] {
  const starts: number[] = [];
  const sizes: number[] = [];
  for (let i = 0; i < count; i += 1) {
    const start = Math.round((i * (length + gap)) / count);
    const next = Math.round(((i + 1) * (length + gap)) / count);
    starts.push(start);
    sizes.push(next - start - gap);
  }
  return [starts, sizes];
}

/** Columns of one whole-pixel width, centred in `length`. */
function even(length: number, count: number, pitch: number, gap: number): [number[], number[]] {
  const used = count * pitch - gap;
  const left = Math.floor((length - used) / 2);
  return [
    Array.from({ length: count }, (_, i) => left + i * pitch),
    Array.from({ length: count }, () => pitch - gap)
  ];
}

/**
 * Lays the map out as rows of tall panes. Every piece gets its own pane while
 * they fit; past that the pieces share the densest grid.
 */
export function layoutFor(total: number, width: number, height: number, scale = 1): Layout {
  const px = (n: number) => Math.max(1, Math.round(n * scale));
  const gapY = px(4);
  const slim = px(3);
  const slimGap = px(2);
  const paneH = (rows: number) => Math.floor((height + gapY) / rows) - gapY;
  const off = (rows: number, paneW: number, shape: number) =>
    Math.abs(Math.log(paneH(rows) / paneW / shape));

  // The densest grid: the slimmest panes, in the rows that suit their shape.
  const most = Math.max(1, Math.floor((height + gapY) / (px(9) + gapY)));
  let dense = 1;
  for (let rows = 2; rows <= most; rows += 1) {
    if (off(rows, slim, SLIM_SHAPE) < off(dense, slim, SLIM_SHAPE)) dense = rows;
  }
  let best = {
    rows: dense,
    cols: Math.max(1, Math.floor((width + slimGap) / (slim + slimGap))),
    pitch: slim + slimGap,
    gap: slimGap
  };

  if (total > 0 && total <= best.rows * best.cols) {
    let least = Infinity;
    for (let rows = 1; rows <= dense; rows += 1) {
      const needed = Math.ceil(total / rows);
      // Bigger panes get wider glazing bars between them.
      const gap = width / needed >= 26 * scale ? px(3) : slimGap;
      const pitch = Math.floor((width + gap) / needed);
      if (pitch - gap < slim) continue;
      // Wider panes can differ by a pixel unnoticed, so they fill the width.
      const wide = pitch - gap >= px(5);
      const cols = wide ? needed : Math.floor((width + gap) / pitch);
      if (Math.ceil(total / cols) !== rows) continue;
      const miss = off(rows, pitch - gap, SHAPE);
      if (miss < least) {
        least = miss;
        best = { rows, cols, pitch: wide ? 0 : pitch, gap };
      }
    }
  }

  const [xs, ws] = best.pitch
    ? even(width, best.cols, best.pitch, best.gap)
    : runs(width, best.cols, best.gap);
  const [ys, hs] = runs(height, best.rows, gapY);
  const cells = Math.max(0, Math.min(total, best.rows * best.cols));
  return { cells, cols: best.cols, rows: best.rows, xs, ws, ys, hs };
}

/** The cell under a point, counting the gap after a pane as part of it. */
export function cellAt(layout: Layout, x: number, y: number): number | null {
  let col = -1;
  for (let i = 0; i < layout.cols && layout.xs[i] <= x; i += 1) col = i;
  let row = -1;
  for (let j = 0; j < layout.rows && layout.ys[j] <= y; j += 1) row = j;
  if (col < 0 || row < 0) return null;
  const cell = row * layout.cols + col;
  return cell < layout.cells ? cell : null;
}
