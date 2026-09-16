const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function bytes(n: number, precise = false): string {
  if (!Number.isFinite(n) || n <= 0) return '0 B';
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : value < 10 && !precise ? 1 : value < 100 ? 1 : 0;
  return `${value.toFixed(precise && unit > 0 ? 2 : digits)} ${UNITS[unit]}`;
}

export function speed(n: number): string {
  if (n <= 0) return '—';
  return `${bytes(n)}/s`;
}

export function duration(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return '—';
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))}s`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  if (d < 100) return `${d}d ${h % 24}h`;
  return 'a long time';
}

export function uptime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h === 0) return `${m}m`;
  return `${h}h ${m}m`;
}

export function percent(fraction: number): string {
  const p = fraction * 100;
  if (p > 0 && p < 1) return '<1%';
  if (p > 99 && p < 100) return '>99%';
  return `${Math.round(p)}%`;
}

export function ratio(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0.00';
  return value.toFixed(2);
}

/** Tracker URLs are long; show the part that identifies the tracker. */
export function trackerHost(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.replace(/^[a-z]+:\/\//i, '').split('/')[0] ?? url;
  }
}

export function trackerScheme(url: string): string {
  const match = /^([a-z]+):/i.exec(url);
  return match ? match[1].toLowerCase() : 'http';
}

export function shortHash(hash: string): string {
  return hash.length > 12 ? `${hash.slice(0, 6)}…${hash.slice(-6)}` : hash;
}

export function fileKind(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  if (['mkv', 'mp4', 'avi', 'mov', 'webm', 'm4v', 'ts'].includes(ext)) return 'video';
  if (['mp3', 'flac', 'wav', 'ogg', 'opus', 'm4a'].includes(ext)) return 'audio';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg'].includes(ext)) return 'image';
  if (['zip', 'rar', '7z', 'tar', 'gz', 'xz', 'zst'].includes(ext)) return 'archive';
  if (['iso', 'img', 'qcow2', 'vdi'].includes(ext)) return 'disk';
  if (['pdf', 'epub', 'mobi', 'djvu'].includes(ext)) return 'book';
  if (['txt', 'nfo', 'md', 'srt', 'sub'].includes(ext)) return 'text';
  return 'file';
}
