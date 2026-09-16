//! End-to-end check of the pieces Greenhouse relies on: session start,
//! metadata parsing, tracker injection, and live stats.
//! Run with: cargo run --example engine_check

use std::time::Duration;

use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Session, SessionOptions};

const TORRENT_URL: &str =
    "https://releases.ubuntu.com/24.04.3/ubuntu-24.04.3-desktop-amd64.iso.torrent";

const EXTRA_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://exodus.desync.com:6969/announce",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join("greenhouse-engine-check");
    std::fs::create_dir_all(&dir)?;
    println!("scratch dir: {}", dir.display());

    println!("fetching a real .torrent…");
    let bytes = reqwest::get(TORRENT_URL).await?.bytes().await?;
    println!("  got {} bytes", bytes.len());

    let session = Session::new_with_opts(
        dir.clone(),
        SessionOptions {
            persistence: None,
            ..Default::default()
        },
    )
    .await?;
    println!("session started, listening on {:?}", session.announce_port());

    // 1. list_only must parse metadata without touching the network.
    let listed = match session
        .add_torrent(
            AddTorrent::from_bytes(bytes.clone()),
            Some(AddTorrentOptions {
                list_only: true,
                ..Default::default()
            }),
        )
        .await?
    {
        AddTorrentResponse::ListOnly(l) => l,
        _ => anyhow::bail!("expected a list-only response"),
    };
    let files: Vec<_> = listed.info.iter_file_details().collect();
    let total: u64 = files.iter().map(|f| f.len).sum();
    println!(
        "PREVIEW ok: name={:?} files={} total={} MB info_hash={}",
        listed.info.name().map(|n| n.into_owned()),
        files.len(),
        total / 1024 / 1024,
        listed.info_hash.as_string()
    );

    // 2. Adding with custom trackers must widen the torrent's tracker set.
    let trackers: Vec<String> = EXTRA_TRACKERS.iter().map(|s| s.to_string()).collect();
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes.clone()),
            Some(AddTorrentOptions {
                paused: false,
                overwrite: true,
                only_files: Some(vec![]),
                trackers: Some(trackers.clone()),
                ..Default::default()
            }),
        )
        .await?
        .into_handle()
        .ok_or_else(|| anyhow::anyhow!("no handle"))?;

    let attached: Vec<String> = handle
        .shared()
        .trackers
        .iter()
        .map(|u| u.to_string())
        .collect();
    println!("TRACKERS ok: {} attached to the torrent", attached.len());
    for t in &trackers {
        let present = attached.iter().any(|a| a.trim_end_matches('/') == t.trim_end_matches('/'));
        println!("  injected {t} -> {}", if present { "present" } else { "MISSING" });
    }

    // 3. Stats must populate the way the UI expects.
    for i in 1..=6 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let stats = handle.stats();
        let live = stats.live.as_ref();
        println!(
            "t+{:>2}s state={} peers(live={} queued={} seen={}) down={} B/s progress={}/{}",
            i * 5,
            stats.state,
            live.map(|l| l.snapshot.peer_stats.live).unwrap_or(0),
            live.map(|l| l.snapshot.peer_stats.queued).unwrap_or(0),
            live.map(|l| l.snapshot.peer_stats.seen).unwrap_or(0),
            live.map(|l| l.download_speed.as_bytes()).unwrap_or(0),
            stats.progress_bytes,
            stats.total_bytes
        );
    }

    session.stop().await;
    let _ = std::fs::remove_dir_all(&dir);
    println!("done");
    Ok(())
}
