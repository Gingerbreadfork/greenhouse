use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{Context, Result};
use librqbit::{
    Api, ListenerMode, ListenerOptions, Session, SessionOptions, SessionPersistenceConfig,
    TorrentStats, TorrentStatsState,
};
use serde::Serialize;

use crate::settings::{Paths, Settings};

/// Which transports the session speaks.
fn listener_mode(transport: &str) -> ListenerMode {
    match transport {
        "utp" => ListenerMode::UtpOnly,
        "both" => ListenerMode::TcpAndUtp,
        _ => ListenerMode::TcpOnly,
    }
}

#[derive(Serialize)]
pub struct NetInterface {
    pub name: String,
    pub up: bool,
}

/// The machine's network interfaces, apart from loopback.
#[cfg(target_os = "linux")]
pub fn list_interfaces() -> Vec<NetInterface> {
    let mut list: Vec<NetInterface> = std::fs::read_dir("/sys/class/net")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "lo")
        .map(|name| {
            let state = std::fs::read_to_string(format!("/sys/class/net/{name}/operstate"));
            let up = !matches!(state.as_deref().map(str::trim), Ok("down") | Err(_));
            NetInterface { name, up }
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Only Linux can tie sockets to a device, so there is nothing to pick elsewhere.
#[cfg(not(target_os = "linux"))]
pub fn list_interfaces() -> Vec<NetInterface> {
    Vec::new()
}

#[cfg(target_os = "linux")]
fn interface_exists(name: &str) -> bool {
    std::path::Path::new("/sys/class/net").join(name).exists()
}

/// The device to bind to on this system, and a warning when the setting
/// cannot be honoured.
fn bind_device(wanted: &str) -> (Option<String>, Option<String>) {
    #[cfg(target_os = "linux")]
    {
        choose_bind_device(wanted, interface_exists)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let wanted = wanted.trim();
        if wanted.is_empty() {
            return (None, None);
        }
        (
            None,
            Some(format!(
                "Tying traffic to the network interface {wanted} only works on Linux, \
                 so that setting was ignored."
            )),
        )
    }
}

/// The device to bind to, and a warning when the wanted one is missing. A
/// missing interface falls back to loopback so nothing leaves the machine.
#[cfg(any(target_os = "linux", test))]
fn choose_bind_device(
    wanted: &str,
    exists: impl Fn(&str) -> bool,
) -> (Option<String>, Option<String>) {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return (None, None);
    }
    if exists(wanted) {
        return (Some(wanted.to_string()), None);
    }
    (
        Some("lo".into()),
        Some(format!(
            "The network interface {wanted} is not available, so torrent traffic is blocked. \
             Bring it up and restart Greenhouse."
        )),
    )
}

pub struct Engine {
    pub session: Arc<Session>,
    /// The interface traffic is tied to for this run, if any.
    pub bound_interface: Option<String>,
    pub blocklist_active: bool,
    pub warnings: Vec<String>,
}

/// The engine in use right now, with what it was started with.
pub struct Running {
    pub session: Arc<Session>,
    pub api: Api,
    pub bound_interface: Option<String>,
    pub blocklist_active: bool,
    /// The settings this engine was built from, to fall back on.
    pub applied: Settings,
}

impl Running {
    pub fn new(engine: Engine, applied: Settings) -> Self {
        Self {
            api: Api::new(engine.session.clone(), None),
            session: engine.session,
            bound_interface: engine.bound_interface,
            blocklist_active: engine.blocklist_active,
            applied,
        }
    }
}

/// Holds the running engine so it can be replaced without restarting the app.
pub struct EngineSlot(parking_lot::RwLock<Arc<Running>>);

impl EngineSlot {
    pub fn new(running: Running) -> Self {
        Self(parking_lot::RwLock::new(Arc::new(running)))
    }

    pub fn get(&self) -> Arc<Running> {
        self.0.read().clone()
    }

    pub fn set(&self, running: Running) {
        *self.0.write() = Arc::new(running);
    }
}

fn session_options(
    paths: &Paths,
    settings: &Settings,
    bind_device_name: Option<String>,
    blocklist_url: Option<String>,
) -> SessionOptions {
    let listen_addr: std::net::SocketAddr =
        (std::net::Ipv6Addr::UNSPECIFIED, settings.listen_port).into();
    SessionOptions {
        persistence: Some(SessionPersistenceConfig::Json {
            folder: Some(paths.session_dir()),
        }),
        fastresume: true,
        dht: Some(librqbit::DhtSessionConfig {
            persistence: Some(librqbit::dht::DhtPersistenceConfig {
                config_filename: paths.dht_file.clone(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        bind_device_name,
        blocklist_url,
        peer_limit: (settings.peer_limit_per_torrent > 0)
            .then_some(settings.peer_limit_per_torrent as usize),
        listen: Some(ListenerOptions {
            mode: listener_mode(&settings.transport),
            listen_addr,
            enable_upnp_port_forwarding: settings.enable_upnp,
            ..Default::default()
        }),
        ratelimits: librqbit::limits::LimitsConfig {
            upload_bps: kbps_to_bps(settings.upload_limit_kbps),
            download_bps: kbps_to_bps(settings.download_limit_kbps),
        },
        client_name_and_version: Some(format!("Greenhouse {}", env!("CARGO_PKG_VERSION"))),
        ..Default::default()
    }
}

/// The downloaded blocklist as a URL the engine can read, when there is one.
fn blocklist_source(paths: &Paths, settings: &Settings) -> Option<String> {
    let file = paths.blocklist_file();
    if settings.blocklist_url.trim().is_empty() || !file.exists() {
        return None;
    }
    url::Url::from_file_path(file).ok().map(String::from)
}

/// Starts the engine, without the blocklist if it turns out to be unreadable,
/// so a bad list cannot keep the app from launching.
async fn start_session(
    folder: std::path::PathBuf,
    blocklist: Option<String>,
    options: impl Fn(Option<String>) -> SessionOptions,
) -> Result<(Arc<Session>, bool, Option<String>)> {
    let wanted = blocklist.is_some();
    match Session::new_with_opts(folder.clone(), options(blocklist)).await {
        Ok(session) => Ok((session, wanted, None)),
        Err(e) if wanted => {
            let warning =
                format!("The peer blocklist could not be read, so it is off for now: {e:#}");
            let session = Session::new_with_opts(folder, options(None)).await?;
            Ok((session, false, Some(warning)))
        }
        Err(e) => Err(e),
    }
}

pub async fn build_session(paths: &Paths, settings: &Settings) -> Result<Engine> {
    let (bind_device_name, bind_warning) = bind_device(&settings.bind_interface);
    let (session, blocklist_active, blocklist_warning) = start_session(
        settings.download_dir.clone().into(),
        blocklist_source(paths, settings),
        |blocklist| session_options(paths, settings, bind_device_name.clone(), blocklist),
    )
    .await
    .context("could not start the torrent session")?;
    Ok(Engine {
        session,
        bound_interface: bind_device_name,
        blocklist_active,
        warnings: bind_warning.into_iter().chain(blocklist_warning).collect(),
    })
}

pub fn kbps_to_bps(kbps: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(kbps.saturating_mul(1024))
}

#[derive(Serialize, Clone)]
pub struct TorrentRow {
    pub id: usize,
    pub info_hash: String,
    pub name: String,
    pub state: String,
    pub error: Option<String>,
    pub progress: f64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub eta_seconds: Option<u64>,
    pub peers_live: u32,
    pub peers_connecting: u32,
    pub peers_queued: u32,
    pub peers_seen: u32,
    pub finished: bool,
    pub output_folder: String,
    pub tracker_count: usize,
    pub ratio: f64,
    pub has_metadata: bool,
}

/// Collapses the engine's state machine into the five states the UI shows.
fn ui_state(stats: &TorrentStats) -> &'static str {
    if stats.error.is_some() {
        return "error";
    }
    match stats.state {
        TorrentStatsState::Error => "error",
        TorrentStatsState::Initializing { .. } => "checking",
        TorrentStatsState::Paused => {
            if stats.finished {
                "complete"
            } else {
                "paused"
            }
        }
        TorrentStatsState::Live => {
            if stats.finished {
                "seeding"
            } else {
                "downloading"
            }
        }
    }
}

pub fn collect_rows(session: &Arc<Session>) -> Vec<TorrentRow> {
    session.with_torrents(|torrents| {
        torrents
            .map(|(id, handle)| {
                let stats = handle.stats();
                let live = stats.live.as_ref();
                let download_speed =
                    live.map(|l| l.download_speed.as_bytes()).unwrap_or_default();
                let upload_speed = live.map(|l| l.upload_speed.as_bytes()).unwrap_or_default();
                let peers = live.map(|l| &l.snapshot.peer_stats);
                let remaining = stats.total_bytes.saturating_sub(stats.progress_bytes);
                let eta_seconds = if stats.finished || download_speed == 0 {
                    None
                } else {
                    Some(remaining / download_speed.max(1))
                };
                let progress = if stats.total_bytes == 0 {
                    0.0
                } else {
                    (stats.progress_bytes as f64 / stats.total_bytes as f64).clamp(0.0, 1.0)
                };
                let ratio = if stats.progress_bytes == 0 {
                    0.0
                } else {
                    stats.uploaded_bytes as f64 / stats.progress_bytes as f64
                };
                let has_metadata = handle.metadata.load().is_some();

                TorrentRow {
                    id,
                    info_hash: handle.info_hash().as_string(),
                    name: handle
                        .name()
                        .unwrap_or_else(|| handle.info_hash().as_string()),
                    state: ui_state(&stats).to_string(),
                    error: stats.error.clone(),
                    progress,
                    progress_bytes: stats.progress_bytes,
                    total_bytes: stats.total_bytes,
                    uploaded_bytes: stats.uploaded_bytes,
                    download_speed,
                    upload_speed,
                    eta_seconds,
                    peers_live: peers.map(|p| p.live).unwrap_or_default(),
                    peers_connecting: peers.map(|p| p.connecting).unwrap_or_default(),
                    peers_queued: peers.map(|p| p.queued).unwrap_or_default(),
                    peers_seen: peers.map(|p| p.seen).unwrap_or_default(),
                    finished: stats.finished,
                    output_folder: handle.output_folder().to_string_lossy().into_owned(),
                    tracker_count: handle.shared().trackers.len(),
                    ratio,
                    has_metadata,
                }
            })
            .collect()
    })
}

#[derive(Serialize, Clone)]
pub struct SessionSummary {
    pub download_speed: u64,
    pub upload_speed: u64,
    pub peers_live: u32,
    pub peers_connecting: u32,
    pub peers_seen: u32,
    pub uptime_seconds: u64,
    pub dht_nodes: Option<usize>,
    pub listen_port: Option<u16>,
    pub download_limit_kbps: u32,
    pub upload_limit_kbps: u32,
    pub fetched_bytes: u64,
    pub uploaded_bytes: u64,
    pub blocked_incoming: u64,
    pub blocked_outgoing: u64,
    pub connect_tcp: u64,
    pub connect_utp: u64,
    pub connect_errors: u64,
}

pub fn session_summary(session: &Arc<Session>, settings: &Settings) -> SessionSummary {
    let stats = session.stats_snapshot();
    let tcp = &stats.connections.tcp;
    let utp = &stats.connections.utp;
    SessionSummary {
        download_speed: stats.download_speed.as_bytes(),
        upload_speed: stats.upload_speed.as_bytes(),
        peers_live: stats.peers.live,
        peers_connecting: stats.peers.connecting,
        peers_seen: stats.peers.seen,
        uptime_seconds: stats.uptime_seconds,
        fetched_bytes: stats.counters.fetched_bytes,
        uploaded_bytes: stats.counters.uploaded_bytes,
        blocked_incoming: stats.counters.blocked_incoming,
        blocked_outgoing: stats.counters.blocked_outgoing,
        connect_tcp: tcp.v4.successes + tcp.v6.successes,
        connect_utp: utp.v4.successes + utp.v6.successes,
        connect_errors: tcp.v4.errors + tcp.v6.errors + utp.v4.errors + utp.v6.errors,
        dht_nodes: session.get_dht().and_then(|d| {
            serde_json::to_value(d.stats())
                .ok()
                .and_then(|v| v.get("size").and_then(|s| s.as_u64()))
                .map(|n| n as usize)
        }),
        listen_port: session.announce_port(),
        download_limit_kbps: settings.download_limit_kbps,
        upload_limit_kbps: settings.upload_limit_kbps,
    }
}

#[derive(Serialize)]
pub struct TorrentFileEntry {
    pub index: usize,
    pub name: String,
    pub components: Vec<String>,
    pub length: u64,
    pub included: bool,
    pub progress_bytes: u64,
}

#[derive(Serialize)]
pub struct TorrentDetail {
    pub id: usize,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
    pub total_pieces: u32,
    pub piece_size: u64,
    pub files: Vec<TorrentFileEntry>,
    pub trackers: Vec<String>,
    pub peers: serde_json::Value,
    pub added_source: Option<String>,
}

pub fn torrent_detail(api: &Api, paths: &Paths, id: usize) -> Result<TorrentDetail> {
    let handle = api
        .mgr_handle(id.into())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let details = api
        .api_torrent_details(id.into())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let stats = handle.stats();

    let files = details
        .files
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, f)| TorrentFileEntry {
            index,
            name: f.name,
            components: f.components,
            length: f.length,
            included: f.included,
            progress_bytes: stats.file_progress.get(index).copied().unwrap_or(0),
        })
        .collect();

    let mut trackers: Vec<String> = handle
        .shared()
        .trackers
        .iter()
        .map(|u| u.to_string())
        .collect();
    trackers.sort();

    let piece_size = if details.total_pieces > 0 {
        stats.total_bytes / details.total_pieces as u64
    } else {
        0
    };

    let peers = peer_stats_value(api, id);

    let source_path = paths.source_file(&details.info_hash);
    let added_source = if source_path.exists() {
        Some(source_path.to_string_lossy().into_owned())
    } else {
        None
    };

    Ok(TorrentDetail {
        id,
        info_hash: details.info_hash,
        name: details
            .name
            .unwrap_or_else(|| handle.info_hash().as_string()),
        output_folder: details.output_folder,
        total_pieces: details.total_pieces,
        piece_size,
        files,
        trackers,
        peers,
        added_source,
    })
}

/// Per-peer stats are only available while a torrent is live; an empty object
/// keeps the UI code free of special cases.
fn peer_stats_value(api: &Api, id: usize) -> serde_json::Value {
    let filter = match serde_json::from_value(serde_json::json!({ "state": "live" })) {
        Ok(f) => f,
        Err(_) => return serde_json::json!({}),
    };
    match api.api_peer_stats(id.into(), filter) {
        Ok(stats) => serde_json::to_value(stats).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_interface_falls_back_to_loopback() {
        assert_eq!(choose_bind_device("", |_| false), (None, None));
        assert_eq!(choose_bind_device(" wg0 ", |_| true), (Some("wg0".into()), None));

        let (device, warning) = choose_bind_device("wg0", |_| false);
        assert_eq!(device.as_deref(), Some("lo"));
        assert!(warning.unwrap().contains("wg0"));
    }

    fn offline(blocklist_url: Option<String>) -> SessionOptions {
        SessionOptions {
            dht: None,
            persistence: None,
            listen: None,
            disable_local_service_discovery: true,
            blocklist_url,
            ..Default::default()
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gh-engine-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_downloaded_blocklist_is_loaded() {
        let dir = scratch("good");
        let file = dir.join("blocklist");
        std::fs::write(&file, "Some range:1.2.3.0-1.2.3.255\n").unwrap();
        let source = url::Url::from_file_path(&file).unwrap().to_string();

        let (_session, active, warning) =
            start_session(dir.join("dl"), Some(source), offline).await.unwrap();
        assert!(active);
        assert_eq!(warning, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unreadable_blocklist_does_not_stop_the_engine() {
        let dir = scratch("bad");
        let file = dir.join("blocklist");
        std::fs::write(&file, [0xff, 0xfe, 0xfd, b'\n', 0xff]).unwrap();
        let source = url::Url::from_file_path(&file).unwrap().to_string();

        let (_session, active, warning) =
            start_session(dir.join("dl"), Some(source), offline).await.unwrap();
        assert!(!active);
        assert!(warning.unwrap().contains("blocklist"));
    }

    #[test]
    fn the_blocklist_needs_both_a_url_and_a_downloaded_copy() {
        let dir = scratch("source");
        let paths = Paths { config_dir: dir.clone(), data_dir: dir.clone(), dht_file: None };
        let mut settings = Settings::default();
        settings.blocklist_url = "https://example.org/list.gz".into();
        assert_eq!(blocklist_source(&paths, &settings), None);

        std::fs::write(paths.blocklist_file(), "x").unwrap();
        assert!(blocklist_source(&paths, &settings).unwrap().starts_with("file://"));

        settings.blocklist_url.clear();
        assert_eq!(blocklist_source(&paths, &settings), None);
    }

    #[test]
    fn loopback_is_not_offered_as_a_choice() {
        assert!(list_interfaces().iter().all(|i| i.name != "lo"));
    }
}
