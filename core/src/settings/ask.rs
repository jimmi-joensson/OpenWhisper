//! `ask` block of `settings.json` (TASK-97).
//!
//! Ask is the second thing the microphone can be used for: a spoken
//! question, answered by a language model and shown in a panel instead
//! of being typed. It sends the question off the machine, so it has
//! its own switch and is off by default, like Refine.
//!
//! What is *not* here: which backend answers, where the Claude binary
//! is, the endpoint URL and the API key. Ask uses the backend Refine
//! is set up with ([`super::RefineSettings`]), so there is one place
//! to sign in and one sentence about where text goes.

use serde::{Deserialize, Serialize};

use super::{DEFAULT_CLAUDE_MODEL, RefineBackendKind, RefineEffort, RefineSettings};

/// Shortest and longest Ask timeout a user can set.
pub const ASK_TIMEOUT_MIN_MS: u64 = 5_000;
pub const ASK_TIMEOUT_MAX_MS: u64 = 300_000;
/// Timeout when the user has not chosen one. An answer is longer than
/// a cleaned-up sentence and is read, not waited for with a cursor
/// blinking, so this is far above Refine's five seconds.
pub const ASK_TIMEOUT_DEFAULT_MS: u64 = 60_000;
/// Longest standing instructions kept, in characters. They ride along
/// in the prompt of every ask.
pub const ASK_INSTRUCTIONS_MAX_CHARS: usize = 4000;

/// The `ask` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct AskSettings {
    /// Off by default: an ask sends the question off the machine.
    /// With it off the ask hotkey is not installed and nothing is sent.
    pub enabled: bool,
    /// Let the model search the web when the answer needs current
    /// facts. Off by default: a search sends the query to a search
    /// provider and takes many seconds. Claude backend only.
    pub web_search: bool,
    /// The user's standing instructions for answers ("answer in
    /// Danish", "I am a nurse, skip the basics"). Empty = none.
    pub instructions: String,
    /// Claude model for asks. `None` = [`DEFAULT_CLAUDE_MODEL`]. Not
    /// used with the OpenAI-compatible backend: a Claude model name
    /// means nothing to another server, so that backend answers with
    /// the model in the Refine settings.
    pub model: Option<String>,
    /// Claude effort for asks. `None` = low. Like `model`, not used
    /// with the OpenAI-compatible backend. A value this app does not
    /// know reads as `None` instead of costing the user the block.
    #[serde(deserialize_with = "super::modes::lenient_effort")]
    pub effort: Option<RefineEffort>,
    /// How long an ask waits for the model. Doubled while web search
    /// is in use, since a search alone takes 15 to 20 seconds.
    pub timeout_ms: u64,
}

impl Default for AskSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            web_search: false,
            instructions: String::new(),
            model: None,
            effort: None,
            timeout_ms: ASK_TIMEOUT_DEFAULT_MS,
        }
    }
}

impl AskSettings {
    /// Trimmed strings and an in-range timeout. Applied on every load
    /// and save, so a hand-edited file cannot push a 0 ms timeout into
    /// an ask.
    pub fn cleaned(mut self) -> Self {
        self.instructions = self
            .instructions
            .trim()
            .chars()
            .take(ASK_INSTRUCTIONS_MAX_CHARS)
            .collect::<String>()
            .trim_end()
            .to_string();
        self.model = self.model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
        // The Claude CLI has no "minimal" level.
        if self.effort == Some(RefineEffort::Minimal) {
            self.effort = Some(RefineEffort::Low);
        }
        self.timeout_ms = self.timeout_ms.clamp(ASK_TIMEOUT_MIN_MS, ASK_TIMEOUT_MAX_MS);
        self
    }

    /// The model an ask is answered with, given the backend `refine`
    /// selects: this block's model or the Claude default on the Claude
    /// backend, the Refine model on the OpenAI-compatible one.
    pub fn model_for(&self, refine: &RefineSettings) -> String {
        match refine.backend {
            RefineBackendKind::OpenaiCompatible => refine.openai.model.trim().to_string(),
            _ => self
                .model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .unwrap_or(DEFAULT_CLAUDE_MODEL)
                .to_string(),
        }
    }

    /// The effort an ask is answered with: this block's effort or low
    /// on the Claude backend, the Refine effort (which may be none) on
    /// the OpenAI-compatible one.
    pub fn effort_for(&self, refine: &RefineSettings) -> Option<RefineEffort> {
        match refine.backend {
            RefineBackendKind::OpenaiCompatible => refine.openai.effort,
            _ => Some(match self.effort {
                None | Some(RefineEffort::Minimal) => RefineEffort::Low,
                Some(effort) => effort,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::OpenAiSettings;

    #[test]
    fn defaults_are_off_without_web_search() {
        let s = AskSettings::default();
        assert!(!s.enabled);
        assert!(!s.web_search);
        assert_eq!(s.instructions, "");
        assert_eq!((s.model.clone(), s.effort), (None, None));
        assert_eq!(s.timeout_ms, 60_000);
        assert_eq!(s.clone().cleaned(), s);
    }

    #[test]
    fn missing_and_partial_blocks_fill_defaults() {
        let s: AskSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, AskSettings::default());
        let s: AskSettings = serde_json::from_str(r#"{"enabled":true,"model":"opus"}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.model.as_deref(), Some("opus"));
        assert_eq!(s.timeout_ms, 60_000);
        // An effort this version does not know does not cost the block.
        let s: AskSettings = serde_json::from_str(r#"{"enabled":true,"effort":"turbo"}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.effort, None);
    }

    #[test]
    fn cleaned_clamps_the_timeout_and_trims() {
        let s = AskSettings {
            timeout_ms: 5,
            instructions: "  Answer in Danish. \n".into(),
            model: Some("  ".into()),
            effort: Some(RefineEffort::Minimal),
            ..AskSettings::default()
        }
        .cleaned();
        assert_eq!(s.timeout_ms, ASK_TIMEOUT_MIN_MS);
        assert_eq!(s.instructions, "Answer in Danish.");
        assert_eq!(s.model, None);
        assert_eq!(s.effort, Some(RefineEffort::Low));
        let s = AskSettings { timeout_ms: 9_000_000, ..AskSettings::default() }.cleaned();
        assert_eq!(s.timeout_ms, ASK_TIMEOUT_MAX_MS);
        let long = AskSettings { instructions: "x".repeat(ASK_INSTRUCTIONS_MAX_CHARS + 50), ..AskSettings::default() };
        assert_eq!(long.cleaned().instructions.chars().count(), ASK_INSTRUCTIONS_MAX_CHARS);
    }

    #[test]
    fn model_and_effort_follow_the_backend_refine_is_set_up_with() {
        let ask = AskSettings::default();
        let claude = RefineSettings::default();
        // Refine's own Claude model does not leak into asks.
        let mut haiku = claude.clone();
        haiku.claude_cli.model = "haiku".into();
        haiku.claude_cli.effort = RefineEffort::High;
        assert_eq!(ask.model_for(&haiku), "sonnet");
        assert_eq!(ask.effort_for(&haiku), Some(RefineEffort::Low));

        let own = AskSettings {
            model: Some("opus".into()),
            effort: Some(RefineEffort::Medium),
            ..AskSettings::default()
        };
        assert_eq!(own.model_for(&claude), "opus");
        assert_eq!(own.effort_for(&claude), Some(RefineEffort::Medium));

        // The other backend never gets a Claude model name.
        let openai = RefineSettings {
            backend: RefineBackendKind::OpenaiCompatible,
            openai: OpenAiSettings {
                model: "local-model".into(),
                effort: None,
                ..OpenAiSettings::default()
            },
            ..RefineSettings::default()
        };
        assert_eq!(own.model_for(&openai), "local-model");
        assert_eq!(own.effort_for(&openai), None);
    }
}
