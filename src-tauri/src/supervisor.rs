use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::activity;
use crate::commands::{forget_inner, restore_inner, AppState, RestoreToken};
use crate::engine::TorrentRow;
use crate::settings::{self, MoveFailed};

/// Torrents the queue paused, kept on disk so they are still the queue's to
/// start after a restart. Ones you paused yourself are never in here.
pub struct QueueHolds {
    held: HashSet<String>,
    file: PathBuf,
}

impl QueueHolds {
    pub fn load(file: PathBuf) -> Self {
        let held = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { held, file }
    }

    pub fn contains(&self, info_hash: &str) -> bool {
        self.held.contains(info_hash)
    }

    pub fn insert(&mut self, info_hash: String) {
        if self.held.insert(info_hash) {
            self.save();
        }
    }

    pub fn remove(&mut self, info_hash: &str) {
        if self.held.remove(info_hash) {
            self.save();
        }
    }

    pub fn clear(&mut self) {
        if !self.held.is_empty() {
            self.held.clear();
            self.save();
        }
    }

    fn retain(&mut self, live: &HashSet<&str>) {
        let before = self.held.len();
        self.held.retain(|h| live.contains(h.as_str()));
        if self.held.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_vec(&self.held) {
            let _ = std::fs::write(&self.file, json);
        }
    }
}

/// Background housekeeping that runs on the same clock as the UI tick:
/// queue limits, moving finished downloads out of the incomplete folder, and
/// telling you when something is done.
#[derive(Default)]
pub struct Supervisor {
    /// Torrents already reported as finished.
    announced: HashSet<String>,
    started: bool,
    last_tick: Option<Instant>,
    last_saved: Option<Instant>,
}

impl Supervisor {
    pub async fn tick<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        rows: &[TorrentRow],
    ) {
        if !self.started {
            self.started = true;
            let live: HashSet<&str> = rows.iter().map(|r| r.info_hash.as_str()).collect();
            state.seeding.lock().retain(&live);
            state.queue_paused.lock().retain(&live);
            settings::retain_pending_moves(&state.paths.pending_moves_file(), &live);
        }

        self.announce_finished(app, state, rows);
        self.move_finished(app, state, rows);
        self.track_seeding(state, rows);
        self.enforce_seed_limits(app, state, rows).await;
        self.enforce_queue(state, rows).await;
    }

    /// Adds this tick's upload and seeding time to the running totals.
    fn track_seeding(&mut self, state: &AppState, rows: &[TorrentRow]) {
        let now = Instant::now();
        let elapsed = self.last_tick.map(|t| now - t).unwrap_or_default();
        self.last_tick = Some(now);

        let mut ledger = state.seeding.lock();
        for row in rows {
            ledger.record(row, elapsed.as_millis() as u64);
        }
        if self.last_saved.is_none_or(|t| now - t > Duration::from_secs(30)) {
            self.last_saved = Some(now);
            ledger.save_if_dirty(&state.paths.seeding_file());
        }
    }

    /// Pauses seeding torrents that have reached the ratio or time limit.
    async fn enforce_seed_limits<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        rows: &[TorrentRow],
    ) {
        let due: Vec<(&TorrentRow, String)> = {
            let settings = state.settings.read();
            let ledger = state.seeding.lock();
            rows.iter()
                .filter_map(|r| Some((r, ledger.limit_reached(r, &settings)?)))
                .collect()
        };
        for (row, reason) in due {
            if state.api().api_torrent_action_pause(row.id.into()).await.is_ok() {
                {
                    let mut ledger = state.seeding.lock();
                    ledger.mark_done(&row.info_hash);
                    ledger.save_if_dirty(&state.paths.seeding_file());
                }
                activity::record(app, "stopped", row.name.clone(), Some(reason));
            }
        }
    }

    /// Reports each download that has just finished. A torrent that was
    /// complete when it started, or only had its files checked, is not news.
    pub(crate) fn announce_finished<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        rows: &[TorrentRow],
    ) {
        let notify = state.settings.read().notify_on_done;
        for row in rows.iter().filter(|r| r.finished) {
            if !self.announced.insert(row.info_hash.clone()) || row.downloaded_now == 0 {
                continue;
            }
            activity::record(app, "finished", row.name.clone(), None);
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

    /// Starts moving each finished torrent out of the incomplete folder. A
    /// move runs on its own, so a slow one does not hold up the tick.
    pub(crate) fn move_finished<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        rows: &[TorrentRow],
    ) {
        let file = state.paths.pending_moves_file();
        let moves = settings::load_pending_moves(&file);
        if moves.is_empty() {
            return;
        }

        for row in rows.iter().filter(|r| r.finished && r.has_metadata) {
            let Some(destination) = moves.get(&row.info_hash) else {
                continue;
            };
            if PathBuf::from(&row.output_folder) == PathBuf::from(destination) {
                settings::drop_pending_move(&file, &row.info_hash);
                continue;
            }
            let shown = TorrentRow {
                state: "moving".into(),
                output_folder: destination.clone(),
                download_speed: 0,
                upload_speed: 0,
                ..row.clone()
            };
            let started = state.moving.lock().insert(row.info_hash.clone(), shown);
            if started.is_none() {
                let task = move_one(app.clone(), row.clone(), destination.clone());
                tauri::async_runtime::spawn(task);
            }
        }
    }

    pub(crate) async fn enforce_queue(&mut self, state: &AppState, rows: &[TorrentRow]) {
        // No limit still starts whatever an earlier limit was holding back.
        let (max_down, max_seed) = {
            let settings = state.settings.read();
            let limit = |max: u32| if max == 0 { usize::MAX } else { max as usize };
            (limit(settings.max_active_downloads), limit(settings.max_active_seeds))
        };

        let downloads: Vec<&TorrentRow> = rows
            .iter()
            .filter(|r| !r.finished && r.state != "error" && self.is_candidate(state, r))
            .collect();
        self.apply_limit(state, &downloads, max_down).await;

        let seeds: Vec<&TorrentRow> = rows
            .iter()
            .filter(|r| r.finished && r.state != "error" && self.is_candidate(state, r))
            .collect();
        self.apply_limit(state, &seeds, max_seed).await;
    }

    /// A torrent belongs to the queue if it is running, or if the queue is the
    /// reason it is stopped. Anything you paused yourself is left alone.
    fn is_candidate(&self, state: &AppState, row: &TorrentRow) -> bool {
        match row.state.as_str() {
            "downloading" | "checking" | "seeding" => true,
            _ => state.queue_paused.lock().contains(&row.info_hash),
        }
    }

    async fn apply_limit(&mut self, state: &AppState, candidates: &[&TorrentRow], limit: usize) {
        // Oldest first, so what you added first runs first.
        let mut ordered: Vec<&&TorrentRow> = candidates.iter().collect();
        ordered.sort_by_key(|r| r.id);

        for (index, row) in ordered.iter().enumerate() {
            let should_run = index < limit;
            let is_stopped = row.state == "paused" || row.state == "complete";
            // A file check that is paused never finishes, so it is left to run.
            let is_running = row.state == "downloading" || row.state == "seeding";

            let held = state.queue_paused.lock().contains(&row.info_hash);
            if should_run && is_stopped && held {
                if state.api().api_torrent_action_start(row.id.into()).await.is_ok() {
                    state.queue_paused.lock().remove(&row.info_hash);
                }
            } else if !should_run
                && is_running
                && state.api().api_torrent_action_pause(row.id.into()).await.is_ok()
            {
                state.queue_paused.lock().insert(row.info_hash.clone());
            }
        }
    }
}

/// Detaches a finished torrent, moves its files to where they belong and
/// puts it back there.
async fn move_one<R: Runtime>(app: AppHandle<R>, row: TorrentRow, destination: String) {
    let state = app.state::<AppState>();
    let from = PathBuf::from(&row.output_folder);
    let to = PathBuf::from(&destination);

    // Still in the incomplete folder, so the next tick tries again.
    let Ok(token) = forget_inner(&state, row.id).await else {
        state.moving.lock().remove(&row.info_hash);
        return;
    };

    let moved = tokio::task::spawn_blocking({
        let (from, to) = (from.clone(), to.clone());
        move || settings::move_contents(&from, &to)
    })
    .await
    .unwrap_or_else(|e| Err(MoveFailed::Stayed(anyhow::anyhow!("{e}"))));

    let restore = match moved {
        Ok(()) => {
            let place = to.to_string_lossy().into_owned();
            let note = format!("Moved to {place}");
            activity::record(&app, "moved", row.name.clone(), Some(note));
            RestoreToken {
                output_folder: place,
                ..token
            }
        }
        Err(MoveFailed::Stayed(e)) => {
            let why = format!("Could not be moved to {}: {e}", to.display());
            let body = format!("{} stayed where it was: {e}", row.name);
            report_failed_move(&app, &row.name, why, body);
            token
        }
        // Left paused so it does not download again what was moved.
        Err(MoveFailed::Split(e)) => {
            let why = format!(
                "Only part of it reached {}, the rest is still in {}: {e}",
                to.display(),
                from.display()
            );
            report_failed_move(&app, &row.name, why.clone(), format!("{}: {why}", row.name));
            RestoreToken {
                paused: true,
                ..token
            }
        }
    };

    settings::drop_pending_move(&state.paths.pending_moves_file(), &row.info_hash);
    let mut retrying = false;
    while let Err(e) = restore_inner(&state, restore.clone()).await {
        if !retrying {
            let why = format!("Could not be put back in the list, trying again: {e}");
            activity::record(&app, "failed", row.name.clone(), Some(why));
            retrying = true;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if retrying {
        activity::record(&app, "moved", row.name.clone(), Some("Back in the list".into()));
    }
    state.moving.lock().remove(&row.info_hash);
}

fn report_failed_move<R: Runtime>(app: &AppHandle<R>, name: &str, note: String, body: String) {
    activity::record(app, "failed", name.to_string(), Some(note));
    let _ = app
        .notification()
        .builder()
        .title("Could not move a finished download")
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_holds_last_between_runs() {
        let file = std::env::temp_dir().join(format!("gh-holds-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut holds = QueueHolds::load(file.clone());
        holds.insert("aaa".into());
        holds.insert("bbb".into());
        holds.remove("bbb");

        let mut again = QueueHolds::load(file.clone());
        assert!(again.contains("aaa"));
        assert!(!again.contains("bbb"));

        again.retain(&HashSet::new());
        assert!(!QueueHolds::load(file.clone()).contains("aaa"));
        std::fs::remove_file(&file).ok();
    }
}
