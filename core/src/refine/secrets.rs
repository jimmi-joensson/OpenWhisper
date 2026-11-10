//! Where the Refine API key lives: the operating system's keychain,
//! never a config file.
//!
//! - macOS: Keychain (login keychain, generic password).
//! - Windows: Credential Manager (generic credential).
//! - Elsewhere: no keychain backend is compiled in; use the
//!   environment variable.
//!
//! [`ENV_API_KEY`] overrides the keychain everywhere. It exists for
//! headless use (CI, the CLI on a server) where there is no keychain
//! to unlock.
//!
//! One entry, shared by the release app, the dev build and the CLI.
//! On macOS each of those is a different program as far as Keychain
//! is concerned, so the first read from a program other than the one
//! that stored the key asks the user for permission once.

/// Environment variable that overrides the stored key.
pub const ENV_API_KEY: &str = "OPENWHISPER_REFINE_API_KEY";

/// Keychain service name of the entry.
pub const SERVICE: &str = "com.openwhisper.app.refine";
/// Keychain account name of the entry.
pub const ACCOUNT: &str = "openai-compatible-api-key";

fn env_key() -> Option<String> {
    std::env::var(ENV_API_KEY)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// The API key for the OpenAI-compatible backend: the environment
/// override when set, else the keychain entry. `Ok(None)` = no key
/// stored.
pub fn api_key() -> Result<Option<String>, String> {
    if let Some(key) = env_key() {
        return Ok(Some(key));
    }
    store::get()
}

/// True when a key is available (environment or keychain). Never
/// returns the key itself; the settings pane uses this to show
/// "a key is stored" without reading it into the UI.
pub fn has_api_key() -> bool {
    matches!(api_key(), Ok(Some(_)))
}

/// Store `key` in the keychain, replacing any earlier one. An empty
/// key clears the entry.
pub fn set_api_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return clear_api_key();
    }
    store::set(key)
}

/// Remove the key from the keychain. Not an error when none is stored.
pub fn clear_api_key() -> Result<(), String> {
    store::delete()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod store {
    use super::{ACCOUNT, SERVICE};
    use keyring::{Entry, Error};

    fn entry() -> Result<Entry, String> {
        Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("keychain: {e}"))
    }

    pub(super) fn get() -> Result<Option<String>, String> {
        match entry()?.get_password() {
            Ok(key) => Ok(Some(key).filter(|k| !k.trim().is_empty())),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("could not read the API key from the keychain: {e}")),
        }
    }

    pub(super) fn set(key: &str) -> Result<(), String> {
        entry()?
            .set_password(key)
            .map_err(|e| format!("could not store the API key in the keychain: {e}"))
    }

    pub(super) fn delete() -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("could not remove the API key from the keychain: {e}")),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod store {
    use super::ENV_API_KEY;

    pub(super) fn get() -> Result<Option<String>, String> {
        Ok(None)
    }

    pub(super) fn set(_key: &str) -> Result<(), String> {
        Err(format!(
            "no keychain support on this platform; set {ENV_API_KEY} instead"
        ))
    }

    pub(super) fn delete() -> Result<(), String> {
        Ok(())
    }
}
