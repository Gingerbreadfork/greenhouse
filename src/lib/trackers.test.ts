import { describe, expect, it } from 'vitest';
import { bulkSummary, healthSummary, parseTrackerBlob, uniqueTrackers } from './trackers';
import type { TrackerHealth } from './types';

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

describe('healthSummary', () => {
  const result = (status: TrackerHealth['status'], n: number): TrackerHealth[] =>
    Array.from({ length: n }, (_, i) => ({
      url: `udp://${status}${i}.example:1/announce`,
      status,
      ms: status === 'answered' ? 80 : null,
      reason: status === 'answered' ? null : 'no reply'
    }));

  it('has nothing to warn about when every tracker answered', () => {
    expect(healthSummary(result('answered', 4))).toEqual({ text: 'All 4 answered', removable: 0 });
    expect(healthSummary(result('answered', 1)).text).toBe('It answered');
  });

  it('counts the ones that did not answer and urges a second check', () => {
    const said = healthSummary([...result('answered', 8), ...result('silent', 2)]);
    expect(said.text).toBe('8 answered, 2 did not');
    expect(said.hint).toContain('before removing them');
    expect(said.removable).toBe(2);
    expect(healthSummary([...result('answered', 8), ...result('silent', 1)]).hint).toContain(
      'before removing it'
    );
  });

  it('points at the connection when nothing answered, and offers none for removal', () => {
    const said = healthSummary(result('silent', 5));
    expect(said.text).toBe('None answered');
    expect(said.hint).toContain('connection');
    expect(said.removable).toBe(0);
  });

  it('keeps trackers that were not checked out of both counts', () => {
    const mixed = [...result('answered', 3), ...result('skipped', 2)];
    expect(healthSummary(mixed)).toEqual({
      text: 'All 3 answered · 2 not checked',
      removable: 0
    });
    expect(healthSummary(result('skipped', 2)).text).toBe('None of these could be checked');
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
