use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, Session};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tokio::sync::watch;

use crate::engine::{self, SessionSummary, TorrentDetail, TorrentRow};
use crate::feeds::{self, FeedStatus, FeedsState};
use crate::seeding::SeedLedger;
use crate::stream::StreamServer;
use crate::settings::{self, Paths, Settings};
use crate::torrentsrc;

pub struct AppState {
    pub session: Arc<Session>,
    pub api: Api,
    pub settings: RwLock<Settings>,
    pub paths: Paths,
    pub staged: Mutex<HashMap<String, Staged>>,
    /// Metadata lookups in flight, by staged token. Locked after `staged`.
    pub resolving: Mutex<HashMap<String, Resolving>>,
    pub seeding: Mutex<SeedLedger>,
    pub feeds: Mutex<FeedsState>,
    /// Serves files to a media player while they download.
    pub stream: Option<StreamServer>,
    pub bound_interface: Option<String>,
    pub blocklist_active: bool,
    pub startup_warnings: Vec<String>,
    pub http: reqwest::Client,
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
    pub bound_interface: Option<String>,
    pub warnings: Vec<String>,
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
        bound_interface: state.bound_interface.clone(),
        warnings: state.startup_warnings.clone(),
    }
}

#[tauri::command]
pub fn list_interfaces() -> Vec<engine::NetInterface> {
    engine::list_interfaces()
}

#[tauri::command]
pub fn save_settings(state: tauri::State<'_, AppState>, next: Settings) -> CmdResult<Settings> {
    state
        .session
        .ratelimits
        .set_download_bps(engine::kbps_to_bps(next.download_limit_kbps));
    state
        .session
        .ratelimits
        .set_upload_bps(engine::kbps_to_bps(next.upload_limit_kbps));
    settings::save(&state.paths.settings_file(), &next).map_err(err)?;
    *state.settings.write() = next.clone();
    Ok(next)
}

#[tauri::command]
pub fn list_torrents(state: tauri::State<'_, AppState>) -> Vec<TorrentRow> {
    rows_with_totals(&state)
}

/// The torrent list, with upload figures that carry across restarts.
pub fn rows_with_totals(state: &AppState) -> Vec<TorrentRow> {
    let mut rows = engine::collect_rows(&state.session);
    state.seeding.lock().overlay(&mut rows);
    rows
}

#[tauri::command]
pub fn session_stats(state: tauri::State<'_, AppState>) -> SessionSummary {
    engine::session_summary(&state.session, &state.settings.read())
}

#[tauri::command]
pub fn torrent_detail(state: tauri::State<'_, AppState>, id: usize) -> CmdResult<TorrentDetail> {
    engine::torrent_detail(&state.api, &state.paths, id).map_err(err)
}

#[tauri::command]
pub async fn torrent_action(
    state: tauri::State<'_, AppState>,
    id: usize,
    action: String,
) -> CmdResult<()> {
    match action.as_str() {
        "pause" => state.api.api_torrent_action_pause(id.into()).await.map(|_| ()),
        "start" => state.api.api_torrent_action_start(id.into()).await.map(|_| ()),
        "forget" => state.api.api_torrent_action_forget(id.into()).await.map(|_| ()),
        "delete" => state.api.api_torrent_action_delete(id.into()).await.map(|_| ()),
        other => return Err(format!("unknown action: {other}")),
    }
    .map_err(err)
}

#[tauri::command]
pub async fn set_file_selection(
    state: tauri::State<'_, AppState>,
    id: usize,
    indices: Vec<usize>,
) -> CmdResult<()> {
    let set: HashSet<usize> = indices.into_iter().collect();
    state
        .api
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
    pub bytes: u64,
}

fn blocklist_status_of(state: &AppState) -> BlocklistStatus {
    let meta = std::fs::metadata(state.paths.blocklist_file()).ok();
    BlocklistStatus {
        active: state.blocklist_active,
        updated_at: meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        bytes: meta.map(|m| m.len()).unwrap_or(0),
    }
}

#[tauri::command]
pub fn blocklist_status(state: tauri::State<'_, AppState>) -> BlocklistStatus {
    blocklist_status_of(&state)
}

/// Fetches the blocklist named in the settings into the copy the engine reads
/// at launch. With no URL set, the copy is removed.
pub async fn download_blocklist(state: &AppState) -> CmdResult<()> {
    let url = state.settings.read().blocklist_url.trim().to_string();
    let file = state.paths.blocklist_file();
    if url.is_empty() {
        let _ = tokio::fs::remove_file(file).await;
        return Ok(());
    }
    let response = state
        .http
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
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join(program).is_file())
    })
}

/// The player to run, as a program and its leading arguments: the one named
/// in the settings, otherwise the first well-known player that is installed.
fn player_command(configured: &str, installed: impl Fn(&str) -> bool) -> Option<Vec<String>> {
    let configured: Vec<String> = configured.split_whitespace().map(String::from).collect();
    if !configured.is_empty() {
        return Some(configured);
    }
    PLAYERS
        .iter()
        .find(|p| installed(p))
        .map(|p| vec![p.to_string()])
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
        .session
        .get(id.into())
        .ok_or("that torrent is no longer in the session")?;
    let details = state.api.api_torrent_details(id.into()).map_err(err)?;
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
                .api
                .api_torrent_action_update_only_files(id.into(), &wanted)
                .await
                .map_err(err)?;
        }
        if handle.is_paused() {
            state.api.api_torrent_action_start(id.into()).await.map_err(err)?;
        }
        let server = state.stream.as_ref().ok_or("streaming could not be started")?;
        server.url(id, file_index, &file.name)
    };

    let configured = state.settings.read().player_command.clone();
    let player = match player_command(&configured, on_path) {
        Some(command) => {
            let mut child = std::process::Command::new(&command[0])
                .args(&command[1..])
                .arg(&target)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
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
                queued: None,
            }
        }
        _ => staged_from_text(&request.value)?,
    };

    // A .torrent file carries its own metadata, so the file list is available
    // immediately without touching the network.
    if staged.bytes.is_some() {
        if let Ok(resolved) = resolve(&state.session, &staged, &[]).await {
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
        None => AddTorrent::from_url(staged.source.clone()),
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
                run_queued(&app, &state, &token, queued.request, outcome).await;
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
    let mut resolved = resolve(&state.session, &staged, &extra).await.map_err(err)?;
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
    mut request: CommitRequest,
    outcome: CmdResult<Staged>,
) {
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
            let _ = app.emit("greenhouse://added", added);
        }
        Err(error) => {
            let name = state.staged.lock().remove(token).and_then(|s| s.name);
            let _ = app.emit("greenhouse://add-failed", AddFailed { name, error });
        }
    }
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
                queued: Some(Queued {
                    request: CommitRequest { token: token.clone(), ..record.request },
                    since: UNIX_EPOCH + Duration::from_secs(record.queued_at),
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
            });
            ensure_resolving(&app, &state, &token);
            Err(result)
        } else {
            Ok(entry.clone())
        }
    };
    match ready {
        Ok(staged) => commit_resolved(&state, &staged, &request).await,
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
        queued: None,
    };
    let mut staged = resolve(&state.session, &staged, &[]).await.map_err(err)?;
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
    let settings = state.settings.read();
    let auto = if request.use_auto_packs {
        settings.active_trackers()
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

    let (session_default, incomplete_dir) = {
        let settings = state.settings.read();
        (settings.download_dir.clone(), settings.incomplete_dir.trim().to_string())
    };

    // With an incomplete folder set, torrents land in their own sub-folder
    // there and move to the real destination on completion.
    let (output_folder, pending_final) = if incomplete_dir.is_empty() {
        (
            resolve_output_folder(&request.download_dir, &session_default, staged),
            None,
        )
    } else {
        let destination = if request.download_dir.trim().is_empty() {
            session_default.clone()
        } else {
            request.download_dir.clone()
        };
        (
            Some(staging_folder(&incomplete_dir, staged)),
            Some(explicit_output_folder(&destination, staged)),
        )
    };

    let add = match &staged.bytes {
        Some(bytes) => AddTorrent::from_bytes(bytes.clone()),
        None => AddTorrent::from_url(staged.source.clone()),
    };
    let opts = AddTorrentOptions {
        paused: request.start_paused,
        overwrite: true,
        output_folder,
        only_files: request.selected_files.clone().filter(|f| {
            // An empty or complete selection means "everything".
            !f.is_empty() && f.len() != staged.files.len()
        }),
        trackers: if trackers.is_empty() {
            None
        } else {
            Some(trackers.clone())
        },
        ..Default::default()
    };

    let response = state
        .session
        .add_torrent(add, Some(opts))
        .await
        .map_err(err)?;
    let (id, handle, already) = match response {
        AddTorrentResponse::Added(id, handle) => (id, handle, false),
        AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle, true),
        AddTorrentResponse::ListOnly(_) => return Err("engine returned no torrent".into()),
    };

    let info_hash = handle.info_hash().as_string();
    if let Some(bytes) = &staged.bytes {
        let path = state.paths.source_file(&info_hash);
        let _ = tokio::fs::write(path, bytes).await;
    }
    // A torrent that was here already keeps the folder it has.
    if let Some(destination) = pending_final.filter(|_| !already) {
        let file = state.paths.pending_moves_file();
        let mut moves = settings::load_pending_moves(&file);
        moves.insert(info_hash, destination);
        settings::save_pending_moves(&file, &moves);
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
/// whole lot can be moved in one go when it finishes.
fn staging_folder(base: &str, staged: &Staged) -> String {
    let mut path = PathBuf::from(base.trim());
    path.push(safe_name(staged).unwrap_or_else(|| "torrent".into()));
    path.to_string_lossy().into_owned()
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
    let response = state.http.get(&url).send().await.map_err(err)?;
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
        .session
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
    let added = merged.len().saturating_sub(existing.len());

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
        .session
        .delete(id.into(), false)
        .await
        .map_err(|e| format!("could not detach the torrent: {e}"))?;

    let add = match &bytes {
        Some(b) => AddTorrent::from_bytes(b.clone()),
        None => AddTorrent::from_url(magnet_for(&info_hash, name.as_deref(), &merged)),
    };
    let opts = AddTorrentOptions {
        paused,
        overwrite: true,
        output_folder: Some(output_folder.clone()),
        only_files: only_files.clone(),
        trackers: Some(merged.clone()),
        ..Default::default()
    };

    let response = match state.session.add_torrent(add, Some(opts)).await {
        Ok(response) => response,
        Err(e) => {
            // Re-attaching failed, so put the torrent back as it was rather
            // than leaving it detached from the session.
            let rollback = match &bytes {
                Some(b) => AddTorrent::from_bytes(b.clone()),
                None => AddTorrent::from_url(magnet_for(&info_hash, name.as_deref(), &existing)),
            };
            let restored = state
                .session
                .add_torrent(
                    rollback,
                    Some(AddTorrentOptions {
                        paused,
                        overwrite: true,
                        output_folder: Some(output_folder.clone()),
                        only_files: only_files.clone(),
                        trackers: Some(existing.clone()),
                        ..Default::default()
                    }),
                )
                .await
                .is_ok();
            return Err(if restored {
                format!("Could not add those trackers, so the torrent was left as it was: {e}")
            } else {
                format!("Could not add those trackers, and the torrent could not be restored: {e}")
            });
        }
    };
    let new_id = match response {
        AddTorrentResponse::Added(id, _) | AddTorrentResponse::AlreadyManaged(id, _) => id,
        AddTorrentResponse::ListOnly(_) => return Err("engine returned no torrent".into()),
    };

    Ok(ApplyTrackersResult {
        id: new_id,
        tracker_count: merged.len(),
        added,
    })
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

/// Bulk-applies the current tracker selection to every torrent in the session.
#[tauri::command]
pub async fn apply_trackers_to_all(
    state: tauri::State<'_, AppState>,
    trackers: Vec<String>,
) -> CmdResult<usize> {
    let ids: Vec<usize> = engine::collect_rows(&state.session)
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
) -> CmdResult<usize> {
    apply_to_each(&state, ids, trackers).await
}

async fn apply_to_each(
    state: &AppState,
    ids: Vec<usize>,
    trackers: Vec<String>,
) -> CmdResult<usize> {
    let mut changed = 0usize;
    let mut last_error = None;
    for id in ids {
        match apply_trackers_inner(state, id, trackers.clone(), false).await {
            Ok(_) => changed += 1,
            Err(e) => last_error = Some(e),
        }
    }
    match last_error {
        Some(e) if changed == 0 => Err(e),
        _ => Ok(changed),
    }
}

/// The magnet link for a torrent, including every tracker it currently has.
#[tauri::command]
pub fn magnet_link(state: tauri::State<'_, AppState>, id: usize) -> CmdResult<String> {
    let handle = state
        .session
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
        .session
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
        .session
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
    let add = match &bytes {
        Some(b) => AddTorrent::from_bytes(b.clone()),
        None => AddTorrent::from_url(magnet_for(
            &token.info_hash,
            token.name.as_deref(),
            &token.trackers,
        )),
    };
    let response = state
        .session
        .add_torrent(
            add,
            Some(AddTorrentOptions {
                paused: token.paused,
                overwrite: true,
                output_folder: Some(token.output_folder.clone()),
                only_files: token.only_files.clone(),
                trackers: Some(token.trackers.clone()),
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("could not put the torrent back: {e}"))?;
    match response {
        AddTorrentResponse::Added(id, _) | AddTorrentResponse::AlreadyManaged(id, _) => Ok(id),
        AddTorrentResponse::ListOnly(_) => Err("engine returned no torrent".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        std::fs::create_dir_all(paths.data_dir.join("sources")).unwrap();

        let app = tauri::test::mock_app();
        app.manage(AppState {
            api: Api::new(session.clone(), None),
            session,
            settings: RwLock::new(settings),
            paths,
            staged: Mutex::new(HashMap::new()),
            resolving: Mutex::new(HashMap::new()),
            seeding: Mutex::new(SeedLedger::default()),
            feeds: Mutex::new(FeedsState::default()),
            stream: None,
            bound_interface: None,
            blocklist_active: false,
            startup_warnings: vec![],
            http: reqwest::Client::new(),
        });
        app
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
            assert_eq!(engine::collect_rows(&state.session).len(), 1);
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
        let has_vlc = |p: &str| p == "vlc";
        assert_eq!(player_command("", has_vlc), Some(vec!["vlc".to_string()]));
        assert_eq!(
            player_command("  mpv --fs ", has_vlc),
            Some(vec!["mpv".to_string(), "--fs".to_string()])
        );
        assert_eq!(player_command("", |_| false), None);
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
                .session
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

            let server = crate::stream::start(state.api.clone()).await.unwrap();
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
                .session
                .add_torrent(AddTorrent::from_url(magnet), Some(AddTorrentOptions { overwrite: true, ..Default::default() }))
                .await
                .unwrap();
            let AddTorrentResponse::Added(id, handle) = added else { panic!("not added") };
            handle.wait_until_initialized().await.unwrap();

            let details = state.api.api_torrent_details(id.into()).unwrap();
            let files = details.files.unwrap();
            let (index, video) = files.iter().enumerate().max_by_key(|(_, f)| f.length).unwrap();
            assert!(handle.stats().file_progress[index] < video.length, "already downloaded");

            let server = crate::stream::start(state.api.clone()).await.unwrap();
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
            let _ = state.session.delete(id.into(), true).await;
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
            assert_eq!(engine::collect_rows(&state.session).len(), 1);
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

            assert_eq!(engine::collect_rows(&state.session).len(), 1);
            assert!(added.try_recv().is_ok());
            assert!(added.try_recv().is_err(), "added twice");
            assert!(failed.try_recv().unwrap().contains("broken.torrent"));
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
            queued: None,
        };
        let single = Staged { files: vec![], ..multi.clone() };

        // Session default: let the engine choose, as it does for a plain add.
        assert_eq!(resolve_output_folder("/dl", "/dl", &multi), None);
        // Custom destination, many files: recreate the sub-folder.
        assert_eq!(
            resolve_output_folder("/other", "/dl", &multi),
            Some("/other/Some Release".to_string())
        );
        // Custom destination, single file: no sub-folder.
        assert_eq!(
            resolve_output_folder("/other", "/dl", &single),
            Some("/other".to_string())
        );
    }
}
