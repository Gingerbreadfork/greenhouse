//! Running inside a Flatpak sandbox, where updates come from the store and
//! starting at login goes through a portal.

use std::path::PathBuf;

/// The app ID, when running as a Flatpak.
pub fn app_id() -> Option<String> {
    std::env::var("FLATPAK_ID").ok().filter(|id| !id.is_empty())
}

/// The runtime folder shared by every launch of the app and visible to the
/// host. Each Flatpak launch otherwise gets a private one.
pub fn shared_runtime_dir() -> Option<PathBuf> {
    Some(dirs::runtime_dir()?.join("app").join(app_id()?))
}

/// Asks the Background portal to add or remove the login entry, since the
/// sandbox cannot write one itself.
#[cfg(target_os = "linux")]
pub fn request_autostart(wanted: bool) -> Result<(), String> {
    let (sent, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(request_background(wanted).map_err(|e| e.to_string()));
    });
    let granted = answer
        .recv_timeout(std::time::Duration::from_secs(120))
        .map_err(|_| "the desktop did not answer".to_string())??;
    if granted != wanted {
        return Err("the desktop did not allow it".into());
    }
    Ok(())
}

/// Returns whether the portal says Greenhouse will start at login.
#[cfg(target_os = "linux")]
fn request_background(wanted: bool) -> zbus::Result<bool> {
    use std::collections::HashMap;
    use zbus::blocking::{proxy::Builder, Connection, Proxy};
    use zbus::proxy::CacheProperties;
    use zbus::zvariant::{OwnedValue, Value};

    const PORTAL: &str = "org.freedesktop.portal.Desktop";

    let connection = Connection::session()?;
    let sender = connection
        .unique_name()
        .ok_or_else(|| zbus::Error::Failure("no name on the session bus".into()))?
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!("greenhouse{}", std::process::id());

    // Listen on the request before making it, so the answer cannot be missed.
    let request: Proxy = Builder::new(&connection)
        .destination(PORTAL)?
        .path(format!("/org/freedesktop/portal/desktop/request/{sender}/{token}"))?
        .interface("org.freedesktop.portal.Request")?
        .cache_properties(CacheProperties::No)
        .build()?;
    let mut answers = request.receive_signal("Response")?;

    let background: Proxy = Builder::new(&connection)
        .destination(PORTAL)?
        .path("/org/freedesktop/portal/desktop")?
        .interface("org.freedesktop.portal.Background")?
        .cache_properties(CacheProperties::No)
        .build()?;
    let options: HashMap<&str, Value> = HashMap::from([
        ("handle_token", Value::from(token.as_str())),
        ("reason", Value::from("Start Greenhouse in the background when you log in")),
        ("autostart", Value::from(wanted)),
        ("commandline", Value::from(vec!["greenhouse", "--hidden"])),
    ]);
    background.call_method("RequestBackground", &("", options))?;

    let answer = answers
        .next()
        .ok_or_else(|| zbus::Error::Failure("the portal did not answer".into()))?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize()?;
    if code != 0 {
        return Err(zbus::Error::Failure("the desktop did not allow it".into()));
    }
    Ok(results
        .get("autostart")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false))
}
