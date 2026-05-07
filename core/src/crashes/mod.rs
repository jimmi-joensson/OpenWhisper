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

/// Reject ids that could escape the crash dir before joining them
/// into a path. Filenames are unix-ms strings; only ASCII digits
/// (and optional leading `-` for future-proofing) are allowed. Used
/// by every read path that takes an external id (CLI, Tauri command).
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() < 64
        && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

/// Errors surfaced from [`read_crash`]. Distinguishes "the file
/// you asked for isn't there" from "filesystem said no" so callers
/// can render the right message.
#[derive(Debug)]
pub enum ReadCrashError {
    UnsafeId(String),
    NotFound,
    Io(io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for ReadCrashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeId(id) => write!(f, "invalid crash id: {id}"),
            Self::NotFound => f.write_str("crash file not found"),
            Self::Io(e) => write!(f, "crash file io: {e}"),
            Self::Parse(e) => write!(f, "crash file parse: {e}"),
        }
    }
}

impl std::error::Error for ReadCrashError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

/// Read a single crash file by id. Validates the id before joining
/// it into a path so the function is safe to call with external
/// input (CLI flag, IPC command).
pub fn read_crash(dir: &Path, id: &str) -> Result<CrashFile, ReadCrashError> {
    if !is_safe_id(id) {
        return Err(ReadCrashError::UnsafeId(id.to_string()));
    }
    let path = dir.join(format!("{id}.json"));
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(ReadCrashError::NotFound)
        }
        Err(e) => return Err(ReadCrashError::Io(e)),
    };
    serde_json::from_str::<CrashFile>(&raw).map_err(ReadCrashError::Parse)
}

/// Enumerate crash files in `dir`, newest first. Files that fail to
/// parse are logged to stderr and skipped — they do NOT abort the
/// listing. Missing dir returns `Ok(empty)` so first-run consumers
/// don't have to special-case `NotFound`.
///
/// Single source of truth used by both the CLI's `crash-dump --list`
/// and the Tauri shell's `crashes_list` command. Per the
/// `openwhisper-headless-first` doctrine: list semantics live in the
/// library, both shells consume them.
pub fn list_crashes(dir: &Path) -> io::Result<Vec<CrashFile>> {
    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out: Vec<CrashFile> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_safe_id(stem) {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[crashes] read {}: {e}", path.display());
                continue;
            }
        };
        match serde_json::from_str::<CrashFile>(&raw) {
            Ok(file) => out.push(file),
            Err(e) => {
                eprintln!("[crashes] parse {}: {e}", path.display());
            }
        }
    }
    out.sort_by(|a, b| b.ts_unix_ms.cmp(&a.ts_unix_ms));
    Ok(out)
}

/// Format a crash file as a GitHub-ready markdown report. Pure
/// function; mirror of the TypeScript `formatCrashAsMarkdown` in
/// `apps/tauri/src/lib/crash-markdown.ts`. Two implementations exist
/// (one per language) because the React side renders Copy /
/// Report-on-GitHub flows synchronously without an IPC roundtrip,
/// and the CLI consumes this Rust version directly. If the shapes
/// diverge, the contract test in `tests/markdown_contract.rs` pins
/// both to the same expected output for a fixture crash.
///
/// On-disk crash files are already redacted at write time inside the
/// panic hook, so this formatter does NOT re-redact — its only
/// contract is "don't re-introduce un-redacted fields."
pub fn format_as_markdown(crash: &CrashFile) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("**OpenWhisper crash report**".into());
    lines.push(String::new());
    lines.push(format!("- Version: {}", crash.app_version));
    lines.push(format!("- OS: {}", crash.os));
    lines.push(format!("- When: {}", format_absolute_utc(crash.ts_unix_ms)));

    if let Some(rs) = crash.recording_state.as_ref() {
        let phase = rs.status_message_at_crash.trim();
        lines.push(format!(
            "- Phase at crash: {phase} ({} in)",
            format_duration(rs.duration_ms),
        ));
        if let Some(model) = rs.model_kind.as_ref() {
            lines.push(format!("- Model: {model}"));
        }
    } else {
        lines.push("- Phase at crash: idle (outside dictation)".into());
    }

    lines.push(String::new());
    lines.push("**What I was doing:**".into());
    lines.push(String::new());
    lines.push("> _replace this with a quick description before submitting_".into());
    lines.push(String::new());
    lines.push("<details>".into());
    lines.push("<summary>Backtrace (click to expand)</summary>".into());
    lines.push(String::new());
    lines.push("```".into());
    lines.push(crash.rust_panic.message.clone());
    lines.push(format!("   at {}", crash.rust_panic.location));
    lines.push(String::new());
    lines.push(crash.rust_panic.backtrace.clone());
    lines.push("```".into());
    lines.push(String::new());
    lines.push("</details>".into());

    if !crash.events.is_empty() {
        lines.push(String::new());
        lines.push("<details>".into());
        lines.push(format!(
            "<summary>Recent events ({})</summary>",
            crash.events.len(),
        ));
        lines.push(String::new());
        lines.push("| time | kind | data |".into());
        lines.push("| --- | --- | --- |".into());
        for ev in &crash.events {
            let time = format_time_of_day(ev.ts_unix_ms);
            let data_str = format_event_data(&ev.data);
            lines.push(format!("| {time} | {} | {data_str} |", ev.kind));
        }
        lines.push(String::new());
        lines.push("</details>".into());
    }

    lines.join("\n")
}

fn format_absolute_utc(ts_unix_ms: i64) -> String {
    let secs = ts_unix_ms.div_euclid(1000);
    let (year, month, day, hour, minute, second) = unix_secs_to_utc(secs);
    format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC",
    )
}

fn format_time_of_day(ts_unix_ms: i64) -> String {
    let secs = ts_unix_ms.div_euclid(1000);
    let (_, _, _, hour, minute, second) = unix_secs_to_utc(secs);
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let secs = ms as f64 / 1000.0;
    if secs < 60.0 {
        return format!("{secs:.1}s");
    }
    let minutes = (secs / 60.0).floor() as u64;
    let rem = (secs - (minutes as f64) * 60.0).round() as u64;
    format!("{minutes}m {rem}s")
}

fn format_event_data(data: &serde_json::Value) -> String {
    use serde_json::Value;
    let raw = match data {
        Value::Null => return String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    raw.replace('|', r"\|").replace('\n', " ").replace('\r', " ")
}

/// Civil date/time conversion from a UTC Unix timestamp (seconds).
/// Hand-rolled so core stays free of `chrono::Utc` (we only depend
/// on `chrono::Local` today) and the formatter stays a pure
/// no-allocation arithmetic function. Algorithm follows Howard
/// Hinnant's `civil_from_days` (public domain) — well-tested,
/// covers the full proleptic Gregorian range we care about.
fn unix_secs_to_utc(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day % 3600) / 60) as u32;
    let second = (secs_of_day % 60) as u32;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month, day, hour, minute, second)
}

const GH_TITLE_MESSAGE_MAX_CHARS: usize = 72;
const GH_BODY_BYTE_BUDGET: usize = 6_000;
const GH_TRUNCATION_MARKER: &str =
    "\n\n_Truncated — paste the full report from `Copy GitHub-ready report` if needed._";

/// Build a prefilled GitHub Issues URL for a crash. Mirror of the
/// TypeScript `buildGitHubIssueUrl` in
/// `apps/tauri/src/lib/crash-github.ts`. Body is truncated at the
/// 6 KB byte budget; the identity block is preserved in full and
/// the backtrace tail is trimmed instead.
pub fn build_github_issue_url(
    crash: &CrashFile,
    owner: &str,
    repo: &str,
) -> String {
    let title = build_github_title(crash);
    let body = build_github_body(crash);
    format!(
        "https://github.com/{owner}/{repo}/issues/new?title={}&body={}&labels={}",
        form_urlencode(&title),
        form_urlencode(&body),
        form_urlencode("bug,crash"),
    )
}

fn build_github_title(crash: &CrashFile) -> String {
    let first_line = crash
        .rust_panic
        .message
        .lines()
        .next()
        .unwrap_or("");
    let trimmed = if first_line.chars().count() <= GH_TITLE_MESSAGE_MAX_CHARS {
        first_line.to_string()
    } else {
        let mut out: String = first_line
            .chars()
            .take(GH_TITLE_MESSAGE_MAX_CHARS)
            .collect();
        out.push('…');
        out
    };
    format!("Crash report — v{} — {trimmed}", crash.app_version)
}

fn build_github_body(crash: &CrashFile) -> String {
    let full = format_as_markdown(crash);
    if full.len() <= GH_BODY_BYTE_BUDGET {
        return full;
    }
    let cap = GH_BODY_BYTE_BUDGET.saturating_sub(GH_TRUNCATION_MARKER.len());
    let truncated = slice_at_byte_boundary(&full, cap);
    format!("{truncated}{GH_TRUNCATION_MARKER}")
}

/// Slice a string at the largest UTF-8 char boundary `<= max_bytes`.
/// `truncate_unsafe` would split a multi-byte codepoint; this finds
/// the boundary by walking down from the cap.
fn slice_at_byte_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

/// Percent-encode a string for `application/x-www-form-urlencoded`
/// — same shape `URLSearchParams.toString()` produces in JS, so the
/// Rust + TS URL builders emit byte-identical URLs for identical
/// crash inputs (asserted by the contract test).
///
/// Rules: ALPHA / DIGIT / `*-._` pass through; space becomes `+`;
/// everything else is `%XX`-encoded.
fn form_urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        let b = *byte;
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Resolve the OS-default crash dir for the **release** bundle id
/// (`com.openwhisper.app`). Honors `OPENWHISPER_CRASH_DIR_OVERRIDE`
/// in debug + test builds so dev workflows can point at a temp dir.
///
/// Returns `None` when the OS isn't macOS or Windows, or when the
/// expected env var (`HOME` / `LOCALAPPDATA`) isn't set. Callers
/// (CLI, in-process integrations) can always pass an explicit dir
/// to bypass the resolver.
///
/// **Bundle id caveat:** dev Tauri builds write to
/// `com.openwhisper.dev/crashes/`, not `com.openwhisper.app`. The
/// CLI's `--dir` flag is the canonical override for inspecting
/// dev-build crashes.
pub fn default_crash_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        if let Ok(s) = std::env::var("OPENWHISPER_CRASH_DIR_OVERRIDE") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Logs/com.openwhisper.app/crashes"),
        )
    }
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(PathBuf::from(local).join("com.openwhisper.app/logs/crashes"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
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
        // home_dir() reads HOME on Unix, USERPROFILE on Windows.
        // Set the platform-correct var so this test exercises the
        // runtime-home substitution branch on both platforms.
        let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        unsafe {
            std::env::set_var(home_var, "/var/myhome");
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

    fn fixture(events: Vec<Event>, recording: Option<RecordingStateSnapshot>) -> CrashFile {
        CrashFile {
            schema_version: SCHEMA_VERSION,
            id: "1777905201000".into(),
            // Date.UTC(2026, 4, 4, 14, 33, 21) = 2026-05-04 14:33:21
            // UTC. Mirror of the TS test fixture so both formatters
            // can be eyeballed against the same wall-clock instant.
            ts_unix_ms: 1_777_905_201_000,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "tokio-runtime-worker".into(),
                message: "called `Result::unwrap()` on an `Err` value".into(),
                location: "core/src/audio.rs:412:17".into(),
                backtrace: "frame 1\nframe 2\nframe 3".into(),
            },
            recording_state: recording,
            events,
        }
    }

    #[test]
    fn format_as_markdown_full_shape() {
        let recording = RecordingStateSnapshot {
            status_message_at_crash: "transcribing on ANE…".into(),
            duration_ms: 18234,
            samples_captured: 291744,
            model_kind: Some("Parakeet".into()),
            device_id_hash: None,
        };
        let events = vec![Event {
            ts_unix_ms: 1_777_991_600_000,
            kind: "DictationStart".into(),
            data: serde_json::json!({}),
        }];
        let out = format_as_markdown(&fixture(events, Some(recording)));
        assert!(out.contains("**OpenWhisper crash report**"));
        assert!(out.contains("- Version: 0.6.0"));
        assert!(out.contains("- OS: macos (arm64)"));
        assert!(out.contains("- When: 2026-05-04 14:33:21 UTC"));
        assert!(out.contains("- Phase at crash: transcribing on ANE… (18.2s in)"));
        assert!(out.contains("- Model: Parakeet"));
        assert!(out.contains("Recent events (1)"));
        assert!(out.contains("| DictationStart | {} |"));
    }

    #[test]
    fn format_as_markdown_no_recording_state() {
        let out = format_as_markdown(&fixture(vec![], None));
        assert!(out.contains("- Phase at crash: idle (outside dictation)"));
        assert!(!out.contains("- Model:"));
    }

    #[test]
    fn format_as_markdown_empty_events_omits_section() {
        let out = format_as_markdown(&fixture(vec![], None));
        assert!(!out.contains("Recent events"));
    }

    #[test]
    fn format_as_markdown_escapes_pipes_and_newlines_in_event_data() {
        let events = vec![Event {
            ts_unix_ms: 1_777_991_601_000,
            kind: "Error".into(),
            data: serde_json::json!({ "msg": "a | b\nc" }),
        }];
        let out = format_as_markdown(&fixture(events, None));
        let row = out
            .lines()
            .find(|l| l.contains("Error"))
            .expect("event row");
        assert!(row.contains(r"\|"));
        assert!(!row.contains('\n'));
    }

    #[test]
    fn build_github_issue_url_short_body_intact() {
        let url =
            build_github_issue_url(&fixture(vec![], None), "jimmi-joensson", "OpenWhisper");
        assert!(url.starts_with(
            "https://github.com/jimmi-joensson/OpenWhisper/issues/new?title=",
        ));
        assert!(url.contains("labels=bug%2Ccrash"));
        assert!(url.contains("Crash+report"));
        // Encoded form of the panic message segment.
        assert!(url.contains("Result%3A%3Aunwrap"));
    }

    #[test]
    fn build_github_issue_url_truncates_when_over_budget() {
        let huge = "frame X\n".repeat(2_000);
        let mut crash = fixture(vec![], None);
        crash.rust_panic.backtrace = huge;
        let url = build_github_issue_url(&crash, "o", "r");
        // Truncation marker percent-encoded — the literal `%20` for
        // space, plus the leading underscore + word "Truncated".
        assert!(url.contains("Truncated"));
        // Whole URL stays bounded; the body is at most the budget +
        // marker length + percent-encoding overhead. 12 KB is a safe
        // ceiling that still fails on >2× regressions.
        assert!(url.len() < 12_000, "url bytes: {}", url.len());
    }

    #[test]
    fn build_github_title_truncates_long_panic_message() {
        let long_msg = "a".repeat(120);
        let mut crash = fixture(vec![], None);
        crash.rust_panic.message = long_msg;
        let url = build_github_issue_url(&crash, "o", "r");
        // The encoded title carries 72 `a`s and an ellipsis. Decode
        // by hand: the percent-encoded ellipsis is %E2%80%A6.
        assert!(url.contains(&"a".repeat(72)));
        assert!(url.contains("%E2%80%A6"));
    }

    #[test]
    fn form_urlencode_matches_url_search_params_shape() {
        // ALPHA/DIGIT/* - . _ pass through; space → '+'; ',' → %2C.
        assert_eq!(form_urlencode("hello"), "hello");
        assert_eq!(form_urlencode("hello world"), "hello+world");
        assert_eq!(form_urlencode("bug,crash"), "bug%2Ccrash");
        assert_eq!(form_urlencode("a-b.c_d*e"), "a-b.c_d*e");
        assert_eq!(form_urlencode("å"), "%C3%A5");
    }
}
