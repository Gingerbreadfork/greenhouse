import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import { checkForUpdate, relaunch, type UpdateHandle } from './updates';
import type {
  ActivityEntry,
  CommitResult,
  PendingAdd,
  SessionSummary,
  Settings,
  TorrentRow
} from './types';

export type View =
  | 'torrents'
  | 'trackers'
  | 'feeds'
  | 'activity'
  | 'stats'
  | 'settings'
  | 'about';
export type Filter = 'all' | 'downloading' | 'seeding' | 'paused' | 'finished' | 'issues';
export type SortKey = 'added' | 'name' | 'progress' | 'size' | 'down' | 'up' | 'ratio';

export interface Toast {
  id: number;
  tone: 'info' | 'good' | 'bad';
  text: string;
  detail?: string;
  action?: { label: string; run: () => void };
}

const EMPTY_SESSION: SessionSummary = {
  download_speed: 0,
  upload_speed: 0,
  peers_live: 0,
  peers_connecting: 0,
  peers_seen: 0,
  uptime_seconds: 0,
  dht_nodes: null,
  listen_port: null,
  download_limit_kbps: 0,
  upload_limit_kbps: 0,
  fetched_bytes: 0,
  uploaded_bytes: 0,
  blocked_incoming: 0,
  blocked_outgoing: 0,
  connect_tcp: 0,
  connect_utp: 0,
  connect_errors: 0
};

const HISTORY = 90;
const UPDATE_EVERY = 6 * 60 * 60 * 1000;

class Store {
  ready = $state(false);
  version = $state('');
  homeDir = $state('');
  platform = $state('');
  defaultDir = $state('');
  /** The interface traffic is tied to for this run, if any. */
  boundInterface = $state<string | null>(null);

  /** A newer release the updater found, until it is installed. */
  update = $state<UpdateHandle | null>(null);
  /** Download progress from 0 to 1 while an update installs. */
  updateProgress = $state<number | null>(null);
  updateError = $state('');
  updateCheckedAt = $state<number | null>(null);
  private updateTimer: ReturnType<typeof setInterval> | null = null;

  torrents = $state.raw<TorrentRow[]>([]);
  pending = $state.raw<PendingAdd[]>([]);
  /** What Greenhouse has done, newest first. */
  activity = $state.raw<ActivityEntry[]>([]);
  /** Entries that arrived while the activity view was not open. */
  unseenActivity = $state(0);
  session = $state.raw<SessionSummary>(EMPTY_SESSION);
  settings = $state<Settings | null>(null);
  /** The network settings the running engine was started with. */
  private appliedNetwork = $state('');
  /** Network settings have changed since the engine started. */
  engineStale = $derived(
    this.settings !== null && networkKey(this.settings) !== this.appliedNetwork
  );
  restartingEngine = $state(false);

  view = $state<View>('torrents');
  filter = $state<Filter>('all');
  query = $state('');
  sortKey = $state<SortKey>('added');
  sortDir = $state<'asc' | 'desc'>('asc');

  /** Ids of every selected row. */
  selection = $state<number[]>([]);
  /** The row that holds keyboard focus, and the one the inspector describes. */
  cursor = $state<number | null>(null);
  /** Where a shift-range selection started. */
  anchor = $state<number | null>(null);
  inspectorOpen = $state(false);

  downHistory = $state.raw<number[]>(new Array(HISTORY).fill(0));
  upHistory = $state.raw<number[]>(new Array(HISTORY).fill(0));
  /** How much of the history buffer holds real readings yet. */
  samples = $state(0);
  /** The graph waits for something to actually transfer before it starts. */
  recording = $state(false);
  readonly historySize = HISTORY;

  toasts = $state<Toast[]>([]);
  private toastSeq = 0;

  /** What the desktop is currently asking for. */
  systemDark = $state(prefersDark());
  /** The theme actually in use, once "system" has been resolved. */
  resolvedTheme = $derived<'dark' | 'light'>(
    !this.settings || this.settings.theme === 'system'
      ? this.systemDark
        ? 'dark'
        : 'light'
      : this.settings.theme
  );

  counts = $derived.by(() => {
    const c = { all: 0, downloading: 0, seeding: 0, paused: 0, finished: 0, issues: 0 };
    for (const t of this.torrents) {
      c.all += 1;
      if (t.state === 'downloading' || t.state === 'checking') c.downloading += 1;
      if (t.state === 'seeding') c.seeding += 1;
      if (t.state === 'paused' || t.state === 'complete') c.paused += 1;
      if (t.finished) c.finished += 1;
      if (t.state === 'error') c.issues += 1;
    }
    return c;
  });

  visible = $derived.by(() => {
    const q = this.query.trim().toLowerCase();
    const isSearch = q.length > 0 && !looksLikeSource(q);
    let list = this.torrents.filter((t) => {
      switch (this.filter) {
        case 'downloading':
          return t.state === 'downloading' || t.state === 'checking';
        case 'seeding':
          return t.state === 'seeding';
        case 'paused':
          return t.state === 'paused' || t.state === 'complete';
        case 'finished':
          return t.finished;
        case 'issues':
          return t.state === 'error';
        default:
          return true;
      }
    });
    if (isSearch) list = list.filter((t) => t.name.toLowerCase().includes(q));

    const dir = this.sortDir === 'asc' ? 1 : -1;
    const key = this.sortKey;
    return [...list].sort((a, b) => {
      switch (key) {
        case 'name':
          return a.name.localeCompare(b.name, undefined, { numeric: true }) * dir;
        case 'progress':
          return (a.progress - b.progress) * dir;
        case 'size':
          return (a.total_bytes - b.total_bytes) * dir;
        case 'down':
          return (a.download_speed - b.download_speed) * dir;
        case 'up':
          return (a.upload_speed - b.upload_speed) * dir;
        case 'ratio':
          return (a.ratio - b.ratio) * dir;
        default:
          return (a.id - b.id) * dir;
      }
    });
  });

  selected = $derived(this.torrents.find((t) => t.id === this.cursor) ?? null);
  selectedTorrents = $derived(this.torrents.filter((t) => this.selection.includes(t.id)));
  isSelected = (id: number) => this.selection.includes(id);

  async init() {
    watchSystemTheme((dark) => {
      this.systemDark = dark;
      this.applyChrome();
    });
    const boot = await api.bootstrap();
    this.settings = boot.settings;
    this.version = boot.version;
    this.homeDir = boot.home_dir;
    this.platform = boot.platform;
    this.defaultDir = boot.default_download_dir;
    this.boundInterface = boot.bound_interface;
    this.appliedNetwork = networkKey(boot.settings);
    for (const warning of boot.warnings) this.toast('Check your network settings', 'bad', warning);
    this.applyChrome();
    this.torrents = await api.listTorrents();
    this.ready = true;

    await listen<{ torrents: TorrentRow[]; pending: PendingAdd[]; session: SessionSummary }>(
      'greenhouse://tick',
      (event) => {
        this.torrents = event.payload.torrents;
        this.pending = event.payload.pending;
        this.session = event.payload.session;
        this.pruneSelection();
        this.recordActivity(
          event.payload.session.download_speed,
          event.payload.session.upload_speed
        );
      }
    );
    await this.watchPendingAdds();
    this.activity = await api.activityList();
    await listen<ActivityEntry>('greenhouse://activity', (event) => {
      this.activity = [event.payload, ...this.activity];
      if (this.view !== 'activity') this.unseenActivity += 1;
    });
    this.scheduleUpdateChecks();
  }

  /** Checks for a newer release now and every few hours, while the setting is on. */
  scheduleUpdateChecks() {
    if (this.updateTimer) clearInterval(this.updateTimer);
    this.updateTimer = null;
    if (!this.settings?.check_updates) return;
    void this.checkForUpdate();
    this.updateTimer = setInterval(() => void this.checkForUpdate(), UPDATE_EVERY);
  }

  /** Looks for a newer release. A check asked for by hand always reports back. */
  async checkForUpdate(byHand = false) {
    try {
      const found = await checkForUpdate();
      this.updateCheckedAt = Date.now();
      this.updateError = '';
      if (!found) {
        this.update = null;
        if (byHand) this.toast('Greenhouse is up to date', 'good');
        return;
      }
      const fresh = this.update?.version !== found.version;
      this.update = found;
      if (this.settings?.install_updates) {
        await this.installUpdate();
      } else if (fresh || byHand) {
        this.toast(`Greenhouse ${found.version} is available`, 'info', undefined, {
          label: 'Install',
          run: () => void this.installUpdate()
        });
      }
    } catch (e) {
      this.updateError = String(e);
      if (byHand) this.toast('Could not check for updates', 'bad', String(e));
    }
  }

  /** Downloads and installs the release found, then restarts. */
  async installUpdate() {
    const update = this.update;
    if (!update || this.updateProgress !== null) return;
    this.updateProgress = 0;
    try {
      await update.install((fraction) => (this.updateProgress = fraction));
      this.toast(`Greenhouse ${update.version} is installed`, 'good', 'Restarting…');
      await relaunch();
      this.updateProgress = null;
    } catch (e) {
      this.updateProgress = null;
      this.updateError = String(e);
      this.toast('The update could not be installed', 'bad', String(e));
    }
  }

  /** Restarts the engine so changed network settings take effect now. */
  async restartEngine() {
    if (!this.settings || this.restartingEngine) return;
    this.restartingEngine = true;
    const wanted = networkKey(this.settings);
    try {
      const info = await api.restartEngine();
      this.boundInterface = info.bound_interface;
      if (info.warnings.length === 0) this.appliedNetwork = wanted;
      for (const warning of info.warnings) this.toast('Check your network settings', 'bad', warning);
      if (info.warnings.length === 0) this.toast('Network settings applied', 'good');
      await this.refresh();
    } catch (e) {
      this.toast('The engine could not be restarted', 'bad', String(e));
    } finally {
      this.restartingEngine = false;
    }
  }

  openActivity() {
    this.view = 'activity';
    this.unseenActivity = 0;
  }

  async clearActivity() {
    await api.clearActivity();
    this.activity = [];
  }

  /** Adds that finish in the background report back here. */
  private async watchPendingAdds() {
    await listen<CommitResult>('greenhouse://added', (event) => this.announceAdd(event.payload));
    await listen<{ name: string | null; error: string }>('greenhouse://add-failed', (event) => {
      this.toast(
        `Could not add ${event.payload.name ?? 'that magnet link'}`,
        'bad',
        event.payload.error
      );
    });
  }

  /** Says what became of an add: queued, already here, or added. */
  announceAdd(result: CommitResult) {
    if (result.pending) {
      this.toast(
        `Looking for ${result.name}`,
        'info',
        'It will be added as soon as a peer sends its details'
      );
      return;
    }
    this.refresh();
    if (result.already) {
      this.toast(`${result.name} is already in your list`, 'info');
      return;
    }
    this.toast(`Added ${result.name}`, 'good', trackerNote(result.tracker_count));
  }

  async cancelPending(token: string) {
    this.pending = this.pending.filter((p) => p.token !== token);
    await api.discardStaged(token);
  }

  /** Recording starts at the first byte transferred. */
  private recordActivity(down: number, up: number) {
    if (!this.recording) {
      if (down <= 0 && up <= 0) return;
      this.recording = true;
    }
    this.downHistory = push(this.downHistory, down);
    this.upHistory = push(this.upHistory, up);
    if (this.samples < HISTORY) this.samples += 1;
  }

  /** Theme and motion live on the root element so CSS can key off them. */
  applyChrome() {
    const s = this.settings;
    if (!s) return;
    const root = document.documentElement;
    root.dataset.theme = this.resolvedTheme;
    root.dataset.accent = s.accent;
    root.dataset.motion = s.reduce_motion ? 'off' : 'on';
    root.dataset.density = s.compact ? 'compact' : 'roomy';
  }

  async patchSettings(patch: Partial<Settings>) {
    if (!this.settings) return;
    const next = { ...this.settings, ...patch };
    this.settings = await api.saveSettings(next);
    this.applyChrome();
    if ('check_updates' in patch) this.scheduleUpdateChecks();
  }

  toast(
    text: string,
    tone: Toast['tone'] = 'info',
    detail?: string,
    action?: Toast['action']
  ) {
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, tone, text, detail, action }];
    const life = tone === 'bad' ? 7000 : action ? 8000 : 4200;
    setTimeout(() => this.dismiss(id), life);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  async refresh() {
    this.torrents = await api.listTorrents();
  }

  // ------------------------------------------------------------- selection

  selectOnly(id: number) {
    this.selection = [id];
    this.cursor = id;
    this.anchor = id;
    this.inspectorOpen = true;
  }

  toggleSelect(id: number) {
    this.selection = this.selection.includes(id)
      ? this.selection.filter((s) => s !== id)
      : [...this.selection, id];
    this.cursor = id;
    this.anchor = id;
    if (this.selection.length === 0) this.inspectorOpen = false;
  }

  /** Selects everything between the shift anchor and `id`, in view order. */
  extendTo(id: number) {
    const ids = this.visible.map((t) => t.id);
    const from = ids.indexOf(this.anchor ?? id);
    const to = ids.indexOf(id);
    if (from === -1 || to === -1) return this.selectOnly(id);
    const [lo, hi] = from < to ? [from, to] : [to, from];
    this.selection = ids.slice(lo, hi + 1);
    this.cursor = id;
  }

  moveCursor(delta: number, extend = false) {
    const ids = this.visible.map((t) => t.id);
    if (ids.length === 0) return;
    const at = this.cursor === null ? -1 : ids.indexOf(this.cursor);
    const next = ids[Math.min(ids.length - 1, Math.max(0, at + delta))] ?? ids[0];
    if (extend) this.extendTo(next);
    else this.selectOnly(next);
  }

  cursorTo(index: number, extend = false) {
    const ids = this.visible.map((t) => t.id);
    const id = ids.at(index);
    if (id === undefined) return;
    if (extend) this.extendTo(id);
    else this.selectOnly(id);
  }

  selectAllVisible() {
    const ids = this.visible.map((t) => t.id);
    this.selection = ids;
    if (this.cursor === null && ids.length) this.cursor = ids[0];
  }

  clearSelection() {
    this.selection = [];
    this.cursor = null;
    this.anchor = null;
    this.inspectorOpen = false;
  }

  closeInspector() {
    this.inspectorOpen = false;
  }

  /** Drops ids that are no longer in the session. */
  private pruneSelection() {
    const live = new Set(this.torrents.map((t) => t.id));
    if (this.selection.some((id) => !live.has(id))) {
      this.selection = this.selection.filter((id) => live.has(id));
    }
    if (this.cursor !== null && !live.has(this.cursor)) {
      this.cursor = this.selection.at(-1) ?? null;
      if (this.cursor === null) this.inspectorOpen = false;
    }
  }
}

/** The desktop's current preference, defaulting to dark where unknown. */
export function prefersDark(): boolean {
  try {
    return !window.matchMedia('(prefers-color-scheme: light)').matches;
  } catch {
    return true;
  }
}

function watchSystemTheme(onChange: (dark: boolean) => void) {
  try {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    media.addEventListener('change', (e) => onChange(e.matches));
  } catch {
    // Nothing to follow; the stored theme still applies.
  }
}

function push(series: number[], value: number): number[] {
  const next = series.slice(1);
  next.push(value);
  return next;
}

/** The settings that only take effect when the engine starts. */
function networkKey(s: Settings): string {
  return JSON.stringify([
    s.listen_port,
    s.enable_upnp,
    s.transport,
    s.peer_limit_per_torrent,
    s.bind_interface,
    s.blocklist_url
  ]);
}

function trackerNote(count: number): string {
  return count > 0 ? `${count} trackers attached` : 'No trackers attached, so it will rely on DHT';
}

export function looksLikeSource(text: string): boolean {
  const t = text.trim().toLowerCase();
  return (
    t.startsWith('magnet:') ||
    t.startsWith('http://') ||
    t.startsWith('https://') ||
    /^[0-9a-f]{40}$/.test(t)
  );
}

export const store = new Store();
