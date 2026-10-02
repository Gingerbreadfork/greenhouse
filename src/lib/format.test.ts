import { describe, expect, it } from 'vitest';
import {
  bytes,
  duration,
  fileKind,
  percent,
  ratio,
  shortHash,
  speed,
  trackerHost,
  trackerScheme,
  uptime
} from './format';

describe('sizes and speeds', () => {
  it('shows bytes with the precision a list can read at a glance', () => {
    expect(bytes(0)).toBe('0 B');
    expect(bytes(-5)).toBe('0 B');
    expect(bytes(512)).toBe('512 B');
    expect(bytes(1536)).toBe('1.5 KB');
    expect(bytes(10 * 1024 ** 2)).toBe('10.0 MB');
    expect(bytes(250 * 1024 ** 3)).toBe('250 GB');
    expect(bytes(5 * 1024 ** 5)).toBe('5120 TB');
  });

  it('shows two decimals when asked to be precise', () => {
    expect(bytes(1536, true)).toBe('1.50 KB');
    expect(bytes(512, true)).toBe('512 B');
  });

  it('shows no speed for an idle transfer', () => {
    expect(speed(0)).toBe('—');
    expect(speed(2048)).toBe('2.0 KB/s');
  });
});

describe('times', () => {
  it('rounds a remaining time to the unit that matters', () => {
    expect(duration(null)).toBe('—');
    expect(duration(Infinity)).toBe('—');
    expect(duration(0)).toBe('1s');
    expect(duration(59.4)).toBe('59s');
    expect(duration(90)).toBe('1m');
    expect(duration(5 * 3600 + 7 * 60)).toBe('5h 7m');
    expect(duration(3 * 86400 + 2 * 3600)).toBe('3d 2h');
    expect(duration(100 * 86400)).toBe('a long time');
  });

  it('shows uptime in hours and minutes', () => {
    expect(uptime(59)).toBe('0m');
    expect(uptime(3660)).toBe('1h 1m');
  });
});

describe('ratios and percentages', () => {
  it('never rounds something in between to 0% or 100%', () => {
    expect(percent(0)).toBe('0%');
    expect(percent(0.005)).toBe('<1%');
    expect(percent(0.5)).toBe('50%');
    expect(percent(0.995)).toBe('>99%');
    expect(percent(1)).toBe('100%');
  });

  it('shows ratios with two decimals', () => {
    expect(ratio(0)).toBe('0.00');
    expect(ratio(NaN)).toBe('0.00');
    expect(ratio(1.2345)).toBe('1.23');
  });
});

describe('names', () => {
  it('reduces a tracker to its host and scheme', () => {
    expect(trackerHost('udp://tracker.opentrackr.org:1337/announce')).toBe(
      'tracker.opentrackr.org:1337'
    );
    expect(trackerHost('not a url')).toBe('not a url');
    expect(trackerScheme('WSS://example.org/announce')).toBe('wss');
    expect(trackerScheme('example.org')).toBe('http');
  });

  it('shortens long hashes', () => {
    expect(shortHash('abcdef123456789012')).toBe('abcdef…789012');
    expect(shortHash('short')).toBe('short');
  });

  it('classifies files by extension, whatever the case', () => {
    expect(fileKind('Film.MKV')).toBe('video');
    expect(fileKind('song.flac')).toBe('audio');
    expect(fileKind('cover.jpg')).toBe('image');
    expect(fileKind('archive.tar.gz')).toBe('archive');
    expect(fileKind('disc.iso')).toBe('disk');
    expect(fileKind('book.epub')).toBe('book');
    expect(fileKind('readme.nfo')).toBe('text');
    expect(fileKind('unknown.xyz')).toBe('file');
    expect(fileKind('noext')).toBe('file');
  });
});
