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
    /// Folder whose .torrent files are added automatically. Empty means off.
    pub watch_dir: String,
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
    /// Closing the window hides it and Greenhouse carries on.
    pub close_to_background: bool,
    /// Start quietly when you log in.
    pub start_on_login: bool,
    /// Look for a newer release at launch and every few hours.
    pub check_updates: bool,
    /// Install a newer release as soon as it is found, then restart.
    pub install_updates: bool,
    /// Program that plays media files. Empty means find one.
    pub player_command: String,
    pub packs: Vec<TrackerPack>,
    pub feeds: Vec<crate::feeds::Feed>,
    /// How often feeds are checked.
    pub feed_interval_minutes: u32,
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
            watch_dir: String::new(),
            skip_types_enabled: true,
            skip_types: default_skip_types(),
            max_active_downloads: 0,
            max_active_seeds: 0,
            seed_ratio_limit: 0.0,
            seed_time_limit_minutes: 0,
            notify_on_done: true,
            close_to_background: false,
            start_on_login: false,
            check_updates: true,
            install_updates: false,
            player_command: String::new(),
            packs: vec![starter_pack()],
            feeds: Vec::new(),
            feed_interval_minutes: 30,
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

#[derive(Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Where the DHT keeps its state. None means the engine's usual place.
    pub dht_file: Option<PathBuf>,
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
            dht_file: None,
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

    /// The record of what Greenhouse has done.
    pub fn activity_file(&self) -> PathBuf {
        self.data_dir.join("activity.json")
    }

    /// Feed items that have already been dealt with.
    pub fn feeds_file(&self) -> PathBuf {
        self.data_dir.join("feeds.json")
    }

    /// Magnets that were confirmed but had no metadata yet when we last quit.
    pub fn queued_file(&self) -> PathBuf {
        self.data_dir.join("queued.json")
    }

    /// Torrents the queue paused, as opposed to ones you paused.
    pub fn queue_paused_file(&self) -> PathBuf {
        self.data_dir.join("queue-paused.json")
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

/// Why a move did not finish.
#[derive(Debug)]
pub enum MoveFailed {
    /// Everything is back where it started.
    Stayed(anyhow::Error),
    /// Some entries reached the destination and could not be put back.
    Split(anyhow::Error),
}

/// Moves everything inside `from` into `to`, falling back to copy-and-delete
/// when the two are on different filesystems. When one entry fails, the ones
/// already moved are put back.
pub fn move_contents(from: &Path, to: &Path) -> std::result::Result<(), MoveFailed> {
    let mut moved = Vec::new();
    match move_each(from, to, &mut moved) {
        Ok(()) => {
            let _ = std::fs::remove_dir(from);
            Ok(())
        }
        Err(e) => {
            let mut undone = true;
            for (source, target, merged) in moved.iter().rev() {
                undone &= !merged && move_entry(target, source).is_ok();
            }
            Err(if undone { MoveFailed::Stayed(e) } else { MoveFailed::Split(e) })
        }
    }
}

/// Moves each entry, noting where it went and whether it was merged into a
/// folder already there, which cannot be undone without taking that along.
fn move_each(from: &Path, to: &Path, moved: &mut Vec<(PathBuf, PathBuf, bool)>) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let source = entry?.path();
        let target = to.join(source.file_name().unwrap_or_default());
        if std::fs::rename(&source, &target).is_ok() {
            moved.push((source, target, false));
            continue;
        }
        let existed = target.exists();
        if let Err(e) = copy_recursive(&source, &target) {
            if !existed {
                let _ = remove_entry(&target);
            }
            return Err(e);
        }
        moved.push((source.clone(), target.clone(), target.is_dir() && existed));
        remove_entry(&source)?;
    }
    Ok(())
}

fn move_entry(source: &Path, target: &Path) -> Result<()> {
    if std::fs::rename(source, target).is_err() {
        copy_recursive(source, target)?;
        remove_entry(source)?;
    }
    Ok(())
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
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

/// Bytes free to us on the volume that holds `path`, measured at the nearest
/// folder that already exists.
#[cfg(unix)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let existing = path.ancestors().find(|p| p.exists())?;
    let c_path = std::ffi::CString::new(existing.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    if unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// Bytes free to us on the volume that holds `path`, measured at the nearest
/// folder that already exists.
#[cfg(windows)]
pub fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let existing = path.ancestors().find(|p| p.exists())?;
    let wide: Vec<u16> = existing
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
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
    fn free_space_is_measured_where_a_new_folder_would_go() {
        let tmp = std::env::temp_dir();
        let here = free_space(&tmp).unwrap();
        assert!(here > 0);
        assert!(free_space(&tmp.join("not/made/yet")).is_some());
        assert_eq!(free_space(Path::new("")), None);
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

    #[test]
    fn a_failed_move_puts_back_what_it_moved() {
        let root = std::env::temp_dir().join(format!("gh-move-fail-{}", std::process::id()));
        let from = root.join("incomplete/Some Release");
        let to = root.join("done/Some Release");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        for name in ["a.bin", "b.bin", "c.bin"] {
            std::fs::write(from.join(name), name).unwrap();
        }
        // A file cannot replace a folder that has something in it.
        std::fs::create_dir_all(to.join("b.bin/inside")).unwrap();

        let outcome = move_contents(&from, &to);

        assert!(matches!(outcome, Err(MoveFailed::Stayed(_))), "{outcome:?}");
        for name in ["a.bin", "b.bin", "c.bin"] {
            assert_eq!(std::fs::read(from.join(name)).unwrap(), name.as_bytes());
        }
        assert!(!to.join("a.bin").exists());
        assert!(!to.join("c.bin").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
