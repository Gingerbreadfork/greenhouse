import type { SessionSummary, Settings, TorrentRow } from '../src/lib/types';

const GB = 1024 ** 3;
const MB = 1024 ** 2;

export const torrents: TorrentRow[] = [
  {
    id: 1,
    info_hash: '8c4adbf9ebe66f1d804fb6a4fb9b74966c3ab609',
    name: 'ubuntu-24.04.1-desktop-amd64.iso',
    state: 'downloading',
    error: null,
    progress: 0.62,
    progress_bytes: 3.6 * GB,
    total_bytes: 5.8 * GB,
    uploaded_bytes: 410 * MB,
    download_speed: 8.4 * MB,
    upload_speed: 1.1 * MB,
    eta_seconds: 268,
    peers_live: 41,
    peers_connecting: 6,
    peers_queued: 12,
    peers_seen: 180,
    finished: false,
    output_folder: '/home/you/Downloads',
    tracker_count: 14,
    ratio: 0.11,
    has_metadata: true
  },
  {
    id: 2,
    info_hash: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678',
    name: 'Big Buck Bunny (2008) 4K master — Blender Foundation',
    state: 'downloading',
    error: null,
    progress: 0.14,
    progress_bytes: 1.2 * GB,
    total_bytes: 8.6 * GB,
    uploaded_bytes: 24 * MB,
    download_speed: 2.2 * MB,
    upload_speed: 180 * 1024,
    eta_seconds: 3480,
    peers_live: 9,
    peers_connecting: 3,
    peers_queued: 22,
    peers_seen: 64,
    finished: false,
    output_folder: '/home/you/Downloads/Big Buck Bunny',
    tracker_count: 22,
    ratio: 0.02,
    has_metadata: true
  },
  {
    id: 3,
    info_hash: 'ffeeddccbbaa99887766554433221100aabbccdd',
    name: 'debian-13.1.0-amd64-netinst.iso',
    state: 'seeding',
    error: null,
    progress: 1,
    progress_bytes: 812 * MB,
    total_bytes: 812 * MB,
    uploaded_bytes: 3.4 * GB,
    download_speed: 0,
    upload_speed: 940 * 1024,
    eta_seconds: null,
    peers_live: 18,
    peers_connecting: 1,
    peers_queued: 4,
    peers_seen: 210,
    finished: true,
    output_folder: '/home/you/Downloads',
    tracker_count: 14,
    ratio: 4.29,
    has_metadata: true
  },
  {
    id: 4,
    info_hash: '0123456789abcdef0123456789abcdef01234567',
    name: 'Cosmos Laundromat — open movie source files',
    state: 'seeding',
    error: null,
    progress: 1,
    progress_bytes: 22.4 * GB,
    total_bytes: 22.4 * GB,
    uploaded_bytes: 9.1 * GB,
    download_speed: 0,
    upload_speed: 220 * 1024,
    eta_seconds: null,
    peers_live: 5,
    peers_connecting: 0,
    peers_queued: 2,
    peers_seen: 47,
    finished: true,
    output_folder: '/home/you/Media/Open movies',
    tracker_count: 8,
    ratio: 0.41,
    has_metadata: true
  },
  {
    id: 5,
    info_hash: 'aaaabbbbccccddddeeeeffff0000111122223333',
    name: 'archlinux-2026.09.01-x86_64.iso',
    state: 'checking',
    error: null,
    progress: 0.38,
    progress_bytes: 480 * MB,
    total_bytes: 1.26 * GB,
    uploaded_bytes: 0,
    download_speed: 0,
    upload_speed: 0,
    eta_seconds: null,
    peers_live: 0,
    peers_connecting: 0,
    peers_queued: 0,
    peers_seen: 0,
    finished: false,
    output_folder: '/home/you/Downloads',
    tracker_count: 14,
    ratio: 0,
    has_metadata: true
  },
  {
    id: 6,
    info_hash: '5566778899aabbccddeeff00112233445566778',
    name: 'Internet Archive — Apollo 11 restored footage (2019)',
    state: 'paused',
    error: null,
    progress: 0.47,
    progress_bytes: 6.1 * GB,
    total_bytes: 13 * GB,
    uploaded_bytes: 140 * MB,
    download_speed: 0,
    upload_speed: 0,
    eta_seconds: null,
    peers_live: 0,
    peers_connecting: 0,
    peers_queued: 0,
    peers_seen: 88,
    finished: false,
    output_folder: '/home/you/Media/Archive',
    tracker_count: 6,
    ratio: 0.02,
    has_metadata: true
  },
  {
    id: 7,
    info_hash: 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef',
    name: 'magnet:?xt=urn:btih:deadbeef… (resolving)',
    state: 'downloading',
    error: null,
    progress: 0,
    progress_bytes: 0,
    total_bytes: 0,
    uploaded_bytes: 0,
    download_speed: 0,
    upload_speed: 0,
    eta_seconds: null,
    peers_live: 2,
    peers_connecting: 8,
    peers_queued: 31,
    peers_seen: 11,
    finished: false,
    output_folder: '/home/you/Downloads',
    tracker_count: 22,
    ratio: 0,
    has_metadata: false
  },
  {
    id: 8,
    info_hash: '1111222233334444555566667777888899990000',
    name: 'LibriVox — The Wind in the Willows (audiobook, FLAC)',
    state: 'complete',
    error: null,
    progress: 1,
    progress_bytes: 1.9 * GB,
    total_bytes: 1.9 * GB,
    uploaded_bytes: 2.7 * GB,
    download_speed: 0,
    upload_speed: 0,
    eta_seconds: null,
    peers_live: 0,
    peers_connecting: 0,
    peers_queued: 0,
    peers_seen: 31,
    finished: true,
    output_folder: '/home/you/Audio',
    tracker_count: 14,
    ratio: 1.42,
    has_metadata: true
  },
  {
    id: 9,
    info_hash: '99887766554433221100ffeeddccbbaa99887766',
    name: 'fedora-workstation-44-x86_64.iso',
    state: 'error',
    error: 'No space left on device while writing to /home/you/Downloads',
    progress: 0.81,
    progress_bytes: 1.8 * GB,
    total_bytes: 2.2 * GB,
    uploaded_bytes: 60 * MB,
    download_speed: 0,
    upload_speed: 0,
    eta_seconds: null,
    peers_live: 0,
    peers_connecting: 0,
    peers_queued: 0,
    peers_seen: 122,
    finished: false,
    output_folder: '/home/you/Downloads',
    tracker_count: 14,
    ratio: 0.03,
    has_metadata: true
  }
];

export const session: SessionSummary = {
  download_speed: 10.6 * MB,
  upload_speed: 2.4 * MB,
  peers_live: 75,
  peers_connecting: 12,
  peers_seen: 1840,
  uptime_seconds: 9240,
  dht_nodes: 412,
  listen_port: 51413,
  download_limit_kbps: 0,
  upload_limit_kbps: 4096,
  fetched_bytes: 64.2 * GB,
  uploaded_bytes: 21.7 * GB,
  blocked_incoming: 14,
  blocked_outgoing: 3,
  connect_tcp: 2914,
  connect_utp: 0,
  connect_errors: 488
};

export const settings: Settings = {
  download_dir: '/home/you/Downloads',
  start_paused: false,
  auto_add: false,
  auto_apply_trackers: true,
  theme: 'system',
  accent: 'glass',
  compact: false,
  reduce_motion: false,
  confirm_remove: true,
  download_limit_kbps: 0,
  upload_limit_kbps: 4096,
  listen_port: 51413,
  enable_upnp: true,
  transport: 'tcp',
  peer_limit_per_torrent: 0,
  bind_interface: '',
  blocklist_url: '',
  incomplete_dir: '/home/you/Downloads/.incomplete',
  watch_dir: '',
  skip_types_enabled: true,
  skip_types: ['exe', 'msi', 'bat', 'cmd', 'com', 'scr', 'vbs', 'ps1'],
  max_active_downloads: 3,
  max_active_seeds: 4,
  seed_ratio_limit: 2,
  seed_time_limit_minutes: 0,
  notify_on_done: true,
  close_to_background: false,
  start_on_login: false,
  check_updates: true,
  install_updates: false,
  player_command: '',
  feed_interval_minutes: 30,
  feeds: [
    {
      id: 'feed-arch',
      name: 'Arch Linux releases',
      url: 'https://archlinux.org/feeds/releases/',
      enabled: true,
      must_contain: '',
      must_not_contain: '',
      download_dir: ''
    },
    {
      id: 'feed-films',
      name: 'Open films',
      url: 'https://films.example/rss',
      enabled: false,
      must_contain: '2160p, hevc',
      must_not_contain: 'cam',
      download_dir: '/home/you/Films'
    }
  ],
  packs: [
    {
      id: 'starter',
      name: 'Open trackers',
      description: '',
      enabled: true,
      source_url: 'https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_best.txt',
      updated_at: '2026-09-14T10:00:00Z',
      trackers: [
        'udp://tracker.opentrackr.org:1337/announce',
        'udp://open.demonii.com:1337/announce',
        'udp://open.stealth.si:80/announce',
        'udp://tracker.torrent.eu.org:451/announce',
        'udp://exodus.desync.com:6969/announce',
        'udp://tracker.openbittorrent.com:6969/announce',
        'http://tracker.openbittorrent.com:80/announce',
        'udp://explodie.org:6969/announce',
        'udp://tracker1.bt.moack.co.kr:80/announce',
        'udp://tracker.tiny-vps.com:6969/announce'
      ]
    },
    {
      id: 'ipv6',
      name: 'IPv6 trackers',
      description: '',
      enabled: true,
      source_url: null,
      updated_at: null,
      trackers: [
        'udp://tracker.birkenwald.de:6969/announce',
        'udp://tracker.dler.org:6969/announce',
        'udp://ipv6.tracker.example:6969/announce',
        'udp://v6.tracker.example.net:6881/announce'
      ]
    },
    {
      id: 'archive',
      name: 'Archive mirrors',
      description: '',
      enabled: false,
      source_url: null,
      updated_at: null,
      trackers: [
        'http://bt1.archive.org:6969/announce',
        'http://bt2.archive.org:6969/announce'
      ]
    }
  ]
};

const file = (index: number, name: string, length: number, done: number) => ({
  index,
  name,
  components: [name],
  length,
  included: true,
  progress_bytes: length * done
});

/** A film and its extras, so the play button has something to show on. */
export const filmFiles = [
  file(0, 'Big Buck Bunny (2008) 4K.mkv', 8.5 * GB, 0.14),
  file(1, 'Big Buck Bunny (2008) 4K.en.srt', 0.0001 * GB, 1),
  file(2, 'Soundtrack.flac', 0.09 * GB, 1),
  file(3, 'poster.jpg', 0.004 * GB, 1)
];

export const detail = {
  id: 1,
  info_hash: '8c4adbf9ebe66f1d804fb6a4fb9b74966c3ab609',
  name: 'ubuntu-24.04.1-desktop-amd64.iso',
  output_folder: '/home/you/Downloads',
  total_pieces: 11_264,
  piece_size: 512 * 1024,
  files: [
    { index: 0, name: 'ubuntu-24.04.1-desktop-amd64.iso', components: ['ubuntu-24.04.1-desktop-amd64.iso'], length: 5.8 * GB, included: true, progress_bytes: 3.6 * GB }
  ],
  trackers: [
    'udp://tracker.opentrackr.org:1337/announce',
    'udp://open.demonii.com:1337/announce',
    'udp://open.stealth.si:80/announce',
    'udp://tracker.torrent.eu.org:451/announce',
    'https://tracker.ubuntu.com:443/announce',
    'udp://exodus.desync.com:6969/announce'
  ],
  peers: {
    peers: {
      '82.14.221.9:51413': { state: 'live', conn_kind: 'Tcp', client_name: 'qBittorrent 5.0.3', counters: { fetched_bytes: 820 * MB, uploaded_bytes: 24 * MB } },
      '[2a01:4f8:1c1c::2]:6881': { state: 'live', conn_kind: 'Utp', client_name: 'Transmission 4.0.6', counters: { fetched_bytes: 610 * MB, uploaded_bytes: 90 * MB } },
      '91.203.44.180:50000': { state: 'live', conn_kind: 'Tcp', client_name: 'libtorrent 2.0.10', counters: { fetched_bytes: 340 * MB, uploaded_bytes: 12 * MB } },
      '203.0.113.77:6889': { state: 'live', conn_kind: 'Tcp', client_name: 'Deluge 2.1.1', counters: { fetched_bytes: 96 * MB, uploaded_bytes: 4 * MB } }
    }
  },
  added_source: null
};

const feedItem = (guid: string, title: string, matches: boolean, added: boolean, days: number) => ({
  guid,
  title,
  link: `https://example.org/${guid}.torrent`,
  published: new Date(Date.now() - days * 86_400_000).toISOString(),
  matches,
  added
});

export const feedStatus = {
  'feed-arch': {
    checked_at: Math.floor(Date.now() / 1000) - 540,
    error: null,
    items: [
      feedItem('arch-2026-09', 'archlinux-2026.09.01-x86_64.iso', true, true, 20),
      feedItem('arch-2026-08', 'archlinux-2026.08.01-x86_64.iso', true, false, 51),
      feedItem('arch-2026-07', 'archlinux-2026.07.01-x86_64.iso', true, false, 82)
    ]
  },
  'feed-films': {
    checked_at: Math.floor(Date.now() / 1000) - 4000,
    error: null,
    items: [
      feedItem('film-3', 'Spring (2019) 2160p HEVC open movie', true, false, 2),
      feedItem('film-2', 'Spring (2019) 1080p', false, false, 2),
      feedItem('film-1', 'Hero (2018) 2160p HEVC CAM', false, false, 9)
    ]
  }
};

const ago = (minutes: number) => Math.floor(Date.now() / 1000) - minutes * 60;

export const activity = [
  { at: ago(4), kind: 'added', title: 'archlinux-2026.09.01-x86_64.iso', detail: 'Added from the feed Arch Linux releases' },
  { at: ago(38), kind: 'finished', title: 'debian-13.1.0-amd64-netinst.iso', detail: null },
  { at: ago(39), kind: 'moved', title: 'debian-13.1.0-amd64-netinst.iso', detail: 'Moved to /home/you/Downloads' },
  { at: ago(180), kind: 'stopped', title: 'LibriVox — The Wind in the Willows (audiobook, FLAC)', detail: 'Reached a ratio of 2' },
  { at: ago(60 * 26), kind: 'failed', title: 'broken-upload.torrent', detail: 'error decoding the torrent file' },
  { at: ago(60 * 27), kind: 'added', title: 'Cosmos Laundromat — open movie source files', detail: 'Added from the watch folder' },
  { at: ago(60 * 75), kind: 'added', title: 'ubuntu-24.04.1-desktop-amd64.iso', detail: 'Added by you' }
];
