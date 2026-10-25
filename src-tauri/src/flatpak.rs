//! Running inside a Flatpak sandbox, where updates come from the store.

/// The app ID, when running as a Flatpak.
pub fn app_id() -> Option<String> {
    std::env::var("FLATPAK_ID").ok().filter(|id| !id.is_empty())
}
