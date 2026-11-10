use super::*;
use tempfile::tempdir;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Local wall-clock time as epoch ms, so the tests hold in any zone.
fn at(date: NaiveDate, h: u32, m: u32, s: u32) -> i64 {
    local_ms(date, NaiveTime::from_hms_opt(h, m, s).unwrap())
}

fn add(dir: &Path, at_ms: i64, text: &str) -> HistoryEntry {
    add_full(dir, at_ms, text, None, ("", ""))
}

fn add_full(dir: &Path, at_ms: i64, text: &str, original: Option<&str>, app: (&str, &str)) -> HistoryEntry {
    add_in_mode(dir, at_ms, text, original, app, "")
}

fn add_in_mode(
    dir: &Path,
    at_ms: i64,
    text: &str,
    original: Option<&str>,
    app: (&str, &str),
    mode: &str,
) -> HistoryEntry {
    append(
        dir,
        &NewEntry {
            at_ms,
            duration_ms: 4200,
            text,
            original,
            app_id: app.0,
            app_name: app.1,
            mode,
            kind: "",
            answer: None,
        },
    )
    .expect("append")
    .expect("stored")
}

/// Store an ask: a question and its answer, with no app.
fn add_ask(dir: &Path, at_ms: i64, question: &str, answer: &str) -> HistoryEntry {
    append(
        dir,
        &NewEntry {
            at_ms,
            duration_ms: 2100,
            text: question,
            original: None,
            app_id: "",
            app_name: "",
            mode: "",
            kind: KIND_ASK,
            answer: Some(answer),
        },
    )
    .expect("append")
    .expect("stored")
}

fn texts(page: &HistoryPage) -> Vec<&str> {
    page.entries.iter().map(|e| e.text.as_str()).collect()
}

fn read(dir: &Path, date: NaiveDate) -> String {
    fs::read_to_string(day_path(dir, date)).expect("day file")
}

const D: fn(i32, u32, u32) -> NaiveDate = day;

#[test]
fn writes_the_documented_file() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add_full(
        dir.path(),
        at(date, 14, 2, 12),
        "Send the report to the team before Friday. ",
        Some("send the reaport to the team before friday"),
        ("com.tinyspeck.slackmacgap", "Slack"),
    );
    add_full(
        dir.path(),
        at(date, 14, 7, 40),
        "Check whether the build passes on Windows before we merge.",
        None,
        ("com.googlecode.iterm2", "iTerm2"),
    );

    let expected = "\
---
date: 2026-11-10
entries: 2
words: 18
apps: [Slack, iTerm2]
---

## 14:02:12 · Slack
<!-- id: 20261110T140212 app: com.tinyspeck.slackmacgap words: 8 seconds: 4.2 -->

Send the report to the team before Friday.

> Before Refine: send the reaport to the team before friday

## 14:07:40 · iTerm2
<!-- id: 20261110T140740 app: com.googlecode.iterm2 words: 10 seconds: 4.2 -->

Check whether the build passes on Windows before we merge.
";
    assert_eq!(read(dir.path(), date), expected);
    assert!(dir.path().join("2026").join("2026-11-10.md").is_file());
}

#[test]
fn reads_back_what_it_wrote() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let written = add_full(
        dir.path(),
        at(date, 9, 5, 0),
        "Vi ses på Ærø i morgen",
        Some("vi ses på aero i morgen"),
        ("com.example.chat", "Chatter"),
    );
    let page = list(dir.path(), "", 50, 0);
    assert_eq!(page.entries, vec![written.clone()]);
    assert_eq!(written.id, "20261110T090500");
    assert_eq!(written.at_ms, at(date, 9, 5, 0));
    assert_eq!(written.duration_ms, 4200);
    assert_eq!(written.word_count, 6);
    assert_eq!(written.original.as_deref(), Some("vi ses på aero i morgen"));
    assert_eq!(written.app_id.as_deref(), Some("com.example.chat"));
    assert_eq!(written.app_name.as_deref(), Some("Chatter"));
    assert_eq!(get(dir.path(), &written.id), Some(written));
}

#[test]
fn empty_text_is_not_stored() {
    let dir = tempdir().unwrap();
    let stored = append(
        dir.path(),
        &NewEntry {
            at_ms: at(D(2026, 11, 10), 9, 0, 0),
            duration_ms: 0,
            text: "  \n ",
            original: None,
            app_id: "",
            app_name: "",
            mode: "Formal",
            kind: "",
            answer: None,
        },
    )
    .unwrap();
    assert_eq!(stored, None);
    assert_eq!(list(dir.path(), "", 50, 0).stored, 0);
    assert!(!dir.path().join("2026").exists());
}

#[test]
fn original_is_kept_only_when_it_differs() {
    let dir = tempdir().unwrap();
    let e = add_full(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "same text ", Some(" same text"), ("", ""));
    assert_eq!(e.original, None);
    assert!(!read(dir.path(), D(2026, 11, 10)).contains("Before Refine"));
}

#[test]
fn unknown_app_leaves_heading_and_comment_bare() {
    let dir = tempdir().unwrap();
    add(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "no app here");
    let file = read(dir.path(), D(2026, 11, 10));
    assert!(file.contains("\n## 09:00:00\n<!-- id: 20261110T090000 words: 3 seconds: 4.2 -->\n"));
    assert!(file.contains("apps: []"));
    let e = &list(dir.path(), "", 1, 0).entries[0];
    assert_eq!((e.app_id.clone(), e.app_name.clone()), (None, None));
}

#[test]
fn app_ids_with_spaces_survive() {
    let dir = tempdir().unwrap();
    add_full(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "hello there", None, ("my tool.exe", "My Tool: Pro"));
    let e = &list(dir.path(), "", 1, 0).entries[0];
    assert_eq!(e.app_id.as_deref(), Some("my tool.exe"));
    assert_eq!(e.app_name.as_deref(), Some("My Tool: Pro"));
    // A name that is not a plain YAML word is quoted in the summary.
    assert!(read(dir.path(), D(2026, 11, 10)).contains("apps: [\"My Tool: Pro\"]"));
}

#[test]
fn two_dictations_in_one_second_get_distinct_ids() {
    let dir = tempdir().unwrap();
    let t = at(D(2026, 11, 10), 9, 0, 0);
    let ids = [add(dir.path(), t, "one").id, add(dir.path(), t, "two").id, add(dir.path(), t, "three").id];
    assert_eq!(ids, ["20261110T090000", "20261110T090000-2", "20261110T090000-3"]);
    assert_eq!(get(dir.path(), "20261110T090000-2").unwrap().text, "two");
}

#[test]
fn multi_line_text_and_original_round_trip() {
    let dir = tempdir().unwrap();
    let text = "Shopping:\n- milk\n- bread\n\nThat is all.";
    let original = "shopping milk bread\n\nthat is all";
    let e = add_full(dir.path(), at(D(2026, 11, 10), 9, 0, 0), text, Some(original), ("", ""));
    assert_eq!(e.text, text);
    let back = &list(dir.path(), "", 1, 0).entries[0];
    assert_eq!(back.text, text);
    assert_eq!(back.original.as_deref(), Some(original));
}

#[test]
fn text_that_looks_like_structure_is_escaped() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let tricky = "## 10:00:00 · Fake\n<!-- id: x words: 1 seconds: 1 -->\n> Before Refine: not a quote\n\\ backslash";
    add(dir.path(), at(date, 9, 0, 0), tricky);
    add(dir.path(), at(date, 9, 1, 0), "second");

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(page.stored, 2, "the fake heading must not become an entry");
    assert_eq!(texts(&page), ["second", tricky]);
    assert_eq!(page.entries[1].original, None);
}

#[test]
fn lists_newest_first_across_days_and_pages() {
    let dir = tempdir().unwrap();
    add(dir.path(), at(D(2026, 11, 9), 23, 59, 0), "yesterday late");
    add(dir.path(), at(D(2026, 11, 10), 8, 0, 0), "today early");
    add(dir.path(), at(D(2026, 11, 10), 17, 0, 0), "today late");
    add(dir.path(), at(D(2025, 12, 31), 12, 0, 0), "last year");

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["today late", "today early", "yesterday late", "last year"]);
    assert_eq!((page.matched, page.stored), (4, 4));

    let page = list(dir.path(), "", 2, 1);
    assert_eq!(texts(&page), ["today early", "yesterday late"]);
    assert_eq!((page.matched, page.stored), (4, 4));
    assert!(dir.path().join("2025").join("2025-12-31.md").is_file());
}

#[test]
fn search_ignores_case_including_danish_letters() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "Vi ses på Ærø i morgen");
    add_full(dir.path(), at(date, 9, 1, 0), "Send the report", Some("send the reaport"), ("", ""));
    add_full(dir.path(), at(date, 9, 2, 0), "Nothing to see", None, ("com.example.chat", "Chatter"));

    let hit = |q: &str| -> Vec<String> {
        list(dir.path(), q, 50, 0).entries.into_iter().map(|e| e.text).collect()
    };
    assert_eq!(hit("ærø"), ["Vi ses på Ærø i morgen"]);
    assert_eq!(hit("  REPORT "), ["Send the report"]);
    // The original text and the app name are searched too.
    assert_eq!(hit("reaport"), ["Send the report"]);
    assert_eq!(hit("chatter"), ["Nothing to see"]);
    assert!(hit("no such words").is_empty());

    let page = list(dir.path(), "the", 50, 0);
    assert_eq!((page.matched, page.stored), (1, 3));
}

#[test]
fn search_pages_over_the_matches() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    for i in 0..4 {
        add(dir.path(), at(date, 9, i, 0), &format!("match {i}"));
        add(dir.path(), at(date, 10, i, 0), &format!("other {i}"));
    }
    let page = list(dir.path(), "match", 2, 2);
    assert_eq!(page.matched, 4);
    assert_eq!(texts(&page), ["match 1", "match 0"]);
}

#[test]
fn delete_cuts_one_entry_and_leaves_the_rest_untouched() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "first");
    let middle = add_full(dir.path(), at(date, 9, 1, 0), "second", Some("secund"), ("a.b", "AB"));
    add(dir.path(), at(date, 9, 2, 0), "third");

    assert!(delete(dir.path(), &middle.id).unwrap());
    assert!(!delete(dir.path(), &middle.id).unwrap(), "second delete finds nothing");
    assert!(!delete(dir.path(), "not-an-id").unwrap());

    let expected = "\
---
date: 2026-11-10
entries: 2
words: 2
apps: []
---

## 09:00:00
<!-- id: 20261110T090000 words: 1 seconds: 4.2 -->

first

## 09:02:00
<!-- id: 20261110T090200 words: 1 seconds: 4.2 -->

third
";
    assert_eq!(read(dir.path(), date), expected);
    assert_eq!(get(dir.path(), &middle.id), None);
}

#[test]
fn deleting_the_last_entry_removes_the_file_and_the_empty_year() {
    let dir = tempdir().unwrap();
    let e = add(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "only one");
    assert!(delete(dir.path(), &e.id).unwrap());
    assert!(!dir.path().join("2026").exists());
    assert_eq!(list(dir.path(), "", 50, 0).stored, 0);
}

#[test]
fn clear_removes_every_entry() {
    let dir = tempdir().unwrap();
    add(dir.path(), at(D(2026, 11, 9), 9, 0, 0), "one");
    add(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "two");
    add(dir.path(), at(D(2025, 1, 2), 9, 0, 0), "three");
    assert_eq!(clear(dir.path()).unwrap(), 3);
    assert_eq!(clear(dir.path()).unwrap(), 0);
    assert_eq!(list(dir.path(), "", 50, 0).stored, 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0, "no files or folders left behind");
}

#[test]
fn prune_removes_days_older_than_the_retention() {
    let dir = tempdir().unwrap();
    let today = D(2026, 11, 10);
    let now = at(today, 12, 0, 0);
    add(dir.path(), at(D(2026, 10, 1), 9, 0, 0), "forty days old");
    add(dir.path(), at(D(2026, 11, 2), 9, 0, 0), "eight days old");
    add(dir.path(), at(D(2026, 11, 3), 9, 0, 0), "seven days old");
    add(dir.path(), at(today, 9, 0, 0), "today");

    let thirty = HistorySettings { enabled: true, retention_days: 30, folder: None };
    assert_eq!(prune(dir.path(), &thirty, now).unwrap(), 1);
    assert_eq!(list(dir.path(), "", 50, 0).stored, 3);

    // Retention still applies while history is switched off, and a
    // day exactly at the limit is kept.
    let seven_off = HistorySettings { enabled: false, retention_days: 7, folder: None };
    assert_eq!(prune(dir.path(), &seven_off, now).unwrap(), 1);
    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["today", "seven days old"]);
}

#[test]
fn prune_keeps_everything_when_retention_is_forever() {
    let dir = tempdir().unwrap();
    add(dir.path(), at(D(2020, 1, 1), 9, 0, 0), "ancient");
    let forever = HistorySettings { enabled: true, retention_days: 0, folder: None };
    assert_eq!(prune(dir.path(), &forever, at(D(2026, 11, 10), 12, 0, 0)).unwrap(), 0);
    assert_eq!(list(dir.path(), "", 50, 0).stored, 1);
}

#[test]
fn hand_edits_are_read_and_kept() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "teh first one");
    let path = day_path(dir.path(), date);

    // Someone fixes a typo, adds a tag to the frontmatter and a note
    // of their own at the top.
    let edited = read(dir.path(), date)
        .replace("teh first one", "the first one, fixed by hand")
        .replace("---\n\n## 09:00:00", "tags: [dictation]\n---\n\nMy own note for the day.\n\n## 09:00:00");
    fs::write(&path, &edited).unwrap();

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["the first one, fixed by hand"]);
    assert_eq!(page.entries[0].word_count, 6, "word count follows the edited text");

    // Adding an entry keeps the note, the tag and the edit as they are.
    add(dir.path(), at(date, 10, 0, 0), "second");
    let file = read(dir.path(), date);
    assert!(file.contains("tags: [dictation]\n---\n\nMy own note for the day.\n\n## 09:00:00"));
    assert!(file.contains("the first one, fixed by hand"));
    assert!(file.contains("entries: 2\nwords: 7\n"));
    assert_eq!(list(dir.path(), "", 50, 0).stored, 2);
}

#[test]
fn clear_and_prune_never_delete_text_openwhisper_did_not_write() {
    let dir = tempdir().unwrap();
    let date = D(2026, 1, 5);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A day file that already was somebody's own note.
    fs::write(&path, "---\ntags: [journal]\n---\n\n# Monday\n\nWent for a walk.\n").unwrap();
    add(dir.path(), at(date, 9, 0, 0), "a dictation on the same day");
    assert_eq!(list(dir.path(), "", 50, 0).stored, 1);

    let settings = HistorySettings { enabled: true, retention_days: 7, folder: None };
    assert_eq!(prune(dir.path(), &settings, at(D(2026, 11, 10), 12, 0, 0)).unwrap(), 1);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "---\ntags: [journal]\n---\n\n# Monday\n\nWent for a walk.\n",
        "the note is back to what it was"
    );

    add(dir.path(), at(date, 9, 0, 0), "again");
    assert_eq!(clear(dir.path()).unwrap(), 1);
    assert!(path.is_file());
    assert!(fs::read_to_string(&path).unwrap().contains("Went for a walk."));
}

#[test]
fn files_with_other_names_are_ignored() {
    let dir = tempdir().unwrap();
    add(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "real");
    fs::write(dir.path().join("README.md"), "## 09:00:00\n<!-- id: 20261110T090000 words: 1 seconds: 1 -->\n\nnope\n").unwrap();
    fs::write(dir.path().join("2026").join("notes.md"), "not a day file").unwrap();
    fs::create_dir_all(dir.path().join("archive")).unwrap();
    fs::write(dir.path().join("archive").join("2026-11-10.md"), "not in a year folder").unwrap();

    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["real"]);
    assert_eq!(clear(dir.path()).unwrap(), 1);
    assert!(dir.path().join("README.md").is_file());
    assert!(dir.path().join("2026").join("notes.md").is_file());
    assert!(dir.path().join("archive").join("2026-11-10.md").is_file());
}

#[test]
fn a_heading_without_the_comment_is_just_text() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "## 08:00:00 · Meeting notes\n\nHand-written, no comment line.\n").unwrap();
    assert_eq!(list(dir.path(), "", 50, 0).stored, 0);
    add(dir.path(), at(date, 9, 0, 0), "dictated");
    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["dictated"]);
    assert!(read(dir.path(), date).contains("## 08:00:00 · Meeting notes\n\nHand-written, no comment line."));
}

#[test]
fn windows_line_endings_are_read() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "---\r\ndate: 2026-11-10\r\n---\r\n\r\n## 09:00:00 · Notepad\r\n<!-- id: 20261110T090000 app: notepad.exe words: 2 seconds: 1.5 -->\r\n\r\nhello there\r\n\r\n> Before Refine: helo there\r\n",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!(e.text, "hello there");
    assert_eq!(e.original.as_deref(), Some("helo there"));
    assert_eq!(e.app_id.as_deref(), Some("notepad.exe"));
    assert_eq!(e.duration_ms, 1500);
}

#[test]
fn a_changed_file_is_read_again() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "before the edit");
    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["before the edit"]);
    // Same length, so only the modification time can give it away;
    // make sure that moves.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let edited = read(dir.path(), date).replace("before the edit", "after the edit!");
    fs::write(day_path(dir.path(), date), edited).unwrap();
    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["after the edit!"]);
}

#[test]
fn missing_folder_is_an_empty_history() {
    let dir = tempdir().unwrap();
    let nowhere = dir.path().join("does-not-exist");
    assert_eq!(list(&nowhere, "", 50, 0), HistoryPage { entries: vec![], matched: 0, stored: 0 });
    assert_eq!(get(&nowhere, "20261110T090000"), None);
    assert_eq!(clear(&nowhere).unwrap(), 0);
}

#[test]
fn resolve_dir_prefers_the_chosen_folder() {
    let default = Path::new("/config/history");
    let none = HistorySettings::default();
    assert_eq!(resolve_dir(&none, default), default);
    let blank = HistorySettings { folder: Some("   ".into()), ..Default::default() };
    assert_eq!(resolve_dir(&blank, default), default);
    let chosen = HistorySettings { folder: Some("/notes/dictations".into()), ..Default::default() };
    assert_eq!(resolve_dir(&chosen, default), Path::new("/notes/dictations"));
}

// --- the extra line and the mode (TASK-96) -----------------------

/// A day file exactly as the released app wrote it before modes
/// existed.
const OLD_FORMAT_DAY: &str = "\
---
date: 2026-11-10
entries: 2
words: 18
apps: [Chatter, Terminal]
---

## 14:02:12 · Chatter
<!-- id: 20261110T140212 app: com.example.chat words: 8 seconds: 4.2 -->

Send the report to the team before Friday.

> Before Refine: send the reaport to the team before friday

## 14:07:40 · Terminal
<!-- id: 20261110T140740 app: com.example.terminal words: 10 seconds: 4.2 -->

Check whether the build passes on Windows before we merge.
";

#[test]
fn a_day_file_from_before_modes_reads_as_it_always_did() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, OLD_FORMAT_DAY).unwrap();

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(
        page.entries,
        vec![
            HistoryEntry {
                id: "20261110T140740".into(),
                at_ms: at(date, 14, 7, 40),
                duration_ms: 4200,
                word_count: 10,
                text: "Check whether the build passes on Windows before we merge.".into(),
                original: None,
                app_id: Some("com.example.terminal".into()),
                app_name: Some("Terminal".into()),
                mode: None,
                kind: None,
                answer: None,
            },
            HistoryEntry {
                id: "20261110T140212".into(),
                at_ms: at(date, 14, 2, 12),
                duration_ms: 4200,
                word_count: 8,
                text: "Send the report to the team before Friday.".into(),
                original: Some("send the reaport to the team before friday".into()),
                app_id: Some("com.example.chat".into()),
                app_name: Some("Chatter".into()),
                mode: None,
                kind: None,
                answer: None,
            },
        ]
    );
}

#[test]
fn an_entry_without_a_mode_is_written_byte_for_byte_as_before() {
    // The same two dictations the old-format file holds, written by
    // this version without a mode: the file must come out identical.
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let first = add_full(
        dir.path(),
        at(date, 14, 2, 12),
        "Send the report to the team before Friday. ",
        Some("send the reaport to the team before friday"),
        ("com.example.chat", "Chatter"),
    );
    add_full(
        dir.path(),
        at(date, 14, 7, 40),
        "Check whether the build passes on Windows before we merge.",
        None,
        ("com.example.terminal", "Terminal"),
    );
    assert_eq!(first.mode, None);
    assert_eq!(read(dir.path(), date), OLD_FORMAT_DAY);
    assert!(!read(dir.path(), date).contains("extra:"));
}

#[test]
fn a_mode_is_written_on_its_own_line_and_reads_back() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let written = add_in_mode(
        dir.path(),
        at(date, 9, 5, 0),
        "Could you please let me know when the price list is ready?",
        Some("hey oskar when is the price list ready"),
        ("com.example.mail", "Mail"),
        "Formal",
    );
    assert_eq!(written.mode.as_deref(), Some("Formal"));

    let expected = "\
---
date: 2026-11-10
entries: 1
words: 12
apps: [Mail]
---

## 09:05:00 · Mail
<!-- id: 20261110T090500 app: com.example.mail words: 12 seconds: 4.2 -->
<!-- extra: mode=Formal -->

Could you please let me know when the price list is ready?

> Before Refine: hey oskar when is the price list ready
";
    assert_eq!(read(dir.path(), date), expected);

    let back = list(dir.path(), "", 50, 0);
    assert_eq!(back.entries, vec![written.clone()]);
    // The extra line is not part of the text or of its word count.
    assert_eq!(back.entries[0].text, "Could you please let me know when the price list is ready?");
    assert_eq!(back.entries[0].word_count, 12);
    assert_eq!(get(dir.path(), &written.id), Some(written));
}

#[test]
fn the_meta_line_of_an_entry_with_a_mode_is_the_line_older_apps_look_for() {
    // An app released before modes finds an entry by a time heading
    // whose next non-blank line matches META. That line must be
    // untouched by the extra line under it.
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add_in_mode(dir.path(), at(date, 9, 5, 0), "With a mode.", None, ("com.example.mail", "Mail"), "Formal");
    let file = read(dir.path(), date);
    let lines: Vec<&str> = file.lines().collect();
    let heading = lines.iter().position(|l| HEADING.is_match(l)).expect("heading");
    let meta = lines[heading + 1];
    assert_eq!(meta, "<!-- id: 20261110T090500 app: com.example.mail words: 3 seconds: 4.2 -->");
    assert!(META.is_match(meta));
    // And the extra line is not mistaken for it.
    assert_eq!(lines[heading + 2], "<!-- extra: mode=Formal -->");
    assert!(!META.is_match(lines[heading + 2]));
    assert!(lines[heading + 3].is_empty());
}

#[test]
fn mode_names_cannot_break_the_extra_line() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let e = add_in_mode(dir.path(), at(date, 9, 0, 0), "odd name", None, ("", ""), "  Short; sharp -->\nand = to the point ");
    assert_eq!(e.mode.as_deref(), Some("Short sharp and = to the point"));
    assert!(read(dir.path(), date).contains("\n<!-- extra: mode=Short sharp and = to the point -->\n"));
    assert_eq!(list(dir.path(), "", 1, 0).entries[0], e);

    // A name with nothing usable in it is no mode.
    let e = add_in_mode(dir.path(), at(date, 9, 1, 0), "no name", None, ("", ""), " ; ");
    assert_eq!(e.mode, None);
    assert_eq!(read(dir.path(), date).matches("extra:").count(), 1);
}

#[test]
fn unknown_extra_keys_are_ignored_and_kept() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Written by a later version: a key this one does not know, in
    // front of the one it does.
    let later = "\
## 09:00:00 · Mail
<!-- id: 20261110T090000 app: com.example.mail words: 2 seconds: 1.0 -->
<!-- extra: kind=ask; mode=Formal; source=screen -->

Good morning.
";
    fs::write(&path, later).unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!(e.mode.as_deref(), Some("Formal"));
    assert_eq!(e.text, "Good morning.");

    // Only unknown keys, or nothing at all: no mode, still not text.
    fs::write(&path, later.replace("kind=ask; mode=Formal; source=screen", "kind=ask")).unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!((e.mode.clone(), e.text.as_str()), (None, "Good morning."));

    // Adding an entry leaves the line it did not understand in place.
    add(dir.path(), at(date, 10, 0, 0), "second");
    assert!(read(dir.path(), date).contains("seconds: 1.0 -->\n<!-- extra: kind=ask -->\n\nGood morning."));
}

#[test]
fn delete_and_append_work_around_entries_with_an_extra_line() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "first");
    let middle = add_in_mode(dir.path(), at(date, 9, 1, 0), "second", Some("secund"), ("a.b", "AB"), "Formal");
    let third = add_in_mode(dir.path(), at(date, 9, 2, 0), "third", None, ("", ""), "Write in English");
    add(dir.path(), at(date, 9, 3, 0), "fourth");

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["fourth", "third", "second", "first"]);
    let modes: Vec<Option<&str>> = page.entries.iter().map(|e| e.mode.as_deref()).collect();
    assert_eq!(modes, [None, Some("Write in English"), Some("Formal"), None]);

    assert!(delete(dir.path(), &middle.id).unwrap());
    let expected = "\
---
date: 2026-11-10
entries: 3
words: 3
apps: []
---

## 09:00:00
<!-- id: 20261110T090000 words: 1 seconds: 4.2 -->

first

## 09:02:00
<!-- id: 20261110T090200 words: 1 seconds: 4.2 -->
<!-- extra: mode=Write in English -->

third

## 09:03:00
<!-- id: 20261110T090300 words: 1 seconds: 4.2 -->

fourth
";
    assert_eq!(read(dir.path(), date), expected);

    // Deleting the entry with the extra line takes the line with it.
    assert!(delete(dir.path(), &third.id).unwrap());
    let file = read(dir.path(), date);
    assert!(!file.contains("extra:"));
    assert_eq!(texts(&list(dir.path(), "", 50, 0)), ["fourth", "first"]);

    // Same second as an entry with an extra line: ids stay distinct.
    let a = add_in_mode(dir.path(), at(date, 11, 0, 0), "one", None, ("", ""), "Formal");
    let b = add(dir.path(), at(date, 11, 0, 0), "two");
    assert_eq!((a.id.as_str(), b.id.as_str()), ("20261110T110000", "20261110T110000-2"));
    assert_eq!(clear(dir.path()).unwrap(), 4);
    assert!(!dir.path().join("2026").exists());
}

#[test]
fn an_extra_looking_line_in_the_body_is_text() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Written by hand into the body of an entry, not directly under
    // the comment line with the machine fields.
    fs::write(
        &path,
        "\
## 09:00:00
<!-- id: 20261110T090000 words: 5 seconds: 1.0 -->

<!-- extra: mode=Formal -->
is what the line looks like.
",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!(e.mode, None);
    assert_eq!(e.text, "<!-- extra: mode=Formal -->\nis what the line looks like.");

    // Dictated text that starts with the same characters is escaped
    // on the way in and comes back as it was said.
    let said = "<!-- extra: mode=Sneaky -->\nand more";
    let written = add(dir.path(), at(date, 10, 0, 0), said);
    assert_eq!(written.mode, None);
    let back = get(dir.path(), &written.id).unwrap();
    assert_eq!((back.text.as_str(), back.mode), (said, None));
}

#[test]
fn apps_lists_each_app_once_most_recent_first() {
    let dir = tempdir().unwrap();
    assert!(apps(dir.path()).is_empty());
    add_full(dir.path(), at(D(2026, 11, 8), 9, 0, 0), "a", None, ("com.example.mail", "Mail"));
    add_full(dir.path(), at(D(2026, 11, 9), 9, 0, 0), "b", None, ("com.example.chat", "Chatter"));
    add_full(dir.path(), at(D(2026, 11, 10), 8, 0, 0), "c", None, ("com.example.mail", "Mail Pro"));
    add_full(dir.path(), at(D(2026, 11, 10), 9, 0, 0), "d", None, ("notes.exe", ""));
    // No app known: nothing to offer.
    add(dir.path(), at(D(2026, 11, 10), 10, 0, 0), "e");

    assert_eq!(
        apps(dir.path()),
        vec![
            // Without a stored name, the id stands in.
            ("notes.exe".to_string(), "notes.exe".to_string()),
            // The newest name an app was seen under.
            ("com.example.mail".to_string(), "Mail Pro".to_string()),
            ("com.example.chat".to_string(), "Chatter".to_string()),
        ]
    );
    assert!(apps(&dir.path().join("does-not-exist")).is_empty());
}

// --- asks (TASK-97) -----------------------------------------------

#[test]
fn an_ask_is_written_with_its_kind_and_answer_and_reads_back() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let written = add_ask(
        dir.path(),
        at(date, 14, 12, 3),
        "What is the capital of Norway? ",
        " The capital of Norway is Oslo.\n\nIt has been the capital since 1814.\n",
    );
    assert!(written.is_ask());
    assert_eq!(written.kind.as_deref(), Some("ask"));
    assert_eq!(written.text, "What is the capital of Norway?");
    assert_eq!(
        written.answer.as_deref(),
        Some("The capital of Norway is Oslo.\n\nIt has been the capital since 1814.")
    );
    // The words counted are the question's: that is what was said.
    assert_eq!(written.word_count, 6);
    assert_eq!((written.app_id.clone(), written.app_name.clone(), written.mode.clone()), (None, None, None));

    let expected = "\
---
date: 2026-11-10
entries: 1
words: 6
apps: []
---

## 14:12:03
<!-- id: 20261110T141203 words: 6 seconds: 2.1 -->
<!-- extra: kind=ask -->

What is the capital of Norway?

> Answer: The capital of Norway is Oslo.
>
> It has been the capital since 1814.
";
    assert_eq!(read(dir.path(), date), expected);
    assert_eq!(list(dir.path(), "", 50, 0).entries, vec![written.clone()]);
    assert_eq!(get(dir.path(), &written.id), Some(written));
}

#[test]
fn an_entry_without_the_new_fields_renders_byte_for_byte_as_before() {
    // The two dictations of the old-format file and one with a mode,
    // written by this version: not a byte differs from what the
    // version before asks wrote.
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add_full(
        dir.path(),
        at(date, 14, 2, 12),
        "Send the report to the team before Friday. ",
        Some("send the reaport to the team before friday"),
        ("com.example.chat", "Chatter"),
    );
    let plain = add_full(
        dir.path(),
        at(date, 14, 7, 40),
        "Check whether the build passes on Windows before we merge.",
        None,
        ("com.example.terminal", "Terminal"),
    );
    assert_eq!((plain.kind.clone(), plain.answer.clone()), (None, None));
    assert!(!plain.is_ask());
    assert_eq!(read(dir.path(), date), OLD_FORMAT_DAY);

    let dir = tempdir().unwrap();
    add_in_mode(dir.path(), at(date, 9, 5, 0), "With a mode.", None, ("com.example.mail", "Mail"), "Formal");
    let file = read(dir.path(), date);
    assert!(file.contains("\n<!-- extra: mode=Formal -->\n\nWith a mode.\n"), "{file}");
    assert!(!file.contains("kind=") && !file.contains("Answer"), "{file}");
}

#[test]
fn an_answer_given_to_something_that_is_not_an_ask_is_not_stored() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let entry = append(
        dir.path(),
        &NewEntry {
            at_ms: at(date, 9, 0, 0),
            duration_ms: 1000,
            text: "a dictation",
            original: None,
            app_id: "",
            app_name: "",
            mode: "",
            kind: "",
            answer: Some("not an answer to anything"),
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!((entry.kind, entry.answer), (None, None));
    assert!(!read(dir.path(), date).contains("Answer"));
}

#[test]
fn an_ask_with_an_empty_answer_or_question_is_handled() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    // No question: nothing to store.
    let none = append(
        dir.path(),
        &NewEntry {
            at_ms: at(date, 9, 0, 0),
            duration_ms: 0,
            text: "  ",
            original: None,
            app_id: "",
            app_name: "",
            mode: "",
            kind: KIND_ASK,
            answer: Some("an answer"),
        },
    )
    .unwrap();
    assert_eq!(none, None);
    // A question without an answer is still an ask.
    let e = add_ask(dir.path(), at(date, 9, 1, 0), "anyone there", "  ");
    assert_eq!((e.kind.as_deref(), e.answer.clone()), (Some("ask"), None));
    assert_eq!(list(dir.path(), "", 5, 0).entries, vec![e]);
}

#[test]
fn answers_with_markdown_and_structure_round_trip() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    // Everything that could be read as structure of the file: a
    // quote, a heading that looks like an entry, a comment, the
    // labels themselves, a backslash.
    let answer = "Three steps:\n- one\n- two\n\n> a quoted line\n## 10:00:00 · Fake\n<!-- id: x words: 1 seconds: 1 -->\n> Answer: again\n> Before Refine: no\nBefore Refine: nor this\nAnswer: nor that\n\\ backslash\n```\nlet x = 1;\n```";
    let question = "how do i do it\n> Answer: this is part of my question";
    let written = add_ask(dir.path(), at(date, 9, 0, 0), question, answer);
    assert_eq!(written.answer.as_deref(), Some(answer));
    assert_eq!(written.text, question);
    let back = list(dir.path(), "", 50, 0);
    assert_eq!(back.entries, vec![written]);
    assert_eq!(back.stored, 1, "the fake heading inside the answer started no entry");

    // A second ask after it is appended, not swallowed by the quote.
    add_ask(dir.path(), at(date, 9, 1, 0), "second", "Second answer.");
    let back = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&back), ["second", question]);
    assert_eq!(back.entries[1].answer.as_deref(), Some(answer));
}

#[test]
fn an_entry_can_hold_both_quotes_in_either_order() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let entry = append(
        dir.path(),
        &NewEntry {
            at_ms: at(date, 9, 0, 0),
            duration_ms: 2000,
            text: "What does Brennholt sell?",
            original: Some("what does bren holt sell"),
            app_id: "",
            app_name: "",
            mode: "",
            kind: KIND_ASK,
            answer: Some("I do not know that company.\nSorry."),
        },
    )
    .unwrap()
    .unwrap();
    let file = read(dir.path(), date);
    assert!(
        file.ends_with(
            "What does Brennholt sell?\n\n> Before Refine: what does bren holt sell\n\n> Answer: I do not know that company.\n> Sorry.\n"
        ),
        "{file}"
    );
    assert_eq!(list(dir.path(), "", 5, 0).entries, vec![entry.clone()]);

    // The other way round, as a person might have rearranged it.
    let path = day_path(dir.path(), date);
    let swapped = file.replace(
        "> Before Refine: what does bren holt sell\n\n> Answer: I do not know that company.\n> Sorry.\n",
        "> Answer: I do not know that company.\n> Sorry.\n\n> Before Refine: what does bren holt sell\n",
    );
    assert_ne!(swapped, file);
    fs::write(&path, swapped).unwrap();
    let back = &list(dir.path(), "", 5, 0).entries[0];
    assert_eq!(back.original, entry.original);
    assert_eq!(back.answer, entry.answer);
    assert_eq!(back.text, entry.text);
}

#[test]
fn an_answer_line_in_an_entry_that_is_not_an_ask_is_text() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A dictation from before asks existed, edited by hand: it reads
    // exactly as that version read it.
    fs::write(
        &path,
        "\
## 09:00:00 · Notes
<!-- id: 20261110T090000 app: com.example.notes words: 4 seconds: 1.0 -->

Is the meeting on Thursday?

> Answer: yes, at ten
> in the big room
",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!((e.kind.clone(), e.answer.clone()), (None, None));
    assert_eq!(e.text, "Is the meeting on Thursday?\n\n> Answer: yes, at ten\n> in the big room");

    // The same with a mode on the extra line: still not an ask.
    fs::write(
        &path,
        "\
## 09:00:00
<!-- id: 20261110T090000 words: 1 seconds: 1.0 -->
<!-- extra: mode=Formal -->

Hello.

> Answer: no
",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!((e.mode.as_deref(), e.answer.clone()), (Some("Formal"), None));
    assert_eq!(e.text, "Hello.\n\n> Answer: no");
}

#[test]
fn in_an_ask_the_first_answer_line_is_the_answer_and_a_later_one_is_text() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Hand-written: the label deeper in the entry, after other text,
    // and then once more. Same rule as for `> Before Refine:`.
    fs::write(
        &path,
        "\
## 09:00:00
<!-- id: 20261110T090000 words: 3 seconds: 1.0 -->
<!-- extra: kind=ask -->

What is it?

A note I added.
> Answer: The first one.
> And its second line.
More notes.
> Answer: not the answer
",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert!(e.is_ask());
    assert_eq!(e.answer.as_deref(), Some("The first one.\nAnd its second line."));
    assert_eq!(e.text, "What is it?\n\nA note I added.\nMore notes.\n> Answer: not the answer");

    // The same shape with the other label behaves the same way.
    fs::write(
        &path,
        "\
## 09:00:00
<!-- id: 20261110T090000 words: 3 seconds: 1.0 -->

What is it?

A note I added.
> Before Refine: the first one
> and its second line
More notes.
> Before Refine: not the original
",
    )
    .unwrap();
    let e = &list(dir.path(), "", 50, 0).entries[0];
    assert_eq!(e.original.as_deref(), Some("the first one\nand its second line"));
    assert_eq!(e.text, "What is it?\n\nA note I added.\nMore notes.\n> Before Refine: not the original");
}

#[test]
fn delete_and_append_work_around_asks() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "first");
    let ask = add_ask(dir.path(), at(date, 9, 1, 0), "what is two plus two", "Four.\n\nThat is all.");
    let after = add_full(dir.path(), at(date, 9, 2, 0), "third", Some("thurd"), ("a.b", "AB"));
    let second_ask = add_ask(dir.path(), at(date, 9, 3, 0), "and three plus three", "Six.");

    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["and three plus three", "third", "what is two plus two", "first"]);
    let kinds: Vec<Option<&str>> = page.entries.iter().map(|e| e.kind.as_deref()).collect();
    assert_eq!(kinds, [Some("ask"), None, Some("ask"), None]);
    // The entry after an ask was not swallowed by its answer quote.
    assert_eq!(page.entries[1], after);

    // Cutting the dictation between the two asks leaves both whole.
    assert!(delete(dir.path(), &after.id).unwrap());
    let expected = "\
---
date: 2026-11-10
entries: 3
words: 10
apps: []
---

## 09:00:00
<!-- id: 20261110T090000 words: 1 seconds: 4.2 -->

first

## 09:01:00
<!-- id: 20261110T090100 words: 5 seconds: 2.1 -->
<!-- extra: kind=ask -->

what is two plus two

> Answer: Four.
>
> That is all.

## 09:03:00
<!-- id: 20261110T090300 words: 4 seconds: 2.1 -->
<!-- extra: kind=ask -->

and three plus three

> Answer: Six.
";
    assert_eq!(read(dir.path(), date), expected);

    // Cutting an ask takes its extra line and its answer with it.
    assert!(delete(dir.path(), &ask.id).unwrap());
    let file = read(dir.path(), date);
    assert!(!file.contains("Four."), "{file}");
    assert_eq!(file.matches("kind=ask").count(), 1);
    assert_eq!(list(dir.path(), "", 50, 0).entries, vec![second_ask.clone(), get(dir.path(), "20261110T090000").unwrap()]);

    // Appending after an ask at the end of the file.
    add(dir.path(), at(date, 10, 0, 0), "after the ask");
    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["after the ask", "and three plus three", "first"]);
    assert_eq!(page.entries[1], second_ask);
    assert_eq!(clear(dir.path()).unwrap(), 3);
    assert!(!dir.path().join("2026").exists());
}

#[test]
fn search_also_matches_the_answer() {
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    add(dir.path(), at(date, 9, 0, 0), "remember to call the plumber");
    add_ask(dir.path(), at(date, 9, 1, 0), "what is the capital of norway", "The capital is Oslo.");
    add_ask(dir.path(), at(date, 9, 2, 0), "hvad hedder hovedstaden i Danmark", "Den hedder København.");

    let by_answer = list(dir.path(), "oslo", 50, 0);
    assert_eq!(texts(&by_answer), ["what is the capital of norway"]);
    assert_eq!((by_answer.matched, by_answer.stored), (1, 3));
    // Danish letters, whatever their case.
    assert_eq!(texts(&list(dir.path(), "KØBENHAVN", 50, 0)), ["hvad hedder hovedstaden i Danmark"]);
    // The question still matches, and so does a plain dictation.
    assert_eq!(list(dir.path(), "capital", 50, 0).matched, 1);
    assert_eq!(list(dir.path(), "plumber", 50, 0).matched, 1);
    assert_eq!(list(dir.path(), "nothing like this", 50, 0).matched, 0);
}

#[test]
fn a_day_file_from_before_asks_reads_as_it_always_did() {
    // Written by the version with modes and without asks: one entry
    // with an extra line, one without.
    let dir = tempdir().unwrap();
    let date = D(2026, 11, 10);
    let path = day_path(dir.path(), date);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "\
---
date: 2026-11-10
entries: 2
words: 15
apps: [Mail]
---

## 09:05:00 · Mail
<!-- id: 20261110T090500 app: com.example.mail words: 12 seconds: 4.2 -->
<!-- extra: mode=Formal -->

Could you please let me know when the price list is ready?

> Before Refine: hey oskar when is the price list ready

## 09:06:00
<!-- id: 20261110T090600 words: 3 seconds: 1.0 -->

Thanks a lot.
",
    )
    .unwrap();
    let page = list(dir.path(), "", 50, 0);
    assert_eq!(texts(&page), ["Thanks a lot.", "Could you please let me know when the price list is ready?"]);
    for e in &page.entries {
        assert_eq!((e.kind.clone(), e.answer.clone()), (None, None));
    }
    assert_eq!(page.entries[1].mode.as_deref(), Some("Formal"));
    assert_eq!(page.entries[1].original.as_deref(), Some("hey oskar when is the price list ready"));
    assert!(META.is_match("<!-- id: 20261110T090500 app: com.example.mail words: 12 seconds: 4.2 -->"));
}
