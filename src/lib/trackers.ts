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
