//! Settings for transcript history (TASK-95): whether dictated text
//! is kept on disk, and for how long.

use serde::{Deserialize, Serialize};

/// Retention when the user has not chosen one.
pub const HISTORY_RETENTION_DEFAULT_DAYS: u32 = 30;
/// Longest retention that still counts as a number of days. Above
/// this the value is clamped; "keep forever" is 0, not a big number.
pub const HISTORY_RETENTION_MAX_DAYS: u32 = 3650;

/// The `history` block of `settings.json`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: HISTORY_RETENTION_DEFAULT_DAYS,
        }
    }
}

impl HistorySettings {
    /// Clamp to what the app supports. Applied on load and on save.
    pub fn cleaned(mut self) -> Self {
        self.retention_days = self.retention_days.min(HISTORY_RETENTION_MAX_DAYS);
        self
    }

    /// Text recorded before this moment has aged out. `None` when the
    /// user keeps history forever.
    pub fn cutoff_ms(&self, now_ms: i64) -> Option<i64> {
        if self.retention_days == 0 {
            return None;
        }
        Some(now_ms - i64::from(self.retention_days) * 24 * 60 * 60 * 1000)
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
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: HistorySettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.retention_days, 30);
    }

    #[test]
    fn cleaned_clamps_retention() {
        let s = HistorySettings { enabled: true, retention_days: 999_999 }.cleaned();
        assert_eq!(s.retention_days, HISTORY_RETENTION_MAX_DAYS);
    }

    #[test]
    fn cutoff_follows_retention() {
        let day = 24 * 60 * 60 * 1000;
        let s = HistorySettings { enabled: true, retention_days: 7 };
        assert_eq!(s.cutoff_ms(10 * day), Some(3 * day));
        let forever = HistorySettings { enabled: true, retention_days: 0 };
        assert_eq!(forever.cutoff_ms(10 * day), None);
    }
}
