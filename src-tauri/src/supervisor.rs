use std::collections::HashSet;
use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::commands::{forget_inner, restore_inner, AppState, RestoreToken};
use crate::engine::TorrentRow;
use crate::settings;

/// Background housekeeping that runs on the same clock as the UI tick:
/// queue limits, moving finished downloads out of the incomplete folder, and
/// telling you when something is done.
#[derive(Default)]
pub struct Supervisor {
    /// Torrents this queue paused, so user-paused ones are left alone.
    queued: HashSet<String>,
    /// Torrents already reported as finished.
    announced: HashSet<String>,
    /// Torrents currently being moved, so a slow move is not started twice.
    moving: HashSet<String>,
    started: bool,
}

impl Supervisor {
    pub async fn tick(&mut self, app: &AppHandle, state: &AppState, rows: &[TorrentRow]) {
        // Anything already finished when Greenhouse starts is not news.
        if !self.started {
            self.started = true;
            for row in rows.iter().filter(|r| r.finished) {
                self.announced.insert(row.info_hash.clone());
            }
        }

        self.announce_finished(app, state, rows);
        self.move_finished(app, state, rows).await;
        self.enforce_queue(state, rows).await;
    }

    fn announce_finished(&mut self, app: &AppHandle, state: &AppState, rows: &[TorrentRow]) {
        let notify = state.settings.read().notify_on_done;
        for row in rows.iter().filter(|r| r.finished) {
            if !self.announced.insert(row.info_hash.clone()) {
                continue;
            }
            if notify {
                let _ = app
                    .notification()
                    .builder()
                    .title("Download finished")
                    .body(&row.name)
                    .show();
            }
        }
        // Forget torrents that have gone, so a re-add can announce again.
        let live: HashSet<&str> = rows.iter().map(|r| r.info_hash.as_str()).collect();
        self.announced.retain(|h| live.contains(h.as_str()));
    }

    /// A finished torrent that downloaded into the incomplete folder is
    /// detached, moved to its real destination, and re-attached there.
    async fn move_finished(&mut self, app: &AppHandle, state: &AppState, rows: &[TorrentRow]) {
        let file = state.paths.pending_moves_file();
        let moves = settings::load_pending_moves(&file);
        if moves.is_empty() {
            return;
        }

        for row in rows.iter().filter(|r| r.finished && r.has_metadata) {
            let destination = match moves.get(&row.info_hash) {
                Some(d) => d.clone(),
                None => continue,
            };
            if !self.moving.insert(row.info_hash.clone()) {
                continue;
            }

            let from = PathBuf::from(&row.output_folder);
            let to = PathBuf::from(&destination);
            if from == to {
                self.clear_pending(state, &row.info_hash);
                self.moving.remove(&row.info_hash);
                continue;
            }

            let token = match forget_inner(state, row.id).await {
                Ok(t) => t,
                Err(_) => {
                    self.moving.remove(&row.info_hash);
                    continue;
                }
            };

            let moved = tokio::task::spawn_blocking({
                let from = from.clone();
                let to = to.clone();
                move || settings::move_contents(&from, &to)
            })
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("{e}")));

            let landed = match moved {
                Ok(()) => to.to_string_lossy().into_owned(),
                Err(e) => {
                    let _ = app
                        .notification()
                        .builder()
                        .title("Could not move a finished download")
                        .body(format!("{} stayed where it was: {e}", row.name))
                        .show();
                    token.output_folder.clone()
                }
            };

            let _ = restore_inner(
                state,
                RestoreToken {
                    output_folder: landed,
                    ..token
                },
            )
            .await;

            self.clear_pending(state, &row.info_hash);
            self.moving.remove(&row.info_hash);
        }
    }

    fn clear_pending(&self, state: &AppState, info_hash: &str) {
        let file = state.paths.pending_moves_file();
        let mut moves = settings::load_pending_moves(&file);
        if moves.remove(info_hash).is_some() {
            settings::save_pending_moves(&file, &moves);
        }
    }

    async fn enforce_queue(&mut self, state: &AppState, rows: &[TorrentRow]) {
        let (max_down, max_seed) = {
            let settings = state.settings.read();
            (settings.max_active_downloads, settings.max_active_seeds)
        };

        if max_down > 0 {
            let candidates: Vec<&TorrentRow> = rows
                .iter()
                .filter(|r| !r.finished && r.state != "error" && self.is_candidate(r))
                .collect();
            self.apply_limit(state, &candidates, max_down as usize).await;
        }
        if max_seed > 0 {
            let candidates: Vec<&TorrentRow> = rows
                .iter()
                .filter(|r| r.finished && r.state != "error" && self.is_candidate(r))
                .collect();
            self.apply_limit(state, &candidates, max_seed as usize).await;
        }
    }

    /// A torrent belongs to the queue if it is running, or if the queue is the
    /// reason it is stopped. Anything you paused yourself is left alone.
    fn is_candidate(&self, row: &TorrentRow) -> bool {
        match row.state.as_str() {
            "downloading" | "checking" | "seeding" => true,
            _ => self.queued.contains(&row.info_hash),
        }
    }

    async fn apply_limit(&mut self, state: &AppState, candidates: &[&TorrentRow], limit: usize) {
        // Oldest first, so what you added first runs first.
        let mut ordered: Vec<&&TorrentRow> = candidates.iter().collect();
        ordered.sort_by_key(|r| r.id);

        for (index, row) in ordered.iter().enumerate() {
            let should_run = index < limit;
            let is_stopped = row.state == "paused" || row.state == "complete";

            if should_run && is_stopped && self.queued.contains(&row.info_hash) {
                if state.api.api_torrent_action_start(row.id.into()).await.is_ok() {
                    self.queued.remove(&row.info_hash);
                }
            } else if !should_run && !is_stopped {
                if state.api.api_torrent_action_pause(row.id.into()).await.is_ok() {
                    self.queued.insert(row.info_hash.clone());
                }
            }
        }
    }
}
