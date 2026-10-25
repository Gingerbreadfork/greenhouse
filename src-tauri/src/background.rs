//! Keeping Greenhouse running with its window closed: hiding instead of
//! quitting, the tray icon, a quiet start, and starting at login.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::activation;
use crate::commands::{self, AppState};

const TRAY_ID: &str = "main";

/// The part of the tray menu that changes, kept so each tick can update it.
struct TrayMenu<R: Runtime> {
    speeds: Mutex<Option<MenuItem<R>>>,
    shown: Mutex<String>,
}

/// Whether this launch asked to start without showing the window.
pub fn launched_hidden() -> bool {
    std::env::args().any(|a| a == "--hidden")
}

pub fn keeps_running<R: Runtime>(app: &tauri::AppHandle<R>) -> bool {
    app.try_state::<AppState>()
        .is_some_and(|state| state.settings.read().close_to_background)
}

/// Brings the window back, from a second launch or the tray.
pub fn show<R: Runtime>(app: &tauri::AppHandle<R>, token: Option<String>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        activation::present(&window, token);
    }
}

/// Hides the window. The first time in a run, says that Greenhouse is still
/// there, since not every desktop shows a tray icon.
pub fn hide<R: Runtime>(window: &tauri::Window<R>) {
    static TOLD: AtomicBool = AtomicBool::new(false);
    let _ = window.hide();
    if !TOLD.swap(true, Ordering::Relaxed) {
        let _ = window
            .app_handle()
            .notification()
            .builder()
            .title("Greenhouse is still running")
            .body("Open it again from your apps. Quit from Settings or with Ctrl+Q.")
            .show();
    }
}

/// The tray needs one of these libraries, and asking for a tray without it
/// would stop the app.
#[cfg(target_os = "linux")]
fn tray_supported() -> bool {
    ["libayatana-appindicator3.so.1", "libappindicator3.so.1"]
        .iter()
        .any(|name| {
            let Ok(name) = std::ffi::CString::new(*name) else {
                return false;
            };
            let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY) };
            if handle.is_null() {
                return false;
            }
            unsafe { libc::dlclose(handle) };
            true
        })
}

/// Every other desktop has a notification area.
#[cfg(not(target_os = "linux"))]
fn tray_supported() -> bool {
    true
}

/// Shows the tray icon while Greenhouse keeps running in the background, and
/// removes it otherwise. Call from the main thread.
pub fn sync_tray<R: Runtime>(app: &tauri::AppHandle<R>, wanted: bool) {
    let present = app.tray_by_id(TRAY_ID).is_some();
    if !wanted {
        if present {
            let _ = app.remove_tray_by_id(TRAY_ID);
            if let Some(menu) = app.try_state::<TrayMenu<R>>() {
                *menu.speeds.lock() = None;
                menu.shown.lock().clear();
            }
        }
        return;
    }
    if present || !tray_supported() {
        return;
    }
    let _ = build_tray(app);
}

fn build_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show Greenhouse", true, None::<&str>)?;
    let speeds = MenuItem::with_id(app, "speeds", "Idle", false, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let resume_item = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &PredefinedMenuItem::separator(app)?,
            &speeds,
            &pause_item,
            &resume_item,
            &PredefinedMenuItem::separator(app)?,
            &quit_item,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Greenhouse")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app, None),
            "pause_all" => run_on_all(app, true),
            "resume_all" => run_on_all(app, false),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    match app.try_state::<TrayMenu<R>>() {
        Some(menu) => {
            *menu.speeds.lock() = Some(speeds);
            menu.shown.lock().clear();
        }
        None => {
            app.manage(TrayMenu::<R> {
                speeds: Mutex::new(Some(speeds)),
                shown: Mutex::new(String::new()),
            });
        }
    }
    Ok(())
}

fn run_on_all<R: Runtime>(app: &tauri::AppHandle<R>, pause: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if pause {
            commands::pause_all_inner(&state).await;
        } else {
            commands::resume_all_inner(&state).await;
        }
    });
}

/// Puts the current speeds in the tray menu and tooltip, while there is a tray.
pub fn show_speeds<R: Runtime>(app: &tauri::AppHandle<R>, down: u64, up: u64) {
    let Some(menu) = app.try_state::<TrayMenu<R>>() else {
        return;
    };
    let text = speeds_text(down, up);
    {
        let mut shown = menu.shown.lock();
        if *shown == text {
            return;
        }
        *shown = text.clone();
    }
    if let Some(item) = menu.speeds.lock().as_ref() {
        let _ = item.set_text(&text);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tip = if down == 0 && up == 0 {
            "Greenhouse".to_string()
        } else {
            format!("Greenhouse · {text}")
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn speeds_text(down: u64, up: u64) -> String {
    if down == 0 && up == 0 {
        return "Idle".into();
    }
    format!("↓ {}   ↑ {}", rate(down), rate(up))
}

/// Bytes per second, in the units the window uses.
fn rate(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if unit == 0 || value >= 100.0 { 0 } else { 1 };
    format!("{value:.digits$} {}/s", UNITS[unit])
}

/// The program to start at login. An AppImage has to be started by its own
/// path, not the one it unpacks to.
fn own_program() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

#[cfg(not(windows))]
fn autostart_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("autostart"))
}

#[cfg(not(windows))]
fn autostart_entry(program: &std::path::Path) -> String {
    let program = program.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Greenhouse\n\
         Comment=Start Greenhouse in the background\n\
         Exec=\"{program}\" --hidden\n\
         Icon=greenhouse\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(not(windows))]
fn write_autostart(
    dir: &std::path::Path,
    program: &std::path::Path,
    wanted: bool,
) -> std::io::Result<()> {
    let file = dir.join("greenhouse.desktop");
    if !wanted {
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(file, autostart_entry(program))
}

/// Makes the login entry match the setting.
#[cfg(not(windows))]
pub fn sync_autostart(wanted: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if crate::flatpak::app_id().is_some() {
        return crate::flatpak::request_autostart(wanted);
    }
    let dir = autostart_dir().ok_or("there is no config folder to put it in")?;
    let program = own_program().ok_or("could not work out where Greenhouse is installed")?;
    write_autostart(&dir, &program, wanted).map_err(|e| e.to_string())
}

/// Makes the login entry match the setting, as a value under the user's Run key.
#[cfg(windows)]
pub fn sync_autostart(wanted: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let program = own_program().ok_or("could not work out where Greenhouse is installed")?;
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    let result = if wanted {
        run.set_value("Greenhouse", &format!("\"{}\" --hidden", program.display()))
    } else {
        match run.delete_value("Greenhouse") {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    };
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    #[test]
    fn the_tray_shows_speeds_the_way_the_window_does() {
        assert_eq!(speeds_text(0, 0), "Idle");
        assert_eq!(rate(0), "0 B/s");
        assert_eq!(rate(900), "900 B/s");
        assert_eq!(rate(12_800), "12.5 kB/s");
        assert_eq!(rate(8_808_038), "8.4 MB/s");
        assert_eq!(rate(150 * 1024 * 1024), "150 MB/s");
        assert_eq!(speeds_text(1024, 0), "↓ 1.0 kB/s   ↑ 0 B/s");
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_login_entry_starts_hidden_and_can_be_removed() {
        let dir = std::env::temp_dir().join(format!("gh-autostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let program = Path::new("/opt/My Apps/greenhouse");

        write_autostart(&dir, program, true).unwrap();
        let entry = std::fs::read_to_string(dir.join("greenhouse.desktop")).unwrap();
        assert!(entry.contains("Exec=\"/opt/My Apps/greenhouse\" --hidden\n"), "{entry}");
        assert!(entry.starts_with("[Desktop Entry]\nType=Application\n"));

        write_autostart(&dir, program, false).unwrap();
        assert!(!dir.join("greenhouse.desktop").exists());
        // Removing what is not there is fine.
        write_autostart(&dir, program, false).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
