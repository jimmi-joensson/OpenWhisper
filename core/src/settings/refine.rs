//! `refine` block of `settings.json` (TASK-93).
//!
//! Everything Refine needs except the API key: that lives in the OS
//! keychain (see [`crate::refine::secrets`]) and is never written to a
//! config file.

use serde::{Deserialize, Serialize};

/// Shortest and longest Refine timeout a user can set.
pub const REFINE_TIMEOUT_MIN_MS: u64 = 1_000;
pub const REFINE_TIMEOUT_MAX_MS: u64 = 30_000;
/// Upper bound for [`RefineSettings::min_words`].
pub const REFINE_MIN_WORDS_MAX: u32 = 50;

/// Which backend Refine sends the transcript to.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RefineBackendKind {
    /// The user's own installed and signed-in Claude Code CLI, run
    /// headless. Uses their Claude subscription; no key in the app.
    #[default]
    ClaudeCli,
    /// Any endpoint that speaks the OpenAI chat-completions API.
    OpenaiCompatible,
}

impl RefineBackendKind {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCli => "claude-cli",
            Self::OpenaiCompatible => "openai-compatible",
        }
    }

    /// Parse the serialized form ("claude-cli", "openai-compatible").
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude-cli" | "claude" => Some(Self::ClaudeCli),
            "openai-compatible" | "openai" => Some(Self::OpenaiCompatible),
            _ => None,
        }
    }
}

/// How hard the model thinks before it answers. The set is whatever
/// the backends take today, not a recommendation: the Claude CLI
/// accepts low to max, OpenAI-style servers minimal to high or more.
/// A level the chosen model does not support comes back as a backend
/// error and the dictation is typed as transcribed.
///
/// The default is low, on evidence: with Haiku, medium effort was not
/// just slower but less reliable at this task. Thinking about a
/// dictated "can you summarize the last three messages" made it start
/// answering the request (16/18 transcripts handled correctly at
/// medium against 18/18 at low; 2026-11-10). Sonnet handled the same
/// set at both levels.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RefineEffort {
    /// OpenAI-style servers only; the Claude CLI has no such level
    /// and gets [`RefineEffort::Low`] instead.
    Minimal,
    #[default]
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl RefineEffort {
    /// Stable id, same as the serialized form and the value both
    /// backends take (`--effort`, `reasoning_effort`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }

    /// Parse a level id ("low", "medium", "high", "xhigh", "max",
    /// "minimal").
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// Model the Claude backend uses unless the user picks another.
pub const DEFAULT_CLAUDE_MODEL: &str = "sonnet";

/// Settings for the Claude Code CLI backend.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct ClaudeCliSettings {
    /// Model alias or full name passed to `--model`.
    ///
    /// Default Sonnet, on evidence (live eval, 19 transcripts,
    /// 2026-11-10): telling the message from a spoken instruction
    /// while still cleaning everything else was 19/19 with Sonnet and
    /// 15 to 16 of 19 with Haiku however the prompt was worded. Haiku
    /// is faster (about 0.85 s against 1.4 s) and lighter on the
    /// user's plan, and stays one click away.
    pub model: String,
    /// Passed to `--effort`.
    pub effort: RefineEffort,
    /// Explicit path to the `claude` binary. `None` = look in the
    /// usual install locations and on `PATH`.
    pub path: Option<String>,
}

impl Default for ClaudeCliSettings {
    fn default() -> Self {
        Self {
            model: DEFAULT_CLAUDE_MODEL.to_string(),
            effort: RefineEffort::default(),
            path: None,
        }
    }
}

/// Settings for the OpenAI-compatible backend. The API key is not
/// here on purpose.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct OpenAiSettings {
    /// Base URL up to and including the version segment, e.g.
    /// `https://api.openai.com/v1`. A full `/chat/completions` URL is
    /// accepted too.
    pub base_url: String,
    /// Model name sent in the request. Empty until the user picks one.
    pub model: String,
    /// Sent as `reasoning_effort` when set. `None` (the default)
    /// leaves it out: many models and local servers do not take it.
    pub effort: Option<RefineEffort>,
}

impl Default for OpenAiSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            model: String::new(),
            effort: None,
        }
    }
}

/// The `refine` settings block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct RefineSettings {
    /// Off by default: Refine sends text off the machine.
    pub enabled: bool,
    pub backend: RefineBackendKind,
    /// The user's cleanup instructions. Empty = the conservative
    /// default ([`crate::refine::prompt::DEFAULT_INSTRUCTIONS`]).
    pub instructions: String,
    /// How long a dictation waits for the model before the unrefined
    /// transcript is inserted instead.
    pub timeout_ms: u64,
    /// Utterances with fewer words skip Refine (no model call).
    pub min_words: u32,
    /// Reject model output that shares too few words with what was
    /// said (an answer, an email, a translation). Turn off when the
    /// instructions ask for heavy rewriting on purpose.
    pub drift_guard: bool,
    pub claude_cli: ClaudeCliSettings,
    pub openai: OpenAiSettings,
}

impl Default for RefineSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: RefineBackendKind::default(),
            instructions: String::new(),
            timeout_ms: 5_000,
            min_words: 4,
            drift_guard: true,
            claude_cli: ClaudeCliSettings::default(),
            openai: OpenAiSettings::default(),
        }
    }
}

impl RefineSettings {
    /// Model of the selected backend, for logs and diagnostics.
    pub fn active_model(&self) -> &str {
        match self.backend {
            RefineBackendKind::OpenaiCompatible => &self.openai.model,
            _ => &self.claude_cli.model,
        }
    }

    /// Effort of the selected backend as an id; `""` when the
    /// OpenAI-compatible backend sends none.
    pub fn active_effort(&self) -> &'static str {
        match self.backend {
            RefineBackendKind::OpenaiCompatible => self.openai.effort.map_or("", RefineEffort::as_str),
            _ => self.claude_cli.effort.as_str(),
        }
    }

    /// Trimmed strings and in-range numbers. Applied on every save
    /// and load, so hand-edited files cannot push a 0 ms timeout into
    /// the pipeline.
    pub fn cleaned(mut self) -> Self {
        self.instructions = self.instructions.trim().to_string();
        self.timeout_ms = self
            .timeout_ms
            .clamp(REFINE_TIMEOUT_MIN_MS, REFINE_TIMEOUT_MAX_MS);
        self.min_words = self.min_words.min(REFINE_MIN_WORDS_MAX);
        self.claude_cli.model = self.claude_cli.model.trim().to_string();
        if self.claude_cli.model.is_empty() {
            self.claude_cli.model = ClaudeCliSettings::default().model;
        }
        if self.claude_cli.effort == RefineEffort::Minimal {
            self.claude_cli.effort = RefineEffort::Low;
        }
        self.claude_cli.path = self
            .claude_cli
            .path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        self.openai.base_url = self.openai.base_url.trim().trim_end_matches('/').to_string();
        self.openai.model = self.openai.model.trim().to_string();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_off_and_conservative() {
        let s = RefineSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.backend, RefineBackendKind::ClaudeCli);
        assert_eq!(s.timeout_ms, 5_000);
        assert_eq!(s.min_words, 4);
        assert!(s.drift_guard);
        assert_eq!(s.claude_cli.model, "sonnet");
        assert_eq!(s.claude_cli.effort, RefineEffort::Low);
        assert_eq!(s.openai.effort, None);
        assert_eq!(s.active_model(), "sonnet");
        assert_eq!(s.active_effort(), "low");
    }

    #[test]
    fn effort_round_trips_and_old_files_get_the_default() {
        // A block written before `effort` existed.
        let s: RefineSettings =
            serde_json::from_str(r#"{"claude_cli":{"model":"sonnet","path":null}}"#).unwrap();
        assert_eq!(s.claude_cli.model, "sonnet");
        assert_eq!(s.claude_cli.effort, RefineEffort::Low);

        let s: RefineSettings = serde_json::from_str(
            r#"{"backend":"openai-compatible","claude_cli":{"effort":"medium"},"openai":{"model":"m","effort":"high"}}"#,
        )
        .unwrap();
        assert_eq!(s.claude_cli.effort, RefineEffort::Medium);
        assert_eq!(s.openai.effort, Some(RefineEffort::High));
        assert_eq!(s.active_effort(), "high");
        assert_eq!(RefineEffort::parse("Medium"), Some(RefineEffort::Medium));
        assert_eq!(RefineEffort::parse("max"), Some(RefineEffort::Max));
        assert_eq!(RefineEffort::parse("turbo"), None);
        // Every level round-trips through its id, as serialized.
        for e in [
            RefineEffort::Minimal,
            RefineEffort::Low,
            RefineEffort::Medium,
            RefineEffort::High,
            RefineEffort::Xhigh,
            RefineEffort::Max,
        ] {
            assert_eq!(RefineEffort::parse(e.as_str()), Some(e));
            assert_eq!(serde_json::to_string(&e).unwrap(), format!("\"{}\"", e.as_str()));
        }
    }

    #[test]
    fn any_model_name_is_kept_and_claude_never_gets_minimal() {
        let s: RefineSettings = serde_json::from_str(
            r#"{"claude_cli":{"model":" claude-fable-5-2 ","effort":"minimal"}}"#,
        )
        .unwrap();
        let s = s.cleaned();
        // Whatever the user's plan offers: the name is passed through.
        assert_eq!(s.claude_cli.model, "claude-fable-5-2");
        assert_eq!(s.claude_cli.effort, RefineEffort::Low);
        let s: RefineSettings =
            serde_json::from_str(r#"{"claude_cli":{"model":"opus","effort":"xhigh"}}"#).unwrap();
        assert_eq!(s.cleaned().claude_cli.effort, RefineEffort::Xhigh);
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s: RefineSettings =
            serde_json::from_str(r#"{"enabled":true,"backend":"openai-compatible"}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.backend, RefineBackendKind::OpenaiCompatible);
        assert_eq!(s.openai.base_url, "https://api.openai.com/v1");
        assert_eq!(s.timeout_ms, 5_000);
    }

    #[test]
    fn cleaned_clamps_and_trims() {
        let s = RefineSettings {
            timeout_ms: 5,
            min_words: 900,
            instructions: "  fix commas \n".into(),
            claude_cli: ClaudeCliSettings {
                model: "  ".into(),
                path: Some("   ".into()),
                ..ClaudeCliSettings::default()
            },
            openai: OpenAiSettings {
                base_url: " http://localhost:11434/v1/ ".into(),
                model: " llama ".into(),
                effort: None,
            },
            ..RefineSettings::default()
        }
        .cleaned();
        assert_eq!(s.timeout_ms, REFINE_TIMEOUT_MIN_MS);
        assert_eq!(s.min_words, REFINE_MIN_WORDS_MAX);
        assert_eq!(s.instructions, "fix commas");
        assert_eq!(s.claude_cli.model, "sonnet");
        assert_eq!(s.claude_cli.path, None);
        assert_eq!(s.openai.base_url, "http://localhost:11434/v1");
        assert_eq!(s.openai.model, "llama");
    }

    #[test]
    fn backend_kind_serializes_kebab() {
        assert_eq!(
            serde_json::to_string(&RefineBackendKind::OpenaiCompatible).unwrap(),
            "\"openai-compatible\""
        );
        assert_eq!(
            RefineBackendKind::parse("claude-cli"),
            Some(RefineBackendKind::ClaudeCli)
        );
        assert_eq!(RefineBackendKind::parse("nope"), None);
    }
}
