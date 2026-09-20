import { invoke } from '@tauri-apps/api/core';
import type {
  ApplyTrackersResult,
  RestoreToken,
  Bootstrap,
  BlocklistStatus,
  CommitResult,
  NetInterface,
  SessionSummary,
  Settings,
  StagedInfo,
  TorrentDetail,
  TorrentRow
} from './types';

export const api = {
  bootstrap: () => invoke<Bootstrap>('bootstrap'),
  listInterfaces: () => invoke<NetInterface[]>('list_interfaces'),
  blocklistStatus: () => invoke<BlocklistStatus>('blocklist_status'),
  refreshBlocklist: () => invoke<BlocklistStatus>('refresh_blocklist'),
  saveSettings: (next: Settings) => invoke<Settings>('save_settings', { next }),
  listTorrents: () => invoke<TorrentRow[]>('list_torrents'),
  sessionStats: () => invoke<SessionSummary>('session_stats'),
  torrentDetail: (id: number) => invoke<TorrentDetail>('torrent_detail', { id }),
  torrentAction: (id: number, action: 'pause' | 'start' | 'forget' | 'delete') =>
    invoke<void>('torrent_action', { id, action }),
  setFileSelection: (id: number, indices: number[]) =>
    invoke<void>('set_file_selection', { id, indices }),

  stageSource: (kind: 'text' | 'file', value: string) =>
    invoke<StagedInfo>('stage_source', { request: { kind, value } }),
  resolveStaged: (token: string) => invoke<StagedInfo>('resolve_staged', { token }),
  discardStaged: (token: string) => invoke<void>('discard_staged', { token }),
  commitStaged: (request: {
    token: string;
    download_dir: string;
    start_paused: boolean;
    use_auto_packs: boolean;
    pack_ids: string[];
    extra_trackers: string[];
    selected_files: number[] | null;
  }) => invoke<CommitResult>('commit_staged', { request }),

  parseTrackers: (blob: string) => invoke<string[]>('parse_trackers', { blob }),
  testNotification: () => invoke<void>('test_notification'),
  fetchTrackerList: (url: string) => invoke<string[]>('fetch_tracker_list', { url }),
  applyTrackers: (id: number, trackers: string[], replace: boolean) =>
    invoke<ApplyTrackersResult>('apply_trackers', { id, trackers, replace }),
  applyTrackersToAll: (trackers: string[]) =>
    invoke<number>('apply_trackers_to_all', { trackers }),
  applyTrackersMany: (ids: number[], trackers: string[]) =>
    invoke<number>('apply_trackers_many', { ids, trackers }),
  magnetLink: (id: number) => invoke<string>('magnet_link', { id }),
  forgetTorrent: (id: number) => invoke<RestoreToken>('forget_torrent', { id }),
  restoreTorrent: (token: RestoreToken) => invoke<number>('restore_torrent', { token })
};
