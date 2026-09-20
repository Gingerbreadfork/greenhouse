use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TrackerPack {
    pub id: String,
    pub name: String,
    pub description: String,
    /// When true, this pack is injected into every new torrent and magnet.
    pub enabled: bool,
    pub source_url: Option<String>,
    pub updated_at: Option<String>,
    pub trackers: Vec<String>,
}

impl Default for TrackerPack {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Untitled pack".into(),
            description: String::new(),
            enabled: false,
            source_url: None,
            updated_at: None,
            trackers: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub start_paused: bool,
    /// Add new torrents with the defaults, without showing the add sheet.
    pub auto_add: bool,
    /// Master switch for injecting enabled packs into every new torrent.
    pub auto_apply_trackers: bool,
    pub theme: String,
    pub accent: String,
    pub compact: bool,
    pub reduce_motion: bool,
    pub confirm_remove: bool,
    pub download_limit_kbps: u32,
    pub upload_limit_kbps: u32,

    // Network. These are fixed when the session starts, so changing them
    // takes effect the next time Greenhouse runs.
    pub listen_port: u16,
    pub enable_upnp: bool,
    pub transport: String,
    pub peer_limit_per_torrent: u32,
    /// Network interface all traffic is tied to. Empty means any.
    pub bind_interface: String,
    /// Where the peer blocklist comes from. Empty means no blocklist.
    pub blocklist_url: String,

    // Files
    pub incomplete_dir: String,
    pub skip_types_enabled: bool,
    pub skip_types: Vec<String>,

    // Queue. 0 means no limit.
    pub max_active_downloads: u32,
    pub max_active_seeds: u32,
    /// Stop seeding at this upload ratio. 0 means never.
    pub seed_ratio_limit: f64,
    /// Stop seeding after this long. 0 means never.
    pub seed_time_limit_minutes: u32,

    pub notify_on_done: bool,
    pub packs: Vec<TrackerPack>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: default_download_dir(),
            start_paused: false,
            auto_add: false,
            auto_apply_trackers: true,
            theme: "system".into(),
            accent: "glass".into(),
            compact: false,
            reduce_motion: false,
            confirm_remove: true,
            download_limit_kbps: 0,
            upload_limit_kbps: 0,
            listen_port: 0,
            enable_upnp: true,
            transport: "tcp".into(),
            peer_limit_per_torrent: 0,
            bind_interface: String::new(),
            blocklist_url: String::new(),
            incomplete_dir: String::new(),
            skip_types_enabled: true,
            skip_types: default_skip_types(),
            max_active_downloads: 0,
            max_active_seeds: 0,
            seed_ratio_limit: 0.0,
            seed_time_limit_minutes: 0,
            notify_on_done: true,
            packs: vec![starter_pack()],
        }
    }
}

/// Extensions left unticked when a torrent is added, unless you pick them.
pub fn default_skip_types() -> Vec<String> {
    ["exe", "msi", "bat", "cmd", "com", "scr", "vbs", "ps1"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub fn default_download_dir() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned()
}

/// A small, well-known set of open trackers so the feature is useful on first run.
fn starter_pack() -> TrackerPack {
    TrackerPack {
        id: "starter".into(),
        name: "Open trackers".into(),
        description: "A starter set of public open trackers.".into(),
        enabled: true,
        source_url: Some("https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_best.txt".into()),
        updated_at: None,
        trackers: vec![
            "udp://tracker.opentrackr.org:1337/announce".into(),
            "udp://open.demonii.com:1337/announce".into(),
            "udp://open.stealth.si:80/announce".into(),
            "udp://tracker.torrent.eu.org:451/announce".into(),
            "udp://exodus.desync.com:6969/announce".into(),
            "udp://tracker.openbittorrent.com:6969/announce".into(),
            "http://tracker.openbittorrent.com:80/announce".into(),
            "udp://explodie.org:6969/announce".into(),
            "udp://tracker1.bt.moack.co.kr:80/announce".into(),
            "udp://tracker.tiny-vps.com:6969/announce".into(),
        ],
    }
}

pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("greenhouse");
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("greenhouse");
        let _ = std::fs::create_dir_all(&config_dir);
        let _ = std::fs::create_dir_all(data_dir.join("session"));
        let _ = std::fs::create_dir_all(data_dir.join("sources"));
        Self {
            config_dir,
            data_dir,
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn session_dir(&self) -> PathBuf {
        self.data_dir.join("session")
    }

    /// Where we keep a copy of each torrent's original source, so trackers can
    /// be re-applied later without re-fetching metadata from the swarm.
    pub fn source_file(&self, info_hash: &str) -> PathBuf {
        self.data_dir.join("sources").join(format!("{info_hash}.torrent"))
    }

    /// Torrents downloading into the incomplete folder, and where each one
    /// should end up once it finishes.
    pub fn pending_moves_file(&self) -> PathBuf {
        self.data_dir.join("incomplete.json")
    }

    /// Running upload and seeding-time totals for each torrent.
    pub fn seeding_file(&self) -> PathBuf {
        self.data_dir.join("seeding.json")
    }

    /// The downloaded copy of the peer blocklist the engine reads at launch.
    pub fn blocklist_file(&self) -> PathBuf {
        self.data_dir.join("blocklist")
    }
}

pub type PendingMoves = std::collections::HashMap<String, String>;

pub fn load_pending_moves(path: &Path) -> PendingMoves {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_pending_moves(path: &Path, moves: &PendingMoves) {
    if let Ok(json) = serde_json::to_vec_pretty(moves) {
        let _ = std::fs::write(path, json);
    }
}

/// Moves everything inside `from` into `to`, falling back to copy-and-delete
/// when the two are on different filesystems.
pub fn move_contents(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if std::fs::rename(entry.path(), &target).is_err() {
            copy_recursive(&entry.path(), &target)?;
            if entry.path().is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            } else {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    let _ = std::fs::remove_dir(from);
    Ok(())
}

fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
    }
    Ok(())
}

pub fn load(path: &Path) -> Settings {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let json = serde_json::to_vec_pretty(settings)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl Settings {
    /// Every tracker from every enabled pack, de-duplicated, order preserved.
    pub fn active_trackers(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        if !self.auto_apply_trackers {
            return out;
        }
        for pack in self.packs.iter().filter(|p| p.enabled) {
            for t in &pack.trackers {
                let t = t.trim();
                if !t.is_empty() && seen.insert(t.to_ascii_lowercase()) {
                    out.push(t.to_string());
                }
            }
        }
        out
    }

    /// True when this filename's extension is one the user skips by default.
    pub fn is_skipped(&self, filename: &str) -> bool {
        if !self.skip_types_enabled {
            return false;
        }
        let ext = match filename.rsplit_once('.') {
            Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
            _ => return false,
        };
        self.skip_types
            .iter()
            .any(|s| s.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext))
    }

    pub fn trackers_in_packs(&self, ids: &[String]) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for pack in self.packs.iter().filter(|p| ids.contains(&p.id)) {
            for t in &pack.trackers {
                let t = t.trim();
                if !t.is_empty() && seen.insert(t.to_ascii_lowercase()) {
                    out.push(t.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            skip_types_enabled: true,
            skip_types: vec!["exe".into(), ".MSI".into()],
            ..Default::default()
        }
    }

    #[test]
    fn skips_by_extension_case_and_dot_insensitively() {
        let s = settings();
        assert!(s.is_skipped("setup.exe"));
        assert!(s.is_skipped("Setup.EXE"));
        assert!(s.is_skipped("installer.msi"));
        assert!(!s.is_skipped("movie.mkv"));
        assert!(!s.is_skipped("noextension"));
        // a name that merely contains the text is not a match
        assert!(!s.is_skipped("exe"));
        assert!(!s.is_skipped("readme.exec"));
    }

    #[test]
    fn skipping_can_be_turned_off() {
        let s = Settings { skip_types_enabled: false, ..settings() };
        assert!(!s.is_skipped("setup.exe"));
    }

    #[test]
    fn moves_a_finished_download_into_place() {
        let root = std::env::temp_dir().join(format!("gh-move-{}", std::process::id()));
        let from = root.join("incomplete/Some Release");
        let to = root.join("done/Some Release");
        std::fs::create_dir_all(from.join("sub")).unwrap();
        std::fs::write(from.join("a.bin"), b"one").unwrap();
        std::fs::write(from.join("sub/b.bin"), b"two").unwrap();

        move_contents(&from, &to).unwrap();

        assert_eq!(std::fs::read(to.join("a.bin")).unwrap(), b"one");
        assert_eq!(std::fs::read(to.join("sub/b.bin")).unwrap(), b"two");
        assert!(!from.exists(), "the staging folder should be cleaned up");
        std::fs::remove_dir_all(&root).ok();
    }
}
