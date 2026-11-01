//! Works around WebKitGTK 2.54 flickering dialogs and panels on Wayland.

/// Moves WebKitGTK's frames through shared memory instead of DMA-BUF.
///
/// From 2.54 the DMA-BUF path paints transparent windows in pieces, so
/// anything that appears with an animation flickers. Shared memory keeps
/// GPU rendering and transparency, and only costs a copy per frame. A value
/// already in the environment wins.
pub fn settle_webkit() {
    #[cfg(target_os = "linux")]
    {
        const FORCE_SHM: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";
        const DISABLE: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
        if std::env::var_os(FORCE_SHM).is_some() || std::env::var_os(DISABLE).is_some() {
            return;
        }
        let Ok(version) = tauri::webview_version() else {
            return;
        };
        if at_least(&version, (2, 54)) {
            std::env::set_var(FORCE_SHM, "1");
        }
    }
}

/// Whether a "major.minor.patch" string is at or past `floor`.
#[cfg(target_os = "linux")]
fn at_least(version: &str, floor: (u32, u32)) -> bool {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    (major, minor) >= floor
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::at_least;

    #[test]
    fn compares_major_and_minor() {
        assert!(at_least("2.54.1", (2, 54)));
        assert!(at_least("2.56.0", (2, 54)));
        assert!(at_least("3.0.0", (2, 54)));
        assert!(!at_least("2.52.6", (2, 54)));
        assert!(!at_least("2.48", (2, 54)));
        assert!(!at_least("garbage", (2, 54)));
    }
}
