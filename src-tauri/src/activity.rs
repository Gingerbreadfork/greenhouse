//! A record of what Greenhouse did, so things that happen on their own can be
//! looked up afterwards.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, Runtime};

use crate::commands::AppState;

const KEPT: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// Seconds since 1970.
    pub at: u64,
    /// added, failed, finished, stopped or moved.
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
}

#[derive(Default)]
pub struct ActivityLog {
    entries: VecDeque<Entry>,
    file: PathBuf,
}

impl ActivityLog {
    pub fn load(file: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { entries, file }
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string(&self.entries) {
            let _ = std::fs::write(&self.file, text);
        }
    }

    pub fn push(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        while self.entries.len() > KEPT {
            self.entries.pop_front();
        }
        self.save();
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Entry> {
        self.entries.iter().rev().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.save();
    }
}

/// Writes an entry and tells the window about it.
pub fn record<R: Runtime>(
    app: &tauri::AppHandle<R>,
    kind: &str,
    title: impl Into<String>,
    detail: Option<String>,
) {
    let entry = Entry {
        at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        kind: kind.to_string(),
        title: title.into(),
        detail,
    };
    app.state::<AppState>().activity.lock().push(entry.clone());
    let _ = app.emit("greenhouse://activity", entry);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u64) -> Entry {
        Entry {
            at: n,
            kind: "added".into(),
            title: format!("torrent {n}"),
            detail: None,
        }
    }

    #[test]
    fn keeps_the_newest_entries_across_restarts() {
        let file = std::env::temp_dir().join(format!("gh-activity-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut log = ActivityLog::load(file.clone());
        for n in 0..(KEPT as u64 + 20) {
            log.push(entry(n));
        }

        let reloaded = ActivityLog::load(file.clone());
        let list = reloaded.list();
        assert_eq!(list.len(), KEPT);
        assert_eq!(list[0].at, KEPT as u64 + 19);
        assert_eq!(list.last().unwrap().at, 20);

        let mut reloaded = reloaded;
        reloaded.clear();
        assert!(ActivityLog::load(file.clone()).list().is_empty());
        let _ = std::fs::remove_file(file);
    }
}
