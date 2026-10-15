//! The open-file limit, raised for the engine's peer connections.

/// Raises the open-file soft limit as far as the system allows.
pub fn raise() {
    if let Err(e) = librqbit::try_increase_nofile_limit() {
        eprintln!("Could not raise the open file limit: {e:#}");
    }
}
