//! Running totals of what each torrent has uploaded and how long it has
//! seeded. The engine's own counters start again on every pause and launch.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::TorrentRow;
use crate::settings::Settings;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SeedRecord {
    pub uploaded: u64,
    pub seeded_ms: u64,
    /// Seeding limits have already stopped this torrent once, so resuming it
    /// by hand is left alone.
    pub done: bool,
}

#[derive(Default)]
pub struct SeedLedger {
    records: HashMap<String, SeedRecord>,
    /// The engine's upload counter for each torrent at the last reading.
    last_seen: HashMap<String, u64>,
    dirty: bool,
}

impl SeedLedger {
    pub fn load(file: &Path) -> Self {
        let records = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            records,
            ..Default::default()
        }
    }

    pub fn save_if_dirty(&mut self, file: &Path) {
        if !self.dirty {
            return;
        }
        if let Ok(text) = serde_json::to_string(&self.records) {
            if std::fs::write(file, text).is_ok() {
                self.dirty = false;
            }
        }
    }

    /// Folds one reading of the engine's counters into the totals.
    pub fn record(&mut self, row: &TorrentRow, elapsed_ms: u64) {
        let last = self
            .last_seen
            .insert(row.info_hash.clone(), row.uploaded_bytes)
            .unwrap_or(0);
        // A counter below the last reading means the engine started it again.
        let gained = if row.uploaded_bytes >= last {
            row.uploaded_bytes - last
        } else {
            row.uploaded_bytes
        };
        let seeding = row.state == "seeding";
        if gained == 0 && !seeding {
            return;
        }
        let record = self.records.entry(row.info_hash.clone()).or_default();
        record.uploaded += gained;
        if seeding {
            record.seeded_ms += elapsed_ms;
        }
        self.dirty = true;
    }

    /// Replaces the engine's per-run upload figures with the running totals.
    pub fn overlay(&self, rows: &mut [TorrentRow]) {
        for row in rows {
            if let Some(record) = self.records.get(&row.info_hash) {
                row.uploaded_bytes = row.uploaded_bytes.max(record.uploaded);
                row.ratio = ratio(row.uploaded_bytes, row.progress_bytes);
            }
        }
    }

    /// Whether a seeding torrent has reached one of the limits.
    pub fn is_due(&self, row: &TorrentRow, settings: &Settings) -> bool {
        if row.state != "seeding" {
            return false;
        }
        let Some(record) = self.records.get(&row.info_hash) else {
            return false;
        };
        if record.done {
            return false;
        }
        let ratio_reached = settings.seed_ratio_limit > 0.0
            && ratio(record.uploaded, row.progress_bytes) >= settings.seed_ratio_limit;
        let time_reached = settings.seed_time_limit_minutes > 0
            && record.seeded_ms >= u64::from(settings.seed_time_limit_minutes) * 60_000;
        ratio_reached || time_reached
    }

    pub fn mark_done(&mut self, info_hash: &str) {
        self.records.entry(info_hash.to_string()).or_default().done = true;
        self.dirty = true;
    }

    /// Forgets torrents that are no longer in the session.
    pub fn retain(&mut self, live: &HashSet<&str>) {
        let before = self.records.len();
        self.records.retain(|hash, _| live.contains(hash.as_str()));
        self.dirty |= self.records.len() != before;
    }
}

pub fn ratio(uploaded: u64, downloaded: u64) -> f64 {
    if downloaded == 0 {
        0.0
    } else {
        uploaded as f64 / downloaded as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: &str, uploaded: u64) -> TorrentRow {
        TorrentRow {
            id: 1,
            info_hash: "abc".into(),
            name: "t".into(),
            state: state.into(),
            error: None,
            progress: 1.0,
            progress_bytes: 1000,
            total_bytes: 1000,
            uploaded_bytes: uploaded,
            download_speed: 0,
            upload_speed: 0,
            eta_seconds: None,
            peers_live: 0,
            peers_connecting: 0,
            peers_queued: 0,
            peers_seen: 0,
            finished: true,
            output_folder: String::new(),
            tracker_count: 0,
            ratio: 0.0,
            has_metadata: true,
        }
    }

    fn limits(ratio: f64, minutes: u32) -> Settings {
        Settings {
            seed_ratio_limit: ratio,
            seed_time_limit_minutes: minutes,
            ..Settings::default()
        }
    }

    #[test]
    fn upload_carries_across_engine_counter_resets() {
        let mut ledger = SeedLedger::default();
        ledger.record(&row("seeding", 400), 900);
        ledger.record(&row("seeding", 700), 900);
        // Paused and resumed: the engine counts from zero again.
        ledger.record(&row("seeding", 50), 900);
        assert_eq!(ledger.records["abc"].uploaded, 750);
        assert_eq!(ledger.records["abc"].seeded_ms, 2700);

        let mut rows = vec![row("seeding", 50)];
        ledger.overlay(&mut rows);
        assert_eq!(rows[0].uploaded_bytes, 750);
        assert_eq!(rows[0].ratio, 0.75);
    }

    #[test]
    fn time_only_counts_while_seeding() {
        let mut ledger = SeedLedger::default();
        ledger.record(&row("complete", 0), 900);
        assert!(ledger.records.is_empty());
    }

    #[test]
    fn limits_stop_a_torrent_once() {
        let mut ledger = SeedLedger::default();
        ledger.record(&row("seeding", 1500), 60_000);
        let seeding = row("seeding", 1500);

        assert!(!ledger.is_due(&seeding, &limits(0.0, 0)));
        assert!(!ledger.is_due(&seeding, &limits(2.0, 5)));
        assert!(ledger.is_due(&seeding, &limits(1.5, 0)));
        assert!(ledger.is_due(&seeding, &limits(0.0, 1)));
        assert!(!ledger.is_due(&row("complete", 1500), &limits(1.5, 0)));

        ledger.mark_done("abc");
        assert!(!ledger.is_due(&seeding, &limits(1.5, 1)));
    }

    #[test]
    fn totals_survive_a_restart() {
        let file = std::env::temp_dir().join(format!("gh-seeding-{}.json", std::process::id()));
        let mut ledger = SeedLedger::default();
        ledger.record(&row("seeding", 640), 900);
        ledger.save_if_dirty(&file);

        let reloaded = SeedLedger::load(&file);
        assert_eq!(reloaded.records, ledger.records);
        let _ = std::fs::remove_file(file);
    }
}
