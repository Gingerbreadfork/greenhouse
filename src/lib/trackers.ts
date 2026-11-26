import type { BulkApplyResult, TrackerHealth } from './types';

const SCHEMES = ['http://', 'https://', 'udp://', 'ws://', 'wss://'];

/** Mirrors the engine-side parser so the add sheet can count as you type. */
export function parseTrackerBlob(blob: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of blob.split(/[\s,;]+/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const lower = line.toLowerCase();
    if (!SCHEMES.some((s) => lower.startsWith(s))) continue;
    if (seen.has(lower)) continue;
    seen.add(lower);
    out.push(line);
  }
  return out;
}

/** What to tell someone after adding trackers to several torrents at once. */
export function bulkSummary(result: BulkApplyResult): {
  text: string;
  tone: 'good' | 'bad';
  detail?: string;
} {
  const torrents = (n: number) => (n === 1 ? '1 torrent' : `${n} torrents`);
  const notes: string[] = [];
  if (result.unchanged > 0) notes.push(`${torrents(result.unchanged)} already had them`);
  if (result.skipped_private > 0) {
    const keep = result.skipped_private === 1 ? 'torrent keeps its' : 'torrents keep their';
    notes.push(`${result.skipped_private} private ${keep} own trackers`);
  }
  if (result.failed > 0) {
    notes.push(`${torrents(result.failed)} could not be changed: ${result.error ?? 'no reason given'}`);
  }
  return {
    text: `Trackers added to ${torrents(result.changed)}`,
    tone: result.failed > 0 ? 'bad' : 'good',
    detail: notes.length > 0 ? notes.join(' · ') : undefined
  };
}

/** What a check of a pack found, and a caution when some did not answer. */
export function healthSummary(results: TrackerHealth[]): {
  text: string;
  hint?: string;
  /** How many to offer for removal. None when nothing answered at all. */
  removable: number;
} {
  const answered = results.filter((r) => r.status === 'answered').length;
  const silent = results.filter((r) => r.status === 'silent').length;
  const skipped = results.length - answered - silent;
  const unchecked = skipped > 0 ? ` · ${skipped} not checked` : '';

  if (silent === 0) {
    if (answered === 0) return { text: 'None of these could be checked', removable: 0 };
    return {
      text: `${answered === 1 ? 'It' : `All ${answered}`} answered${unchecked}`,
      removable: 0
    };
  }
  if (answered === 0) {
    return {
      text: `None answered${unchecked}`,
      hint: 'That is more often the connection than the trackers, so look there first.',
      removable: 0
    };
  }
  return {
    text: `${answered} answered, ${silent} did not${unchecked}`,
    hint: `One miss can be a busy tracker. Check again before removing ${silent === 1 ? 'it' : 'them'}.`,
    removable: silent
  };
}

export function uniqueTrackers(...lists: string[][]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const list of lists) {
    for (const raw of list) {
      const t = raw.trim();
      if (!t) continue;
      const key = t.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(t);
    }
  }
  return out;
}
