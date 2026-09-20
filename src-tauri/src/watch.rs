//! Adds .torrent files that appear in the watch folder.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::Bytes;
use tauri::{Emitter, Manager, Runtime};

use crate::activity;
use crate::commands::{add_file_with_defaults, record_add, AddFailed, AppState};

const EVERY: Duration = Duration::from_secs(5);
/// A file this fresh may still be being written.
const SETTLE: Duration = Duration::from_secs(2);

pub fn spawn<R: Runtime>(app: tauri::AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(EVERY);
        loop {
            interval.tick().await;
            scan(&app, SETTLE).await;
        }
    });
}

/// Adds every settled .torrent file in the watch folder, then renames it so
/// it is not picked up again.
pub async fn scan<R: Runtime>(app: &tauri::AppHandle<R>, settle: Duration) {
    let state = app.state::<AppState>();
    let dir = state.settings.read().watch_dir.trim().to_string();
    if dir.is_empty() {
        return;
    }
    for file in torrent_files(Path::new(&dir), settle) {
        let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let added = match tokio::fs::read(&file).await {
            Ok(bytes) => {
                let source = file.to_string_lossy().into_owned();
                add_file_with_defaults(&state, source, Bytes::from(bytes)).await
            }
            Err(e) => Err(e.to_string()),
        };
        let suffix = match added {
            Ok(result) => {
                record_add(app, &result, "Added from the watch folder".into());
                let _ = app.emit("greenhouse://added", result);
                "added"
            }
            Err(error) => {
                activity::record(app, "failed", name.clone(), Some(error.clone()));
                let _ = app.emit("greenhouse://add-failed", AddFailed { name: Some(name), error });
                "failed"
            }
        };
        let _ = tokio::fs::rename(&file, marked(&file, suffix)).await;
    }
}

fn marked(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    name.into()
}

fn torrent_files(dir: &Path, settle: Duration) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let is_torrent = entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("torrent"));
            let settled = entry
                .metadata()
                .ok()
                .filter(|m| m.is_file() && m.len() > 0)
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age >= settle);
            is_torrent && settled
        })
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}
