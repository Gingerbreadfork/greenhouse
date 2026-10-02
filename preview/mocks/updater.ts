// `?update=1` makes the preview offer a newer version.
let available: string | null =
  typeof location !== 'undefined' && location.search.includes('update') ? '0.9.0' : null;

/** What the next check will find. */
export function offer(version: string | null) {
  available = version;
}

export const installed: string[] = [];

export async function check() {
  if (!available) return null;
  const version = available;
  return {
    version,
    currentVersion: '0.3.1',
    body: 'Greenhouse now does something new.\n\n## New\n\n- A thing.',
    date: '2026-10-03T00:00:00Z',
    rawJson: {},
    async downloadAndInstall(onEvent?: (event: { event: string; data?: unknown }) => void) {
      onEvent?.({ event: 'Started', data: { contentLength: 100 } });
      for (let i = 0; i < 4; i += 1) {
        await new Promise((r) => setTimeout(r, 120));
        onEvent?.({ event: 'Progress', data: { chunkLength: 25 } });
      }
      onEvent?.({ event: 'Finished' });
      installed.push(version);
      available = null;
    },
    async download() {},
    async install() {},
    async close() {}
  };
}
