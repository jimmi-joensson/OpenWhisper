//! Settings for transcript history (TASK-95): whether dictated text
//! is kept on disk, and for how long.

use serde::{Deserialize, Serialize};

/// Retention when the user has not chosen one.
pub const HISTORY_RETENTION_DEFAULT_DAYS: u32 = 30;
/// Longest retention that still counts as a number of days. Above
/// this the value is clamped; "keep forever" is 0, not a big number.
pub const HISTORY_RETENTION_MAX_DAYS: u32 = 3650;

/// The `history` block of `settings.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct HistorySettings {
    /// Keep the text of each dictation. Off by default: no dictated
    /// text is written to disk until the user asks for it. Switching
    /// it off stops saving; what is already stored stays until it is
    /// cleared or `retention_days` removes it.
    pub enabled: bool,
    /// Stored text older than this many days is removed. 0 keeps it
    /// until the user deletes it.
    pub retention_days: u32,
    /// Folder the day files are written to. `None` = `history/` in
    /// the app's config folder, next to `dictionary.json`. A folder
    /// inside a synced location (iCloud, Dropbox, a notes vault) is
    /// synced with it; that is the user's call, and the settings say
    /// so. Files are not moved when this changes.
    pub folder: Option<String>,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: HISTORY_RETENTION_DEFAULT_DAYS,
            folder: None,
        }
    }
}

impl HistorySettings {
    /// Clamp to what the app supports. Applied on load and on save.
    pub fn cleaned(mut self) -> Self {
        self.retention_days = self.retention_days.min(HISTORY_RETENTION_MAX_DAYS);
        self.folder = self.folder.map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_thirty_days() {
        let s = HistorySettings::default();
        assert!(!s.enabled);
        assert_eq!(s.retention_days, 30);
        assert_eq!(s.folder, None);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: HistorySettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.retention_days, 30);
    }

    #[test]
    fn cleaned_clamps_retention_and_tidies_the_folder() {
        let s = HistorySettings { enabled: true, retention_days: 999_999, folder: Some("  ".into()) }
            .cleaned();
        assert_eq!(s.retention_days, HISTORY_RETENTION_MAX_DAYS);
        assert_eq!(s.folder, None);
        let s = HistorySettings { folder: Some(" /notes/dictations ".into()), ..Default::default() }
            .cleaned();
        assert_eq!(s.folder.as_deref(), Some("/notes/dictations"));
    }
}
