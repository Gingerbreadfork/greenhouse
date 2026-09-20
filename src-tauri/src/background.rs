//! Keeping Greenhouse running with its window closed: hiding instead of
//! quitting, the tray icon, a quiet start, and starting at login.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::activation;
use crate::commands::AppState;

const TRAY_ID: &str = "main";

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

/// Shows the tray icon while Greenhouse keeps running in the background, and
/// removes it otherwise. Call from the main thread.
pub fn sync_tray<R: Runtime>(app: &tauri::AppHandle<R>, wanted: bool) {
    let present = app.tray_by_id(TRAY_ID).is_some();
    if !wanted {
        if present {
            let _ = app.remove_tray_by_id(TRAY_ID);
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
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Greenhouse")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app, None),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn autostart_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("autostart"))
}

/// The program to start at login. An AppImage has to be started by its own
/// path, not the one it unpacks to.
fn own_program() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

fn autostart_entry(program: &Path) -> String {
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

fn write_autostart(dir: &Path, program: &Path, wanted: bool) -> std::io::Result<()> {
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
pub fn sync_autostart(wanted: bool) -> Result<(), String> {
    let dir = autostart_dir().ok_or("there is no config folder to put it in")?;
    let program = own_program().ok_or("could not work out where Greenhouse is installed")?;
    write_autostart(&dir, &program, wanted).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
