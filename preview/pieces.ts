import type { TorrentFileEntry, TorrentPieces, TorrentRow } from '../src/lib/types';
import { filmFiles } from './fixtures';

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const KB = 1024;

/** The order pieces arrive in, which gives each torrent's map its character. */
type Order = 'scattered' | 'streaming' | 'clustered';

interface Shape {
  pieceSize: number;
  order: Order;
  /** Name, size, and whether it is wanted. One file of the whole size if left out. */
  files?: [string, number, boolean][];
}

const shapes: Record<number, Shape> = {
  1: { pieceSize: 512 * KB, order: 'scattered' },
  2: {
    pieceSize: 4 * MB,
    order: 'streaming',
    files: filmFiles.map((f) => [f.name, f.length, true])
  },
  3: { pieceSize: 256 * KB, order: 'scattered' },
  4: {
    pieceSize: 128 * KB,
    order: 'clustered',
    files: [
      ['film/Cosmos Laundromat - First Cycle (2K).mov', 3.2 * GB, true],
      ['production/blend/01-island.blend', 2.6 * GB, true],
      ['production/blend/02-laundromat.blend', 3.4 * GB, true],
      ['production/renders/exr-frames-0001-2400.tar', 9.8 * GB, false],
      ['production/textures.zip', 6.9 * GB, true],
      ['production/sound/stems.flac', 4.4 * GB, true],
      ['production/storyboards.pdf', 1.3 * GB, true],
      ['extras/making-of.mkv', 1.5 * GB, false],
      ['extras/poster-artwork.zip', 0.6 * GB, true]
    ]
  },
  5: { pieceSize: 512 * KB, order: 'scattered' },
  6: {
    pieceSize: 64 * KB,
    order: 'clustered',
    files: [
      ['reels/reel-01-launch.mkv', 2.9 * GB, true],
      ['reels/reel-02-translunar.mkv', 2.4 * GB, true],
      ['reels/reel-03-landing.mkv', 3.3 * GB, true],
      ['reels/reel-04-eva-raw-scan.mkv', 5.2 * GB, false],
      ['reels/reel-05-return.mkv', 2.6 * GB, true],
      ['audio/mission-control-loops.flac', 1.7 * GB, true],
      ['notes/shot-log.pdf', 0.1 * GB, true]
    ]
  },
  8: {
    pieceSize: MB,
    order: 'scattered',
    files: Array.from({ length: 12 }, (_, i) => [
      `The Wind in the Willows/Chapter ${String(i + 1).padStart(2, '0')}.flac`,
      (1.9 * GB) / 12,
      true
    ])
  },
  9: { pieceSize: MB, order: 'scattered' },
  10: { pieceSize: 4 * MB, order: 'scattered' },
  11: { pieceSize: MB, order: 'scattered' }
};

/** File ticks changed in the preview, by torrent. */
const picked = new Map<number, Set<number>>();

export function pickFiles(id: number, indices: number[]) {
  picked.set(id, new Set(indices));
}

function filesOf(row: TorrentRow): { name: string; length: number; included: boolean }[] {
  const shape = shapes[row.id];
  const files: [string, number, boolean][] = shape?.files ?? [[row.name, row.total_bytes, true]];
  const chosen = picked.get(row.id);
  return files.map(([name, length, included], index) => ({
    name,
    length: Math.round(length),
    included: chosen ? chosen.has(index) : included
  }));
}

/** A small repeatable generator, so the preview draws the same map each time. */
function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function sortedBy(total: number, key: (piece: number) => number): Uint32Array {
  const keys = new Float64Array(total);
  for (let i = 0; i < total; i += 1) keys[i] = key(i);
  return Uint32Array.from({ length: total }, (_, i) => i).sort((a, b) => keys[a] - keys[b]);
}

function arrival(order: Order, total: number, seed: number, firstFile: number): Uint32Array {
  const random = seeded(seed);
  if (order === 'scattered') {
    // Mostly front to back, with plenty taken out of turn.
    return sortedBy(total, (i) => (i / total) * 0.55 + random() * 0.75);
  }
  if (order === 'clustered') {
    // Whole stretches at a time, in no particular order.
    const stretches = 46;
    const turn = Array.from({ length: stretches }, () => random());
    return sortedBy(total, (i) => {
      const at = (i / total) * stretches;
      const stretch = Math.floor(at);
      return turn[stretch] + (at - stretch) * 0.16 + random() * 0.05;
    });
  }
  // A player reads the start and the end of the film, then plays from where
  // it was left, so the pieces gather there and run forward.
  const head = Math.round(firstFile * 0.014);
  const tail = Math.round(firstFile * 0.006);
  const playhead = Math.round(firstFile * 0.31);
  return sortedBy(total, (i) => {
    const jitter = random() * 7;
    if (i < head) return i + jitter;
    if (i >= firstFile) return total + i;
    if (i >= firstFile - tail) return 2 * total + i;
    if (i >= playhead) return 3 * total + (i - playhead) + jitter;
    return 5 * total + i + jitter;
  });
}

const orders = new Map<string, Uint32Array>();

interface Pieces {
  total: number;
  pieceSize: number;
  files: { name: string; length: number; included: boolean }[];
  have: Uint8Array | null;
}

function build(row: TorrentRow): Pieces {
  const shape = shapes[row.id];
  const files = filesOf(row);
  if (!shape || !row.has_metadata) return { total: 0, pieceSize: 0, files: [], have: null };

  const size = files.reduce((sum, f) => sum + f.length, 0);
  const total = Math.max(1, Math.ceil(size / shape.pieceSize));
  if (row.state === 'checking' || row.state === 'error') {
    return { total, pieceSize: shape.pieceSize, files, have: null };
  }

  const wanted = new Uint8Array(total);
  let offset = 0;
  for (const f of files) {
    const first = Math.floor(offset / shape.pieceSize);
    offset += f.length;
    const last = Math.min(total - 1, Math.floor((offset - 1) / shape.pieceSize));
    if (f.included && f.length > 0) wanted.fill(1, first, last + 1);
  }
  const wantedCount = wanted.reduce((sum, w) => sum + w, 0);

  const key = `${row.id}:${total}`;
  let order = orders.get(key);
  if (!order) {
    const firstFile = Math.ceil(files[0].length / shape.pieceSize);
    order = arrival(shape.order, total, row.id * 7919, firstFile);
    orders.set(key, order);
  }

  const have = new Uint8Array(Math.ceil(total / 8));
  let left = Math.round(Math.min(1, row.progress) * wantedCount);
  for (let i = 0; i < total && left > 0; i += 1) {
    const piece = order[i];
    if (!wanted[piece]) continue;
    have[piece >> 3] |= 0x80 >> (piece & 7);
    left -= 1;
  }
  return { total, pieceSize: shape.pieceSize, files, have };
}

/** What `torrent_pieces` answers for a fixture torrent. */
export function piecesOf(row: TorrentRow): TorrentPieces {
  const { total, have } = build(row);
  const hex = have && Array.from(have, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return { total, have: hex };
}

/** The parts of `torrent_detail` that have to agree with the piece map. */
export function detailOf(row: TorrentRow): {
  name: string;
  info_hash: string;
  output_folder: string;
  total_pieces: number;
  piece_size: number;
  files: TorrentFileEntry[];
} {
  const { total, pieceSize, files, have } = build(row);
  let offset = 0;
  const entries = files.map((f, index) => {
    const first = pieceSize ? Math.floor(offset / pieceSize) : 0;
    offset += f.length;
    const last = pieceSize ? Math.min(total - 1, Math.floor((offset - 1) / pieceSize)) : 0;
    let got = 0;
    for (let piece = first; have && piece <= last; piece += 1) {
      if (have[piece >> 3] & (0x80 >> (piece & 7))) got += 1;
    }
    const share = have ? got / (last - first + 1) : row.progress;
    return {
      index,
      name: f.name,
      components: f.name.split('/'),
      length: f.length,
      included: f.included,
      progress_bytes: Math.round(f.length * share)
    };
  });
  return {
    name: row.name,
    info_hash: row.info_hash,
    output_folder: row.output_folder,
    total_pieces: total,
    piece_size: pieceSize,
    files: entries
  };
}
