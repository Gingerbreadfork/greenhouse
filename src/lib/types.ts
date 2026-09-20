export type TorrentState =
  | 'downloading'
  | 'seeding'
  | 'paused'
  | 'complete'
  | 'checking'
  | 'error';

export interface TorrentRow {
  id: number;
  info_hash: string;
  name: string;
  state: TorrentState;
  error: string | null;
  progress: number;
  progress_bytes: number;
  total_bytes: number;
  uploaded_bytes: number;
  download_speed: number;
  upload_speed: number;
  eta_seconds: number | null;
  peers_live: number;
  peers_connecting: number;
  peers_queued: number;
  peers_seen: number;
  finished: boolean;
  output_folder: string;
  tracker_count: number;
  ratio: number;
  has_metadata: boolean;
}

export interface SessionSummary {
  download_speed: number;
  upload_speed: number;
  peers_live: number;
  peers_connecting: number;
  peers_seen: number;
  uptime_seconds: number;
  dht_nodes: number | null;
  listen_port: number | null;
  download_limit_kbps: number;
  upload_limit_kbps: number;
  fetched_bytes: number;
  uploaded_bytes: number;
  blocked_incoming: number;
  blocked_outgoing: number;
  connect_tcp: number;
  connect_utp: number;
  connect_errors: number;
}

export interface TorrentFileEntry {
  index: number;
  name: string;
  components: string[];
  length: number;
  included: boolean;
  progress_bytes: number;
}

export interface PeerEntry {
  addr: string;
  state: string;
  conn_kind: string | null;
  client_name: string | null;
  fetched_bytes: number;
  uploaded_bytes: number;
}

export interface TorrentDetail {
  id: number;
  info_hash: string;
  name: string;
  output_folder: string;
  total_pieces: number;
  piece_size: number;
  files: TorrentFileEntry[];
  trackers: string[];
  peers: Record<string, unknown>;
  added_source: string | null;
}

export interface TrackerPack {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  source_url: string | null;
  updated_at: string | null;
  trackers: string[];
}

export interface Settings {
  download_dir: string;
  start_paused: boolean;
  auto_add: boolean;
  auto_apply_trackers: boolean;
  theme: 'system' | 'dark' | 'light';
  accent: string;
  compact: boolean;
  reduce_motion: boolean;
  confirm_remove: boolean;
  download_limit_kbps: number;
  upload_limit_kbps: number;
  listen_port: number;
  enable_upnp: boolean;
  transport: 'tcp' | 'utp' | 'both';
  peer_limit_per_torrent: number;
  incomplete_dir: string;
  skip_types_enabled: boolean;
  skip_types: string[];
  max_active_downloads: number;
  max_active_seeds: number;
  notify_on_done: boolean;
  packs: TrackerPack[];
}

export interface Bootstrap {
  settings: Settings;
  version: string;
  default_download_dir: string;
  home_dir: string;
}

export interface StagedFile {
  index: number;
  name: string;
  components: string[];
  length: number;
}

export interface StagedInfo {
  token: string;
  kind: 'magnet' | 'file' | 'url';
  source: string;
  info_hash: string | null;
  name: string | null;
  total_bytes: number;
  trackers: string[];
  files: StagedFile[];
  skipped: number[];
  resolved: boolean;
  needs_resolve: boolean;
}

export interface CommitResult {
  /** Null while the add is still waiting for metadata. */
  id: number | null;
  name: string;
  tracker_count: number;
  pending: boolean;
}

/** A confirmed add that is still waiting for peers to send its metadata. */
export interface PendingAdd {
  token: string;
  name: string;
  info_hash: string | null;
  waiting_seconds: number;
}

export interface RestoreToken {
  info_hash: string;
  name: string | null;
  output_folder: string;
  trackers: string[];
  only_files: number[] | null;
  paused: boolean;
}

export interface ApplyTrackersResult {
  id: number;
  tracker_count: number;
  added: number;
}
