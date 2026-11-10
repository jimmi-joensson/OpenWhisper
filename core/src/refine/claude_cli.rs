//! Refine backend: Claude, through the Claude Code CLI the user
//! installed and signed in themselves.
//!
//! This app holds no Claude login and no API key for this backend. It
//! spawns the user's `claude` binary in headless mode and talks to it
//! over stdin/stdout; the CLI uses whatever the user signed it in
//! with, normally their Claude subscription. Every dictation therefore
//! draws on the same plan as the user's own Claude Code use, which is
//! why the default model is a small one and short utterances are
//! skipped upstream.
//!
//! ## One process per dictation, started early
//!
//! Measured with Claude Code 2.1.322 and Haiku (2026-11-10):
//!
//! | how the CLI is run                         | time to answer |
//! |--------------------------------------------|----------------|
//! | default flags (hooks, skills, MCP, tools)  | 6.6 to 9.4 s   |
//! | lean flags, spawned when the text is ready | 1.4 to 2.8 s   |
//! | lean flags, spawned at record start        | 0.7 to 0.8 s   |
//!
//! So [`ClaudeCli::warm`] spawns the process when recording starts and
//! [`ClaudeCli::refine`] only has to write the transcript to its
//! stdin. An idle warmed process makes no model call, so a cancelled
//! recording costs nothing against the plan. The process is used for
//! exactly one transcript and then killed: no conversation, and so no
//! earlier dictation, is ever shared between two requests.
//!
//! ## The lean flags
//!
//! See [`spawn_args`]. With default flags the CLI loads the user's
//! whole coding setup (tens of thousands of tokens of instructions and
//! tools) and readily treats a dictated "move the meeting to Thursday"
//! as a task. `--safe-mode`, `--tools ""` and a replaced system prompt
//! turn it into a plain text-in, text-out call. `--bare` is not an
//! option: it never reads the subscription login.
//!
//! ## Windows
//!
//! Same flow. Differences handled here: the binary is `claude.exe`
//! (native installer) or a `claude.cmd` shim (npm), with the `.exe`
//! preferred; processes are started without a console window; the
//! system prompt is handed over in a temp file instead of an argument
//! (see [`spawn_args`]), because a `.cmd` shim goes through `cmd.exe`,
//! which cannot carry quotes and newlines safely and caps the command
//! line at 8191 characters.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    BackendState, BackendStatus, CallUsage, PlanWindows, Prompt, RefineError, Refiner,
};
use crate::settings::{ClaudeCliSettings, RefineEffort};

/// Backend id.
pub const ID: &str = "claude-cli";

/// Where the text goes with this backend. Shown in settings.
pub const DESTINATION: &str =
    "Anthropic (Claude), through the Claude Code app signed in on this computer";

const NOT_INSTALLED: &str = "Claude Code is not installed on this computer. Install it from \
claude.com/claude-code, run `claude` once in a terminal and sign in.";
const NOT_SIGNED_IN: &str =
    "Claude Code is installed but not signed in. Run `claude` in a terminal and sign in.";

/// How long the `claude auth status` / `--version` probes may take.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The Claude Code CLI as a [`Refiner`].
pub struct ClaudeCli {
    settings: ClaudeCliSettings,
    warm: Option<Process>,
    /// How long before the last request its process was started, when
    /// that process had been warmed. For diagnostics.
    last_warm_lead: Option<Duration>,
    /// What the last answered request used.
    last_usage: Option<CallUsage>,
}

impl ClaudeCli {
    pub fn new(settings: ClaudeCliSettings) -> Self {
        Self {
            settings,
            warm: None,
            last_warm_lead: None,
            last_usage: None,
        }
    }

    fn spawn(&self, prompt: &Prompt) -> Result<Process, RefineError> {
        let bin = find_binary(&self.settings)
            .ok_or_else(|| RefineError::Unavailable(NOT_INSTALLED.to_string()))?;
        Process::spawn(
            &bin,
            &self.settings.model,
            self.settings.effort,
            prompt.system(),
        )
    }
}

impl Refiner for ClaudeCli {
    fn id(&self) -> &'static str {
        ID
    }

    fn warm(&mut self, prompt: &Prompt) {
        // A failed spawn is reported by `refine`, which tries again.
        self.warm = self.spawn(prompt).ok();
    }

    fn warm_lead(&self) -> Option<Duration> {
        self.last_warm_lead
    }

    fn last_usage(&self) -> Option<CallUsage> {
        self.last_usage.clone()
    }

    fn refine(
        &mut self,
        prompt: &Prompt,
        transcript: &str,
        timeout: Duration,
    ) -> Result<String, RefineError> {
        let started = Instant::now();
        self.last_warm_lead = None;
        self.last_usage = None;
        let process = match self.warm.take() {
            // The system prompt is fixed at spawn. If it changed since
            // (dictionary edited mid-recording), start over.
            Some(p) if p.system == prompt.system() => {
                self.last_warm_lead = Some(p.spawned_at.elapsed());
                p
            }
            _ => self.spawn(prompt)?,
        };
        let left = timeout.saturating_sub(started.elapsed());
        let (answer, usage) = process.ask(&prompt.user_message(transcript), left);
        self.last_usage = usage;
        answer
    }
}

// --- finding the binary -------------------------------------------

/// Resolve the `claude` binary: the settings override when set, then
/// the usual install locations, then `PATH`. A desktop app started
/// from Finder or the Start menu has a minimal `PATH`, so the install
/// locations are checked explicitly.
pub fn find_binary(settings: &ClaudeCliSettings) -> Option<PathBuf> {
    if let Some(custom) = settings.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        let path = PathBuf::from(custom);
        return path.is_file().then_some(path);
    }
    candidates().into_iter().find(|p| p.is_file())
}

fn candidates() -> Vec<PathBuf> {
    candidate_paths(
        home_dir().as_deref(),
        std::env::var_os("PATH").as_deref(),
        cfg!(windows),
    )
}

/// Where to look, in order. `windows` picks the file names and the
/// per-OS install folders; a parameter (not `cfg!`) so both layouts
/// are unit-tested on every platform.
fn candidate_paths(home: Option<&Path>, path_var: Option<&OsStr>, windows: bool) -> Vec<PathBuf> {
    // `.exe` before `.cmd`: a real executable takes arguments as
    // given and is killed directly, a shim goes through `cmd.exe`.
    let names: &[&str] = if windows {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = home {
        // Native installer (all platforms).
        dirs.push(home.join(".local").join("bin"));
        // Older local npm install.
        dirs.push(home.join(".claude").join("local"));
        // Global npm install.
        if windows {
            dirs.push(home.join("AppData").join("Roaming").join("npm"));
        } else {
            dirs.push(home.join(".npm-global").join("bin"));
        }
    }
    if !windows {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    if let Some(path) = path_var {
        dirs.extend(std::env::split_paths(path));
    }
    let mut out = Vec::with_capacity(dirs.len() * names.len());
    for name in names {
        for dir in &dirs {
            let candidate = dir.join(name);
            if !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

// --- status --------------------------------------------------------

/// Is the CLI installed and signed in? Runs `claude auth status` and
/// `claude --version`; makes no model call.
pub fn status(settings: &ClaudeCliSettings) -> BackendStatus {
    let mut status = BackendStatus {
        backend: ID.to_string(),
        state: BackendState::NotInstalled,
        ready: false,
        message: NOT_INSTALLED.to_string(),
        destination: DESTINATION.to_string(),
        path: None,
        version: None,
        auth_method: None,
        plan: None,
        has_api_key: false,
    };
    let Some(bin) = find_binary(settings) else {
        if let Some(custom) = settings.path.as_deref().filter(|p| !p.trim().is_empty()) {
            status.message = format!("No Claude Code binary at {custom}. Check the path in settings.");
        }
        return status;
    };
    status.path = Some(bin.display().to_string());
    status.version = run_probe(&bin, &["--version"])
        .ok()
        .map(|out| out.trim().to_string())
        .filter(|v| !v.is_empty());

    match run_probe(&bin, &["auth", "status", "--json"]) {
        Ok(out) => apply_auth_status(&mut status, &out),
        Err(e) => {
            status.state = BackendState::Unknown;
            status.message = format!("Could not check whether Claude Code is signed in: {e}");
        }
    }
    status
}

/// Fill `status` from the JSON `claude auth status --json` prints.
fn apply_auth_status(status: &mut BackendStatus, json: &str) {
    let Ok(v) = serde_json::from_str::<Value>(json.trim()) else {
        status.state = BackendState::Unknown;
        status.message =
            "Could not read the sign-in state from Claude Code. Is it up to date?".to_string();
        return;
    };
    status.auth_method = v["authMethod"].as_str().map(str::to_string);
    status.plan = v["subscriptionType"].as_str().map(str::to_string);
    if v["loggedIn"].as_bool() == Some(true) {
        status.state = BackendState::Ready;
        status.ready = true;
        status.message = match (status.auth_method.as_deref(), status.plan.as_deref()) {
            (Some("claude.ai"), Some(plan)) => {
                format!("Claude Code is signed in with your Claude subscription ({plan}).")
            }
            (Some("claude.ai"), None) => {
                "Claude Code is signed in with your Claude subscription.".to_string()
            }
            (Some(other), _) => format!(
                "Claude Code is signed in ({other}). Refine is billed the way that sign-in is."
            ),
            (None, _) => "Claude Code is signed in.".to_string(),
        };
    } else {
        status.state = BackendState::NotSignedIn;
        status.message = NOT_SIGNED_IN.to_string();
    }
}

/// Run `bin args…` and return stdout, killing it after
/// [`PROBE_TIMEOUT`].
fn run_probe(bin: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = command(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", bin.display()))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let out = rx.recv_timeout(PROBE_TIMEOUT);
    let _ = child.kill();
    let _ = child.wait();
    out.map_err(|_| "Claude Code did not answer".to_string())
}

// --- the process ---------------------------------------------------

/// Arguments for one headless, text-in/text-out run. Verified against
/// Claude Code 2.1.322.
///
/// - `-p` + stream-json in/out: headless, and the transcript can be
///   written to stdin after the process is up (`--verbose` is required
///   by stream-json output).
/// - `--system-prompt-file`: replaces the coding-agent prompt
///   entirely. A file rather than `--system-prompt <text>` so the
///   prompt (quotes, newlines, a long dictionary) never has to survive
///   a command line; see the module docs on Windows.
/// - `--safe-mode`: no CLAUDE.md, skills, plugins, hooks or MCP
///   servers from the user's setup; sign-in still works.
/// - `--tools ""`, `--strict-mcp-config`, `--disable-slash-commands`:
///   the model can do nothing but answer with text.
/// - `--no-session-persistence`: the dictation is not written to the
///   CLI's session history on disk.
/// - `--effort`: how much the model thinks first. Low answers
///   fastest; the setting is the user's (default medium).
pub fn spawn_args(model: &str, effort: RefineEffort, system_prompt_file: &Path) -> Vec<String> {
    let file = system_prompt_file.display().to_string();
    [
        "-p",
        "--model",
        model,
        "--effort",
        effort.as_str(),
        "--safe-mode",
        "--tools",
        "",
        "--strict-mcp-config",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--system-prompt-file",
        file.as_str(),
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// Write the system prompt to a private temp file for
/// `--system-prompt-file`. It holds the cleanup instructions and the
/// dictionary terms, never a transcript. Removed when the process is
/// dropped.
fn write_prompt_file(system: &str) -> Result<PathBuf, RefineError> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "openwhisper-refine-{}-{}.txt",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let path = std::env::temp_dir().join(name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .and_then(|mut f| f.write_all(system.as_bytes()))
        .map_err(|e| RefineError::Backend(format!("Could not write the Refine prompt file: {e}")))?;
    Ok(path)
}

fn command(bin: &Path) -> Command {
    let mut cmd = Command::new(bin);
    // A neutral directory: nothing project-specific to pick up.
    cmd.current_dir(std::env::temp_dir());
    // An npm-installed `claude` is a script that needs `node`. A
    // desktop app started from Finder has a bare PATH, so add the
    // binary's own folder and the usual tool folders.
    if let Some(path) = child_path(bin, std::env::var_os("PATH").as_deref(), cfg!(windows)) {
        cmd.env("PATH", path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashing up per dictation.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// `PATH` for the child: the inherited one plus the folder `bin`
/// lives in and, on Unix, the Homebrew and `/usr/local` tool folders.
fn child_path(bin: &Path, inherited: Option<&OsStr>, windows: bool) -> Option<std::ffi::OsString> {
    let mut dirs: Vec<PathBuf> = inherited.map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    let mut extra: Vec<PathBuf> = Vec::new();
    if let Some(dir) = bin.parent().filter(|d| !d.as_os_str().is_empty()) {
        extra.push(dir.to_path_buf());
    }
    if !windows {
        extra.push(PathBuf::from("/opt/homebrew/bin"));
        extra.push(PathBuf::from("/usr/local/bin"));
    }
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).ok()
}

/// One spawned CLI process, good for one request.
struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    /// The single result the reader thread sends.
    rx: Receiver<Result<String, RefineError>>,
    /// Filled by the reader thread before it sends the result.
    usage: Arc<Mutex<Option<CallUsage>>>,
    /// The system prompt this process was started with.
    system: String,
    /// Temp file holding `system`; deleted on drop.
    prompt_file: PathBuf,
    spawned_at: Instant,
}

impl Process {
    fn spawn(
        bin: &Path,
        model: &str,
        effort: RefineEffort,
        system: &str,
    ) -> Result<Self, RefineError> {
        let prompt_file = write_prompt_file(system)?;
        let mut cmd = command(bin);
        cmd.args(spawn_args(model, effort, &prompt_file))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let _ = fs::remove_file(&prompt_file);
                return Err(RefineError::Unavailable(format!(
                    "Could not start Claude Code ({}): {e}",
                    bin.display()
                )));
            }
        };
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Keep the tail of stderr for error messages.
        let errors = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = stderr {
            let errors = errors.clone();
            thread::spawn(move || {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                if let Ok(mut e) = errors.lock() {
                    *e = buf;
                }
            });
        }

        let (tx, rx) = mpsc::channel();
        let usage = Arc::new(Mutex::new(None));
        if let Some(stdout) = stdout {
            let usage = usage.clone();
            thread::spawn(move || {
                let mut plan: Option<PlanWindows> = None;
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    // The plan windows arrive in their own event,
                    // shortly before the result.
                    if let Some(windows) = parse_rate_limit_line(&line) {
                        plan = Some(windows);
                        continue;
                    }
                    if let Some(result) = parse_result_line(&line) {
                        if let Some(mut call) = parse_usage(&line) {
                            call.plan = plan.take();
                            if let Ok(mut slot) = usage.lock() {
                                *slot = Some(call);
                            }
                        }
                        let _ = tx.send(result);
                        return;
                    }
                }
                // The process ended without a result line. Give the
                // stderr reader a moment, then report what it said.
                thread::sleep(Duration::from_millis(100));
                let said = errors.lock().map(|e| e.clone()).unwrap_or_default();
                let _ = tx.send(Err(exit_error(&said)));
            });
        }

        Ok(Self {
            child,
            stdin,
            rx,
            usage,
            system: system.to_string(),
            prompt_file,
            spawned_at: Instant::now(),
        })
    }

    /// Write the one user message, close stdin, wait for the result.
    /// Consumes the process: it is killed when this returns.
    fn ask(
        mut self,
        user_message: &str,
        timeout: Duration,
    ) -> (Result<String, RefineError>, Option<CallUsage>) {
        let answer = self.ask_inner(user_message, timeout);
        let usage = self.usage.lock().ok().and_then(|mut slot| slot.take());
        (answer, usage)
        // `self` drops here and kills the process.
    }

    fn ask_inner(&mut self, user_message: &str, timeout: Duration) -> Result<String, RefineError> {
        let line = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": user_message }],
            },
        })
        .to_string();
        let mut stdin = self
            .stdin
            .take()
            .ok_or_else(|| RefineError::Backend("Claude Code has no input channel".into()))?;
        let written = stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush());
        // Closing stdin tells the CLI this was the only message.
        drop(stdin);
        if written.is_err() {
            // The process is already gone; its reader thread knows why.
            return match self.rx.recv_timeout(Duration::from_millis(500)) {
                Ok(result) => result,
                Err(_) => Err(RefineError::Backend("Claude Code exited before it was asked".into())),
            };
        }
        match self.rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(RefineError::Timeout),
            Err(RecvTimeoutError::Disconnected) => {
                Err(RefineError::Backend("Claude Code stopped without an answer".into()))
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Close stdin first: a CLI behind a `claude.cmd` shim is a
        // grandchild that `kill` does not reach, but it exits on its
        // own once its input is closed.
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.prompt_file);
    }
}

/// `Some` when `line` is the CLI's final `result` event.
fn parse_result_line(line: &str) -> Option<Result<String, RefineError>> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v["type"].as_str() != Some("result") {
        return None;
    }
    let text = v["result"].as_str().unwrap_or("").to_string();
    let failed = v["is_error"].as_bool().unwrap_or(false)
        || v["subtype"].as_str().is_some_and(|s| s != "success");
    if !failed {
        return Some(Ok(text));
    }
    let message = if text.trim().is_empty() {
        v["subtype"].as_str().unwrap_or("unknown error").to_string()
    } else {
        text
    };
    Some(Err(classify_failure(&message)))
}

/// Tokens and list-price cost from the CLI's `result` event.
fn parse_usage(result_line: &str) -> Option<CallUsage> {
    let v: Value = serde_json::from_str(result_line.trim()).ok()?;
    let usage = v.get("usage")?;
    let n = |key: &str| usage[key].as_u64().unwrap_or(0);
    let cached = n("cache_read_input_tokens");
    Some(CallUsage {
        tokens_in: n("input_tokens") + n("cache_creation_input_tokens") + cached,
        tokens_cached: cached,
        tokens_out: n("output_tokens"),
        cost_usd: v["total_cost_usd"].as_f64(),
        plan: None,
    })
}

/// The plan's usage windows from the CLI's `rate_limit_event`. Only
/// present when the CLI is signed in with a subscription.
fn parse_rate_limit_line(line: &str) -> Option<PlanWindows> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v["type"].as_str() != Some("rate_limit_event") {
        return None;
    }
    let windows = &v["rate_limit_info"]["unifiedWindows"];
    Some(PlanWindows {
        five_hour_used: windows["five_hour"]["utilization"].as_f64(),
        five_hour_resets_at: windows["five_hour"]["resetsAt"].as_i64(),
        seven_day_used: windows["seven_day"]["utilization"].as_f64(),
        seven_day_resets_at: windows["seven_day"]["resetsAt"].as_i64(),
    })
}

/// Turn an error text from the CLI into the right [`RefineError`].
fn classify_failure(message: &str) -> RefineError {
    let lower = message.to_lowercase();
    let auth = ["/login", "not logged in", "log in", "sign in", "authentication", "invalid api key", "oauth"]
        .iter()
        .any(|needle| lower.contains(needle));
    if auth {
        RefineError::Unavailable(NOT_SIGNED_IN.to_string())
    } else {
        RefineError::Backend(format!("Claude Code: {}", one_line(message)))
    }
}

fn exit_error(stderr: &str) -> RefineError {
    let said = one_line(stderr);
    if said.is_empty() {
        RefineError::Backend("Claude Code exited without an answer".into())
    } else {
        classify_failure(&said)
    }
}

/// First non-empty line, capped, for a status message.
fn one_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(200).collect();
    if line.chars().count() > 200 {
        out.push('…');
    }
    out
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `claude`: a shell script standing in for the real CLI.
    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        ClaudeCliSettings {
            model: "haiku".into(),
            path: Some(path.display().to_string()),
            ..ClaudeCliSettings::default()
        }
    }

    /// Reads the one stdin line, then prints a successful result that
    /// echoes a marker, so tests can see the request arrived.
    const ECHO_OK: &str = r#"
if [ "$1" = "auth" ]; then echo '{"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "max"}'; exit 0; fi
if [ "$1" = "--version" ]; then echo '9.9.9 (Fake Claude)'; exit 0; fi
echo "$@" > "$(dirname "$0")/args.txt"
prev=""; for a in "$@"; do if [ "$prev" = "--system-prompt-file" ]; then cp "$a" "$(dirname "$0")/system.txt"; fi; prev="$a"; done
read -r line
printf '%s\n' "$line" > "$(dirname "$0")/stdin.txt"
echo '{"type":"system","subtype":"init"}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Hello there."}]}}'
echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour","unifiedWindows":{"five_hour":{"utilization":0.09,"resetsAt":1794325200,"status":"allowed"},"seven_day":{"utilization":0.21,"resetsAt":1794646800,"status":"allowed"}}}}'
echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello there.","total_cost_usd":0.0025,"usage":{"input_tokens":2,"cache_creation_input_tokens":560,"cache_read_input_tokens":40,"output_tokens":11}}'
"#;

    fn prompt() -> Prompt {
        Prompt::compose("", &["PayProff".to_string()])
    }

    #[test]
    fn lean_flags_are_all_present() {
        let args = spawn_args("haiku", RefineEffort::Low, Path::new("/tmp/prompt.txt"));
        let joined = args.join(" ");
        for flag in [
            "-p",
            "--safe-mode",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--input-format stream-json",
            "--output-format stream-json",
            "--verbose",
            "--model haiku",
            "--effort low",
        ] {
            assert!(joined.contains(flag), "missing {flag} in {joined}");
        }
        // `--tools ""`: an empty tool list.
        let tools = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[tools + 1], "");
        // `--bare` would skip the subscription login.
        assert!(!args.iter().any(|a| a == "--bare"));
        // The prompt travels in a file, never as an argument.
        assert!(!args.iter().any(|a| a == "--system-prompt"));
        let sys = args.iter().position(|a| a == "--system-prompt-file").unwrap();
        assert_eq!(args[sys + 1], "/tmp/prompt.txt");
    }

    #[test]
    fn model_and_effort_come_from_settings() {
        let args = spawn_args("sonnet", RefineEffort::High, Path::new("/tmp/p.txt")).join(" ");
        assert!(args.contains("--model sonnet"), "{args}");
        assert!(args.contains("--effort high"), "{args}");
        let args = spawn_args("haiku", RefineEffort::Medium, Path::new("/tmp/p.txt")).join(" ");
        assert!(args.contains("--effort medium"), "{args}");
    }

    #[test]
    fn cold_call_sends_the_transcript_as_one_user_message() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), ECHO_OK);
        let mut cli = ClaudeCli::new(settings);
        let p = prompt();
        let out = cli
            .refine(&p, "hello there", Duration::from_secs(5))
            .unwrap();
        assert_eq!(out, "Hello there.");

        let sent: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("stdin.txt")).unwrap())
                .unwrap();
        assert_eq!(sent["type"], "user");
        assert_eq!(
            sent["message"]["content"][0]["text"],
            "<transcript>\nhello there\n</transcript>"
        );
        // The system prompt went in through the prompt file, not in
        // the message and not on the command line.
        let system = fs::read_to_string(dir.path().join("system.txt")).unwrap();
        assert_eq!(system, p.system());
        assert!(system.contains("PayProff"));
        let args = fs::read_to_string(dir.path().join("args.txt")).unwrap();
        assert!(!args.contains("never addressed to you"));
        // And the prompt file is cleaned up afterwards.
        let file = args
            .split_whitespace()
            .skip_while(|a| *a != "--system-prompt-file")
            .nth(1)
            .unwrap();
        assert!(!Path::new(file).exists(), "{file} was left behind");
    }

    #[test]
    fn reports_tokens_cost_and_plan_windows_of_the_call() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCli::new(fake_cli(dir.path(), ECHO_OK));
        assert_eq!(cli.last_usage(), None);
        cli.refine(&prompt(), "hello there", Duration::from_secs(5)).unwrap();
        let usage = cli.last_usage().unwrap();
        assert_eq!(usage.tokens_in, 602);
        assert_eq!(usage.tokens_cached, 40);
        assert_eq!(usage.tokens_out, 11);
        assert_eq!(usage.cost_usd, Some(0.0025));
        let plan = usage.plan.unwrap();
        assert_eq!(plan.five_hour_used, Some(0.09));
        assert_eq!(plan.five_hour_resets_at, Some(1794325200));
        assert_eq!(plan.seven_day_used, Some(0.21));
        assert_eq!(plan.seven_day_resets_at, Some(1794646800));
    }

    #[test]
    fn a_cli_without_usage_fields_still_answers() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":false,"result":"Hi."}'"#,
        );
        let mut cli = ClaudeCli::new(settings);
        assert_eq!(cli.refine(&prompt(), "hi there", Duration::from_secs(5)).unwrap(), "Hi.");
        assert_eq!(cli.last_usage(), None);
    }

    #[test]
    fn warm_process_answers_a_later_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCli::new(fake_cli(dir.path(), ECHO_OK));
        let p = prompt();
        cli.warm(&p);
        assert!(cli.warm.is_some());
        // The process is up and waiting on stdin while "recording".
        thread::sleep(Duration::from_millis(150));
        assert!(!dir.path().join("stdin.txt").exists());
        let out = cli.refine(&p, "hello there", Duration::from_secs(5)).unwrap();
        assert_eq!(out, "Hello there.");
        assert!(cli.warm.is_none());
        // Diagnostics can tell the process had a head start.
        assert!(cli.warm_lead().unwrap() >= Duration::from_millis(150));
        // A second request on the same backend runs cold.
        let _ = cli.refine(&p, "hello there", Duration::from_secs(5)).unwrap();
        assert_eq!(cli.warm_lead(), None);
    }

    #[test]
    fn dropping_a_warm_backend_kills_the_process() {
        let dir = tempfile::tempdir().unwrap();
        // Records its pid, then waits for input that never comes.
        let settings = fake_cli(
            dir.path(),
            r#"echo $$ > "$(dirname "$0")/pid.txt"; read -r line; sleep 30"#,
        );
        let mut cli = ClaudeCli::new(settings);
        cli.warm(&prompt());
        let pid_file = dir.path().join("pid.txt");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pid_file.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let pid = fs::read_to_string(&pid_file).unwrap().trim().to_string();
        drop(cli);
        // `kill -0` fails once the process is gone.
        let alive = Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "warm process {pid} survived the drop");
    }

    #[test]
    fn stale_warm_process_is_replaced_when_the_prompt_changed() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCli::new(fake_cli(dir.path(), ECHO_OK));
        cli.warm(&Prompt::compose("old instructions", &[]));
        let new = Prompt::compose("new instructions", &[]);
        let out = cli.refine(&new, "hello there", Duration::from_secs(5)).unwrap();
        assert_eq!(out, "Hello there.");
        let system = fs::read_to_string(dir.path().join("system.txt")).unwrap();
        assert!(system.contains("new instructions"), "{system}");
    }

    #[test]
    fn slow_cli_times_out_and_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "read -r line; sleep 30");
        let mut cli = ClaudeCli::new(settings);
        let started = Instant::now();
        let err = cli
            .refine(&prompt(), "hello there", Duration::from_millis(300))
            .unwrap_err();
        assert_eq!(err, RefineError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn error_result_is_a_backend_error() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded"}'"#,
        );
        let err = ClaudeCli::new(settings)
            .refine(&prompt(), "hello there", Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(
            err,
            RefineError::Backend("Claude Code: API Error: 529 overloaded".into())
        );
    }

    #[test]
    fn signed_out_result_is_unavailable_with_instructions() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"read -r line; echo '{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}'"#,
        );
        let err = ClaudeCli::new(settings)
            .refine(&prompt(), "hello there", Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(err, RefineError::Unavailable(NOT_SIGNED_IN.into()));
    }

    #[test]
    fn exit_without_result_reports_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "echo \"error: unknown option '--safe-mode'\" >&2; exit 1");
        let err = ClaudeCli::new(settings)
            .refine(&prompt(), "hello there", Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(
            err,
            RefineError::Backend("Claude Code: error: unknown option '--safe-mode'".into())
        );
    }

    #[test]
    fn missing_binary_is_unavailable() {
        let settings = ClaudeCliSettings {
            model: "haiku".into(),
            path: Some("/nonexistent/claude".into()),
            ..ClaudeCliSettings::default()
        };
        let err = ClaudeCli::new(settings.clone())
            .refine(&prompt(), "hello there", Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(err, RefineError::Unavailable(NOT_INSTALLED.into()));

        let s = status(&settings);
        assert_eq!(s.state, BackendState::NotInstalled);
        assert!(!s.ready);
        assert!(s.message.contains("/nonexistent/claude"));
    }

    #[test]
    fn status_reports_signed_in_subscription() {
        let dir = tempfile::tempdir().unwrap();
        let s = status(&fake_cli(dir.path(), ECHO_OK));
        assert_eq!(s.state, BackendState::Ready);
        assert!(s.ready);
        assert_eq!(s.version.as_deref(), Some("9.9.9 (Fake Claude)"));
        assert_eq!(s.auth_method.as_deref(), Some("claude.ai"));
        assert_eq!(s.plan.as_deref(), Some("max"));
        assert!(s.message.contains("Claude subscription (max)"));
        assert_eq!(s.destination, DESTINATION);
    }

    #[test]
    fn status_reports_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(
            dir.path(),
            r#"if [ "$1" = "auth" ]; then echo '{"loggedIn": false}'; exit 1; fi; echo 1.0.0"#,
        );
        let s = status(&settings);
        assert_eq!(s.state, BackendState::NotSignedIn);
        assert!(!s.ready);
        assert_eq!(s.message, NOT_SIGNED_IN);
    }

    #[test]
    fn status_with_unreadable_answer_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let s = status(&fake_cli(dir.path(), "echo 'not json'"));
        assert_eq!(s.state, BackendState::Unknown);
        assert!(!s.ready);
    }

    #[test]
    fn windows_candidates_prefer_exe_and_cover_npm() {
        let home = Path::new("C:/Users/me");
        let found = candidate_paths(Some(home), None, true);
        let names: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
        assert_eq!(
            names,
            vec![
                "C:/Users/me/.local/bin/claude.exe",
                "C:/Users/me/.claude/local/claude.exe",
                "C:/Users/me/AppData/Roaming/npm/claude.exe",
                "C:/Users/me/.local/bin/claude.cmd",
                "C:/Users/me/.claude/local/claude.cmd",
                "C:/Users/me/AppData/Roaming/npm/claude.cmd",
            ]
        );
    }

    #[test]
    fn unix_candidates_cover_installer_homebrew_and_path() {
        let found = candidate_paths(
            Some(Path::new("/Users/me")),
            Some(OsStr::new("/custom/bin:/usr/bin")),
            false,
        );
        let names: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
        assert_eq!(names[0], "/Users/me/.local/bin/claude");
        assert!(names.contains(&"/opt/homebrew/bin/claude".to_string()));
        assert!(names.contains(&"/usr/local/bin/claude".to_string()));
        assert!(names.contains(&"/custom/bin/claude".to_string()));
        assert!(!names.iter().any(|n| n.ends_with(".exe")));
    }

    #[test]
    fn child_path_adds_tool_folders_to_a_bare_path() {
        let path = child_path(
            Path::new("/Users/me/.npm-global/bin/claude"),
            Some(OsStr::new("/usr/bin:/bin")),
            false,
        )
        .unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/usr/bin:/bin:/Users/me/.npm-global/bin:/opt/homebrew/bin:/usr/local/bin"
        );
        // Nothing is duplicated when it is already there.
        let path = child_path(
            Path::new("/opt/homebrew/bin/claude"),
            Some(OsStr::new("/opt/homebrew/bin:/usr/local/bin")),
            false,
        )
        .unwrap();
        assert_eq!(path.to_str().unwrap(), "/opt/homebrew/bin:/usr/local/bin");
    }

    #[test]
    fn parses_only_the_result_event() {
        assert!(parse_result_line(r#"{"type":"system","subtype":"init"}"#).is_none());
        assert!(parse_result_line("not json").is_none());
        assert_eq!(
            parse_result_line(r#"{"type":"result","subtype":"success","is_error":false,"result":"Hi."}"#),
            Some(Ok("Hi.".into()))
        );
        assert!(matches!(
            parse_result_line(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#),
            Some(Err(RefineError::Backend(_)))
        ));
    }
}

/// The same checks against a fake `claude.cmd`, for the Windows box.
///
/// Written on macOS and compile-checked for `x86_64-pc-windows-gnu`,
/// but not yet run: the first `cargo test` on Windows is what verifies
/// spawning through a `.cmd` shim, the stdin hand-over, the timeout
/// and the kill.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    fn fake_cli(dir: &Path, body: &str) -> ClaudeCliSettings {
        let path = dir.join("claude.cmd");
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
        ClaudeCliSettings {
            model: "haiku".into(),
            path: Some(path.display().to_string()),
            ..ClaudeCliSettings::default()
        }
    }

    const ANSWER_OK: &str = "if \"%~1\"==\"auth\" goto auth\r\n\
if \"%~1\"==\"--version\" goto version\r\n\
set /p line=\r\n\
echo {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Hello there.\"}\r\n\
exit /b 0\r\n\
:auth\r\n\
echo {\"loggedIn\": true, \"authMethod\": \"claude.ai\", \"subscriptionType\": \"max\"}\r\n\
exit /b 0\r\n\
:version\r\n\
echo 9.9.9 Fake Claude\r\n\
exit /b 0";

    fn prompt() -> Prompt {
        Prompt::compose("", &["PayProff".to_string()])
    }

    #[test]
    fn cold_call_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let out = ClaudeCli::new(fake_cli(dir.path(), ANSWER_OK))
            .refine(&prompt(), "hello there", Duration::from_secs(10))
            .unwrap();
        assert_eq!(out, "Hello there.");
    }

    #[test]
    fn warm_then_ask_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let mut cli = ClaudeCli::new(fake_cli(dir.path(), ANSWER_OK));
        let p = prompt();
        cli.warm(&p);
        assert!(cli.warm.is_some());
        thread::sleep(Duration::from_millis(300));
        let out = cli.refine(&p, "hello there", Duration::from_secs(10)).unwrap();
        assert_eq!(out, "Hello there.");
    }

    #[test]
    fn slow_cmd_shim_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let settings = fake_cli(dir.path(), "set /p line=\r\nping -n 30 127.0.0.1 >nul");
        let started = Instant::now();
        let err = ClaudeCli::new(settings)
            .refine(&prompt(), "hello there", Duration::from_millis(500))
            .unwrap_err();
        assert_eq!(err, RefineError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn status_through_a_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let s = status(&fake_cli(dir.path(), ANSWER_OK));
        assert_eq!(s.state, BackendState::Ready);
        assert_eq!(s.plan.as_deref(), Some("max"));
        assert_eq!(s.version.as_deref(), Some("9.9.9 Fake Claude"));
    }

    #[test]
    fn missing_binary_is_not_installed() {
        let settings = ClaudeCliSettings {
            model: "haiku".into(),
            path: Some("C:\\nonexistent\\claude.exe".into()),
            ..ClaudeCliSettings::default()
        };
        assert_eq!(status(&settings).state, BackendState::NotInstalled);
    }
}
