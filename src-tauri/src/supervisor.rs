use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::activity;
use crate::commands::{forget_inner, restore_inner, AppState, RestoreToken};
use crate::engine::TorrentRow;
use crate::settings::{self, MoveFailed, Volume};

/// How often free disk space is read.
const SPACE_CHECK_EVERY: Duration = Duration::from_secs(3);

/// Torrents Greenhouse paused on its own, kept on disk so they are still its
/// to start after a restart. Ones you paused yourself are never in here.
pub struct Holds {
    held: HashSet<String>,
    file: PathBuf,
}

impl Holds {
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

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    pub fn hashes(&self) -> HashSet<String> {
        self.held.clone()
    }

    /// False when it was held already.
    pub fn insert(&mut self, info_hash: String) -> bool {
        let added = self.held.insert(info_hash);
        if added {
            self.save();
        }
        added
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
            let _ = settings::write_atomic(&self.file, json);
        }
    }
}

/// What the disk guard does with the torrents it looked at, by info hash.
#[derive(Debug, Default, PartialEq)]
pub struct SpaceVerdict {
    /// Downloads to stop and hold until there is room.
    pub hold: Vec<String>,
    /// Held torrents that can carry on.
    pub release: Vec<String>,
}

/// Room wanted beyond the minimum before held downloads carry on, so one
/// that is let go is not stopped again a moment later.
fn headroom(minimum: u64) -> u64 {
    (minimum / 10).max(128 * 1024 * 1024)
}

/// Decides which downloads have too little room and which have enough again.
/// `held` are the disk guard's, `queued` the queue's, `free` is the room on
/// the volume of an output folder and `minimum` the room to keep, 0 for off.
pub fn judge_space(
    rows: &[TorrentRow],
    held: &HashSet<String>,
    queued: &HashSet<String>,
    free: impl Fn(&str) -> Option<u64>,
    minimum: u64,
) -> SpaceVerdict {
    let mut verdict = SpaceVerdict::default();
    for row in rows {
        let hash = &row.info_hash;
        let free = || free(&row.output_folder);
        if held.contains(hash) {
            // A file check or an error is left to settle first.
            if matches!(row.state.as_str(), "checking" | "error") {
                continue;
            }
            if minimum == 0 || row.finished {
                verdict.release.push(hash.clone());
                continue;
            }
            let Some(free) = free() else {
                continue;
            };
            if row.stopped {
                if free >= minimum + headroom(minimum) {
                    verdict.release.push(hash.clone());
                }
            } else if free < minimum {
                // You started it, and there is still no room.
                verdict.hold.push(hash.clone());
            } else {
                verdict.release.push(hash.clone());
            }
        } else if minimum > 0 && !row.finished && row.has_metadata {
            // The queue would start the next in line once this one stops.
            let waiting_its_turn = row.state == "paused" && queued.contains(hash);
            let short = || free().is_some_and(|free| free < minimum);
            if (row.state == "downloading" || waiting_its_turn) && short() {
                verdict.hold.push(hash.clone());
            }
        }
    }
    verdict
}

/// Downloads the disk guard stopped or let go in one check, by name, each
/// with the volume it is on when that was read.
#[derive(Debug, Default, PartialEq)]
pub struct SpaceChanges {
    pub stopped: Vec<(Option<Volume>, String)>,
    pub resumed: Vec<(Option<Volume>, String)>,
    /// Volumes that had a download held on them before this check.
    pub already_short: HashSet<String>,
}

/// Carries out the disk guard's verdict. A download it lets go is handed to
/// the queue, which starts it or holds it until its turn comes.
pub(crate) async fn settle_space(
    state: &AppState,
    rows: &[TorrentRow],
    volumes: &HashMap<String, Volume>,
    minimum: u64,
) -> SpaceChanges {
    let held = state.disk_paused.lock().hashes();
    let queued = state.queue_paused.lock().hashes();
    let free = |folder: &str| volumes.get(folder).map(|v| v.free);
    let verdict = judge_space(rows, &held, &queued, free, minimum);
    let row_of = |hash: &String| rows.iter().find(|r| &r.info_hash == hash);
    let mut changes = SpaceChanges {
        already_short: rows
            .iter()
            .filter(|r| held.contains(&r.info_hash))
            .filter_map(|r| volumes.get(&r.output_folder))
            .map(|v| v.root.clone())
            .collect(),
        ..Default::default()
    };

    for row in verdict.hold.iter().filter_map(row_of) {
        let paused =
            row.stopped || state.api().api_torrent_action_pause(row.id.into()).await.is_ok();
        if !paused {
            continue;
        }
        state.queue_paused.lock().remove(&row.info_hash);
        if state.disk_paused.lock().insert(row.info_hash.clone()) {
            let volume = volumes.get(&row.output_folder).cloned();
            changes.stopped.push((volume, row.name.clone()));
        }
    }
    for row in verdict.release.iter().filter_map(row_of) {
        state.disk_paused.lock().remove(&row.info_hash);
        if !row.stopped {
            continue;
        }
        state.queue_paused.lock().insert(row.info_hash.clone());
        if !row.finished {
            let volume = volumes.get(&row.output_folder).cloned();
            changes.resumed.push((volume, row.name.clone()));
        }
    }
    changes
}

/// Something the disk guard did, as the activity log words it.
#[derive(Debug, PartialEq)]
pub struct SpaceNote {
    pub kind: &'static str,
    pub title: String,
    pub detail: Option<String>,
}

/// Names gathered under the volume they share.
fn by_volume(named: Vec<(Option<Volume>, String)>) -> Vec<(Option<Volume>, Vec<String>)> {
    let mut groups: Vec<(Option<Volume>, Vec<String>)> = Vec::new();
    for (volume, name) in named {
        let root = volume.as_ref().map(|v| &v.root);
        match groups.iter_mut().find(|(v, _)| v.as_ref().map(|v| &v.root) == root) {
            Some((_, names)) => names.push(name),
            None => groups.push((volume, vec![name])),
        }
    }
    groups
}

/// What to say about a check: once when a volume runs short, however many
/// downloads that stops, and once when they carry on.
fn space_notes(changes: SpaceChanges, minimum: u64) -> Vec<SpaceNote> {
    let mut notes = Vec::new();
    for (volume, names) in by_volume(changes.stopped) {
        let Some(volume) = volume.filter(|v| !changes.already_short.contains(&v.root)) else {
            continue;
        };
        let detail = format!(
            "{} free on {}, less than the {} to keep free",
            size(volume.free),
            volume.root,
            size(minimum)
        );
        notes.push(SpaceNote { kind: "waiting", title: subject(&names), detail: Some(detail) });
    }
    for (volume, names) in by_volume(changes.resumed) {
        let detail = volume.map(|v| format!("{} free on {}", size(v.free), v.root));
        notes.push(SpaceNote { kind: "resumed", title: subject(&names), detail });
    }
    notes
}

/// A size in the units the window uses.
fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if unit == 0 || value >= 100.0 { 0 } else { 1 };
    format!("{value:.digits$} {}", UNITS[unit])
}

/// One torrent by name, several by count.
fn subject(names: &[String]) -> String {
    match names {
        [one] => one.clone(),
        _ => format!("{} downloads", names.len()),
    }
}

/// Background housekeeping that runs on the same clock as the UI tick:
/// queue limits, the disk guard, moving finished downloads out of the
/// incomplete folder, and telling you when something is done.
#[derive(Default)]
pub struct Supervisor {
    /// Torrents already reported as finished.
    announced: HashSet<String>,
    started: bool,
    last_tick: Option<Instant>,
    last_saved: Option<Instant>,
    space_checked: Option<Instant>,
    /// Reads of free space that have not come back yet, by folder.
    measuring: HashMap<String, tokio::task::JoinHandle<Option<Volume>>>,
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
            state.disk_paused.lock().retain(&live);
            state.pending_moves.lock().retain(&live);
        }

        self.announce_finished(app, state, rows);
        self.move_finished(app, state, rows);
        self.track_seeding(state, rows);
        self.enforce_seed_limits(app, state, rows).await;
        self.guard_disk(app, state, rows).await;
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

    /// Pauses downloads whose volume is nearly full, and lets them go once
    /// there is room. It runs before the queue, which starts what is let go.
    async fn guard_disk<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        state: &AppState,
        rows: &[TorrentRow],
    ) {
        for note in self.check_space(state, rows).await {
            if note.kind == "waiting" {
                let _ = app
                    .notification()
                    .builder()
                    .title("Downloads paused, the disk is nearly full")
                    .body(format!("{}: {}", note.title, note.detail.as_deref().unwrap_or_default()))
                    .show();
            }
            activity::record(app, note.kind, note.title, note.detail);
        }
    }

    /// Reads free space when that is due, acts on it and says what changed.
    pub(crate) async fn check_space(
        &mut self,
        state: &AppState,
        rows: &[TorrentRow],
    ) -> Vec<SpaceNote> {
        let now = Instant::now();
        if self.space_checked.is_some_and(|t| now - t < SPACE_CHECK_EVERY) {
            return Vec::new();
        }
        let minimum = u64::from(state.settings.read().min_free_space_mb) * 1024 * 1024;
        if minimum == 0 && state.disk_paused.lock().is_empty() {
            return Vec::new();
        }
        // Pause all makes every hold a pause of yours, so it goes first.
        let Ok(_gate) = state.queue_gate.try_lock() else {
            return Vec::new();
        };
        let volumes = if minimum == 0 {
            HashMap::new()
        } else {
            let mut folders: Vec<String> = rows
                .iter()
                .filter(|r| !r.finished)
                .map(|r| r.output_folder.clone())
                .collect();
            folders.sort();
            folders.dedup();
            self.measure(folders).await
        };
        self.space_checked = Some(now);

        let changes = settle_space(state, rows, &volumes, minimum).await;
        space_notes(changes, minimum)
    }

    /// Reads the free space behind each folder. A folder whose read is stuck,
    /// as one on a dead network mount can be, is left out until it comes back.
    async fn measure(&mut self, folders: Vec<String>) -> HashMap<String, Volume> {
        self.measuring.retain(|_, read| !read.is_finished());
        let reads: Vec<_> = folders
            .into_iter()
            .filter(|folder| !self.measuring.contains_key(folder))
            .map(|folder| {
                let path = PathBuf::from(&folder);
                let read = tokio::task::spawn_blocking(move || settings::volume_of(&path));
                (folder, read)
            })
            .collect();

        let give_up = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut volumes = HashMap::new();
        for (folder, mut read) in reads {
            match tokio::time::timeout_at(give_up, &mut read).await {
                Ok(Ok(Some(volume))) => {
                    volumes.insert(folder, volume);
                }
                Ok(_) => {}
                Err(_) => {
                    self.measuring.insert(folder, read);
                }
            }
        }
        volumes
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
        let mut moves = state.pending_moves.lock();
        if moves.is_empty() {
            return;
        }

        for row in rows.iter().filter(|r| r.finished && r.has_metadata) {
            let Some(destination) = moves.get(&row.info_hash).cloned() else {
                continue;
            };
            if Path::new(&row.output_folder) == Path::new(&destination) {
                moves.remove(&row.info_hash);
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
        let Ok(_gate) = state.queue_gate.try_lock() else {
            return;
        };
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

    state.pending_moves.lock().remove(&row.info_hash);
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
        let mut holds = Holds::load(file.clone());
        holds.insert("aaa".into());
        holds.insert("bbb".into());
        holds.remove("bbb");

        let mut again = Holds::load(file.clone());
        assert!(again.contains("aaa"));
        assert!(!again.contains("bbb"));

        again.retain(&HashSet::new());
        assert!(!Holds::load(file.clone()).contains("aaa"));
        std::fs::remove_file(&file).ok();
    }

    const MB: u64 = 1024 * 1024;
    const KEEP: u64 = 1024 * MB;

    /// An unfinished torrent, named after its info hash.
    fn row(hash: &str, state: &str, folder: &str) -> TorrentRow {
        TorrentRow {
            id: 1,
            info_hash: hash.into(),
            name: hash.into(),
            state: state.into(),
            error: None,
            progress: 0.5,
            progress_bytes: 500,
            total_bytes: 1000,
            uploaded_bytes: 0,
            download_speed: 0,
            upload_speed: 0,
            eta_seconds: None,
            peers_live: 0,
            peers_connecting: 0,
            peers_queued: 0,
            peers_seen: 0,
            finished: false,
            output_folder: folder.into(),
            tracker_count: 0,
            ratio: 0.0,
            has_metadata: true,
            private: false,
            stopped: !matches!(state, "downloading" | "seeding" | "checking"),
            waiting_for_space: false,
            downloaded_now: 0,
        }
    }

    fn seed(hash: &str, state: &str, folder: &str) -> TorrentRow {
        TorrentRow { finished: true, ..row(hash, state, folder) }
    }

    fn set(hashes: &[&str]) -> HashSet<String> {
        hashes.iter().map(|h| h.to_string()).collect()
    }

    /// The verdict with this much free in each folder.
    fn judged(
        rows: &[TorrentRow],
        held: &[&str],
        queued: &[&str],
        free: &[(&str, u64)],
        minimum: u64,
    ) -> SpaceVerdict {
        let free = |folder: &str| free.iter().find(|(f, _)| *f == folder).map(|(_, free)| *free);
        judge_space(rows, &set(held), &set(queued), free, minimum)
    }

    fn verdict(hold: &[&str], release: &[&str]) -> SpaceVerdict {
        SpaceVerdict {
            hold: hold.iter().map(|h| h.to_string()).collect(),
            release: release.iter().map(|h| h.to_string()).collect(),
        }
    }

    #[test]
    fn a_download_is_held_once_its_volume_runs_short() {
        let rows = [row("a", "downloading", "/data")];
        assert_eq!(judged(&rows, &[], &[], &[("/data", KEEP)], KEEP), verdict(&[], &[]));
        assert_eq!(judged(&rows, &[], &[], &[("/data", KEEP - 1)], KEEP), verdict(&["a"], &[]));
        // A volume that cannot be read is no reason to stop anything.
        assert_eq!(judged(&rows, &[], &[], &[], KEEP), verdict(&[], &[]));
    }

    #[test]
    fn seeds_and_what_you_paused_are_left_alone() {
        let waiting_for_details = TorrentRow { has_metadata: false, ..row("e", "downloading", "/data") };
        let rows = [
            seed("a", "seeding", "/data"),
            seed("b", "complete", "/data"),
            row("c", "paused", "/data"),
            row("d", "checking", "/data"),
            waiting_for_details,
        ];
        assert_eq!(judged(&rows, &[], &[], &[("/data", 0)], KEEP), verdict(&[], &[]));
    }

    /// The queue would start the next one in line as soon as one is stopped.
    #[test]
    fn a_download_the_queue_holds_is_held_too() {
        let rows = [row("a", "downloading", "/data"), row("b", "paused", "/data")];
        let short = [("/data", 10 * MB)];
        assert_eq!(judged(&rows, &[], &["b"], &short, KEEP), verdict(&["a", "b"], &[]));
    }

    #[test]
    fn a_held_download_waits_for_more_than_the_minimum() {
        let rows = [row("a", "paused", "/data")];
        let enough = KEEP + headroom(KEEP);
        for (free, release) in [(0, false), (KEEP, false), (enough - 1, false), (enough, true)] {
            let wanted = if release { verdict(&[], &["a"]) } else { verdict(&[], &[]) };
            assert_eq!(judged(&rows, &["a"], &[], &[("/data", free)], KEEP), wanted, "{free}");
        }
        assert_eq!(judged(&rows, &["a"], &[], &[], KEEP), verdict(&[], &[]));
        assert_eq!(headroom(100 * MB), 128 * MB);
        assert_eq!(headroom(50 * 1024 * MB), 5 * 1024 * MB);
    }

    /// Started by hand, it is stopped again while the volume is short and is
    /// no longer the guard's once it is not.
    #[test]
    fn a_held_download_you_start_is_judged_like_any_other() {
        let rows = [row("a", "downloading", "/data")];
        assert_eq!(judged(&rows, &["a"], &[], &[("/data", KEEP - 1)], KEEP), verdict(&["a"], &[]));
        assert_eq!(judged(&rows, &["a"], &[], &[("/data", KEEP)], KEEP), verdict(&[], &["a"]));

        let settling = [row("a", "checking", "/data"), row("b", "error", "/data")];
        assert_eq!(judged(&settling, &["a", "b"], &[], &[("/data", KEEP * 4)], 0), verdict(&[], &[]));
    }

    #[test]
    fn each_volume_is_judged_on_its_own() {
        let rows = [
            row("a", "downloading", "/small/a"),
            row("b", "downloading", "/big/b"),
            row("c", "paused", "/small/c"),
            row("d", "paused", "/big/d"),
        ];
        let free = [("/small/a", 10 * MB), ("/small/c", 10 * MB), ("/big/b", KEEP * 9), ("/big/d", KEEP * 9)];
        assert_eq!(judged(&rows, &["c", "d"], &[], &free, KEEP), verdict(&["a"], &["d"]));
    }

    #[test]
    fn turning_it_off_lets_everything_go() {
        let rows = [
            row("a", "paused", "/data"),
            row("b", "downloading", "/data"),
            seed("c", "complete", "/data"),
        ];
        assert_eq!(judged(&rows, &["a", "c"], &[], &[], 0), verdict(&[], &["a", "c"]));
        // What it held is let go when it finishes some other way, too.
        assert_eq!(judged(&rows, &["c"], &[], &[("/data", 0)], KEEP), verdict(&["b"], &["c"]));
    }

    #[test]
    fn a_volume_that_runs_short_is_reported_once() {
        let small = Volume { root: "/small".into(), free: 10 * MB };
        let big = Volume { root: "/big".into(), free: 9 * KEEP };
        let on = |volume: &Volume, name: &str| (Some(volume.clone()), name.to_string());

        let first = SpaceChanges {
            stopped: vec![on(&small, "a"), on(&small, "b"), on(&big, "c")],
            ..Default::default()
        };
        let notes = space_notes(first, KEEP);
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].kind, notes[0].title.as_str()), ("waiting", "2 downloads"));
        let detail = notes[0].detail.as_deref().unwrap();
        assert_eq!(detail, "10.0 MB free on /small, less than the 1.0 GB to keep free");
        assert_eq!(notes[1].title, "c");

        let later = SpaceChanges {
            stopped: vec![on(&small, "d")],
            already_short: set(&["/small"]),
            ..Default::default()
        };
        assert!(space_notes(later, KEEP).is_empty());

        let room = SpaceChanges {
            resumed: vec![on(&big, "c"), (None, "e".into())],
            ..Default::default()
        };
        let notes = space_notes(room, KEEP);
        assert_eq!(notes[0].detail.as_deref(), Some("9.0 GB free on /big"));
        assert_eq!((notes[1].kind, notes[1].title.as_str(), &notes[1].detail), ("resumed", "e", &None));
    }
}
