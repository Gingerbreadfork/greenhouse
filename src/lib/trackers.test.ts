import { describe, expect, it } from 'vitest';
import { parseTrackerBlob, uniqueTrackers } from './trackers';

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
