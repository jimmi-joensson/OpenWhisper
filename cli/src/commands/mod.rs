pub mod crash_dump;
pub mod enumerate_devices;
pub mod memory;
pub mod recognizer_info;
pub mod refine;
pub mod settings;
pub mod transcribe;

/// Bundle id of the desktop app; its config dir is where
/// `settings.json` and `dictionary.json` live.
pub const BUNDLE_ID: &str = "com.openwhisper.app";

/// The desktop app's config directory, mirroring Tauri's
/// `app.path().app_config_dir()`. `OPENWHISPER_CONFIG_DIR` overrides
/// it (tests, the dev build's `com.openwhisper.app.dev`).
pub fn config_dir() -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;
    if let Some(dir) = std::env::var_os("OPENWHISPER_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(std::path::PathBuf::from(dir));
    }
    let dir = dirs::config_dir().context("could not resolve OS config directory")?;
    Ok(dir.join(BUNDLE_ID))
}

/// The desktop app's SQLite file (`openwhisper.db` in Tauri's
/// `app_data_dir()`). `OPENWHISPER_CONFIG_DIR` overrides the folder,
/// like for [`config_dir`].
pub fn database_path() -> anyhow::Result<std::path::PathBuf> {
    use anyhow::Context;
    if let Some(dir) = std::env::var_os("OPENWHISPER_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(std::path::PathBuf::from(dir).join("openwhisper.db"));
    }
    let dir = dirs::data_dir().context("could not resolve OS data directory")?;
    Ok(dir.join(BUNDLE_ID).join("openwhisper.db"))
}
