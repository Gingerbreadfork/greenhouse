import { describe, expect, it } from 'vitest';
import { bulkSummary, parseTrackerBlob, uniqueTrackers } from './trackers';

describe('bulkSummary', () => {
  const none = { changed: 0, unchanged: 0, skipped_private: 0, failed: 0, error: null };

  it('says how many torrents were changed', () => {
    expect(bulkSummary({ ...none, changed: 1 })).toEqual({
      text: 'Trackers added to 1 torrent',
      tone: 'good',
      detail: undefined
    });
  });

  it('does not let one success hide the failures', () => {
    const said = bulkSummary({
      changed: 1,
      unchanged: 2,
      skipped_private: 1,
      failed: 3,
      error: 'no longer in the session'
    });
    expect(said.tone).toBe('bad');
    expect(said.detail).toBe(
      '2 torrents already had them · 1 private torrent keeps its own trackers · ' +
        '3 torrents could not be changed: no longer in the session'
    );
  });
});

describe('parseTrackerBlob', () => {
  it('keeps tracker URLs and drops comments, other schemes and duplicates', () => {
    const blob = `
      udp://a.example:1337/announce, https://b.example/announce
      # a comment
      ftp://nope.example
      UDP://A.EXAMPLE:1337/announce wss://c.example
    `;
    expect(parseTrackerBlob(blob)).toEqual([
      'udp://a.example:1337/announce',
      'https://b.example/announce',
      'wss://c.example'
    ]);
  });

  it('returns nothing for an empty paste', () => {
    expect(parseTrackerBlob('')).toEqual([]);
  });
});

describe('uniqueTrackers', () => {
  it('merges lists without repeating a tracker in a different case', () => {
    expect(
      uniqueTrackers(['udp://a.example', ' udp://b.example '], ['UDP://A.EXAMPLE', ''])
    ).toEqual(['udp://a.example', 'udp://b.example']);
  });
});
