//! Crash file schema, redactor, and Rust panic hook.
//!
//! v1 captures Rust panics from any thread to versioned JSON in the
//! OS-correct app log directory. The shell resolves the directory and
//! passes it in via [`install_panic_hook`] — the hook itself stays
//! shell-agnostic so SwiftUI and Tauri share one capture path.
//!
//! Storage layout:
//! - `<dir>/<unix-ms>.json` — one immutable file per panic
//! - `<dir>/state.json`     — sibling UI flags (TASK-78.2)
//!
//! Redaction is per-`String` field on the typed schema, not against the
//! serialized blob — scrubbing the JSON text would risk eating field
//! names that happen to match a path pattern. Numeric and hashed fields
//! are skipped.

pub mod event_buffer;

use std::backtrace::Backtrace;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dictation;

/// Schema version. Reader treats unknown top-level fields as additive.
pub const SCHEMA_VERSION: u32 = 1;

/// Bounded ring buffer capacity (oldest-first drain into crash file).
pub const EVENT_BUFFER_CAPACITY: usize = 64;

/// Top-level crash file. One per panic; filename is `<id>.json` where
/// `id == ts_unix_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrashFile {
    pub schema_version: u32,
    pub id: String,
    pub ts_unix_ms: i64,
    pub app_version: String,
    pub os: String,
    pub rust_panic: RustPanic,
    #[serde(default)]
    pub recording_state: Option<RecordingStateSnapshot>,
    #[serde(default)]
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RustPanic {
    pub thread_name: String,
    pub message: String,
    pub location: String,
    pub backtrace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordingStateSnapshot {
    pub status_message_at_crash: String,
    pub duration_ms: u64,
    pub samples_captured: u64,
    /// `Some("Parakeet")` / `Some("FluidAudio")` once the recognizer
    /// surfaces it. v1 emits `None` — plumbing the active engine name
    /// into core state is follow-up scope.
    #[serde(default)]
    pub model_kind: Option<String>,
    /// SHA-256 prefix of the active input device id, never the raw
    /// label. v1 emits `None`; populated when audio device selection
    /// learns to expose a hashed id.
    #[serde(default)]
    pub device_id_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub ts_unix_ms: i64,
    pub kind: String,
    pub data: serde_json::Value,
}

/// Redact a single string per the v1 rules:
/// - `/Users/<name>/...` → `/Users/<redacted>/...`
/// - `C:\Users\<name>\...` (and `\\?\` and forward-slash variants) →
///   `C:\Users\<redacted>\...`
/// - The runtime home dir → `<HOME>` (lowest precedence)
/// - Env-var-token assignments in text — `(AWS|OPENAI|ANTHROPIC)_..` and
///   `*_TOKEN`/`*_KEY`/`*_SECRET` keep the key, value becomes `<redacted>`
///
/// Backtrace symbol names (e.g. `openwhisper_core::audio::process_chunk`)
/// are out of scope — those are crate paths and contain no PII.
pub fn redact(input: &str) -> String {
    use regex::Regex;
    static USERS_UNIX: OnceLock<Regex> = OnceLock::new();
    static USERS_WIN: OnceLock<Regex> = OnceLock::new();
    static USERS_WIN_FWD: OnceLock<Regex> = OnceLock::new();
    static USERS_WIN_NS: OnceLock<Regex> = OnceLock::new();
    static ENV_TOKEN: OnceLock<Regex> = OnceLock::new();

    let users_unix =
        USERS_UNIX.get_or_init(|| Regex::new(r"/Users/[^/\s]+").expect("redact unix re"));
    let users_win =
        USERS_WIN.get_or_init(|| Regex::new(r"C:\\Users\\[^\\\s]+").expect("redact win re"));
    let users_win_fwd = USERS_WIN_FWD
        .get_or_init(|| Regex::new(r"C:/Users/[^/\s]+").expect("redact win fwd re"));
    let users_win_ns = USERS_WIN_NS
        .get_or_init(|| Regex::new(r"\\\\\?\\C:\\Users\\[^\\\s]+").expect("redact win ns re"));
    let env_token = ENV_TOKEN.get_or_init(|| {
        Regex::new(r"((AWS|OPENAI|ANTHROPIC)_[A-Z0-9_]+|[A-Z][A-Z0-9_]*_(TOKEN|KEY|SECRET))=\S+")
            .expect("redact env re")
    });

    let mut s = input.to_string();
    // Order matters: namespaced Win path first (most specific), then
    // backslash, then forward-slash, then unix.
    s = users_win_ns
        .replace_all(&s, r"\\?\C:\Users\<redacted>")
        .to_string();
    s = users_win
        .replace_all(&s, r"C:\Users\<redacted>")
        .to_string();
    s = users_win_fwd
        .replace_all(&s, "C:/Users/<redacted>")
        .to_string();
    s = users_unix.replace_all(&s, "/Users/<redacted>").to_string();

    s = env_token
        .replace_all(&s, |caps: &regex::Captures| {
            let full = caps.get(0).unwrap().as_str();
            if let Some(eq_pos) = full.find('=') {
                format!("{}=<redacted>", &full[..eq_pos])
            } else {
                full.to_string()
            }
        })
        .to_string();

    // Lowest-precedence home replacement — runtime home may not match
    // /Users/... on non-standard layouts.
    if let Some(home) = home_dir() {
        let home_str = home.to_string_lossy();
        if !home_str.is_empty() && s.contains(home_str.as_ref()) {
            s = s.replace(home_str.as_ref(), "<HOME>");
        }
    }

    s
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Apply [`redact`] to every `String`-typed field on a `CrashFile`. Run
/// before serialization so the on-disk file is already clean.
pub fn redact_crash_file(file: &mut CrashFile) {
    file.app_version = redact(&file.app_version);
    file.os = redact(&file.os);
    file.rust_panic.thread_name = redact(&file.rust_panic.thread_name);
    file.rust_panic.message = redact(&file.rust_panic.message);
    file.rust_panic.location = redact(&file.rust_panic.location);
    file.rust_panic.backtrace = redact(&file.rust_panic.backtrace);
    if let Some(rs) = file.recording_state.as_mut() {
        rs.status_message_at_crash = redact(&rs.status_message_at_crash);
        if let Some(s) = rs.model_kind.as_mut() {
            *s = redact(s);
        }
        if let Some(s) = rs.device_id_hash.as_mut() {
            *s = redact(s);
        }
    }
    for ev in file.events.iter_mut() {
        ev.kind = redact(&ev.kind);
        redact_value_strings(&mut ev.data);
    }
}

fn redact_value_strings(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::String(s) => *s = redact(s),
        Value::Array(arr) => {
            for item in arr {
                redact_value_strings(item);
            }
        }
        Value::Object(map) => {
            for (_, val) in map {
                redact_value_strings(val);
            }
        }
        _ => {}
    }
}

/// Write a (caller-redacted) crash file to `<dir>/<id>.json`. Returns
/// the absolute path. Does NOT redact for the caller — caller decides
/// when redaction runs (the panic hook applies it before serialization;
/// external test code may want raw input/output).
pub fn write_crash_file(file: &CrashFile, dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", file.id));
    let f = fs::File::create(&path)?;
    serde_json::to_writer_pretty(f, file).map_err(io::Error::other)?;
    Ok(path)
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn os_descriptor() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    // Free-form per spec. Fine-grained OS version (macOS 15.4, Win 11) needs
    // platform-specific probes; not yet wired.
    format!("{os} ({arch})")
}

/// Install the global panic hook. Idempotent — first install wins; later
/// calls are no-ops. The closure captures `dir` and `app_version` by
/// move, so swapping at runtime would surprise live captures anyway.
pub fn install_panic_hook(dir: PathBuf, app_version: String) {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort: never panic inside the hook. Swallow any internal
        // failure so the previous hook still runs and Rust's default
        // stderr message reaches the user.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_panic_to_dir(&dir, &app_version, info);
        }));
        prev(info);
    }));
}

fn write_panic_to_dir(dir: &Path, app_version: &str, info: &std::panic::PanicHookInfo<'_>) {
    let id = now_unix_ms();
    let thread_name = std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_string();
    let message = panic_message(info);
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = Backtrace::force_capture().to_string();

    let recording_state = dictation::try_snapshot_for_crash();
    let events = event_buffer::drain();

    let mut file = CrashFile {
        schema_version: SCHEMA_VERSION,
        id: id.to_string(),
        ts_unix_ms: id,
        app_version: app_version.to_string(),
        os: os_descriptor(),
        rust_panic: RustPanic {
            thread_name,
            message,
            location,
            backtrace,
        },
        recording_state,
        events,
    };

    redact_crash_file(&mut file);
    let _ = write_crash_file(&file, dir);
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_unix_user_path() {
        let out = redact("/Users/jimmijoensson/secret/file.txt");
        assert_eq!(out, "/Users/<redacted>/secret/file.txt");
    }

    #[test]
    fn redact_strips_windows_user_path() {
        let out = redact(r"C:\Users\Bob\Desktop\notes.txt");
        assert_eq!(out, r"C:\Users\<redacted>\Desktop\notes.txt");
    }

    #[test]
    fn redact_strips_windows_forwardslash_user_path() {
        let out = redact("C:/Users/Bob/Desktop/notes.txt");
        assert_eq!(out, "C:/Users/<redacted>/Desktop/notes.txt");
    }

    #[test]
    fn redact_strips_windows_namespaced_path() {
        let out = redact(r"\\?\C:\Users\Bob\file.txt");
        assert_eq!(out, r"\\?\C:\Users\<redacted>\file.txt");
    }

    #[test]
    fn redact_strips_env_token_assignments() {
        let out = redact("panic at OPENAI_API_KEY=sk-deadbeef inside backtrace");
        assert!(out.contains("OPENAI_API_KEY=<redacted>"));
        assert!(!out.contains("sk-deadbeef"));
    }

    #[test]
    fn redact_strips_aws_token_assignments() {
        let out = redact("AWS_ACCESS_KEY_ID=AKIA1234567890ABCDEF leaked");
        assert!(out.contains("AWS_ACCESS_KEY_ID=<redacted>"));
        assert!(!out.contains("AKIA1234567890ABCDEF"));
    }

    #[test]
    fn redact_strips_generic_token_suffix() {
        let out = redact("GH_PERSONAL_TOKEN=ghp_abc123 used");
        assert!(out.contains("GH_PERSONAL_TOKEN=<redacted>"));
        assert!(!out.contains("ghp_abc123"));
    }

    #[test]
    fn redact_preserves_backtrace_symbol_names() {
        let input = "openwhisper_core::audio::process_chunk at audio.rs:412";
        assert_eq!(redact(input), input);
    }

    #[test]
    fn redact_strips_runtime_home_when_set() {
        unsafe {
            std::env::set_var("HOME", "/var/myhome");
        }
        let out = redact("crash inside /var/myhome/secret");
        assert_eq!(out, "crash inside <HOME>/secret");
    }

    #[test]
    fn round_trip_through_serde() {
        let file = CrashFile {
            schema_version: SCHEMA_VERSION,
            id: "1717503600123".into(),
            ts_unix_ms: 1717503600123,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "main".into(),
                message: "kaboom".into(),
                location: "core/src/audio.rs:412:17".into(),
                backtrace: "frame 1\nframe 2".into(),
            },
            recording_state: Some(RecordingStateSnapshot {
                status_message_at_crash: "transcribing on ANE…".into(),
                duration_ms: 18234,
                samples_captured: 291744,
                model_kind: Some("Parakeet".into()),
                device_id_hash: Some("sha256:abcd1234".into()),
            }),
            events: vec![Event {
                ts_unix_ms: 1717503600000,
                kind: "DictationStart".into(),
                data: serde_json::json!({"phase": "Recording"}),
            }],
        };
        let json = serde_json::to_string(&file).expect("serialize");
        let back: CrashFile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, file);
    }

    #[test]
    fn unknown_top_level_fields_are_ignored() {
        let json = r#"{
            "schema_version": 1,
            "id": "1",
            "ts_unix_ms": 1,
            "app_version": "0.6.0",
            "os": "macos (arm64)",
            "rust_panic": {
                "thread_name": "main",
                "message": "kaboom",
                "location": "x.rs:1:1",
                "backtrace": ""
            },
            "future_field_we_have_not_invented_yet": 42
        }"#;
        let parsed: CrashFile = serde_json::from_str(json).expect("forward-compat");
        assert_eq!(parsed.schema_version, 1);
        assert!(parsed.recording_state.is_none());
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn redact_crash_file_scrubs_every_string_field() {
        let mut file = CrashFile {
            schema_version: SCHEMA_VERSION,
            id: "1".into(),
            ts_unix_ms: 1,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "main".into(),
                message: "panic at /Users/jimmijoensson/secret".into(),
                location: "/Users/jimmijoensson/repo/audio.rs:1:1".into(),
                backtrace: "OPENAI_API_KEY=sk-abc inside frame".into(),
            },
            recording_state: Some(RecordingStateSnapshot {
                status_message_at_crash: "loading /Users/Alice/model".into(),
                duration_ms: 100,
                samples_captured: 16000,
                model_kind: Some("Parakeet".into()),
                device_id_hash: Some("sha256:abc".into()),
            }),
            events: vec![Event {
                ts_unix_ms: 0,
                kind: "Error".into(),
                data: serde_json::json!({
                    "msg": "failed at /Users/Bob/file",
                    "nested": ["AWS_SECRET_ACCESS_KEY=hunter2"]
                }),
            }],
        };
        redact_crash_file(&mut file);

        assert!(file.rust_panic.message.contains("<redacted>"));
        assert!(!file.rust_panic.message.contains("jimmijoensson"));
        assert!(file.rust_panic.location.contains("<redacted>"));
        assert!(file.rust_panic.backtrace.contains("OPENAI_API_KEY=<redacted>"));
        assert!(!file.rust_panic.backtrace.contains("sk-abc"));
        let rs = file.recording_state.as_ref().unwrap();
        assert!(rs.status_message_at_crash.contains("<redacted>"));
        let event_data = file.events[0].data.to_string();
        assert!(event_data.contains("<redacted>"));
        assert!(!event_data.contains("Bob"));
        assert!(!event_data.contains("hunter2"));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = CrashFile {
            schema_version: SCHEMA_VERSION,
            id: "9999999999999".into(),
            ts_unix_ms: 9999999999999,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "main".into(),
                message: "kaboom at /Users/jimmijoensson/secret".into(),
                location: "audio.rs:1:1".into(),
                backtrace: "frame 1\nframe 2".into(),
            },
            recording_state: None,
            events: vec![],
        };
        redact_crash_file(&mut file);
        assert!(file.rust_panic.message.contains("<redacted>"));

        let path = write_crash_file(&file, dir.path()).expect("write");
        let raw = std::fs::read_to_string(&path).unwrap();
        let back: CrashFile = serde_json::from_str(&raw).expect("re-read");
        assert_eq!(back, file);
    }

    #[test]
    fn os_descriptor_is_nonempty() {
        let s = os_descriptor();
        assert!(s.contains('('));
        assert!(s.contains(')'));
    }
}
