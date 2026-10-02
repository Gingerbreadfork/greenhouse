import { activity, detail, feedStatus, filmFiles, session, settings, torrents } from '../fixtures';

const empty = typeof location !== 'undefined' && location.search.includes('empty');
let live = empty ? [] : torrents.map((t) => ({ ...t }));
const state = { settings: { ...settings }, session: { ...session } };

export const listeners = new Map<string, (e: { payload: unknown }) => void>();

const slow = typeof location !== 'undefined' && location.search.includes('slow');
const idle = typeof location !== 'undefined' && location.search.includes('idle');
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function invoke<T>(cmd: string, args?: Record<string, any>): Promise<T> {
  if (slow && (cmd === 'bootstrap' || cmd === 'list_torrents')) await delay(2500);
  switch (cmd) {
    case 'bootstrap':
      return {
        settings: state.settings,
        version: '0.2.0',
        default_download_dir: '/home/you/Downloads',
        home_dir: '/home/you',
        platform: 'linux',
        bound_interface: null,
        warnings: []
      } as T;
    case 'activity_list':
      return activity as T;
    case 'clear_activity':
      return undefined as T;
    case 'feeds_status':
      return feedStatus as T;
    case 'check_feed':
      await delay(700);
      return (feedStatus[args?.id as keyof typeof feedStatus] ?? {
        checked_at: Math.floor(Date.now() / 1000),
        error: null,
        items: []
      }) as T;
    case 'add_feed_item':
      return undefined as T;
    case 'play_file':
      return { player: 'mpv', streaming: true } as T;
    case 'blocklist_status':
      return { active: false, updated_at: null, bytes: 0 } as T;
    case 'refresh_blocklist':
      await delay(900);
      return { active: false, updated_at: Math.floor(Date.now() / 1000), bytes: 4_812_330 } as T;
    case 'quit':
      return undefined as T;
    case 'restart_engine':
      await delay(1200);
      return {
        bound_interface: state.settings.bind_interface || null,
        blocklist_active: Boolean(state.settings.blocklist_url),
        warnings: []
      } as T;
    case 'free_space':
      return (String(args?.path).includes('small') ? 3.2 : 212) * 1024 ** 3 as T;
    case 'list_interfaces':
      return [
        { name: 'enp8s0', up: true },
        { name: 'wg0', up: true },
        { name: 'wlan0', up: false }
      ] as T;
    case 'save_settings':
      state.settings = args?.next;
      return state.settings as T;
    case 'list_torrents':
      return live as T;
    case 'session_stats':
      return state.session as T;
    case 'torrent_detail':
      return { ...detail, id: args?.id, files: args?.id === 2 ? filmFiles : detail.files } as T;
    case 'stage_source':
      return {
        token: 'preview',
        kind: 'magnet',
        source: args?.request?.value ?? '',
        info_hash: '8c4adbf9ebe66f1d804fb6a4fb9b74966c3ab609',
        name: 'ubuntu-24.04.1-desktop-amd64.iso',
        total_bytes: 5.8 * 1024 ** 3,
        trackers: ['udp://tracker.opentrackr.org:1337/announce'],
        files: detail.files,
        skipped: [],
        resolved: true,
        needs_resolve: false
      } as T;
    case 'commit_staged':
      return {
        id: 99,
        name: 'ubuntu-24.04.1-desktop-amd64.iso',
        tracker_count: 15,
        pending: false,
        already: false
      } as T;
    case 'apply_trackers':
      return { id: args?.id, tracker_count: 20, added: 6 } as T;
    case 'apply_trackers_to_all':
      return live.length as T;
    case 'apply_trackers_many':
      return (args?.ids?.length ?? 0) as T;
    case 'magnet_link':
      return `magnet:?xt=urn:btih:${'0'.repeat(40)}&dn=preview` as T;
    case 'forget_torrent': {
      const gone = live.find((t) => t.id === args?.id);
      live = live.filter((t) => t.id !== args?.id);
      return {
        info_hash: gone?.info_hash ?? '',
        name: gone?.name ?? null,
        output_folder: gone?.output_folder ?? '',
        trackers: [],
        only_files: null,
        paused: false
      } as T;
    }
    case 'restore_torrent':
      return 0 as T;
    case 'test_notification':
      return undefined as T;
    case 'torrent_action':
      if (args?.action === 'delete' || args?.action === 'forget') {
        live = live.filter((t) => t.id !== args?.id);
      } else if (args?.action === 'pause' || args?.action === 'start') {
        live = live.map((t) =>
          t.id === args?.id
            ? { ...t, state: args.action === 'pause' ? 'paused' : 'downloading' }
            : t
        );
      }
      return undefined as T;
    case 'fetch_tracker_list':
      return ['udp://tracker.opentrackr.org:1337/announce'] as T;
    default:
      return undefined as T;
  }
}

/** Drives the same tick the Rust side emits, so speeds and bars move. */
export function startTicker() {
  let phase = 0;
  setInterval(() => {
    phase += 1;
    live = live.map((t) => {
      if (t.state !== 'downloading' || t.total_bytes === 0) return t;
      const step = t.download_speed * 0.9;
      const progress_bytes = Math.min(t.total_bytes, t.progress_bytes + step);
      return { ...t, progress_bytes, progress: progress_bytes / t.total_bytes };
    });
    const wobble = 1 + Math.sin(phase / 3) * 0.28;
    state.session = {
      ...state.session,
      download_speed: idle ? 0 : Math.round(session.download_speed * wobble),
      upload_speed: idle ? 0 : Math.round(session.upload_speed * (2 - wobble))
    };
    listeners.get('greenhouse://tick')?.({
      payload: { torrents: live, pending: [], session: state.session }
    });
  }, 900);
}
