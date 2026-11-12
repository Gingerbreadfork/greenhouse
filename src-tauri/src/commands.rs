use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, Session};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tokio::sync::watch;

use crate::activity::{self, ActivityLog};
use crate::background;
use crate::engine::{self, SessionSummary, TorrentDetail, TorrentRow};
use crate::feeds::{self, FeedStatus, FeedsState};
use crate::seeding::SeedLedger;
use crate::stream::StreamServer;
use crate::supervisor::QueueHolds;
use crate::settings::{self, Paths, Settings};
use crate::torrentsrc;

pub struct AppState {
    pub engine: Arc<engine::EngineSlot>,
    /// Held while the engine is being restarted.
    pub restarting: tokio::sync::Mutex<()>,
    pub settings: RwLock<Settings>,
    pub paths: Paths,
    pub staged: Mutex<HashMap<String, Staged>>,
    /// Metadata lookups in flight, by staged token. Locked after `staged`.
    pub resolving: Mutex<HashMap<String, Resolving>>,
    pub seeding: Mutex<SeedLedger>,
    pub feeds: Mutex<FeedsState>,
    /// Torrents the queue paused, so ones you paused yourself are left alone.
    pub queue_paused: Mutex<QueueHolds>,
    /// Finished torrents that are out of the session while their files are
    /// moved, by info hash, as they were last seen.
    pub moving: Mutex<HashMap<String, TorrentRow>>,
    pub activity: Mutex<ActivityLog>,
    /// Serves files to a media player while they download.
    pub stream: Option<StreamServer>,
    pub startup_warnings: Vec<String>,
    /// Why the saved settings were not used, when they could not be read.
    pub settings_problem: Option<String>,
}

impl AppState {
    pub fn session(&self) -> Arc<Session> {
        self.engine.get().session.clone()
    }

    pub fn http(&self) -> reqwest::Client {
        self.engine.get().http.clone()
    }

    pub fn api(&self) -> Api {
        self.engine.get().api.clone()
    }
}

pub struct Resolving {
    task: tauri::async_runtime::JoinHandle<()>,
    done: watch::Receiver<Option<Result<(), String>>>,
}

/// An add that was confirmed before the metadata arrived.
#[derive(Clone)]
pub struct Queued {
    pub request: CommitRequest,
    pub since: SystemTime,
    /// What asked for this add, when it was not you in the add sheet.
    pub origin: Option<String>,
}

/// A queued add as it is kept on disk between runs.
#[derive(Serialize, Deserialize)]
struct QueuedRecord {
    source: String,
    name: Option<String>,
    info_hash: Option<String>,
    trackers: Vec<String>,
    request: CommitRequest,
    /// Seconds since 1970.
    queued_at: u64,
    #[serde(default)]
    origin: Option<String>,
}

#[derive(Clone)]
pub struct Staged {
    pub kind: String,
    pub source: String,
    pub bytes: Option<Bytes>,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub total_bytes: u64,
    pub trackers: Vec<String>,
    pub files: Vec<StagedFile>,
    pub skipped: Vec<usize>,
    pub resolved: bool,
    /// The metainfo's private flag: only its own trackers may announce it.
    pub private: bool,
    pub queued: Option<Queued>,
}

#[derive(Clone, Serialize)]
pub struct StagedFile {
    pub index: usize,
    pub name: String,
    pub components: Vec<String>,
    pub length: u64,
}

#[derive(Serialize)]
pub struct StagedInfo {
    pub token: String,
    pub kind: String,
    pub source: String,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub total_bytes: u64,
    pub trackers: Vec<String>,
    pub files: Vec<StagedFile>,
    /// File indices left unticked because of their extension.
    pub skipped: Vec<usize>,
    pub resolved: bool,
    pub needs_resolve: bool,
    pub private: bool,
}

impl StagedInfo {
    fn from(token: String, s: &Staged) -> Self {
        Self {
            token,
            kind: s.kind.clone(),
            source: s.source.clone(),
            info_hash: s.info_hash.clone(),
            name: s.name.clone(),
            total_bytes: s.total_bytes,
            trackers: s.trackers.clone(),
            files: s.files.clone(),
            skipped: s.skipped.clone(),
            resolved: s.resolved,
            needs_resolve: !s.resolved,
            private: s.private,
        }
    }
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub settings: Settings,
    pub version: String,
    pub default_download_dir: String,
    pub home_dir: String,
    /// The operating system, as Rust names it: "linux", "windows" or "macos".
    pub platform: &'static str,
    pub bound_interface: Option<String>,
    pub warnings: Vec<String>,
    /// Why the saved settings were not used, when they could not be read.
    pub settings_problem: Option<String>,
    /// Whether Greenhouse updates itself, rather than a store updating it.
    pub self_updates: bool,
}

#[tauri::command]
pub fn bootstrap(state: tauri::State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        settings: state.settings.read().clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        default_download_dir: settings::default_download_dir(),
        home_dir: dirs::home_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        platform: std::env::consts::OS,
        bound_interface: state.engine.get().bound_interface.clone(),
        warnings: state.startup_warnings.clone(),
        settings_problem: state.settings_problem.clone(),
        self_updates: crate::flatpak::app_id().is_none(),
    }
}

/// Bytes free on the volume a folder is on, or will be on once it exists.
#[tauri::command]
pub fn free_space(path: String) -> Option<u64> {
    settings::free_space(std::path::Path::new(path.trim()))
}

#[tauri::command]
pub fn list_interfaces() -> Vec<engine::NetInterface> {
    engine::list_interfaces()
}

#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    next: Settings,
) -> CmdResult<Settings> {
    feeds::validate(&next.feeds)?;
    if next.start_on_login != state.settings.read().start_on_login {
        background::sync_autostart(next.start_on_login)
            .map_err(|e| format!("could not change the login entry: {e}"))?;
    }
    background::sync_tray(&app, next.close_to_background);
    state
        .session()
        .ratelimits
        .set_download_bps(engine::kbps_to_bps(next.download_limit_kbps));
    state
        .session()
        .ratelimits
        .set_upload_bps(engine::kbps_to_bps(next.upload_limit_kbps));
    settings::save(&state.paths.settings_file(), &next).map_err(err)?;
    // Both are held so a check in flight cannot record the old address.
    let mut kept = state.feeds.lock();
    let mut current = state.settings.write();
    kept.start_over(&current.feeds, &next.feeds, &state.paths.feeds_file());
    *current = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn list_torrents(state: tauri::State<'_, AppState>) -> Vec<TorrentRow> {
    rows_with_totals(&state)
}

/// The torrent list, with upload figures that carry across restarts.
pub fn rows_with_totals(state: &AppState) -> Vec<TorrentRow> {
    let mut rows = engine::collect_rows(&state.session());
    let away: Vec<TorrentRow> = state
        .moving
        .lock()
        .values()
        .filter(|moving| rows.iter().all(|row| row.info_hash != moving.info_hash))
        .cloned()
        .collect();
    rows.extend(away);
    state.seeding.lock().overlay(&mut rows);
    rows
}

#[tauri::command]
pub fn session_stats(state: tauri::State<'_, AppState>) -> SessionSummary {
    engine::session_summary(&state.session(), &state.settings.read())
}

#[tauri::command]
pub fn torrent_detail(state: tauri::State<'_, AppState>, id: usize) -> CmdResult<TorrentDetail> {
    engine::torrent_detail(&state.api(), &state.paths, id).map_err(err)
}

#[tauri::command]
pub async fn torrent_action(
    state: tauri::State<'_, AppState>,
    id: usize,
    action: String,
) -> CmdResult<()> {
    let info_hash = state
        .session()
        .get(id.into())
        .map(|handle| handle.info_hash().as_string());
    match action.as_str() {
        "pause" => state.api().api_torrent_action_pause(id.into()).await.map(|_| ()),
        "start" => state.api().api_torrent_action_start(id.into()).await.map(|_| ()),
        "forget" => state.api().api_torrent_action_forget(id.into()).await.map(|_| ()),
        "delete" => state.api().api_torrent_action_delete(id.into()).await.map(|_| ()),
        other => return Err(format!("unknown action: {other}")),
    }
    .map_err(err)?;
    let Some(info_hash) = info_hash else {
        return Ok(());
    };
    match action.as_str() {
        "pause" => state.queue_paused.lock().remove(&info_hash),
        // Nothing is left to move once the torrent is gone for good.
        "forget" | "delete" => {
            settings::drop_pending_move(&state.paths.pending_moves_file(), &info_hash)
        }
        _ => {}
    }
    Ok(())
}

/// Pauses every running torrent, as if you had paused each one yourself.
pub async fn pause_all_inner(state: &AppState) -> usize {
    let rows = engine::collect_rows(&state.session());
    let mut paused = 0;
    // Pausing one that is already held would break off its file check.
    for row in rows.iter().filter(|r| !r.stopped) {
        if state.api().api_torrent_action_pause(row.id.into()).await.is_ok() {
            paused += 1;
        }
    }
    state.queue_paused.lock().clear();
    paused
}

/// Starts every torrent that is not running, including ones stopped by an
/// error. The queue takes over again from there.
pub async fn resume_all_inner(state: &AppState) -> usize {
    let rows = engine::collect_rows(&state.session());
    let mut started = 0;
    for row in rows.iter().filter(|r| r.stopped) {
        if state.api().api_torrent_action_start(row.id.into()).await.is_ok() {
            started += 1;
        }
    }
    started
}

#[tauri::command]
pub async fn pause_all(state: tauri::State<'_, AppState>) -> CmdResult<usize> {
    Ok(pause_all_inner(&state).await)
}

#[tauri::command]
pub async fn resume_all(state: tauri::State<'_, AppState>) -> CmdResult<usize> {
    Ok(resume_all_inner(&state).await)
}

#[tauri::command]
pub async fn set_file_selection(
    state: tauri::State<'_, AppState>,
    id: usize,
    indices: Vec<usize>,
) -> CmdResult<()> {
    // The engine counts a torrent with nothing selected as finished.
    if indices.is_empty() {
        return Err("At least one file has to stay included.".into());
    }
    let set: HashSet<usize> = indices.into_iter().collect();
    state
        .api()
        .api_torrent_action_update_only_files(id.into(), &set)
        .await
        .map(|_| ())
        .map_err(err)
}

#[derive(Serialize)]
pub struct BlocklistStatus {
    /// Whether this run of the engine loaded the list.
    pub active: bool,
    /// When the downloaded copy was last refreshed, in seconds since 1970.
    pub updated_at: Option<u64>,
    /// When the copy the engine is using was downloaded. Older than
    /// `updated_at` means a newer copy waits for the engine to restart.
    pub loaded_at: Option<u64>,
    pub bytes: u64,
}

fn blocklist_status_of(state: &AppState) -> BlocklistStatus {
    let file = state.paths.blocklist_file();
    let running = state.engine.get();
    BlocklistStatus {
        active: running.blocklist_active,
        updated_at: settings::modified_secs(&file),
        loaded_at: running.blocklist_loaded,
        bytes: std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
    }
}

#[tauri::command]
pub fn blocklist_status(state: tauri::State<'_, AppState>) -> BlocklistStatus {
    blocklist_status_of(&state)
}

/// Fetches the blocklist named in the settings into the copy the engine reads
/// when it starts. With no URL set, the copy is removed.
pub async fn download_blocklist(state: &AppState) -> CmdResult<()> {
    let url = state.settings.read().blocklist_url.trim().to_string();
    let file = state.paths.blocklist_file();
    if url.is_empty() {
        let _ = tokio::fs::remove_file(file).await;
        return Ok(());
    }
    let response = state
        .http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(180))
        .send()
        .await
        .map_err(err)?;
    if !response.status().is_success() {
        return Err(format!("the server replied {}", response.status()));
    }
    let body = response.bytes().await.map_err(err)?;
    if body.is_empty() {
        return Err("that address returned an empty list".into());
    }
    let partial = file.with_extension("part");
    tokio::fs::write(&partial, &body).await.map_err(err)?;
    tokio::fs::rename(&partial, &file).await.map_err(err)
}

#[tauri::command]
pub async fn refresh_blocklist(state: tauri::State<'_, AppState>) -> CmdResult<BlocklistStatus> {
    download_blocklist(&state).await?;
    Ok(blocklist_status_of(&state))
}

// ---------------------------------------------------------------- engine

#[derive(Serialize)]
pub struct EngineInfo {
    pub bound_interface: Option<String>,
    pub blocklist_active: bool,
    pub warnings: Vec<String>,
}

/// Stops the engine and starts it again with the current settings, so network
/// settings apply without relaunching. Torrents keep their state.
#[tauri::command]
pub async fn restart_engine<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> CmdResult<EngineInfo> {
    let paths = state.paths.clone();
    restart_engine_with(&app, &state, |settings| {
        let paths = paths.clone();
        async move { engine::build_session(&paths, &settings).await }
    })
    .await
}

pub async fn restart_engine_with<R, F, Fut>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    build: F,
) -> CmdResult<EngineInfo>
where
    R: tauri::Runtime,
    F: Fn(Settings) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<engine::Engine>>,
{
    let _guard = state
        .restarting
        .try_lock()
        .map_err(|_| "the engine is already restarting")?;

    // Lookups belong to the engine that is about to go away.
    for (_, lookup) in state.resolving.lock().drain() {
        lookup.task.abort();
    }
    let old = state.engine.get();
    old.session.stop().await;

    let wanted = state.settings.read().clone();
    let (engine, applied, mut warnings) = match build_with_retries(&build, &wanted).await {
        Ok(engine) => (engine, wanted, Vec::new()),
        // Whatever was changed cannot be started, so go back to what worked.
        Err(e) => {
            let engine = build_with_retries(&build, &old.applied).await.map_err(|again| {
                format!("The engine could not be restarted. Relaunch Greenhouse. ({again:#})")
            })?;
            let warning = format!("Those settings could not be applied, so the previous ones are still in use: {e:#}");
            (engine, old.applied.clone(), vec![warning])
        }
    };
    drop(old);

    let mut engine = engine;
    warnings.append(&mut engine.warnings);
    state.engine.set(engine::Running::new(engine, applied));

    let waiting: Vec<String> = {
        let staged = state.staged.lock();
        staged
            .iter()
            .filter(|(_, s)| s.queued.is_some())
            .map(|(token, _)| token.clone())
            .collect()
    };
    {
        let _staged = state.staged.lock();
        for token in &waiting {
            ensure_resolving(app, state, token);
        }
    }

    let running = state.engine.get();
    Ok(EngineInfo {
        bound_interface: running.bound_interface.clone(),
        blocklist_active: running.blocklist_active,
        warnings,
    })
}

/// Starts the engine at launch, with the settings it ended up using. A fixed
/// port that cannot be opened gives way to any free one, so a taken port does
/// not keep Greenhouse from starting.
pub async fn start_engine_with<F, Fut>(
    wanted: &Settings,
    build: F,
) -> anyhow::Result<(engine::Engine, Settings)>
where
    F: Fn(Settings) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<engine::Engine>>,
{
    let failed = match build(wanted.clone()).await {
        Ok(engine) => return Ok((engine, wanted.clone())),
        Err(e) => e,
    };
    if wanted.listen_port == 0 {
        return Err(failed);
    }
    let any_port = Settings {
        listen_port: 0,
        ..wanted.clone()
    };
    let Ok(mut engine) = build(any_port.clone()).await else {
        return Err(failed);
    };
    engine.warnings.push(format!(
        "Port {} could not be opened, so another one is in use for now: {failed:#}",
        wanted.listen_port
    ));
    Ok((engine, any_port))
}

/// The old engine's ports can take a moment to come free.
async fn build_with_retries<F, Fut>(build: &F, settings: &Settings) -> anyhow::Result<engine::Engine>
where
    F: Fn(Settings) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<engine::Engine>>,
{
    let mut last = None;
    for attempt in 0..5 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
        match build(settings.clone()).await {
            Ok(engine) => return Ok(engine),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("the engine did not start")))
}

// ----------------------------------------------------------------- feeds

#[tauri::command]
pub fn feeds_status(state: tauri::State<'_, AppState>) -> HashMap<String, FeedStatus> {
    state.feeds.lock().statuses()
}

fn feed_by_id(state: &AppState, id: &str) -> CmdResult<feeds::Feed> {
    let settings = state.settings.read();
    let feed = settings.feeds.iter().find(|f| f.id == id);
    feed.cloned().ok_or_else(|| "that feed no longer exists".to_string())
}

#[tauri::command]
pub async fn check_feed<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    id: String,
) -> CmdResult<FeedStatus> {
    let feed = feed_by_id(&state, &id)?;
    if feed.url.trim().is_empty() {
        return Err("give the feed an address first".into());
    }
    Ok(feeds::check(&app, &feed).await)
}

#[tauri::command]
pub fn add_feed_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    feed_id: String,
    guid: String,
) -> CmdResult<()> {
    let feed = feed_by_id(&state, &feed_id)?;
    feeds::add_item(&app, &feed, &guid)
}

// --------------------------------------------------------------- playing

const PLAYERS: &[&str] = &["mpv", "vlc", "celluloid", "haruna", "smplayer", "totem"];

fn on_path(program: &str) -> bool {
    let mut names = vec![program.to_string()];
    if cfg!(windows) {
        names.push(format!("{program}.exe"));
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| names.iter().any(|n| dir.join(n).is_file()))
    })
}

/// How to start a well-known player: by name when it is on PATH, otherwise
/// from its usual install folder on Windows.
fn locate_player(program: &str) -> Option<String> {
    if on_path(program) {
        return Some(program.to_string());
    }
    #[cfg(windows)]
    if program == "vlc" {
        for root in ["ProgramFiles", "ProgramFiles(x86)"] {
            let Some(root) = std::env::var_os(root) else {
                continue;
            };
            let exe = PathBuf::from(root).join(r"VideoLAN\VLC\vlc.exe");
            if exe.is_file() {
                return Some(exe.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// The player to run, as a program and its leading arguments: the one named
/// in the settings, otherwise the first well-known player that is installed.
fn player_command(
    configured: &str,
    locate: impl Fn(&str) -> Option<String>,
) -> Option<Vec<String>> {
    let configured = configured.trim();
    if !configured.is_empty() {
        // A full path to a player can have spaces in it.
        if std::path::Path::new(configured).is_file() {
            return Some(vec![configured.to_string()]);
        }
        return Some(configured.split_whitespace().map(String::from).collect());
    }
    PLAYERS.iter().find_map(|p| locate(p)).map(|p| vec![p])
}

#[derive(Serialize)]
pub struct PlayResult {
    pub player: String,
    /// True when the file is still downloading and is being streamed.
    pub streaming: bool,
}

/// Opens a file in a media player. A finished file is opened from disk; an
/// unfinished one is streamed, which makes the engine fetch what is being
/// watched first.
#[tauri::command]
pub async fn play_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: usize,
    file_index: usize,
) -> CmdResult<PlayResult> {
    use tauri_plugin_opener::OpenerExt;

    let handle = state
        .session()
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;
    let details = state.api().api_torrent_details(id.into()).map_err(err)?;
    let files = details.files.unwrap_or_default();
    let file = files.get(file_index).ok_or("that file is not in the torrent")?;

    let finished = handle
        .stats()
        .file_progress
        .get(file_index)
        .is_some_and(|done| *done >= file.length);

    let target = if finished {
        let mut path = PathBuf::from(&details.output_folder);
        path.extend(&file.components);
        path.to_string_lossy().into_owned()
    } else {
        if !file.included {
            let wanted: HashSet<usize> = files
                .iter()
                .enumerate()
                .filter(|(index, f)| f.included || *index == file_index)
                .map(|(index, _)| index)
                .collect();
            state
                .api()
                .api_torrent_action_update_only_files(id.into(), &wanted)
                .await
                .map_err(err)?;
        }
        if handle.is_paused() {
            state.api().api_torrent_action_start(id.into()).await.map_err(err)?;
        }
        let server = state.stream.as_ref().ok_or("streaming could not be started")?;
        server.url(id, file_index, &file.name)
    };

    let configured = state.settings.read().player_command.clone();
    let player = match player_command(&configured, locate_player) {
        Some(command) => {
            let mut launch = std::process::Command::new(&command[0]);
            launch
                .args(&command[1..])
                .arg(&target)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(unix)]
            crate::fdlimit::restore(&mut launch);
            let mut child = launch
                .spawn()
                .map_err(|e| format!("could not start {}: {e}", command[0]))?;
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            command[0].clone()
        }
        None if finished => {
            app.opener().open_path(&target, None::<&str>).map_err(err)?;
            "your default player".to_string()
        }
        None => {
            app.opener().open_url(&target, None::<&str>).map_err(err)?;
            "your browser".to_string()
        }
    };
    Ok(PlayResult {
        player,
        streaming: !finished,
    })
}

/// Opens the folder a torrent's files live in with the system file manager.
/// Before anything has been written that folder may not exist yet, so the
/// closest folder that does is opened instead.
#[tauri::command]
pub async fn open_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: usize,
) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;

    let handle = state
        .session()
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;
    let folder = nearest_existing_folder(handle.output_folder())
        .ok_or("that folder does not exist")?;
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(err)
}

/// The path itself if it is a folder, else the first ancestor that is.
fn nearest_existing_folder(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.is_dir()).map(Path::to_path_buf)
}

// ---------------------------------------------------------------- adding

#[derive(Deserialize)]
pub struct StageRequest {
    pub kind: String,
    pub value: String,
}

#[tauri::command]
pub async fn stage_source(
    state: tauri::State<'_, AppState>,
    request: StageRequest,
) -> CmdResult<StagedInfo> {
    let mut staged = match request.kind.as_str() {
        "file" => {
            let bytes = tokio::fs::read(&request.value).await.map_err(err)?;
            Staged {
                kind: "file".into(),
                source: request.value.clone(),
                bytes: Some(Bytes::from(bytes)),
                info_hash: None,
                name: PathBuf::from(&request.value)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned()),
                total_bytes: 0,
                trackers: Vec::new(),
                files: Vec::new(),
                skipped: Vec::new(),
                resolved: false,
                private: false,
                queued: None,
            }
        }
        _ => staged_from_text(&request.value)?,
    };

    // A .torrent file carries its own metadata, so the file list is available
    // immediately without touching the network.
    if staged.bytes.is_some() {
        if let Ok(resolved) = resolve(&state.session(), &staged, &[]).await {
            staged = resolved;
        }
    }
    staged.skipped = skipped_indices(&staged.files, &state.settings.read());

    let token = uuid::Uuid::new_v4().to_string();
    state.staged.lock().insert(token.clone(), staged.clone());
    Ok(StagedInfo::from(token, &staged))
}

/// A magnet link, torrent URL or info hash, staged with what it says about itself.
fn staged_from_text(value: &str) -> CmdResult<Staged> {
    let value = value.trim().to_string();
    if !torrentsrc::looks_like_source(&value) {
        return Err("That does not look like a magnet link, torrent URL or info hash.".into());
    }
    let parsed = torrentsrc::parse_magnet(&value);
    Ok(Staged {
        kind: if value.starts_with("magnet:") {
            "magnet".into()
        } else {
            "url".into()
        },
        source: value,
        bytes: None,
        info_hash: parsed.info_hash,
        name: parsed.name,
        total_bytes: parsed.size.unwrap_or(0),
        trackers: parsed.trackers,
        files: Vec::new(),
        skipped: Vec::new(),
        resolved: false,
        private: false,
        queued: None,
    })
}

/// Queues a magnet link or torrent URL with the saved defaults. It is added
/// in the background once its metadata arrives.
pub fn queue_with_defaults<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    source: String,
    name: Option<String>,
    download_dir: Option<String>,
    origin: Option<String>,
) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let mut entry = staged_from_text(&source)?;
    entry.name = entry.name.or(name);

    let token = uuid::Uuid::new_v4().to_string();
    let request = {
        let settings = state.settings.read();
        CommitRequest {
            token: token.clone(),
            download_dir: download_dir.unwrap_or_else(|| settings.download_dir.clone()),
            start_paused: settings.start_paused,
            use_auto_packs: settings.auto_apply_trackers,
            pack_ids: Vec::new(),
            extra_trackers: Vec::new(),
            selected_files: None,
        }
    };
    entry.queued = Some(Queued {
        request,
        since: SystemTime::now(),
        origin,
    });
    {
        let mut staged = state.staged.lock();
        staged.insert(token.clone(), entry);
        ensure_resolving(app, &state, &token);
    }
    save_queue(&state);
    Ok(())
}

/// Files whose extension the user has chosen not to download by default.
fn skipped_indices(files: &[StagedFile], settings: &Settings) -> Vec<usize> {
    files
        .iter()
        .filter(|f| settings.is_skipped(&f.name))
        .map(|f| f.index)
        .collect()
}

async fn resolve(
    session: &Arc<Session>,
    staged: &Staged,
    extra_trackers: &[String],
) -> anyhow::Result<Staged> {
    let add = match &staged.bytes {
        Some(bytes) => AddTorrent::from_bytes(bytes.clone()),
        // The engine takes a magnet's trackers from the link and nowhere else.
        None => AddTorrent::from_url(torrentsrc::magnet_with_trackers(
            &staged.source,
            extra_trackers,
        )),
    };
    let trackers = torrentsrc::merge_trackers(&[&staged.trackers, extra_trackers]);
    let opts = AddTorrentOptions {
        list_only: true,
        trackers: if trackers.is_empty() {
            None
        } else {
            Some(trackers)
        },
        ..Default::default()
    };
    let response = session.add_torrent(add, Some(opts)).await?;
    let listed = match response {
        AddTorrentResponse::ListOnly(l) => l,
        _ => anyhow::bail!("unexpected engine response while inspecting the torrent"),
    };

    let files: Vec<StagedFile> = listed
        .info
        .iter_file_details()
        .enumerate()
        .map(|(index, d)| StagedFile {
            index,
            name: d.filename.to_string(),
            components: d.filename.to_vec(),
            length: d.len,
        })
        .collect();

    let mut out = staged.clone();
    out.total_bytes = files.iter().map(|f| f.length).sum();
    out.name = listed
        .info
        .name()
        .map(|n| n.into_owned())
        .or_else(|| staged.name.clone());
    out.info_hash = Some(listed.info_hash.as_string());
    out.files = files;
    out.bytes = Some(listed.torrent_bytes);
    out.resolved = true;
    out.private = listed.info.info().private;
    Ok(out)
}

/// Starts the metadata lookup for a staged source unless one is already
/// running. Call with `staged` locked so a finishing lookup cannot be missed.
fn ensure_resolving<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    token: &str,
) -> watch::Receiver<Option<Result<(), String>>> {
    let mut resolving = state.resolving.lock();
    if let Some(lookup) = resolving.get(token) {
        return lookup.done.clone();
    }
    let (tx, done) = watch::channel(None);
    let task = tauri::async_runtime::spawn({
        let app = app.clone();
        let token = token.to_string();
        async move {
            let state = app.state::<AppState>();
            let outcome = lookup(&state, &token).await;
            let queued = finish_lookup(&state, &token, &outcome);
            let _ = tx.send(Some(outcome.as_ref().map(|_| ()).map_err(String::clone)));
            if let Some(queued) = queued {
                run_queued(&app, &state, &token, queued, outcome).await;
                save_queue(&state);
            }
        }
    });
    resolving.insert(
        token.to_string(),
        Resolving {
            task,
            done: done.clone(),
        },
    );
    done
}

async fn lookup(state: &AppState, token: &str) -> CmdResult<Staged> {
    let staged = state
        .staged
        .lock()
        .get(token)
        .cloned()
        .ok_or("that torrent is no longer staged")?;
    let extra = state.settings.read().active_trackers();
    let mut resolved = resolve(&state.session(), &staged, &extra).await.map_err(err)?;
    resolved.skipped = skipped_indices(&resolved.files, &state.settings.read());
    Ok(resolved)
}

/// Records what a lookup found and hands back the add waiting on it, if any.
fn finish_lookup(state: &AppState, token: &str, outcome: &CmdResult<Staged>) -> Option<Queued> {
    let mut staged = state.staged.lock();
    state.resolving.lock().remove(token);
    let entry = staged.get_mut(token)?;
    let queued = entry.queued.take();
    if let Ok(resolved) = outcome {
        *entry = Staged {
            queued: None,
            ..resolved.clone()
        };
    }
    queued
}

/// Every file except the ones skipped by default, or None for "everything".
fn default_selection(staged: &Staged) -> Option<Vec<usize>> {
    if staged.skipped.is_empty() {
        return None;
    }
    Some(
        staged
            .files
            .iter()
            .map(|f| f.index)
            .filter(|i| !staged.skipped.contains(i))
            .collect(),
    )
}

#[derive(Serialize, Clone)]
pub struct AddFailed {
    pub name: Option<String>,
    pub error: String,
}

async fn run_queued<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    token: &str,
    queued: Queued,
    outcome: CmdResult<Staged>,
) {
    let mut request = queued.request;
    let result = match outcome {
        Ok(staged) => {
            if request.selected_files.is_none() {
                request.selected_files = default_selection(&staged);
            }
            commit_resolved(state, &staged, &request).await
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(added) => {
            let via = queued.origin.unwrap_or_else(|| "Added once a peer answered".into());
            record_add(app, &added, via);
            let _ = app.emit("greenhouse://added", added);
        }
        Err(error) => {
            let name = state.staged.lock().remove(token).and_then(|s| s.name);
            let title = name.clone().unwrap_or_else(|| "A magnet link".into());
            activity::record(app, "failed", title, Some(error.clone()));
            let _ = app.emit("greenhouse://add-failed", AddFailed { name, error });
        }
    }
}

/// Notes a finished add in the activity log, unless nothing new was added.
pub fn record_add<R: tauri::Runtime>(app: &tauri::AppHandle<R>, added: &CommitResult, via: String) {
    if !added.already && !added.pending {
        activity::record(app, "added", added.name.clone(), Some(via));
    }
}

#[tauri::command]
pub fn activity_list(state: tauri::State<'_, AppState>) -> Vec<activity::Entry> {
    state.activity.lock().list()
}

#[tauri::command]
pub fn clear_activity(state: tauri::State<'_, AppState>) {
    state.activity.lock().clear();
}

#[tauri::command]
pub async fn resolve_staged<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    token: String,
) -> CmdResult<StagedInfo> {
    let mut done = {
        let staged = state.staged.lock();
        let entry = staged
            .get(&token)
            .ok_or("that torrent is no longer staged")?;
        if entry.resolved {
            return Ok(StagedInfo::from(token.clone(), entry));
        }
        ensure_resolving(&app, &state, &token)
    };
    let outcome = done
        .wait_for(|v| v.is_some())
        .await
        .map_err(|_| "the lookup was cancelled")?
        .clone();
    outcome.unwrap_or(Ok(()))?;
    let staged = state.staged.lock();
    let entry = staged
        .get(&token)
        .ok_or("that torrent is no longer staged")?;
    Ok(StagedInfo::from(token.clone(), entry))
}

/// Drops a staged source, stopping its lookup and any add waiting on it.
#[tauri::command]
pub fn discard_staged(state: tauri::State<'_, AppState>, token: String) {
    let was_queued = {
        let mut staged = state.staged.lock();
        let removed = staged.remove(&token);
        if let Some(lookup) = state.resolving.lock().remove(&token) {
            lookup.task.abort();
        }
        removed.is_some_and(|s| s.queued.is_some())
    };
    if was_queued {
        save_queue(&state);
    }
}

/// Writes the adds still waiting for metadata to disk, so quitting keeps them.
fn save_queue(state: &AppState) {
    let records: Vec<QueuedRecord> = state
        .staged
        .lock()
        .values()
        .filter_map(|s| {
            let queued = s.queued.as_ref()?;
            Some(QueuedRecord {
                source: s.source.clone(),
                name: s.name.clone(),
                info_hash: s.info_hash.clone(),
                trackers: s.trackers.clone(),
                request: queued.request.clone(),
                queued_at: queued
                    .since
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                origin: queued.origin.clone(),
            })
        })
        .collect();
    if let Ok(text) = serde_json::to_string(&records) {
        let _ = std::fs::write(state.paths.queued_file(), text);
    }
}

/// Puts back the adds that were still waiting when Greenhouse last quit.
pub fn restore_queue<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<AppState>();
    let records: Vec<QueuedRecord> = std::fs::read_to_string(state.paths.queued_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    let mut staged = state.staged.lock();
    for record in records {
        let token = uuid::Uuid::new_v4().to_string();
        let kind = if record.source.starts_with("magnet:") { "magnet" } else { "url" };
        staged.insert(
            token.clone(),
            Staged {
                kind: kind.into(),
                source: record.source,
                bytes: None,
                info_hash: record.info_hash,
                name: record.name,
                total_bytes: 0,
                trackers: record.trackers,
                files: Vec::new(),
                skipped: Vec::new(),
                resolved: false,
                private: false,
                queued: Some(Queued {
                    request: CommitRequest { token: token.clone(), ..record.request },
                    since: UNIX_EPOCH + Duration::from_secs(record.queued_at),
                    origin: record.origin,
                }),
            },
        );
        ensure_resolving(app, &state, &token);
    }
}

#[derive(Serialize, Clone)]
pub struct PendingAdd {
    pub token: String,
    pub name: String,
    pub info_hash: Option<String>,
    pub waiting_seconds: u64,
}

/// Adds that are confirmed but still waiting for metadata, oldest first.
pub fn pending_adds(state: &AppState) -> Vec<PendingAdd> {
    let mut list: Vec<PendingAdd> = state
        .staged
        .lock()
        .iter()
        .filter_map(|(token, s)| {
            let queued = s.queued.as_ref()?;
            Some(PendingAdd {
                token: token.clone(),
                name: display_name(s),
                info_hash: s.info_hash.clone(),
                waiting_seconds: queued.since.elapsed().unwrap_or_default().as_secs(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.waiting_seconds.cmp(&a.waiting_seconds));
    list
}

fn display_name(staged: &Staged) -> String {
    staged
        .name
        .clone()
        .or_else(|| staged.info_hash.clone())
        .unwrap_or_else(|| staged.source.clone())
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CommitRequest {
    pub token: String,
    pub download_dir: String,
    pub start_paused: bool,
    pub use_auto_packs: bool,
    pub pack_ids: Vec<String>,
    pub extra_trackers: Vec<String>,
    pub selected_files: Option<Vec<usize>>,
}

#[derive(Serialize, Clone)]
pub struct CommitResult {
    /// None while the add is still waiting for metadata.
    pub id: Option<usize>,
    pub name: String,
    pub tracker_count: usize,
    pub pending: bool,
    /// The torrent was in the session already, so nothing new was added.
    pub already: bool,
}

/// Adds a staged source. Without metadata yet, the add is queued behind the
/// lookup and finishes in the background.
#[tauri::command]
pub async fn commit_staged<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    request: CommitRequest,
) -> CmdResult<CommitResult> {
    let ready = {
        let mut map = state.staged.lock();
        let entry = map
            .get_mut(&request.token)
            .ok_or("that torrent is no longer staged")?;
        if entry.bytes.is_none() {
            let result = CommitResult {
                id: None,
                name: display_name(entry),
                tracker_count: commit_trackers(&state, entry, &request).len(),
                pending: true,
                already: false,
            };
            let token = request.token.clone();
            entry.queued = Some(Queued {
                request: request.clone(),
                since: SystemTime::now(),
                origin: None,
            });
            ensure_resolving(&app, &state, &token);
            Err(result)
        } else {
            Ok(entry.clone())
        }
    };
    match ready {
        Ok(staged) => {
            let added = commit_resolved(&state, &staged, &request).await?;
            record_add(&app, &added, "Added by you".into());
            Ok(added)
        }
        Err(pending) => {
            save_queue(&state);
            Ok(pending)
        }
    }
}

/// Adds a .torrent file with the saved defaults, as the add sheet would if
/// nothing in it were changed.
pub async fn add_file_with_defaults(
    state: &AppState,
    source: String,
    bytes: Bytes,
) -> CmdResult<CommitResult> {
    let staged = Staged {
        kind: "file".into(),
        source,
        bytes: Some(bytes),
        info_hash: None,
        name: None,
        total_bytes: 0,
        trackers: Vec::new(),
        files: Vec::new(),
        skipped: Vec::new(),
        resolved: false,
        private: false,
        queued: None,
    };
    let mut staged = resolve(&state.session(), &staged, &[]).await.map_err(err)?;
    staged.skipped = skipped_indices(&staged.files, &state.settings.read());

    let request = {
        let settings = state.settings.read();
        CommitRequest {
            token: String::new(),
            download_dir: settings.download_dir.clone(),
            start_paused: settings.start_paused,
            use_auto_packs: settings.auto_apply_trackers,
            pack_ids: Vec::new(),
            extra_trackers: Vec::new(),
            selected_files: default_selection(&staged),
        }
    };
    commit_resolved(state, &staged, &request).await
}

fn commit_trackers(state: &AppState, staged: &Staged, request: &CommitRequest) -> Vec<String> {
    trackers_for(&state.settings.read(), staged, request)
}

/// The trackers a new torrent is added with. A private torrent keeps only
/// its own: announcing it anywhere else is what private trackers ban for.
fn trackers_for(settings: &Settings, staged: &Staged, request: &CommitRequest) -> Vec<String> {
    if staged.private {
        return torrentsrc::merge_trackers(&[&staged.trackers]);
    }
    let auto = if request.use_auto_packs {
        settings.pack_trackers()
    } else {
        Vec::new()
    };
    torrentsrc::merge_trackers(&[
        &staged.trackers,
        &auto,
        &settings.trackers_in_packs(&request.pack_ids),
        &request.extra_trackers,
    ])
}

async fn commit_resolved(
    state: &AppState,
    staged: &Staged,
    request: &CommitRequest,
) -> CmdResult<CommitResult> {
    let trackers = commit_trackers(state, staged, request);

    // The engine reads an empty selection as "everything".
    if request.selected_files.as_ref().is_some_and(|f| f.is_empty()) && !staged.files.is_empty() {
        return Err("Every file in it is a type you skip by default, so nothing was added.".into());
    }

    let (chosen, incomplete_dir) = {
        let settings = state.settings.read();
        let chosen = match request.download_dir.trim() {
            "" => settings.download_dir.clone(),
            dir => dir.to_string(),
        };
        (chosen, settings.incomplete_dir.trim().to_string())
    };
    // The engine's own default is the folder it was started with, which is
    // not the current setting if that has changed since.
    let engine_default = state.engine.get().applied.download_dir.clone();

    // With an incomplete folder set, torrents land in their own sub-folder
    // there and move to the real destination on completion.
    let (output_folder, pending_final) = if incomplete_dir.is_empty() {
        (resolve_output_folder(&chosen, &engine_default, staged), None)
    } else {
        (
            Some(staging_folder(&incomplete_dir, staged)),
            Some(explicit_output_folder(&chosen, staged)),
        )
    };

    let (bytes, extra_trackers) = match &staged.bytes {
        Some(bytes) => {
            let all = torrentsrc::merge_trackers(&[&torrentsrc::announced(bytes), &trackers]);
            match announcing(bytes, &all) {
                Some(rewritten) => (Some(rewritten), Vec::new()),
                None => (Some(bytes.clone()), trackers.clone()),
            }
        }
        None => (None, trackers.clone()),
    };
    let add = match &bytes {
        Some(bytes) => AddTorrent::from_bytes(bytes.clone()),
        None => AddTorrent::from_url(staged.source.clone()),
    };
    let opts = AddTorrentOptions {
        paused: request.start_paused,
        overwrite: true,
        output_folder,
        only_files: request
            .selected_files
            .clone()
            .filter(|f| f.len() != staged.files.len()),
        trackers: (!extra_trackers.is_empty()).then_some(extra_trackers),
        ..Default::default()
    };

    let response = state
        .session()
        .add_torrent(add, Some(opts))
        .await
        .map_err(err)?;
    let (id, handle, already) = match response {
        AddTorrentResponse::Added(id, handle) => (id, handle, false),
        AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle, true),
        AddTorrentResponse::ListOnly(_) => return Err("engine returned no torrent".into()),
    };

    let info_hash = handle.info_hash().as_string();
    if let Some(bytes) = bytes.as_ref().filter(|_| !already) {
        let path = state.paths.source_file(&info_hash);
        let _ = tokio::fs::write(path, bytes).await;
    }
    // A torrent that was here already keeps the folder it has.
    if !already {
        let file = state.paths.pending_moves_file();
        match pending_final {
            Some(destination) => {
                let mut moves = settings::load_pending_moves(&file);
                moves.insert(info_hash, destination);
                settings::save_pending_moves(&file, &moves);
            }
            // A move left over from an earlier add would empty the new folder.
            None => settings::drop_pending_move(&file, &info_hash),
        }
    }
    state.staged.lock().remove(&request.token);

    Ok(CommitResult {
        id: Some(id),
        name: handle
            .name()
            .unwrap_or_else(|| handle.info_hash().as_string()),
        tracker_count: trackers.len(),
        pending: false,
        already,
    })
}

/// Recreates librqbit's per-torrent sub-folder for an explicit destination.
fn resolve_output_folder(chosen: &str, session_default: &str, staged: &Staged) -> Option<String> {
    let chosen = chosen.trim();
    if chosen.is_empty() || chosen == session_default {
        return None;
    }
    Some(explicit_output_folder(chosen, staged))
}

fn safe_name(staged: &Staged) -> Option<String> {
    staged
        .name
        .as_ref()
        .map(|n| n.replace(['/', '\\'], "_"))
        .filter(|n| !n.is_empty())
}

/// Where the files finally live: the base folder, plus the torrent's own
/// sub-folder when it holds more than one file.
fn explicit_output_folder(base: &str, staged: &Staged) -> String {
    let mut path = PathBuf::from(base.trim());
    if staged.files.len() > 1 {
        if let Some(name) = safe_name(staged) {
            path.push(name);
        }
    }
    path.to_string_lossy().into_owned()
}

/// Inside the incomplete folder every torrent gets its own sub-folder, so the
/// whole lot can be moved in one go when it finishes. The info hash keeps two
/// torrents with the same name apart.
fn staging_folder(base: &str, staged: &Staged) -> String {
    let name = safe_name(staged).unwrap_or_else(|| "torrent".into());
    let mut path = PathBuf::from(base.trim());
    path.push(match staged.info_hash.as_deref() {
        Some(hash) => format!("{name} ({})", hash.get(..8).unwrap_or(hash)),
        None => name,
    });
    path.to_string_lossy().into_owned()
}

/// The torrent file rewritten to announce to exactly `trackers`. The engine
/// only adds to a file's own list, and after a restart it reloads the file
/// alone, so trackers must be written into it to be removed or to last.
fn announcing(bytes: &Bytes, trackers: &[String]) -> Option<Bytes> {
    if torrentsrc::announced(bytes) == trackers {
        return Some(bytes.clone());
    }
    torrentsrc::with_trackers(bytes, trackers).map(Bytes::from)
}

// ------------------------------------------------------- tracker plumbing

/// Fires a notification so you can check they actually reach your desktop.
#[tauri::command]
pub fn test_notification(app: tauri::AppHandle) -> CmdResult<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title("Greenhouse")
        .body("Notifications are working.")
        .show()
        .map_err(err)
}

#[tauri::command]
pub fn parse_trackers(blob: String) -> Vec<String> {
    torrentsrc::parse_tracker_blob(&blob)
}

#[tauri::command]
pub async fn fetch_tracker_list(
    state: tauri::State<'_, AppState>,
    url: String,
) -> CmdResult<Vec<String>> {
    let response = state.http().get(&url).send().await.map_err(err)?;
    if !response.status().is_success() {
        return Err(format!("the server replied {}", response.status()));
    }
    let body = response.text().await.map_err(err)?;
    let list = torrentsrc::parse_tracker_blob(&body);
    if list.is_empty() {
        return Err("no tracker URLs found at that address".into());
    }
    Ok(list)
}

#[derive(Serialize)]
pub struct ApplyTrackersResult {
    pub id: usize,
    pub tracker_count: usize,
    pub added: usize,
}

/// The engine fixes a torrent's tracker set when it is added, so applying new
/// trackers means re-adding it in place: same files, same destination, same
/// selection, no re-download.
#[tauri::command]
pub async fn apply_trackers(
    state: tauri::State<'_, AppState>,
    id: usize,
    trackers: Vec<String>,
    replace: bool,
) -> CmdResult<ApplyTrackersResult> {
    apply_trackers_inner(&state, id, trackers, replace).await
}

async fn apply_trackers_inner(
    state: &AppState,
    id: usize,
    trackers: Vec<String>,
    replace: bool,
) -> CmdResult<ApplyTrackersResult> {
    let handle = state
        .session()
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;

    let info_hash = handle.info_hash().as_string();
    let output_folder = handle.output_folder().to_string_lossy().into_owned();
    let only_files = handle.only_files();
    let paused = handle.is_paused();
    let name = handle.name();
    let existing: Vec<String> = handle
        .shared()
        .trackers
        .iter()
        .map(|u| u.to_string())
        .collect();

    let merged = if replace {
        torrentsrc::merge_trackers(&[&trackers])
    } else {
        torrentsrc::merge_trackers(&[&existing, &trackers])
    };
    // Adding it again checks every file again, so not without a reason.
    if same_trackers(&merged, &existing) {
        return Ok(ApplyTrackersResult {
            id,
            tracker_count: existing.len(),
            added: 0,
        });
    }

    let bytes: Option<Bytes> = handle
        .metadata
        .load()
        .as_ref()
        .map(|m| m.torrent_bytes.clone())
        .or_else(|| {
            std::fs::read(state.paths.source_file(&info_hash))
                .ok()
                .map(Bytes::from)
        });

    state
        .session()
        .delete(id.into(), false)
        .await
        .map_err(|e| format!("could not detach the torrent: {e}"))?;

    let opts = || AddTorrentOptions {
        paused,
        overwrite: true,
        output_folder: Some(output_folder.clone()),
        only_files: only_files.clone(),
        ..Default::default()
    };
    let (hash, name, bytes) = (&info_hash, name.as_deref(), bytes.as_ref());

    let handle = match attach(state, hash, name, bytes, &merged, opts()).await {
        Ok((_, handle)) => handle,
        Err(e) => {
            // Re-attaching failed, so put the torrent back as it was rather
            // than leaving it detached from the session.
            let restored = attach(state, hash, name, bytes, &existing, opts()).await.is_ok();
            return Err(if restored {
                format!("Could not add those trackers, so the torrent was left as it was: {e}")
            } else {
                format!("Could not add those trackers, and the torrent could not be restored: {e}")
            });
        }
    };

    let tracker_count = handle.shared().trackers.len();
    Ok(ApplyTrackersResult {
        id: handle.id(),
        tracker_count,
        added: tracker_count.saturating_sub(existing.len()),
    })
}

/// Whether two lists name the same trackers, however each is written.
fn same_trackers(a: &[String], b: &[String]) -> bool {
    let set = |list: &[String]| -> HashSet<String> {
        list.iter()
            .map(|t| match url::Url::parse(t.trim()) {
                Ok(url) => String::from(url).to_ascii_lowercase(),
                Err(_) => t.trim().to_ascii_lowercase(),
            })
            .collect()
    };
    set(a) == set(b)
}

/// Adds a torrent that announces to exactly `trackers`, from its file when
/// there is one and from a magnet link otherwise.
async fn attach(
    state: &AppState,
    info_hash: &str,
    name: Option<&str>,
    bytes: Option<&Bytes>,
    trackers: &[String],
    mut opts: AddTorrentOptions,
) -> Result<(usize, Arc<librqbit::ManagedTorrent>), String> {
    let add = match bytes {
        Some(bytes) => match announcing(bytes, trackers) {
            Some(rewritten) => AddTorrent::from_bytes(rewritten),
            None => {
                opts.trackers = Some(trackers.to_vec());
                AddTorrent::from_bytes(bytes.clone())
            }
        },
        None => AddTorrent::from_url(magnet_for(info_hash, name, trackers)),
    };
    match state.session().add_torrent(add, Some(opts)).await.map_err(err)? {
        AddTorrentResponse::Added(id, handle) | AddTorrentResponse::AlreadyManaged(id, handle) => {
            Ok((id, handle))
        }
        AddTorrentResponse::ListOnly(_) => Err("engine returned no torrent".into()),
    }
}

fn magnet_for(info_hash: &str, name: Option<&str>, trackers: &[String]) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    let mut uri = format!("magnet:?xt=urn:btih:{info_hash}");
    if let Some(name) = name {
        uri.push_str(&format!(
            "&dn={}",
            utf8_percent_encode(name, NON_ALPHANUMERIC)
        ));
    }
    for t in trackers {
        uri.push_str(&format!("&tr={}", utf8_percent_encode(t, NON_ALPHANUMERIC)));
    }
    uri
}

#[derive(Serialize, Clone)]
pub struct BulkApplyResult {
    pub changed: usize,
    /// Torrents that had every one of the trackers already.
    pub unchanged: usize,
    /// Private torrents, which are left with their own trackers.
    pub skipped_private: usize,
    /// Torrents the trackers could not be added to.
    pub failed: usize,
    /// Why the last of those failed.
    pub error: Option<String>,
}

/// Bulk-applies the current tracker selection to every torrent in the session.
#[tauri::command]
pub async fn apply_trackers_to_all(
    state: tauri::State<'_, AppState>,
    trackers: Vec<String>,
) -> CmdResult<BulkApplyResult> {
    let ids: Vec<usize> = engine::collect_rows(&state.session())
        .iter()
        .map(|r| r.id)
        .collect();
    apply_to_each(&state, ids, trackers).await
}

/// Bulk-applies trackers to a chosen set of torrents.
#[tauri::command]
pub async fn apply_trackers_many(
    state: tauri::State<'_, AppState>,
    ids: Vec<usize>,
    trackers: Vec<String>,
) -> CmdResult<BulkApplyResult> {
    apply_to_each(&state, ids, trackers).await
}

async fn apply_to_each(
    state: &AppState,
    ids: Vec<usize>,
    trackers: Vec<String>,
) -> CmdResult<BulkApplyResult> {
    let mut result = BulkApplyResult {
        changed: 0,
        unchanged: 0,
        skipped_private: 0,
        failed: 0,
        error: None,
    };
    for id in ids {
        if state
            .session()
            .get(id.into())
            .is_some_and(|h| engine::is_private(&h))
        {
            result.skipped_private += 1;
            continue;
        }
        match apply_trackers_inner(state, id, trackers.clone(), false).await {
            Ok(applied) if applied.added == 0 => result.unchanged += 1,
            Ok(_) => result.changed += 1,
            Err(e) => {
                result.failed += 1;
                result.error = Some(e);
            }
        }
    }
    let nothing_else = result.changed + result.unchanged + result.skipped_private == 0;
    match result.error.clone() {
        Some(e) if nothing_else => Err(e),
        _ => Ok(result),
    }
}

/// The magnet link for a torrent, including every tracker it currently has.
#[tauri::command]
pub fn magnet_link(state: tauri::State<'_, AppState>, id: usize) -> CmdResult<String> {
    let handle = state
        .session()
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;
    let trackers: Vec<String> = handle
        .shared()
        .trackers
        .iter()
        .map(|u| u.to_string())
        .collect();
    Ok(magnet_for(
        &handle.info_hash().as_string(),
        handle.name().as_deref(),
        &trackers,
    ))
}

/// Everything needed to put a removed torrent back exactly as it was.
#[derive(Serialize, Deserialize, Clone)]
pub struct RestoreToken {
    pub info_hash: String,
    pub name: Option<String>,
    pub output_folder: String,
    pub trackers: Vec<String>,
    pub only_files: Option<Vec<usize>>,
    pub paused: bool,
}

/// Detaches a torrent but leaves the files, returning what is needed to undo it.
#[tauri::command]
pub async fn forget_torrent(
    state: tauri::State<'_, AppState>,
    id: usize,
) -> CmdResult<RestoreToken> {
    forget_inner(&state, id).await
}

pub async fn forget_inner(state: &AppState, id: usize) -> CmdResult<RestoreToken> {
    let handle = state
        .session()
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;
    let token = RestoreToken {
        info_hash: handle.info_hash().as_string(),
        name: handle.name(),
        output_folder: handle.output_folder().to_string_lossy().into_owned(),
        trackers: handle
            .shared()
            .trackers
            .iter()
            .map(|u| u.to_string())
            .collect(),
        only_files: handle.only_files(),
        paused: handle.is_paused(),
    };
    // Keep a copy of the metadata so undo does not need the swarm.
    if let Some(bytes) = handle.metadata.load().as_ref().map(|m| m.torrent_bytes.clone()) {
        let _ = tokio::fs::write(state.paths.source_file(&token.info_hash), &bytes).await;
    }
    state
        .session()
        .delete(id.into(), false)
        .await
        .map_err(|e| format!("could not remove the torrent: {e}"))?;
    Ok(token)
}

/// Puts a forgotten torrent back, files and all.
#[tauri::command]
pub async fn restore_torrent(
    state: tauri::State<'_, AppState>,
    token: RestoreToken,
) -> CmdResult<usize> {
    restore_inner(&state, token).await
}

pub async fn restore_inner(state: &AppState, token: RestoreToken) -> CmdResult<usize> {
    let source = state.paths.source_file(&token.info_hash);
    let bytes = tokio::fs::read(&source).await.ok().map(Bytes::from);
    let opts = AddTorrentOptions {
        paused: token.paused,
        overwrite: true,
        output_folder: Some(token.output_folder.clone()),
        only_files: token.only_files.clone(),
        ..Default::default()
    };
    attach(
        state,
        &token.info_hash,
        token.name.as_deref(),
        bytes.as_ref(),
        &token.trackers,
        opts,
    )
    .await
    .map(|(id, _)| id)
    .map_err(|e| format!("could not put the torrent back: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_folder_falls_back_to_the_closest_existing_folder() {
        let root = std::env::temp_dir().join(format!("gh-open-{}", std::process::id()));
        let inner = root.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("file"), b"x").unwrap();

        assert_eq!(nearest_existing_folder(&inner), Some(inner.clone()));
        let missing = inner.join("not").join("yet");
        assert_eq!(nearest_existing_folder(&missing), Some(inner.clone()));
        assert_eq!(nearest_existing_folder(&inner.join("file")), Some(inner.clone()));

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The exact payloads `src/lib/api.ts` sends must deserialize into the
    /// command argument types, so the two halves cannot drift apart silently.
    #[test]
    fn stage_request_matches_frontend_payload() {
        let json = serde_json::json!({ "kind": "text", "value": "magnet:?xt=urn:btih:abc" });
        let parsed: StageRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.kind, "text");
        assert_eq!(parsed.value, "magnet:?xt=urn:btih:abc");
    }

    #[test]
    fn commit_request_matches_frontend_payload() {
        let json = serde_json::json!({
            "token": "abc",
            "download_dir": "/home/you/Downloads",
            "start_paused": false,
            "use_auto_packs": true,
            "pack_ids": ["starter"],
            "extra_trackers": ["udp://tracker.example:1337/announce"],
            "selected_files": [0, 2]
        });
        let parsed: CommitRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.token, "abc");
        assert!(parsed.use_auto_packs);
        assert_eq!(parsed.selected_files, Some(vec![0, 2]));
    }

    #[test]
    fn commit_request_accepts_null_file_selection() {
        let json = serde_json::json!({
            "token": "abc",
            "download_dir": "",
            "start_paused": true,
            "use_auto_packs": false,
            "pack_ids": [],
            "extra_trackers": [],
            "selected_files": null
        });
        let parsed: CommitRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.selected_files, None);
    }

    use std::time::Duration;
    use tauri::Listener;

    const QUIET_TRACKER: &str = "&tr=udp%3A%2F%2F127.0.0.1%3A1%2Fannounce";

    /// A mock app around a real engine. Offline, nothing leaves the machine.
    async fn test_app(name: &str, online: bool) -> tauri::App<tauri::test::MockRuntime> {
        test_app_bound(name, online, None).await
    }

    async fn test_app_bound(
        name: &str,
        online: bool,
        bind: Option<&str>,
    ) -> tauri::App<tauri::test::MockRuntime> {
        let root = std::env::temp_dir().join(format!("gh-add-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let session = Session::new_with_opts(
            root.join("downloads"),
            librqbit::SessionOptions {
                dht: online.then(|| librqbit::DhtSessionConfig {
                    persistence: None,
                    ..Default::default()
                }),
                persistence: None,
                listen: None,
                bind_device_name: bind.map(str::to_string),
                disable_local_service_discovery: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let mut settings = Settings::default();
        settings.packs.clear();
        settings.download_dir = root.join("downloads").to_string_lossy().into_owned();
        let paths = Paths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            dht_file: Some(root.join("dht.json")),
        };
        std::fs::create_dir_all(paths.data_dir.join("sources")).unwrap();

        let app = tauri::test::mock_app();
        let running = engine::Running::new(
            engine::Engine {
                session,
                bound_interface: None,
                blocklist_active: false,
                blocklist_loaded: None,
                warnings: vec![],
            },
            settings.clone(),
        );
        app.manage(AppState {
            engine: Arc::new(engine::EngineSlot::new(running)),
            restarting: tokio::sync::Mutex::new(()),
            settings: RwLock::new(settings),
            paths,
            staged: Mutex::new(HashMap::new()),
            resolving: Mutex::new(HashMap::new()),
            seeding: Mutex::new(SeedLedger::default()),
            feeds: Mutex::new(FeedsState::default()),
            queue_paused: Mutex::new(QueueHolds::load(root.join("queue-paused.json"))),
            moving: Mutex::new(HashMap::new()),
            activity: Mutex::new(ActivityLog::load(root.join("activity.json"))),
            stream: None,
            startup_warnings: vec![],
            settings_problem: None,
        });
        app
    }

    /// A one-byte single-file torrent, private or not.
    fn torrent_bytes(private: bool) -> Vec<u8> {
        let flag = if private { "7:privatei1e" } else { "" };
        format!(
            "d8:announce18:udp://a:1/announce4:infod6:lengthi1e4:name1:a\
             12:piece lengthi16384e6:pieces20:AAAAAAAAAAAAAAAAAAAA{flag}ee"
        )
        .into_bytes()
    }

    fn staged_file(bytes: Vec<u8>) -> Staged {
        Staged {
            kind: "file".into(),
            source: "a.torrent".into(),
            bytes: Some(Bytes::from(bytes)),
            info_hash: None,
            name: None,
            total_bytes: 0,
            trackers: Vec::new(),
            files: Vec::new(),
            skipped: Vec::new(),
            resolved: false,
            private: false,
            queued: None,
        }
    }

    #[test]
    fn the_private_flag_is_read_from_the_file() {
        tauri::async_runtime::block_on(async {
            let app = test_app("private", false).await;
            let state = app.state::<AppState>();
            for private in [false, true] {
                let staged = staged_file(torrent_bytes(private));
                let resolved = resolve(&state.session(), &staged, &[]).await.unwrap();
                assert_eq!(resolved.private, private);
                assert_eq!(resolved.files.len(), 1);
            }
        });
    }

    #[test]
    fn a_private_torrent_keeps_only_its_own_trackers() {
        let settings = Settings::default();
        let own = vec!["https://private.example/announce?passkey=abc".to_string()];
        let staged = Staged {
            trackers: own.clone(),
            private: true,
            ..staged_file(Vec::new())
        };
        let request = CommitRequest {
            use_auto_packs: true,
            pack_ids: vec!["starter".into()],
            extra_trackers: vec!["udp://open.example:1337/announce".into()],
            ..commit_request("t")
        };
        assert_eq!(trackers_for(&settings, &staged, &request), own);

        let public = Staged {
            private: false,
            ..staged
        };
        assert!(trackers_for(&settings, &public, &request).len() > own.len());
    }

    /// Packs ticked in the add sheet are attached even when they are not
    /// attached to every new torrent.
    #[test]
    fn packs_ticked_in_the_add_sheet_are_attached() {
        let settings = Settings { auto_apply_trackers: false, ..Default::default() };
        let staged = staged_file(Vec::new());
        let ticked = CommitRequest { use_auto_packs: true, ..commit_request("t") };

        assert!(!trackers_for(&settings, &staged, &ticked).is_empty());
        assert!(trackers_for(&settings, &staged, &commit_request("t")).is_empty());
        assert!(settings.active_trackers().is_empty());
    }

    fn commit_request(token: &str) -> CommitRequest {
        CommitRequest {
            token: token.into(),
            download_dir: String::new(),
            start_paused: true,
            use_auto_packs: false,
            pack_ids: vec![],
            extra_trackers: vec![],
            selected_files: None,
        }
    }

    async fn stage_magnet(app: &tauri::App<tauri::test::MockRuntime>, magnet: String) -> String {
        let request = StageRequest { kind: "text".into(), value: magnet };
        stage_source(app.state(), request).await.unwrap().token
    }

    fn events(
        app: &tauri::App<tauri::test::MockRuntime>,
        name: &str,
    ) -> tokio::sync::mpsc::UnboundedReceiver<String> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app.listen(name, move |event| {
            let _ = tx.send(event.payload().to_string());
        });
        rx
    }

    /// Adding a magnet nobody answers for returns at once, shows up as
    /// pending, and cancelling it stops the lookup and wakes anything waiting.
    #[test]
    fn unanswered_magnet_is_queued_and_can_be_cancelled() {
        tauri::async_runtime::block_on(async {
            let app = test_app("quiet", false).await;
            let magnet =
                format!("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Quiet{QUIET_TRACKER}");
            let token = stage_magnet(&app, magnet).await;

            let sheet = tauri::async_runtime::spawn({
                let (handle, token) = (app.handle().clone(), token.clone());
                async move { resolve_staged(handle.clone(), handle.state(), token).await.map(|_| ()) }
            });

            let committed = tokio::time::timeout(
                Duration::from_secs(2),
                commit_staged(app.handle().clone(), app.state(), commit_request(&token)),
            )
            .await
            .expect("commit must not wait for the swarm")
            .unwrap();
            assert!(committed.pending);
            assert_eq!(committed.id, None);

            let state = app.state::<AppState>();
            let pending = pending_adds(&state);
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].name, "Quiet");
            assert_eq!(state.resolving.lock().len(), 1);

            discard_staged(app.state(), token);
            assert!(pending_adds(&state).is_empty());
            assert!(state.resolving.lock().is_empty());
            let waited = tokio::time::timeout(Duration::from_secs(2), sheet).await;
            assert!(waited.expect("the sheet must be released").unwrap().is_err());
        });
    }

    /// A queued add whose lookup fails reports the failure and cleans up.
    #[test]
    fn failed_lookup_reports_and_clears_the_queued_add() {
        tauri::async_runtime::block_on(async {
            let app = test_app("fail", false).await;
            let mut failed = events(&app, "greenhouse://add-failed");
            let magnet = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Nowhere";
            let token = stage_magnet(&app, magnet.into()).await;

            let committed = commit_staged(app.handle().clone(), app.state(), commit_request(&token))
                .await
                .unwrap();
            assert!(committed.pending);

            let payload = tokio::time::timeout(Duration::from_secs(5), failed.recv())
                .await
                .expect("the failure must be reported")
                .unwrap();
            assert!(payload.contains("Nowhere"), "{payload}");
            let state = app.state::<AppState>();
            let log = state.activity.lock().list();
            assert_eq!((log[0].kind.as_str(), log[0].title.as_str()), ("failed", "Nowhere"));
            assert!(state.staged.lock().is_empty());
            assert!(state.resolving.lock().is_empty());
        });
    }

    /// Uses the network: a magnet confirmed before its metadata arrives is
    /// added on its own once a peer answers.
    #[test]
    #[ignore]
    fn queued_magnet_is_added_once_metadata_arrives() {
        tauri::async_runtime::block_on(async {
            let app = test_app("live", true).await;
            let mut added = events(&app, "greenhouse://added");
            let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
            let token = stage_magnet(&app, magnet.into()).await;

            let committed = commit_staged(app.handle().clone(), app.state(), commit_request(&token))
                .await
                .unwrap();
            assert!(committed.pending);

            let payload = tokio::time::timeout(Duration::from_secs(90), added.recv())
                .await
                .expect("no peer answered in time")
                .unwrap();
            assert!(payload.contains("\"pending\":false"), "{payload}");
            let state = app.state::<AppState>();
            assert_eq!(engine::collect_rows(&state.session()).len(), 1);
            assert!(state.staged.lock().is_empty());
            assert!(pending_adds(&state).is_empty());
        });
    }

    fn rss(items: &[(&str, &str)]) -> String {
        let body: String = items
            .iter()
            .map(|(guid, title)| {
                format!(
                    "<item><title>{title}</title><guid>{guid}</guid>\
                     <link>magnet:?xt=urn:btih:{guid:0>40}&amp;dn={guid}{}</link></item>",
                    QUIET_TRACKER.replace('&', "&amp;")
                )
            })
            .collect();
        format!("<rss version=\"2.0\"><channel><title>t</title>{body}</channel></rss>")
    }

    /// Checking a feed queues new items that pass its filters, and only those.
    #[test]
    fn a_feed_queues_new_matching_items() {
        tauri::async_runtime::block_on(async {
            let app = test_app("feeds", false).await;
            let state = app.state::<AppState>();

            let served = Arc::new(Mutex::new(rss(&[("1", "Show E01 1080p")])));
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let router = axum::Router::new().route(
                "/feed",
                axum::routing::get({
                    let served = served.clone();
                    move || async move { served.lock().clone() }
                }),
            );
            tokio::spawn(async move { axum::serve(listener, router).await });

            let feed = feeds::Feed {
                id: "shows".into(),
                name: "Shows".into(),
                url: format!("http://127.0.0.1:{port}/feed"),
                enabled: true,
                must_contain: "1080p".into(),
                download_dir: "/films".into(),
                ..Default::default()
            };

            let first = feeds::check(app.handle(), &feed).await;
            assert_eq!(first.error, None);
            assert_eq!(first.items.len(), 1);
            assert!(pending_adds(&state).is_empty(), "the backlog was downloaded");

            *served.lock() = rss(&[("3", "Show E02 480p"), ("2", "Show E02 1080p"), ("1", "Show E01 1080p")]);
            let second = feeds::check(app.handle(), &feed).await;
            let pending = pending_adds(&state);
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].name, "2");
            assert!(second.items.iter().find(|i| i.guid == "2").unwrap().added);
            let queued = state.staged.lock()[&pending[0].token].queued.clone().unwrap();
            assert_eq!(queued.request.download_dir, "/films");

            feeds::check(app.handle(), &feed).await;
            assert_eq!(pending_adds(&state).len(), 1, "added twice");

            feeds::add_item(app.handle(), &feed, "3").unwrap();
            assert_eq!(pending_adds(&state).len(), 2);

            let gone = feeds::Feed { url: format!("http://127.0.0.1:{port}/missing"), ..feed };
            assert!(feeds::check(app.handle(), &gone).await.error.unwrap().contains("404"));
        });
    }

    #[test]
    fn the_configured_player_wins_over_installed_ones() {
        let has_vlc = |p: &str| (p == "vlc").then(|| p.to_string());
        assert_eq!(player_command("", has_vlc), Some(vec!["vlc".to_string()]));
        assert_eq!(
            player_command("  mpv --fs ", has_vlc),
            Some(vec!["mpv".to_string(), "--fs".to_string()])
        );
        assert_eq!(player_command("", |_| None), None);
    }

    /// A player can read any part of a file through the local stream server,
    /// and nothing is served without the secret in the address.
    #[test]
    fn files_stream_by_byte_range() {
        tauri::async_runtime::block_on(async {
            let app = test_app("stream", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();

            let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
            let payload = root.join("seeded");
            std::fs::create_dir_all(&payload).unwrap();
            std::fs::write(payload.join("film.mkv"), &content).unwrap();
            let torrent = librqbit::create_torrent(
                &payload,
                librqbit::CreateTorrentOptions { name: None, trackers: vec![], piece_length: None },
                &librqbit::spawn_utils::BlockingSpawner::new(1),
            )
            .await
            .unwrap()
            .as_bytes()
            .unwrap();
            let added = state
                .session()
                .add_torrent(
                    AddTorrent::from_bytes(torrent),
                    Some(AddTorrentOptions {
                        overwrite: true,
                        output_folder: Some(payload.to_string_lossy().into_owned()),
                        ..Default::default()
                    }),
                )
                .await
                .unwrap();
            let id = match added {
                AddTorrentResponse::Added(id, handle) => {
                    handle.wait_until_initialized().await.unwrap();
                    id
                }
                _ => panic!("not added"),
            };

            let server = crate::stream::start(state.engine.clone()).await.unwrap();
            let url = server.url(id, 0, "film.mkv");
            let part = reqwest::Client::new()
                .get(&url)
                .header("Range", "bytes=1000-1999")
                .send()
                .await
                .unwrap();
            assert_eq!(part.status(), 206);
            assert_eq!(part.headers()["content-range"], "bytes 1000-1999/200000");
            assert_eq!(part.bytes().await.unwrap().as_ref(), &content[1000..2000]);

            let whole = reqwest::get(&url).await.unwrap();
            assert_eq!(whole.status(), 200);
            assert_eq!(whole.bytes().await.unwrap().as_ref(), content.as_slice());

            let port_end = url[7..].find('/').unwrap() + 7;
            let wrong = format!("{}/not-the-token/{id}/0/film.mkv", &url[..port_end]);
            assert_eq!(reqwest::get(&wrong).await.unwrap().status(), 404);
        });
    }

    /// Uses the network: the start of a video that has only just been added
    /// can be read straight away, because the engine fetches it first.
    #[test]
    #[ignore]
    fn an_unfinished_file_streams_from_the_swarm() {
        tauri::async_runtime::block_on(async {
            let app = test_app("live-stream", true).await;
            let state = app.state::<AppState>();
            let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
            let added = state
                .session()
                .add_torrent(AddTorrent::from_url(magnet), Some(AddTorrentOptions { overwrite: true, ..Default::default() }))
                .await
                .unwrap();
            let AddTorrentResponse::Added(id, handle) = added else { panic!("not added") };
            handle.wait_until_initialized().await.unwrap();

            let details = state.api().api_torrent_details(id.into()).unwrap();
            let files = details.files.unwrap();
            let (index, video) = files.iter().enumerate().max_by_key(|(_, f)| f.length).unwrap();
            assert!(handle.stats().file_progress[index] < video.length, "already downloaded");

            let server = crate::stream::start(state.engine.clone()).await.unwrap();
            let response = reqwest::Client::new()
                .get(server.url(id, index, &video.name))
                .header("Range", "bytes=0-262143")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 206);
            let body = tokio::time::timeout(Duration::from_secs(90), response.bytes())
                .await
                .expect("the start of the file never arrived")
                .unwrap();
            assert_eq!(body.len(), 262_144);
            let _ = state.session().delete(id.into(), true).await;
        });
    }

    /// A magnet still waiting for peers comes back after a restart, with the
    /// choices made for it, and stops coming back once it is cancelled.
    #[test]
    fn queued_magnets_survive_a_restart() {
        tauri::async_runtime::block_on(async {
            let first = test_app("queue-a", false).await;
            let magnet =
                format!("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Patient{QUIET_TRACKER}");
            let token = stage_magnet(&first, magnet).await;
            let mut request = commit_request(&token);
            request.download_dir = "/somewhere/else".into();
            commit_staged(first.handle().clone(), first.state(), request).await.unwrap();

            let second = test_app("queue-b", false).await;
            let (from, to) = (first.state::<AppState>(), second.state::<AppState>());
            std::fs::copy(from.paths.queued_file(), to.paths.queued_file()).unwrap();
            restore_queue(second.handle());

            let pending = pending_adds(&to);
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].name, "Patient");
            assert_eq!(to.resolving.lock().len(), 1);
            let kept = to.staged.lock()[&pending[0].token].queued.clone().unwrap();
            assert_eq!(kept.request.download_dir, "/somewhere/else");

            discard_staged(second.state(), pending[0].token.clone());
            assert_eq!(std::fs::read_to_string(to.paths.queued_file()).unwrap(), "[]");
        });
    }

    /// An engine for restart tests: offline, but it remembers its torrents.
    async fn persistent_engine(root: PathBuf, settings: Settings) -> anyhow::Result<engine::Engine> {
        anyhow::ensure!(settings.listen_port != 1, "port 1 is not ours to take");
        let session = Session::new_with_opts(
            root.join("downloads"),
            librqbit::SessionOptions {
                dht: None,
                listen: None,
                disable_local_service_discovery: true,
                fastresume: true,
                persistence: Some(librqbit::SessionPersistenceConfig::Json {
                    folder: Some(root.join("session")),
                }),
                ..Default::default()
            },
        )
        .await?;
        Ok(engine::Engine {
            session,
            bound_interface: Some(format!("port-{}", settings.listen_port)),
            blocklist_active: false,
            blocklist_loaded: None,
            warnings: vec![],
        })
    }

    /// Restarting the engine swaps it for a new one without losing torrents or
    /// the magnets still waiting, and bad settings fall back to the last good ones.
    #[test]
    fn the_engine_restarts_in_place() {
        tauri::async_runtime::block_on(async {
            let app = test_app("restart", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let build = |settings: Settings| persistent_engine(root.clone(), settings);
            restart_engine_with(app.handle(), &state, build).await.unwrap();

            let payload = root.join("payload");
            std::fs::create_dir_all(&payload).unwrap();
            std::fs::write(payload.join("a.bin"), vec![7u8; 40_000]).unwrap();
            let torrent = librqbit::create_torrent(
                &payload,
                librqbit::CreateTorrentOptions { name: None, trackers: vec![], piece_length: None },
                &librqbit::spawn_utils::BlockingSpawner::new(1),
            )
            .await
            .unwrap()
            .as_bytes()
            .unwrap();
            let added = add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();
            let magnet =
                format!("magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Waiting{QUIET_TRACKER}");
            let token = stage_magnet(&app, magnet).await;
            commit_staged(app.handle().clone(), app.state(), commit_request(&token)).await.unwrap();

            let before = state.session();
            state.settings.write().listen_port = 4242;
            let info = restart_engine_with(app.handle(), &state, build).await.unwrap();

            assert!(!Arc::ptr_eq(&before, &state.session()));
            assert_eq!(info.bound_interface.as_deref(), Some("port-4242"));
            assert!(info.warnings.is_empty());
            let rows = engine::collect_rows(&state.session());
            assert_eq!(rows.len(), 1);
            assert_eq!(Some(rows[0].id), added.id);
            assert_ne!(rows[0].state, "paused");
            assert_eq!(pending_adds(&state).len(), 1);
            assert_eq!(state.resolving.lock().len(), 1);

            state.settings.write().listen_port = 1;
            let info = restart_engine_with(app.handle(), &state, build).await.unwrap();
            assert_eq!(info.bound_interface.as_deref(), Some("port-4242"));
            assert!(info.warnings[0].contains("previous ones are still in use"));
            assert_eq!(engine::collect_rows(&state.session()).len(), 1);
        });
    }

    /// A port that cannot be opened at launch gives way to a free one, with a
    /// warning, instead of stopping the app from starting.
    #[test]
    fn a_taken_port_does_not_stop_the_launch() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!("gh-launch-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let build = |settings: Settings| persistent_engine(root.clone(), settings);

            let taken = Settings { listen_port: 1, ..Default::default() };
            let (engine, applied) = start_engine_with(&taken, build).await.unwrap();
            assert_eq!(applied.listen_port, 0);
            assert!(engine.warnings[0].contains("Port 1"), "{:?}", engine.warnings);
            engine.session.stop().await;

            let fine = Settings { listen_port: 4242, ..Default::default() };
            let (engine, applied) = start_engine_with(&fine, build).await.unwrap();
            assert_eq!(applied.listen_port, 4242);
            assert!(engine.warnings.is_empty());
        });
    }

    /// Uses the network: the real engine comes back on the same port.
    #[test]
    #[ignore]
    fn the_real_engine_restarts_on_its_port() {
        tauri::async_runtime::block_on(async {
            let app = test_app("real-restart", false).await;
            let state = app.state::<AppState>();
            let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
            {
                let mut settings = state.settings.write();
                settings.listen_port = port;
                settings.enable_upnp = false;
            }
            std::fs::create_dir_all(state.paths.session_dir()).unwrap();
            let paths = state.paths.clone();
            let build = |settings: Settings| {
                let paths = paths.clone();
                async move { engine::build_session(&paths, &settings).await }
            };

            restart_engine_with(app.handle(), &state, build).await.unwrap();
            assert_eq!(state.session().announce_port(), Some(port));
            let first = state.session();

            let info = restart_engine_with(app.handle(), &state, build).await.unwrap();
            assert!(info.warnings.is_empty(), "{:?}", info.warnings);
            assert!(!Arc::ptr_eq(&first, &state.session()));
            assert_eq!(state.session().announce_port(), Some(port));
        });
    }

    /// A download folder changed in the settings is used straight away, not
    /// only after the engine next starts.
    #[test]
    fn a_changed_download_folder_applies_to_the_next_add() {
        tauri::async_runtime::block_on(async {
            let app = test_app("folder", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let payload = root.join("payload");
            std::fs::create_dir_all(&payload).unwrap();
            std::fs::write(payload.join("a.bin"), b"some bytes").unwrap();
            let torrent = librqbit::create_torrent(
                &payload,
                librqbit::CreateTorrentOptions { name: None, trackers: vec![], piece_length: None },
                &librqbit::spawn_utils::BlockingSpawner::new(1),
            )
            .await
            .unwrap()
            .as_bytes()
            .unwrap();

            let elsewhere = root.join("elsewhere").to_string_lossy().into_owned();
            {
                let mut settings = state.settings.write();
                settings.download_dir = elsewhere.clone();
                settings.start_paused = true;
            }
            add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();
            assert_eq!(engine::collect_rows(&state.session())[0].output_folder, elsewhere);
        });
    }

    /// Adding a torrent that is already in the session says so, and does not
    /// schedule its files to be moved somewhere new.
    #[test]
    fn adding_a_torrent_twice_is_reported_not_repeated() {
        tauri::async_runtime::block_on(async {
            let app = test_app("twice", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let payload = root.join("payload");
            std::fs::create_dir_all(&payload).unwrap();
            std::fs::write(payload.join("a.bin"), b"some bytes").unwrap();
            let torrent = librqbit::create_torrent(
                &payload,
                librqbit::CreateTorrentOptions { name: None, trackers: vec![], piece_length: None },
                &librqbit::spawn_utils::BlockingSpawner::new(1),
            )
            .await
            .unwrap()
            .as_bytes()
            .unwrap();
            state.settings.write().start_paused = true;

            let first = add_file_with_defaults(&state, "a".into(), torrent.clone()).await.unwrap();
            assert!(!first.already);

            state.settings.write().incomplete_dir = root.join("partial").to_string_lossy().into_owned();
            let second = add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();
            assert!(second.already);
            assert_eq!(second.id, first.id);
            assert_eq!(engine::collect_rows(&state.session()).len(), 1);
            assert!(settings::load_pending_moves(&state.paths.pending_moves_file()).is_empty());
        });
    }

    /// A torrent taken out of the incomplete folder and added again without
    /// one does not keep the old move.
    #[test]
    fn a_fresh_add_drops_the_move_left_by_an_earlier_one() {
        tauri::async_runtime::block_on(async {
            let app = test_app("stale-move", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            {
                let mut settings = state.settings.write();
                settings.start_paused = true;
                settings.incomplete_dir = root.join("partial").to_string_lossy().into_owned();
            }
            let torrent = make_torrent(&root, "payload", b"some bytes", &[]).await;
            let moves = || settings::load_pending_moves(&state.paths.pending_moves_file());

            let added = add_file_with_defaults(&state, "a".into(), torrent.clone()).await.unwrap();
            assert_eq!(moves().len(), 1);
            forget_inner(&state, added.id.unwrap()).await.unwrap();
            assert_eq!(moves().len(), 1, "putting it back still needs the move");

            state.settings.write().incomplete_dir.clear();
            add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();
            assert!(moves().is_empty());
        });
    }

    async fn make_torrent(dir: &Path, name: &str, contents: &[u8], trackers: &[&str]) -> Bytes {
        let payload = dir.join(name);
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join("a.bin"), contents).unwrap();
        std::fs::write(payload.join("b.bin"), contents).unwrap();
        librqbit::create_torrent(
            &payload,
            librqbit::CreateTorrentOptions {
                name: None,
                trackers: trackers.iter().map(|t| t.to_string()).collect(),
                piece_length: None,
            },
            &librqbit::spawn_utils::BlockingSpawner::new(1),
        )
        .await
        .unwrap()
        .as_bytes()
        .unwrap()
    }

    /// An automatic add where every file is a skipped type adds nothing.
    #[test]
    fn nothing_is_added_when_every_file_is_skipped() {
        tauri::async_runtime::block_on(async {
            let app = test_app("all-skipped", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            state.settings.write().skip_types = vec!["bin".into()];
            let torrent = make_torrent(&root, "payload", b"some bytes", &[]).await;

            let Err(error) = add_file_with_defaults(&state, "a".into(), torrent).await else {
                panic!("it was added");
            };
            assert!(error.contains("skip"), "{error}");
            assert!(engine::collect_rows(&state.session()).is_empty());
        });
    }

    /// Two torrents with the same name download into separate folders.
    #[test]
    fn same_named_torrents_do_not_share_an_incomplete_folder() {
        tauri::async_runtime::block_on(async {
            let app = test_app("same-name", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            {
                let mut settings = state.settings.write();
                settings.start_paused = true;
                settings.incomplete_dir = root.join("partial").to_string_lossy().into_owned();
            }
            let one = make_torrent(&root.join("one"), "Release", b"first", &[]).await;
            let two = make_torrent(&root.join("two"), "Release", b"second", &[]).await;
            add_file_with_defaults(&state, "one".into(), one).await.unwrap();
            add_file_with_defaults(&state, "two".into(), two).await.unwrap();

            let rows = engine::collect_rows(&state.session());
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].name, rows[1].name);
            assert_ne!(rows[0].output_folder, rows[1].output_folder);
        });
    }

    fn trackers_of(state: &AppState, id: usize) -> Vec<String> {
        let handle = state.session().get(id.into()).unwrap();
        let mut list: Vec<String> = handle.shared().trackers.iter().map(|u| u.to_string()).collect();
        list.sort();
        list
    }

    /// Trackers added to a torrent file survive a restart, and ones the file
    /// came with can be removed.
    #[test]
    fn trackers_can_be_removed_and_survive_a_restart() {
        tauri::async_runtime::block_on(async {
            let app = test_app("tracker-file", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let build = |settings: Settings| persistent_engine(root.clone(), settings);
            restart_engine_with(app.handle(), &state, build).await.unwrap();

            let own = "udp://own.example:1/announce";
            let pack = "udp://pack.example:2/announce";
            let fresh = "udp://fresh.example:3/announce";
            {
                let mut settings = state.settings.write();
                settings.start_paused = true;
                settings.packs = vec![settings::TrackerPack {
                    enabled: true,
                    trackers: vec![pack.into()],
                    ..Default::default()
                }];
            }
            let torrent = make_torrent(&root, "payload", b"some bytes", &[own]).await;
            let id = add_file_with_defaults(&state, "a".into(), torrent).await.unwrap().id.unwrap();
            assert_eq!(trackers_of(&state, id), vec![own, pack]);

            restart_engine_with(app.handle(), &state, build).await.unwrap();
            assert_eq!(trackers_of(&state, id), vec![own, pack]);

            let applied = apply_trackers_inner(&state, id, vec![fresh.into()], true).await.unwrap();
            assert_eq!(applied.tracker_count, 1);
            assert_eq!(trackers_of(&state, applied.id), vec![fresh]);

            restart_engine_with(app.handle(), &state, build).await.unwrap();
            assert_eq!(trackers_of(&state, applied.id), vec![fresh]);
        });
    }

    /// Adding trackers to several torrents says which ones failed, and leaves
    /// alone the ones that have them already.
    #[test]
    fn bulk_trackers_count_failures_and_skip_what_is_unchanged() {
        tauri::async_runtime::block_on(async {
            let app = test_app("bulk-trackers", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            state.settings.write().start_paused = true;
            let tracker = "udp://pack.example:2/announce";
            let has = make_torrent(&root.join("one"), "One", b"first", &[tracker]).await;
            let lacks = make_torrent(&root.join("two"), "Two", b"second", &[]).await;
            let has = add_file_with_defaults(&state, "one".into(), has).await.unwrap().id.unwrap();
            let lacks = add_file_with_defaults(&state, "two".into(), lacks).await.unwrap().id.unwrap();

            let result = apply_to_each(&state, vec![has, lacks, 999], vec![tracker.to_uppercase()])
                .await
                .unwrap();

            assert_eq!((result.changed, result.unchanged, result.failed), (1, 1, 1));
            assert!(result.error.unwrap().contains("no longer"));
            assert!(state.session().get(has.into()).is_some(), "it was added again for nothing");
            let rows = engine::collect_rows(&state.session());
            assert!(rows.iter().all(|r| r.tracker_count == 1));

            let Err(error) = apply_to_each(&state, vec![999], vec![tracker.into()]).await else {
                panic!("a failure was reported as a success");
            };
            assert!(error.contains("no longer"));
        });
    }

    /// Taking the download limit away starts what the queue was holding, and
    /// leaves alone what you paused yourself.
    #[test]
    fn the_queue_lets_go_when_its_limit_is_removed() {
        tauri::async_runtime::block_on(async {
            let app = test_app("unlimited", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            state.settings.write().start_paused = true;
            let held = make_torrent(&root.join("one"), "Held", b"first", &[]).await;
            let mine = make_torrent(&root.join("two"), "Mine", b"second", &[]).await;
            let held = add_file_with_defaults(&state, "one".into(), held).await.unwrap().id.unwrap();
            let mine = add_file_with_defaults(&state, "two".into(), mine).await.unwrap().id.unwrap();
            let hash = state.session().get(held.into()).unwrap().info_hash().as_string();
            state.queue_paused.lock().insert(hash.clone());

            let state_of = |id: usize| {
                engine::collect_rows(&state.session()).into_iter().find(|r| r.id == id).unwrap().state
            };
            // Both check their files before they settle as paused.
            for _ in 0..100 {
                if state_of(held) == "paused" && state_of(mine) == "paused" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            let rows = engine::collect_rows(&state.session());
            crate::supervisor::Supervisor::default().enforce_queue(&state, &rows).await;

            assert_ne!(state_of(held), "paused");
            assert_eq!(state_of(mine), "paused");
            assert!(!state.queue_paused.lock().contains(&hash));
        });
    }

    /// Resume all starts whatever Pause all stopped, and pausing twice does
    /// not touch what is already stopped.
    #[test]
    fn resume_all_starts_what_pause_all_stopped() {
        tauri::async_runtime::block_on(async {
            let app = test_app("pause-all", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let one = make_torrent(&root.join("one"), "One", b"first", &[]).await;
            let two = make_torrent(&root.join("two"), "Two", b"second", &[]).await;
            add_file_with_defaults(&state, "one".into(), one).await.unwrap();
            add_file_with_defaults(&state, "two".into(), two).await.unwrap();

            async fn both_downloading(state: &AppState) {
                let states = || -> Vec<String> {
                    let rows = engine::collect_rows(&state.session());
                    rows.into_iter().map(|r| r.state).collect()
                };
                for _ in 0..100 {
                    if states() == ["downloading", "downloading"] {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                panic!("they never started: {:?}", states());
            }

            both_downloading(&state).await;
            assert_eq!(pause_all_inner(&state).await, 2);
            assert!(engine::collect_rows(&state.session()).iter().all(|r| r.stopped));
            assert_eq!(pause_all_inner(&state).await, 0);

            assert_eq!(resume_all_inner(&state).await, 2);
            both_downloading(&state).await;
        });
    }

    /// A download that finishes is reported once. A torrent that was already
    /// complete, or was only checked again, is not.
    #[test]
    fn only_a_download_that_just_finished_is_announced() {
        tauri::async_runtime::block_on(async {
            let app = test_app("announce", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            {
                let mut settings = state.settings.write();
                settings.start_paused = true;
                settings.notify_on_done = false;
            }
            let torrent = make_torrent(&root, "payload", b"some bytes", &[]).await;
            add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();
            let finished = |downloaded_now| {
                let mut rows = engine::collect_rows(&state.session());
                rows[0].finished = true;
                rows[0].downloaded_now = downloaded_now;
                rows
            };
            let announced = || {
                let log = state.activity.lock().list();
                log.iter().filter(|e| e.kind == "finished").count()
            };

            let mut seeding = crate::supervisor::Supervisor::default();
            seeding.announce_finished(app.handle(), &state, &finished(0));
            seeding.announce_finished(app.handle(), &state, &finished(0));
            assert_eq!(announced(), 0, "a torrent that was already complete was announced");

            let mut downloaded = crate::supervisor::Supervisor::default();
            downloaded.announce_finished(app.handle(), &state, &finished(10));
            downloaded.announce_finished(app.handle(), &state, &finished(10));
            assert_eq!(announced(), 1);
        });
    }

    /// A torrent over the limit is not paused while it is checking its files.
    #[test]
    fn the_queue_leaves_a_file_check_running() {
        tauri::async_runtime::block_on(async {
            let app = test_app("queue-check", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            state.settings.write().max_active_downloads = 1;
            let one = make_torrent(&root.join("one"), "One", b"first", &[]).await;
            let two = make_torrent(&root.join("two"), "Two", b"second", &[]).await;
            add_file_with_defaults(&state, "one".into(), one).await.unwrap();
            let two = add_file_with_defaults(&state, "two".into(), two).await.unwrap().id.unwrap();

            let mut rows = engine::collect_rows(&state.session());
            let checking = rows.iter_mut().find(|r| r.id == two).unwrap();
            checking.state = "checking".into();
            let hash = checking.info_hash.clone();
            crate::supervisor::Supervisor::default().enforce_queue(&state, &rows).await;

            assert!(!state.queue_paused.lock().contains(&hash));
            assert!(!state.session().get(two.into()).unwrap().is_paused());
        });
    }

    /// A download that finishes in the incomplete folder ends up where it
    /// belongs, and stays in the list while its files are moved.
    #[test]
    fn a_finished_download_moves_out_of_the_incomplete_folder() {
        tauri::async_runtime::block_on(async {
            let app = test_app("move-out", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();
            let partial = root.join("partial").to_string_lossy().into_owned();
            state.settings.write().incomplete_dir = partial.clone();

            // With the files already in place, the first check finds it finished.
            let torrent = make_torrent(&root.join("made"), "Release", b"some bytes", &[]).await;
            let staged = resolve(&state.session(), &staged_file(torrent.to_vec()), &[]).await.unwrap();
            let staging = PathBuf::from(staging_folder(&partial, &staged));
            std::fs::create_dir_all(&staging).unwrap();
            for name in ["a.bin", "b.bin"] {
                std::fs::copy(root.join("made/Release").join(name), staging.join(name)).unwrap();
            }
            add_file_with_defaults(&state, "a".into(), torrent).await.unwrap();

            let mut finished = None;
            for _ in 0..100 {
                finished = engine::collect_rows(&state.session()).into_iter().find(|r| r.finished);
                if finished.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let rows = [finished.expect("it never finished checking")];
            crate::supervisor::Supervisor::default().move_finished(app.handle(), &state, &rows);

            let mut last = rows_with_totals(&state);
            for _ in 0..100 {
                assert_eq!(last.len(), 1, "it dropped out of the list");
                if last[0].finished && last[0].state != "moving" && state.moving.lock().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                last = rows_with_totals(&state);
            }

            let destination = root.join("downloads").join("Release");
            assert_eq!(PathBuf::from(&last[0].output_folder), destination);
            assert!(destination.join("a.bin").exists() && destination.join("b.bin").exists());
            assert!(!staging.exists());
            assert!(settings::load_pending_moves(&state.paths.pending_moves_file()).is_empty());
        });
    }

    /// A .torrent file in the watch folder is added once, and one that cannot
    /// be read is set aside instead of being retried forever.
    #[test]
    fn watch_folder_adds_each_torrent_once() {
        tauri::async_runtime::block_on(async {
            let app = test_app("watch", false).await;
            let state = app.state::<AppState>();
            let root = PathBuf::from(state.settings.read().download_dir.clone())
                .parent()
                .unwrap()
                .to_path_buf();

            let payload = root.join("payload");
            std::fs::create_dir_all(&payload).unwrap();
            std::fs::write(payload.join("hello.txt"), b"hello from the watch folder").unwrap();
            let torrent = librqbit::create_torrent(
                &payload,
                librqbit::CreateTorrentOptions { name: None, trackers: vec![], piece_length: None },
                &librqbit::spawn_utils::BlockingSpawner::new(1),
            )
            .await
            .unwrap()
            .as_bytes()
            .unwrap();

            let watch = root.join("watch");
            std::fs::create_dir_all(&watch).unwrap();
            std::fs::write(watch.join("hello.torrent"), &torrent).unwrap();
            std::fs::write(watch.join("broken.torrent"), b"not a torrent").unwrap();
            std::fs::write(watch.join("notes.txt"), b"left alone").unwrap();
            {
                let mut settings = state.settings.write();
                settings.watch_dir = watch.to_string_lossy().into_owned();
                settings.start_paused = true;
            }
            let mut added = events(&app, "greenhouse://added");
            let mut failed = events(&app, "greenhouse://add-failed");

            crate::watch::scan(app.handle(), Duration::ZERO).await;
            crate::watch::scan(app.handle(), Duration::ZERO).await;

            assert_eq!(engine::collect_rows(&state.session()).len(), 1);
            assert!(added.try_recv().is_ok());
            assert!(added.try_recv().is_err(), "added twice");
            assert!(failed.try_recv().unwrap().contains("broken.torrent"));
            let log = state.activity.lock().list();
            assert_eq!(log.len(), 2);
            assert_eq!(log[0].kind, "added");
            assert_eq!(log[0].detail.as_deref(), Some("Added from the watch folder"));
            assert_eq!((log[1].kind.as_str(), log[1].title.as_str()), ("failed", "broken.torrent"));
            assert!(watch.join("hello.torrent.added").exists());
            assert!(watch.join("broken.torrent.failed").exists());
            assert!(!watch.join("hello.torrent").exists());
            assert!(watch.join("notes.txt").exists());
        });
    }

    /// Uses the network: tied to an interface that is up, adds still work.
    #[test]
    #[ignore]
    fn a_live_interface_still_reaches_the_swarm() {
        tauri::async_runtime::block_on(async {
            let up = engine::list_interfaces().into_iter().find(|i| i.up).unwrap();
            let app = test_app_bound("iface", true, Some(&up.name)).await;
            let mut added = events(&app, "greenhouse://added");
            let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
            let token = stage_magnet(&app, magnet.into()).await;
            commit_staged(app.handle().clone(), app.state(), commit_request(&token))
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(90), added.recv())
                .await
                .unwrap_or_else(|_| panic!("no peer answered over {}", up.name));
        });
    }

    /// Uses the network: tied to loopback, a well-seeded magnet finds nobody.
    #[test]
    #[ignore]
    fn nothing_is_reached_when_bound_to_loopback() {
        tauri::async_runtime::block_on(async {
            let app = test_app_bound("lo", true, Some("lo")).await;
            let mut added = events(&app, "greenhouse://added");
            let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
            let token = stage_magnet(&app, magnet.into()).await;
            commit_staged(app.handle().clone(), app.state(), commit_request(&token))
                .await
                .unwrap();

            let outcome = tokio::time::timeout(Duration::from_secs(20), added.recv()).await;
            assert!(outcome.is_err(), "traffic left the machine: {outcome:?}");
            assert_eq!(pending_adds(&app.state::<AppState>()).len(), 1);
        });
    }

    /// A queued add has no file selection yet, so it falls back to everything
    /// except the file types skipped by default.
    #[test]
    fn queued_adds_leave_out_skipped_files() {
        let file = |index| StagedFile { index, name: String::new(), components: vec![], length: 1 };
        let staged = Staged {
            kind: "magnet".into(),
            source: String::new(),
            bytes: None,
            info_hash: None,
            name: None,
            total_bytes: 0,
            trackers: vec![],
            files: vec![file(0), file(1), file(2)],
            skipped: vec![1],
            resolved: true,
            private: false,
            queued: None,
        };
        assert_eq!(default_selection(&staged), Some(vec![0, 2]));
        assert_eq!(default_selection(&Staged { skipped: vec![], ..staged }), None);
    }

    /// A custom destination keeps librqbit's per-torrent sub-folder behaviour.
    #[test]
    fn output_folder_follows_the_chosen_destination() {
        let multi = Staged {
            kind: "file".into(),
            source: String::new(),
            bytes: None,
            info_hash: None,
            name: Some("Some Release".into()),
            total_bytes: 0,
            trackers: vec![],
            files: vec![
                StagedFile { index: 0, name: "a".into(), components: vec![], length: 1 },
                StagedFile { index: 1, name: "b".into(), components: vec![], length: 1 },
            ],
            skipped: vec![],
            resolved: true,
            private: false,
            queued: None,
        };
        let single = Staged { files: vec![], ..multi.clone() };

        // Session default: let the engine choose, as it does for a plain add.
        assert_eq!(resolve_output_folder("/dl", "/dl", &multi), None);
        // Custom destination, many files: recreate the sub-folder.
        assert_eq!(
            resolve_output_folder("/other", "/dl", &multi),
            Some(PathBuf::from("/other").join("Some Release").to_string_lossy().into_owned())
        );
        // Custom destination, single file: no sub-folder.
        assert_eq!(
            resolve_output_folder("/other", "/dl", &single),
            Some("/other".to_string())
        );
    }
}
