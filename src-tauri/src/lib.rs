mod activation;
mod commands;
mod engine;
mod supervisor;
mod settings;
mod torrentsrc;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use librqbit::Api;
use parking_lot::{Mutex, RwLock};
use tauri::{Emitter, Manager};

use commands::AppState;
use settings::Paths;

#[derive(serde::Serialize, Clone)]
struct Tick {
    torrents: Vec<engine::TorrentRow>,
    pending: Vec<commands::PendingAdd>,
    session: engine::SessionSummary,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    activation::stash_launch_token();

    tauri::Builder::default()
        // Registered first so a second launch hands its arguments to the
        // running window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let args = launch_arguments(argv.into_iter().skip(1));
            if !args.is_empty() {
                let _ = app.emit("greenhouse://open", args);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                activation::present(&window, activation::take_launch_token());
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Only a first launch gets here, and GTK has already used its token.
            activation::take_launch_token();

            let paths = Paths::resolve();
            let loaded = settings::load(&paths.settings_file());
            // Write the defaults out on first run so the file is there to edit.
            if !paths.settings_file().exists() {
                let _ = settings::save(&paths.settings_file(), &loaded);
            }

            let engine = tauri::async_runtime::block_on(engine::build_session(&paths, &loaded))?;
            let session = engine.session;
            let api = Api::new(session.clone(), None);

            app.manage(AppState {
                session: session.clone(),
                api,
                settings: RwLock::new(loaded),
                paths,
                staged: Mutex::new(HashMap::new()),
                resolving: Mutex::new(HashMap::new()),
                bound_interface: engine.bound_interface,
                blocklist_active: engine.blocklist_active,
                startup_warnings: engine.warnings,
                http: reqwest::Client::builder()
                    .user_agent(concat!("Greenhouse/", env!("CARGO_PKG_VERSION")))
                    .timeout(Duration::from_secs(20))
                    .build()
                    .unwrap_or_default(),
            });

            spawn_ticker(app.handle().clone(), session);
            refresh_stale_blocklist(app.handle().clone());
            forward_launch_arguments(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_interfaces,
            commands::blocklist_status,
            commands::refresh_blocklist,
            commands::save_settings,
            commands::list_torrents,
            commands::session_stats,
            commands::torrent_detail,
            commands::torrent_action,
            commands::set_file_selection,
            commands::stage_source,
            commands::resolve_staged,
            commands::discard_staged,
            commands::commit_staged,
            commands::parse_trackers,
            commands::test_notification,
            commands::fetch_tracker_list,
            commands::apply_trackers,
            commands::apply_trackers_to_all,
            commands::apply_trackers_many,
            commands::magnet_link,
            commands::forget_torrent,
            commands::restore_torrent,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Greenhouse");
}

/// Pushes a full snapshot to the window on a fixed cadence, so the UI never
/// has to poll and every row animates from the same clock.
fn spawn_ticker(app: tauri::AppHandle, session: Arc<librqbit::Session>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(900));
        let mut supervisor = supervisor::Supervisor::default();
        loop {
            interval.tick().await;
            let state = match app.try_state::<AppState>() {
                Some(s) => s,
                None => continue,
            };
            let torrents = engine::collect_rows(&session);
            supervisor.tick(&app, &state, &torrents).await;

            let session_summary = engine::session_summary(&session, &state.settings.read());
            let _ = app.emit(
                "greenhouse://tick",
                Tick {
                    torrents: engine::collect_rows(&session),
                    pending: commands::pending_adds(&state),
                    session: session_summary,
                },
            );
        }
    });
}

/// Keeps the downloaded blocklist no more than a day old.
fn refresh_stale_blocklist(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let age = std::fs::metadata(state.paths.blocklist_file())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok());
        if age.is_none_or(|a| a > Duration::from_secs(24 * 60 * 60)) {
            let _ = commands::download_blocklist(&state).await;
        }
    });
}

fn launch_arguments(args: impl Iterator<Item = String>) -> Vec<String> {
    args.filter(|a| !a.starts_with('-') && !a.is_empty())
        .collect()
}

/// Magnet links and .torrent paths passed on the command line, e.g. when the
/// app is registered as the system handler.
fn forward_launch_arguments(app: tauri::AppHandle) {
    let args = launch_arguments(std::env::args().skip(1));
    if args.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        let _ = app.emit("greenhouse://open", args);
    });
}
