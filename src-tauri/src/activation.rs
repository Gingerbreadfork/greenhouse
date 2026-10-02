//! Hands the desktop's activation token from a second launch to the running
//! window. Wayland only lets a window raise itself when it presents one.

use std::path::PathBuf;

use tauri::{Runtime, WebviewWindow};

fn token_file() -> Option<PathBuf> {
    Some(dirs::runtime_dir()?.join("greenhouse-activation"))
}

/// Saves this launch's token. Call before GTK starts, which consumes it.
pub fn stash_launch_token() {
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .or_else(|_| std::env::var("DESKTOP_STARTUP_ID"))
        .unwrap_or_default();
    if let Some(file) = token_file() {
        let _ = std::fs::write(file, token);
    }
}

/// The token left by the most recent launch, if it had one.
pub fn take_launch_token() -> Option<String> {
    let file = token_file()?;
    let token = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(file);
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// Raises the window, with the token when there is one so the compositor
/// allows it and ends the launch's busy cursor.
#[cfg(target_os = "linux")]
pub fn present<R: Runtime>(window: &WebviewWindow<R>, token: Option<String>) {
    use gtk::prelude::GtkWindowExt;

    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(gtk_window) = target.gtk_window() else {
            return;
        };
        match token {
            Some(token) => gtk_window.set_startup_id(&token),
            None => gtk_window.present(),
        }
    });
}

/// Raises the window. Other desktops let a window focus itself.
#[cfg(not(target_os = "linux"))]
pub fn present<R: Runtime>(window: &WebviewWindow<R>, _token: Option<String>) {
    let _ = window.set_focus();
}
