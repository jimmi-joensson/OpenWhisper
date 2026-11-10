//! Transcript history (TASK-95): the text of past dictations, kept on
//! this machine only, and only after the user switches it on
//! ([`crate::settings::HistorySettings`]).
//!
//! The store is a folder of Markdown files, one per day, so a person
//! or another program (a coding agent, a notes app) can read and
//! search it without OpenWhisper:
//!
//! ```text
//! history/
//!   2026/
//!     2026-11-10.md
//!     2026-11-11.md
//! ```
//!
//! ```markdown
//! ---
//! date: 2026-11-10
//! entries: 2
//! words: 19
//! apps: [Slack, iTerm2]
//! ---
//!
//! ## 14:02:12 · Slack
//! <!-- id: 20261110T140212 app: com.tinyspeck.slackmacgap words: 8 seconds: 4.2 -->
//!
//! Send the report to the team before Friday.
//!
//! > Before Refine: send the reaport to the team before friday
//!
//! ## 14:07:40 · iTerm2
//! <!-- id: 20261110T140740 app: com.googlecode.iterm2 words: 11 seconds: 6.0 -->
//! <!-- extra: mode=Prompt for a coding agent -->
//!
//! Rewrite the retry logic so it backs off after three failures.
//!
//! > Before Refine: so the retry logic it should back off after three failures
//! ```
//!
//! One `##` heading per dictation (time, then the app it went into),
//! a comment line with the machine fields, the text, and the text
//! before Refine as a quote when Refine changed it. The frontmatter
//! sums up the day.
//!
//! Anything else worth keeping about a dictation goes on a second
//! comment line directly under the first: `<!-- extra: key=value;
//! key=value -->`. Today that is `mode`, the name of the mode the
//! text was rewritten in (TASK-96), written only for a mode the user
//! made. The line is left out when there is nothing to say, so a
//! plain dictation looks exactly as it did before modes existed.
//!
//! The first comment line is never given new fields. An entry is
//! recognised by that line matching exactly, also by versions of the
//! app that are already out: a field they do not know would make them
//! stop seeing the entry and glue its text to the one above. A second
//! line they do not know is only shown as text. Keys this version
//! does not know are ignored and kept.
//!
//! The files are the only copy of the text. The SQLite store keeps
//! counters for the stats and never any text. Nothing here is an
//! index: three years of dictation is a few megabytes, which is
//! scanned in milliseconds, and parsed files are cached by their
//! modification time.
//!
//! Rules this module keeps, because the files may be edited by hand
//! or live in a folder the user also uses for other notes:
//! - Existing content is never re-rendered. Adding an entry appends
//!   one block; removing entries cuts their blocks out. Everything
//!   else in the file stays byte for byte.
//! - Text OpenWhisper did not write (anything before the first
//!   entry) is kept, and a file that still has such text is never
//!   deleted.
//! - Only files named `YYYY/YYYY-MM-DD.md` are read or touched.
//!
//! The same functions back the desktop app and `openwhisper history`.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use chrono::{Duration as ChronoDuration, Local, NaiveDate, NaiveTime, TimeZone};
use regex::Regex;
use serde::Serialize;

use crate::settings::HistorySettings;

/// Most entries one [`list`] call returns.
pub const MAX_PAGE: u32 = 500;

/// Name of the history folder inside the app's config folder, when
/// the user has not chosen another place.
pub const DEFAULT_FOLDER_NAME: &str = "history";

/// Label of the quote that holds the text before Refine.
const BEFORE_REFINE: &str = "> Before Refine:";

static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^## (\d{2}):(\d{2}):(\d{2})(?: · (.*))?$").unwrap());
static META: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^<!-- id: (\S+)(?: app: (.*?))? words: (\d+) seconds: (\d+(?:\.\d+)?) -->$").unwrap()
});
/// The optional second comment line of an entry. See the module docs.
static EXTRA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<!-- extra:(.*)-->$").unwrap());
/// Key of the mode name on the extra line.
const EXTRA_MODE: &str = "mode";
static DAY_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})-(\d{2})-(\d{2})\.md$").unwrap());
static YEAR_DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}$").unwrap());
static ENTRY_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})(\d{2})(\d{2})T\d{6}(?:-\d+)?$").unwrap());

/// One stored dictation.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    /// `YYYYMMDDTHHMMSS` of the recording's start in local time, with
    /// `-2`, `-3` appended if two share a second. Names the day file
    /// the entry is in.
    pub id: String,
    /// When the recording started, Unix epoch milliseconds.
    pub at_ms: i64,
    /// Voiced duration of the recording.
    pub duration_ms: i64,
    pub word_count: i64,
    /// The text that was inserted.
    pub text: String,
    /// The transcript before Refine, present only when Refine changed
    /// it.
    pub original: Option<String>,
    /// See [`crate::focus::FocusContext::app_id`]. `None` when the
    /// app was not known (or the text only went to the clipboard).
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    /// Name of the mode the text was rewritten in, when that was a
    /// mode the user made. `None` for the built-in Dictation mode,
    /// for dictations Refine did not change, and for entries written
    /// before modes existed.
    pub mode: Option<String>,
}

/// A dictation to add.
pub struct NewEntry<'a> {
    pub at_ms: i64,
    pub duration_ms: i64,
    pub text: &'a str,
    /// Stored only when it differs from `text`.
    pub original: Option<&'a str>,
    /// Empty when unknown.
    pub app_id: &'a str,
    pub app_name: &'a str,
    /// Name of the custom mode that rewrote the text. Empty for none.
    pub mode: &'a str,
}

/// A slice of the history, newest first.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    /// Entries matching the search (all stored entries when the
    /// search is empty), before `limit` and `offset`.
    pub matched: i64,
    /// Entries stored in total.
    pub stored: i64,
}

// ---------------------------------------------------------------
// Where the files live
// ---------------------------------------------------------------

static DEFAULT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Register the folder used when the user has not chosen one: the
/// shell passes `<config dir>/history`. First call wins.
pub fn set_default_dir(dir: PathBuf) {
    let _ = DEFAULT_DIR.set(dir);
}

/// The folder for `settings`: the user's own choice, else `default`.
/// A leading `~/` is expanded.
pub fn resolve_dir(settings: &HistorySettings, default: &Path) -> PathBuf {
    match settings.folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(folder) => expand_home(folder),
        None => default.to_path_buf(),
    }
}

/// The folder the running app writes to: the registered default with
/// the current settings applied. `None` before [`set_default_dir`]
/// and with no folder chosen.
pub fn current_dir() -> Option<PathBuf> {
    let settings = crate::settings::current_history_settings();
    match DEFAULT_DIR.get() {
        Some(default) => Some(resolve_dir(&settings, default)),
        None => settings
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(expand_home),
    }
}

fn expand_home(folder: &str) -> PathBuf {
    let rest = folder.strip_prefix("~/").or_else(|| folder.strip_prefix("~\\"));
    if let Some(rest) = rest {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(folder)
}

fn day_path(dir: &Path, date: NaiveDate) -> PathBuf {
    dir.join(date.format("%Y").to_string())
        .join(format!("{}.md", date.format("%Y-%m-%d")))
}

/// Every day file under `dir`, newest day first.
fn day_files(dir: &Path) -> Vec<(NaiveDate, PathBuf)> {
    let mut files = Vec::new();
    let Ok(years) = fs::read_dir(dir) else {
        return files;
    };
    for year in years.flatten() {
        let name = year.file_name();
        if !YEAR_DIR.is_match(&name.to_string_lossy()) {
            continue;
        }
        let Ok(days) = fs::read_dir(year.path()) else {
            continue;
        };
        for day in days.flatten() {
            let name = day.file_name();
            let name = name.to_string_lossy();
            if let Some(date) = date_from_file_name(&name) {
                files.push((date, day.path()));
            }
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
}

fn date_from_file_name(name: &str) -> Option<NaiveDate> {
    let c = DAY_FILE.captures(name)?;
    NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?)
}

fn date_from_id(id: &str) -> Option<NaiveDate> {
    let c = ENTRY_ID.captures(id)?;
    NaiveDate::from_ymd_opt(c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?)
}

fn local_ms(date: NaiveDate, time: NaiveTime) -> i64 {
    date.and_time(time)
        .and_local_timezone(Local)
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

// ---------------------------------------------------------------
// The file format
// ---------------------------------------------------------------

/// One entry as found in a day file, with the exact text it occupies.
struct Block {
    entry: HistoryEntry,
    source: String,
}

/// A day file taken apart. `prefix` + every `source` is the file's
/// body, byte for byte.
struct Day {
    /// Frontmatter lines between the `---` fences, without them.
    front: Vec<String>,
    /// Whatever precedes the first entry: text OpenWhisper did not
    /// write.
    prefix: String,
    blocks: Vec<Block>,
}

fn bare(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

fn parse_day(text: &str, date: NaiveDate) -> Day {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut front = Vec::new();
    let mut body_start = 0;
    if lines.first().is_some_and(|l| bare(l) == "---")
        && let Some(end) = lines.iter().skip(1).position(|l| bare(l) == "---")
    {
        front = lines[1..=end].iter().map(|l| bare(l).to_string()).collect();
        body_start = end + 2;
    }
    let body = &lines[body_start..];

    // An entry starts at a time heading whose next non-blank line is
    // the comment with the machine fields. A heading without it is
    // ordinary text.
    let starts: Vec<usize> = (0..body.len())
        .filter(|&i| {
            HEADING.is_match(bare(body[i]))
                && body[i + 1..]
                    .iter()
                    .map(|l| bare(l))
                    .find(|l| !l.trim().is_empty())
                    .is_some_and(|l| META.is_match(l))
        })
        .collect();

    let prefix: String = body[..starts.first().copied().unwrap_or(body.len())].concat();
    let mut blocks = Vec::new();
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(body.len());
        let chunk = &body[start..end];
        if let Some(entry) = parse_block(chunk, date) {
            blocks.push(Block { entry, source: chunk.concat() });
        }
    }
    Day { front, prefix, blocks }
}

fn parse_block(chunk: &[&str], date: NaiveDate) -> Option<HistoryEntry> {
    let heading = HEADING.captures(bare(chunk[0]))?;
    let time = NaiveTime::from_hms_opt(
        heading[1].parse().ok()?,
        heading[2].parse().ok()?,
        heading[3].parse().ok()?,
    )?;
    let app_name = heading.get(4).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());

    let meta_at = chunk.iter().skip(1).position(|l| !bare(l).trim().is_empty())? + 1;
    let meta = META.captures(bare(chunk[meta_at]))?;
    let id = meta[1].to_string();
    let app_id = meta.get(2).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());
    let seconds: f64 = meta[4].parse().unwrap_or(0.0);

    // The extra line counts only directly under the first comment
    // line. Anywhere else, a line that looks like it is text.
    let mut text_from = meta_at + 1;
    let mut mode = None;
    if let Some(extra) = chunk.get(text_from).and_then(|l| EXTRA.captures(bare(l))) {
        for pair in extra[1].split(';') {
            if let Some((key, value)) = pair.split_once('=')
                && key.trim() == EXTRA_MODE
            {
                mode = Some(value.trim().to_string()).filter(|m| !m.is_empty());
            }
        }
        text_from += 1;
    }

    let mut text_lines: Vec<String> = Vec::new();
    let mut original_lines: Vec<String> = Vec::new();
    let mut in_quote = false;
    let mut had_quote = false;
    for line in &chunk[text_from..] {
        let line = bare(line);
        if !had_quote && let Some(first) = line.strip_prefix(BEFORE_REFINE) {
            in_quote = true;
            had_quote = true;
            original_lines.push(first.trim().to_string());
        } else if in_quote && line.starts_with('>') {
            let rest = line.strip_prefix("> ").unwrap_or(&line[1..]);
            original_lines.push(rest.to_string());
        } else {
            in_quote = false;
            text_lines.push(unescape_line(line).to_string());
        }
    }
    let text = text_lines.join("\n").trim().to_string();
    let original = Some(original_lines.join("\n").trim().to_string()).filter(|o| !o.is_empty());
    // The count in the comment is what was written; if the text was
    // edited by hand since, the text wins.
    let word_count = text.split_whitespace().count() as i64;

    Some(HistoryEntry {
        id,
        at_ms: local_ms(date, time),
        duration_ms: (seconds * 1000.0).round() as i64,
        word_count,
        text,
        original,
        app_id,
        app_name,
        mode,
    })
}

/// A text line that would read as structure (an entry heading, a
/// quote, an escape) gets a backslash in front.
fn escape_line(line: &str) -> String {
    if line.starts_with("## ") || line.starts_with('>') || line.starts_with('\\') || line.starts_with("<!--") {
        format!("\\{line}")
    } else {
        line.to_string()
    }
}

fn unescape_line(line: &str) -> &str {
    line.strip_prefix('\\').unwrap_or(line)
}

/// One line, no line breaks, and nothing that would end the comment.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").replace("-->", "")
}

/// A value on the extra line: one line, without `;` (it separates
/// the pairs) and without anything that would end the comment.
fn extra_value(s: &str) -> String {
    one_line(&s.replace(';', " ").replace("-->", " "))
}

/// The `key=value` pairs of the extra line, empty when the entry has
/// nothing beyond the fields of the first comment line.
fn extra_pairs(entry: &HistoryEntry) -> Vec<String> {
    let mut pairs = Vec::new();
    if let Some(mode) = entry.mode.as_deref().map(extra_value).filter(|m| !m.is_empty()) {
        pairs.push(format!("{EXTRA_MODE}={mode}"));
    }
    pairs
}

fn render_block(entry: &HistoryEntry, time: NaiveTime) -> String {
    let mut out = format!("## {}", time.format("%H:%M:%S"));
    let label = entry.app_name.as_deref().or(entry.app_id.as_deref()).map(one_line);
    if let Some(label) = label.filter(|l| !l.is_empty()) {
        out.push_str(" · ");
        out.push_str(&label);
    }
    out.push_str(&format!("\n<!-- id: {}", entry.id));
    if let Some(app) = entry.app_id.as_deref().map(one_line).filter(|a| !a.is_empty()) {
        out.push_str(&format!(" app: {app}"));
    }
    out.push_str(&format!(
        " words: {} seconds: {:.1} -->\n",
        entry.word_count,
        entry.duration_ms.max(0) as f64 / 1000.0
    ));
    let extra = extra_pairs(entry);
    if !extra.is_empty() {
        out.push_str(&format!("<!-- extra: {} -->\n", extra.join("; ")));
    }
    out.push('\n');
    for line in entry.text.lines() {
        out.push_str(&escape_line(line));
        out.push('\n');
    }
    if let Some(original) = &entry.original {
        out.push('\n');
        for (n, line) in original.lines().enumerate() {
            match (n, line.is_empty()) {
                (0, _) => out.push_str(&format!("{BEFORE_REFINE} {line}\n")),
                (_, true) => out.push_str(">\n"),
                (_, false) => out.push_str(&format!("> {line}\n")),
            }
        }
    }
    out
}

/// Frontmatter keys this module owns.
const OWN_KEYS: [&str; 4] = ["date:", "entries:", "words:", "apps:"];

fn yaml_scalar(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '+' | '-'))
        && !s.starts_with([' ', '-'])
        && !s.ends_with(' ');
    if plain {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// The frontmatter for a day: our four keys, then any other keys the
/// file already had. Empty when there is nothing to say (no entries
/// and no other keys).
fn render_front(existing: &[String], date: NaiveDate, entries: &[&HistoryEntry]) -> String {
    let others: Vec<&String> = existing
        .iter()
        .filter(|l| !OWN_KEYS.iter().any(|k| l.starts_with(k)))
        .collect();
    if entries.is_empty() && others.iter().all(|l| l.trim().is_empty()) {
        return String::new();
    }
    let mut out = String::from("---\n");
    if !entries.is_empty() {
        let words: i64 = entries.iter().map(|e| e.word_count).sum();
        let mut apps: Vec<String> = Vec::new();
        for e in entries {
            if let Some(app) = e.app_name.as_deref().or(e.app_id.as_deref())
                && !apps.iter().any(|a| a == app)
            {
                apps.push(app.to_string());
            }
        }
        let apps: Vec<String> = apps.iter().map(|a| yaml_scalar(a)).collect();
        out.push_str(&format!("date: {}\n", date.format("%Y-%m-%d")));
        out.push_str(&format!("entries: {}\n", entries.len()));
        out.push_str(&format!("words: {words}\n"));
        out.push_str(&format!("apps: [{}]\n", apps.join(", ")));
    }
    for line in others {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("---\n\n");
    out
}

// ---------------------------------------------------------------
// Reading and writing files
// ---------------------------------------------------------------

/// Serializes writers inside this process (the dictation thread and
/// a delete from the UI).
static WRITE: Mutex<()> = Mutex::new(());

fn read_day(path: &Path, date: NaiveDate) -> io::Result<Day> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(parse_day(&text, date)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(parse_day("", date)),
        Err(e) => Err(e),
    }
}

/// Replace `path` with `content` in one step, so a reader never sees
/// half a file.
fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp"));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

/// Write the day back: frontmatter for `kept`, then `body`. Removes
/// the file (and an emptied year folder) when nothing is left.
fn store_day(path: &Path, date: NaiveDate, front: &[String], kept: &[&HistoryEntry], body: &str) -> io::Result<()> {
    let front = render_front(front, date, kept);
    let body = body.trim_start_matches(['\n', '\r']);
    if front.is_empty() && body.trim().is_empty() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if let Some(year) = path.parent() {
            // Fails, as intended, while the folder still has files.
            let _ = fs::remove_dir(year);
        }
        return Ok(());
    }
    let mut content = format!("{front}{}", body.trim_end());
    content.push('\n');
    write_atomic(path, &content)
}

/// Add a dictation to its day's file. Text that is empty after
/// trimming is not stored (`Ok(None)`).
pub fn append(dir: &Path, new: &NewEntry<'_>) -> io::Result<Option<HistoryEntry>> {
    let text = new.text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let Some(at) = Local.timestamp_millis_opt(new.at_ms).single() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "history: bad timestamp"));
    };
    // Whole seconds, like the heading that stores the time.
    let time = chrono::Timelike::with_nanosecond(&at.time(), 0).unwrap_or(at.time());
    let date = at.date_naive();
    let non_empty = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());

    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let path = day_path(dir, date);
    let day = read_day(&path, date)?;

    let base = at.format("%Y%m%dT%H%M%S").to_string();
    let taken = |id: &str| day.blocks.iter().any(|b| b.entry.id == id);
    let mut id = base.clone();
    let mut n = 2;
    while taken(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }

    let entry = HistoryEntry {
        id,
        at_ms: local_ms(date, time),
        duration_ms: (new.duration_ms.max(0) as f64 / 100.0).round() as i64 * 100,
        word_count: text.split_whitespace().count() as i64,
        text: text.to_string(),
        original: new.original.and_then(non_empty).filter(|o| o != text),
        app_id: non_empty(new.app_id),
        app_name: non_empty(new.app_name),
        // Stored the way it will read back.
        mode: Some(extra_value(new.mode)).filter(|m| !m.is_empty()),
    };

    let mut body = day.prefix.clone();
    for block in &day.blocks {
        body.push_str(&block.source);
    }
    let block = render_block(&entry, time);
    let body = if body.trim().is_empty() {
        block
    } else {
        format!("{}\n\n{block}", body.trim_end())
    };
    let mut kept: Vec<&HistoryEntry> = day.blocks.iter().map(|b| &b.entry).collect();
    kept.push(&entry);
    store_day(&path, date, &day.front, &kept, &body)?;
    Ok(Some(entry))
}

/// Cut the entries matching `remove` out of one day file. Returns how
/// many were removed.
fn remove_from_day(path: &Path, date: NaiveDate, remove: impl Fn(&HistoryEntry) -> bool) -> io::Result<usize> {
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let day = read_day(path, date)?;
    let removed = day.blocks.iter().filter(|b| remove(&b.entry)).count();
    if removed == 0 {
        return Ok(0);
    }
    let mut body = day.prefix.clone();
    let mut kept = Vec::new();
    for block in day.blocks.iter().filter(|b| !remove(&b.entry)) {
        body.push_str(&block.source);
        kept.push(&block.entry);
    }
    store_day(path, date, &day.front, &kept, &body)?;
    Ok(removed)
}

type Cached = (SystemTime, u64, Arc<Vec<HistoryEntry>>);
static CACHE: LazyLock<Mutex<HashMap<PathBuf, Cached>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The entries of one day file, newest first. Parsed once per version
/// of the file (modification time and size).
fn day_entries(path: &Path, date: NaiveDate) -> Arc<Vec<HistoryEntry>> {
    let stamp = fs::metadata(path).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
    if let Some((modified, len)) = stamp
        && let Ok(cache) = CACHE.lock()
        && let Some((m, l, entries)) = cache.get(path)
        && *m == modified
        && *l == len
    {
        return entries.clone();
    }
    let mut entries: Vec<HistoryEntry> = read_day(path, date)
        .map(|day| day.blocks.into_iter().map(|b| b.entry).collect())
        .unwrap_or_default();
    entries.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then_with(|| b.id.cmp(&a.id)));
    let entries = Arc::new(entries);
    if let Some((modified, len)) = stamp
        && let Ok(mut cache) = CACHE.lock()
    {
        cache.insert(path.to_path_buf(), (modified, len, entries.clone()));
    }
    entries
}

fn matches(entry: &HistoryEntry, needle: &str) -> bool {
    let has = |s: &str| s.to_lowercase().contains(needle);
    has(&entry.text)
        || entry.original.as_deref().is_some_and(has)
        || entry.app_name.as_deref().is_some_and(has)
}

// ---------------------------------------------------------------
// The API
// ---------------------------------------------------------------

/// Stored entries, newest first. `search` is matched without regard
/// to case (Danish letters included) against the text, the original
/// text and the app name; an empty search returns everything. `limit`
/// is capped at [`MAX_PAGE`].
pub fn list(dir: &Path, search: &str, limit: u32, offset: u32) -> HistoryPage {
    let needle = search.trim().to_lowercase();
    let limit = limit.clamp(1, MAX_PAGE) as usize;
    let offset = offset as usize;
    let mut stored = 0usize;
    let mut matched = 0usize;
    let mut entries = Vec::new();
    for (date, path) in day_files(dir) {
        let day = day_entries(&path, date);
        stored += day.len();
        for entry in day.iter() {
            if !needle.is_empty() && !matches(entry, &needle) {
                continue;
            }
            if matched >= offset && entries.len() < limit {
                entries.push(entry.clone());
            }
            matched += 1;
        }
    }
    HistoryPage { entries, matched: matched as i64, stored: stored as i64 }
}

/// The apps dictations went into, most recently used first, as
/// (app id, app name). The name is the id when none was stored. This
/// is what the settings offer when the user gives an app a default
/// mode: apps they actually dictate into.
pub fn apps(dir: &Path) -> Vec<(String, String)> {
    let mut seen: Vec<(String, String)> = Vec::new();
    for (date, path) in day_files(dir) {
        for entry in day_entries(&path, date).iter() {
            let Some(id) = entry.app_id.as_deref() else {
                continue;
            };
            if seen.iter().any(|(known, _)| known == id) {
                continue;
            }
            let name = entry.app_name.clone().unwrap_or_else(|| id.to_string());
            seen.push((id.to_string(), name));
        }
    }
    seen
}

/// One stored entry, by id.
pub fn get(dir: &Path, id: &str) -> Option<HistoryEntry> {
    let date = date_from_id(id)?;
    day_entries(&day_path(dir, date), date).iter().find(|e| e.id == id).cloned()
}

/// Remove one entry. `false` when there is no such entry.
pub fn delete(dir: &Path, id: &str) -> io::Result<bool> {
    let Some(date) = date_from_id(id) else {
        return Ok(false);
    };
    Ok(remove_from_day(&day_path(dir, date), date, |e| e.id == id)? > 0)
}

/// Remove every entry. Files that held nothing else are deleted.
/// Returns how many entries were removed.
pub fn clear(dir: &Path) -> io::Result<usize> {
    let mut removed = 0;
    for (date, path) in day_files(dir) {
        removed += remove_from_day(&path, date, |_| true)?;
    }
    Ok(removed)
}

/// Remove the entries of days that are older than the retention in
/// `settings`. Whole days: with 30 days, a dictation is kept through
/// the 30th day after the one it was made on. Applies whether history
/// is on or off: switching history off must not freeze old text in
/// place. Returns how many entries were removed.
pub fn prune(dir: &Path, settings: &HistorySettings, now_ms: i64) -> io::Result<usize> {
    if settings.retention_days == 0 {
        return Ok(0);
    }
    let Some(now) = Local.timestamp_millis_opt(now_ms).single() else {
        return Ok(0);
    };
    let oldest_kept = now.date_naive() - ChronoDuration::days(i64::from(settings.retention_days));
    let mut removed = 0;
    for (date, path) in day_files(dir) {
        if date < oldest_kept {
            removed += remove_from_day(&path, date, |_| true)?;
        }
    }
    Ok(removed)
}

/// Unix epoch milliseconds now.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
