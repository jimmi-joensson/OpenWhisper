//! `ow_navigate`: telling the main window which view to show.
//!
//! The main window is one React tree with routes (Home, Settings,
//! History, ...). Anything outside it that wants a view shown (the
//! tray's Preferences item, the pill, the answer window of Ask) brings
//! the window forward and emits this event; `App.tsx` listens and
//! swaps the route.
//!
//! The payload is either a view name as a plain string (`"settings"`,
//! `"main"`, `"diagnostics"`, `"transcribe"`), which is all there was
//! at first and what the tray still sends, or an object that also
//! names a settings pane and a row on it to bring into view:
//!
//! ```json
//! { "view": "settings", "pane": "ask", "focus": "web-search" }
//! ```
//!
//! The listener takes both. Which panes and rows exist is the
//! frontend's knowledge (`src/lib/settings-panes.ts`,
//! `src/lib/navigate.ts`): a name it does not know is ignored there.

use serde_json::{json, Value};

/// The event name.
pub(crate) const NAVIGATE_EVENT: &str = "ow_navigate";

/// The payload that shows Settings. Without a pane it is the plain
/// string the tray sends, so the window lands where it always did.
/// With one, the object form; `focus` (a row on that pane) only
/// counts together with a pane.
pub(crate) fn settings_payload(pane: Option<&str>, focus: Option<&str>) -> Value {
    fn named(name: Option<&str>) -> Option<&str> {
        name.map(str::trim).filter(|name| !name.is_empty())
    }
    match (named(pane), named(focus)) {
        (None, _) => json!("settings"),
        (Some(pane), None) => json!({ "view": "settings", "pane": pane }),
        (Some(pane), Some(focus)) => json!({ "view": "settings", "pane": pane, "focus": focus }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_without_a_pane_is_the_plain_string_it_always_was() {
        assert_eq!(settings_payload(None, None), json!("settings"));
        assert_eq!(settings_payload(Some("  "), None), json!("settings"));
        // A row without a pane to find it on means nothing.
        assert_eq!(settings_payload(None, Some("web-search")), json!("settings"));
    }

    #[test]
    fn a_pane_and_a_row_on_it_travel_as_an_object() {
        assert_eq!(settings_payload(Some("ask"), None), json!({ "view": "settings", "pane": "ask" }));
        assert_eq!(
            settings_payload(Some(" ask "), Some("web-search")),
            json!({ "view": "settings", "pane": "ask", "focus": "web-search" })
        );
        assert_eq!(settings_payload(Some("ask"), Some("")), json!({ "view": "settings", "pane": "ask" }));
    }
}
