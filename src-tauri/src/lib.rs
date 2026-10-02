mod activation;
mod activity;
mod background;
mod commands;
mod engine;
mod feeds;
mod seeding;
mod supervisor;
mod settings;
mod stream;
mod torrentsrc;
mod watch;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

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
            background::show(app, activation::take_launch_token());
        }))
        // Whether the window shows is ours to decide, not something to restore.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && background::keeps_running(window.app_handle()) {
                    api.prevent_close();
                    background::hide(window);
                }
            }
        })
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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

            let mut engine =
                tauri::async_runtime::block_on(engine::build_session(&paths, &loaded))?;
            let startup_warnings = std::mem::take(&mut engine.warnings);
            let engine = Arc::new(engine::EngineSlot::new(engine::Running::new(
                engine,
                loaded.clone(),
            )));
            let seeding = seeding::SeedLedger::load(&paths.seeding_file());
            let feeds = feeds::FeedsState::load(&paths.feeds_file());
            let activity = activity::ActivityLog::load(paths.activity_file());
            let stream = tauri::async_runtime::block_on(stream::start(engine.clone())).ok();

            app.manage(AppState {
                engine,
                restarting: tokio::sync::Mutex::new(()),
                settings: RwLock::new(loaded),
                paths,
                staged: Mutex::new(HashMap::new()),
                resolving: Mutex::new(HashMap::new()),
                seeding: Mutex::new(seeding),
                feeds: Mutex::new(feeds),
                activity: Mutex::new(activity),
                stream,
                startup_warnings,
                http: reqwest::Client::builder()
                    .user_agent(concat!("Greenhouse/", env!("CARGO_PKG_VERSION")))
                    .timeout(Duration::from_secs(20))
                    .build()
                    .unwrap_or_default(),
            });

            background::sync_tray(app.handle(), background::keeps_running(app.handle()));
            if !background::launched_hidden() {
                background::show(app.handle(), None);
            }
            commands::restore_queue(app.handle());
            spawn_ticker(app.handle().clone());
            refresh_stale_blocklist(app.handle().clone());
            watch::spawn(app.handle().clone());
            feeds::spawn(app.handle().clone());
            forward_launch_arguments(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::quit,
            commands::list_interfaces,
            commands::free_space,
            commands::restart_engine,
            commands::blocklist_status,
            commands::refresh_blocklist,
            commands::save_settings,
            commands::list_torrents,
            commands::session_stats,
            commands::torrent_detail,
            commands::torrent_action,
            commands::set_file_selection,
            commands::play_file,
            commands::activity_list,
            commands::clear_activity,
            commands::feeds_status,
            commands::check_feed,
            commands::add_feed_item,
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
        .build(tauri::generate_context!())
        .expect("error while starting Greenhouse")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.seeding.lock().save_if_dirty(&state.paths.seeding_file());
                }
            }
        });
}

/// Pushes a full snapshot to the window on a fixed cadence, so the UI never
/// has to poll and every row animates from the same clock.
fn spawn_ticker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(900));
        let mut supervisor = supervisor::Supervisor::default();
        loop {
            interval.tick().await;
            let state = match app.try_state::<AppState>() {
                Some(s) => s,
                None => continue,
            };
            let session = state.session();
            let torrents = engine::collect_rows(&session);
            supervisor.tick(&app, &state, &torrents).await;

            // A hidden window has nothing to draw.
            let visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(true);
            if !visible {
                continue;
            }
            let session_summary = engine::session_summary(&session, &state.settings.read());
            let _ = app.emit(
                "greenhouse://tick",
                Tick {
                    torrents: commands::rows_with_totals(&state),
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
