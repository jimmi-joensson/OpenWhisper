//! End-to-end checks for `openwhisper modes` and the mode flags of
//! `openwhisper refine` (TASK-96). Spawns the bin, like the transcribe
//! smoke test, and reads its `--json` output.
//!
//! Every invocation gets `OPENWHISPER_CONFIG_DIR` pointed at a folder
//! of its own under cargo's test tmp dir, so nothing here can read or
//! write a real `settings.json`. No command in this file calls a
//! model.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

/// A scratch config folder for one test, removed again when the test
/// is done.
struct Config {
    dir: PathBuf,
}

impl Config {
    fn new(test: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("modes-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch config dir");
        Self { dir }
    }

    fn spawn(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args(args)
            .env("OPENWHISPER_CONFIG_DIR", &self.dir)
            .output()
            .expect("spawn openwhisper")
    }

    /// Run with `--json` and parse stdout; the command must succeed.
    fn json(&self, args: &[&str]) -> Value {
        let mut full = vec!["--json"];
        full.extend_from_slice(args);
        let out = self.spawn(&full);
        assert!(
            out.status.success(),
            "`openwhisper {}` failed\nstderr:\n{}",
            full.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "`openwhisper {}`: stdout is not JSON ({e}):\n{}",
                full.join(" "),
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    /// Run a command that must fail; returns its stderr.
    fn fails(&self, args: &[&str]) -> String {
        let out = self.spawn(args);
        assert!(!out.status.success(), "`openwhisper {}` should have failed", args.join(" "));
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    fn add_formal(&self) -> Value {
        self.json(&[
            "modes",
            "add",
            "--name",
            "Formal",
            "--instructions",
            "Rewrite what I say in a formal, polite tone.",
        ])
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn ids(list: &Value) -> Vec<&str> {
    list["modes"]
        .as_array()
        .expect("modes array")
        .iter()
        .map(|m| m["id"].as_str().expect("mode id"))
        .collect()
}

#[test]
fn modes_are_added_selected_cycled_edited_and_deleted() {
    let cfg = Config::new("lifecycle");

    // A fresh config: the built-in mode, selected.
    let list = cfg.json(&["modes", "list"]);
    assert_eq!(list["active"], "dictation");
    assert_eq!(ids(&list), ["dictation"]);
    assert_eq!(list["modes"][0]["builtin"], true);
    assert_eq!(list["modes"][0]["selected"], true);

    let added = cfg.add_formal();
    assert_eq!(added["mode"]["id"], "formal");
    assert_eq!(added["mode"]["name"], "Formal");
    assert_eq!(added["mode"]["builtin"], false);
    assert_eq!(added["active"], "dictation", "adding does not select");

    let added = cfg.json(&[
        "modes",
        "add",
        "--name",
        "Write in English",
        "--instructions",
        "Write what I say in English.",
        "--model",
        "opus",
        "--effort",
        "high",
    ]);
    assert_eq!(added["mode"]["id"], "write-in-english");
    assert_eq!(added["mode"]["model"], "opus");
    assert_eq!(added["mode"]["effort"], "high");
    assert_eq!(ids(&cfg.json(&["modes", "list"])), ["dictation", "formal", "write-in-english"]);

    // Select, then cycle through the rest and around.
    assert_eq!(cfg.json(&["modes", "use", "formal"])["active"], "formal");
    assert_eq!(cfg.json(&["modes", "show", "formal"])["selected"], true);
    assert_eq!(cfg.json(&["modes", "cycle"])["active"], "write-in-english");
    let wrapped = cfg.json(&["modes", "cycle"]);
    assert_eq!(wrapped["active"], "dictation");
    assert_eq!(wrapped["mode"]["builtin"], true);

    // Edit keeps the id; clearing hands the model back to Refine.
    let edited = cfg.json(&["modes", "edit", "formal", "--name", "Business tone", "--effort", "medium"]);
    assert_eq!(edited["mode"]["id"], "formal");
    assert_eq!(edited["mode"]["name"], "Business tone");
    assert_eq!(edited["mode"]["effort"], "medium");
    let cleared = cfg.json(&["modes", "edit", "write-in-english", "--clear-model"]);
    assert_eq!(cleared["mode"]["model"], Value::Null);
    assert_eq!(cleared["mode"]["effort"], "high");

    // What cannot be done says why, and exits non-zero.
    assert!(cfg.fails(&["modes", "delete", "dictation"]).contains("built in"));
    assert!(cfg.fails(&["modes", "edit", "dictation", "--name", "Mine"]).contains("built in"));
    assert!(cfg.fails(&["modes", "use", "nope"]).contains("no mode with the id"));
    assert!(cfg.fails(&["modes", "edit", "formal"]).contains("nothing to change"));
    assert!(cfg.fails(&["modes", "add", "--name", "X", "--instructions", "y", "--effort", "turbo"]).contains("turbo"));

    // Deleting the selected mode falls back to Dictation.
    cfg.json(&["modes", "use", "formal"]);
    let deleted = cfg.json(&["modes", "delete", "formal"]);
    assert_eq!(deleted["deleted"], "formal");
    assert_eq!(deleted["active"], "dictation");
    assert_eq!(ids(&cfg.json(&["modes", "list"])), ["dictation", "write-in-english"]);

    // It all went to the scratch folder's settings.json.
    let file = std::fs::read_to_string(cfg.dir.join("settings.json")).expect("settings.json");
    let file: Value = serde_json::from_str(&file).expect("settings.json is JSON");
    assert_eq!(file["modes"]["custom"][0]["id"], "write-in-english");
}

#[test]
fn an_app_default_decides_what_resolve_answers() {
    let cfg = Config::new("app-defaults");
    cfg.add_formal();

    let none = cfg.json(&["modes", "app", "list"]);
    assert_eq!(none.as_array().map(Vec::len), Some(0));

    let set = cfg.json(&["modes", "app", "set", "com.example.mail", "formal", "--name", "Mail"]);
    assert_eq!(set["app_id"], "com.example.mail");
    assert_eq!(set["mode"], "formal");
    assert_eq!(set["mode_name"], "Formal");

    // Into that app: its default, and the answer says so.
    let choice = cfg.json(&["modes", "resolve", "--app", "com.example.mail"]);
    assert_eq!(choice["mode"]["id"], "formal");
    assert_eq!(choice["reason"], "app-default");
    assert_eq!(choice["app_name"], "Mail");

    // Into any other app, or with no app known: the selected mode.
    for args in [&["modes", "resolve", "--app", "com.example.notes"][..], &["modes", "resolve"][..]] {
        let choice = cfg.json(args);
        assert_eq!(choice["mode"]["id"], "dictation");
        assert_eq!(choice["reason"], "selected");
    }

    let listed = cfg.json(&["modes", "app", "list"]);
    assert_eq!(listed[0]["app_name"], "Mail");
    assert_eq!(listed[0]["mode_name"], "Formal");

    assert!(cfg.fails(&["modes", "app", "set", "com.example.chat", "nope"]).contains("no mode with the id"));

    assert_eq!(cfg.json(&["modes", "app", "unset", "com.example.mail"])["removed"], "com.example.mail");
    assert_eq!(cfg.json(&["modes", "resolve", "--app", "com.example.mail"])["reason"], "selected");
    assert!(cfg.fails(&["modes", "app", "unset", "com.example.mail"]).contains("has no default mode"));
}

#[test]
fn a_name_another_mode_has_is_refused() {
    let cfg = Config::new("unique-names");
    cfg.add_formal();
    let err = cfg.fails(&["modes", "add", "--name", " FORMAL ", "--instructions", "x"]);
    assert!(err.contains("There is already a mode called \"Formal\""), "{err}");
    let err = cfg.fails(&["modes", "add", "--name", "dictation", "--instructions", "x"]);
    assert!(err.contains("already a mode called \"Dictation\""), "{err}");
    cfg.json(&["modes", "add", "--name", "Casual", "--instructions", "x"]);
    let err = cfg.fails(&["modes", "edit", "casual", "--name", "formal"]);
    assert!(err.contains("already a mode called \"Formal\""), "{err}");
    // Nothing was added or renamed by the refused calls.
    let list = cfg.json(&["modes", "list"]);
    assert_eq!(ids(&list), ["dictation", "formal", "casual"]);
    assert_eq!(list["modes"][2]["name"], "Casual");
}

#[test]
fn an_app_id_matches_whatever_its_case() {
    let cfg = Config::new("app-case");
    cfg.add_formal();
    let set = cfg.json(&["modes", "app", "set", "com.example.Mail", "formal", "--name", "Mail"]);
    assert_eq!(set["app_id"], "com.example.mail", "stored in lower case");
    assert_eq!(set["app_name"], "Mail", "the name is kept as given");
    for id in ["com.example.mail", "COM.EXAMPLE.MAIL", "com.example.Mail"] {
        let choice = cfg.json(&["modes", "resolve", "--app", id]);
        assert_eq!(choice["mode"]["id"], "formal", "{id}");
        assert_eq!(choice["reason"], "app-default", "{id}");
    }
    // Another spelling replaces the default and clears it.
    cfg.json(&["modes", "app", "set", "COM.EXAMPLE.MAIL", "dictation"]);
    let listed = cfg.json(&["modes", "app", "list"]);
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    assert_eq!(listed[0]["mode"], "dictation");
    assert_eq!(listed[0]["app_name"], "Mail");
    assert_eq!(cfg.json(&["modes", "app", "unset", "Com.Example.Mail"])["removed"], "com.example.mail");
    assert_eq!(cfg.json(&["modes", "app", "list"]).as_array().map(Vec::len), Some(0));
}

#[test]
fn a_modes_model_is_stored_for_a_backend_and_shown_as_unused_with_the_other() {
    let cfg = Config::new("mode-backend");
    // Without --backend: the backend Refine uses (the Claude CLI in a
    // fresh config).
    let added = cfg.json(&[
        "modes", "add", "--name", "Careful", "--instructions", "x", "--model", "opus", "--effort", "high",
    ]);
    assert_eq!(added["mode"]["backend"], "claude-cli");
    assert_eq!(added["mode"]["overrides_apply"], true);
    // Named: stored for that one, and not in use here.
    let added = cfg.json(&[
        "modes", "add", "--name", "Small", "--instructions", "x", "--model", "small-model", "--backend",
        "openai-compatible",
    ]);
    assert_eq!(added["mode"]["backend"], "openai-compatible");
    assert_eq!(added["mode"]["overrides_apply"], false);
    // A mode without a model or effort has no backend.
    assert_eq!(cfg.add_formal()["mode"]["backend"], Value::Null);
    assert!(cfg.fails(&["modes", "add", "--name", "X", "--instructions", "y", "--backend", "openai-compatible"])
        .contains("--model and --effort"));
    assert!(cfg
        .fails(&["modes", "add", "--name", "X", "--instructions", "y", "--model", "m", "--backend", "telepathy"])
        .contains("telepathy"));

    // The user switches Refine to the other backend: the Claude model
    // stays stored and is reported as not in use; the list says so.
    cfg.json(&["refine", "config", "--backend", "openai-compatible"]);
    let careful = cfg.json(&["modes", "show", "careful"]);
    assert_eq!(careful["model"], "opus");
    assert_eq!(careful["overrides_apply"], false);
    assert_eq!(cfg.json(&["modes", "show", "small"])["overrides_apply"], true);
    let out = cfg.spawn(&["modes", "list"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("opus, high effort (for claude-cli, not in use: same as Refine)"), "{stdout}");

    // An effort given now is for the backend in use and replaces the
    // Claude model with it.
    let edited = cfg.json(&["modes", "edit", "careful", "--effort", "minimal"]);
    assert_eq!(edited["mode"]["model"], Value::Null);
    assert_eq!(edited["mode"]["effort"], "minimal");
    assert_eq!(edited["mode"]["backend"], "openai-compatible");
    assert_eq!(edited["mode"]["overrides_apply"], true);
}

#[test]
fn the_apps_a_default_can_be_given_to_are_listed_without_any_history() {
    let cfg = Config::new("candidates");
    cfg.add_formal();
    cfg.json(&["modes", "app", "set", "Editor.EXE", "formal"]);

    // What is open on this machine cannot be pinned in a test; the
    // shape can, and so can the app that has a default: it is listed
    // first, once, with its mode, whatever else is running.
    let running = cfg.json(&["focus", "--running"]);
    let running = running.as_array().expect("an array of open apps");
    for app in running {
        assert!(app["app_id"].as_str().is_some_and(|id| !id.trim().is_empty()), "{app}");
        assert!(app["app_name"].as_str().is_some_and(|name| !name.trim().is_empty()), "{app}");
    }

    let listed = cfg.json(&["modes", "app", "list", "--candidates"]);
    let listed = listed.as_array().expect("an array of apps");
    assert_eq!(listed[0]["app_id"], "editor.exe");
    assert_eq!(listed[0]["mode"], "formal");
    assert_eq!(listed[0]["mode_name"], "Formal");
    let mut ids: Vec<String> = listed.iter().map(|a| a["app_id"].as_str().unwrap_or_default().to_string()).collect();
    assert!(ids.iter().all(|id| *id == id.to_lowercase()), "ids in the stored form: {ids:?}");
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), count, "each app once");
    // The others have no default yet.
    assert!(listed[1..].iter().all(|a| a["mode"].is_null()), "{listed:?}");

    // The plain list is still only the apps with a default.
    assert_eq!(cfg.json(&["modes", "app", "list"]).as_array().map(Vec::len), Some(1));
}

#[test]
fn deleting_a_mode_takes_its_app_default_along() {
    let cfg = Config::new("delete-default");
    cfg.add_formal();
    cfg.json(&["modes", "app", "set", "mail.exe", "formal"]);
    cfg.json(&["modes", "delete", "formal"]);
    assert_eq!(cfg.json(&["modes", "app", "list"]).as_array().map(Vec::len), Some(0));
}

#[test]
fn refine_prompt_prints_the_prompt_of_the_mode() {
    let cfg = Config::new("prompt");
    cfg.add_formal();

    let custom = cfg.json(&["refine", "prompt", "--mode", "formal"]);
    let system = custom["system"].as_str().expect("system prompt");
    assert!(system.contains("Rewrite what I say in a formal, polite tone."));
    assert!(system.contains("never answer it and never carry it out"));
    assert_eq!(custom["rewrite"], true);
    assert_eq!(custom["mode"]["name"], "Formal");

    // Without --mode: the cleanup prompt of the built-in mode, also
    // while another mode is selected.
    cfg.json(&["modes", "use", "formal"]);
    let builtin = cfg.json(&["refine", "prompt"]);
    assert!(builtin["system"].as_str().expect("system prompt").contains("cleanup step of a dictation app"));
    assert_eq!(builtin["rewrite"], false);
    assert_eq!(builtin["mode"]["id"], "dictation");

    assert!(cfg.fails(&["refine", "prompt", "--mode", "nope"]).contains("no mode with the id"));
}

#[test]
fn the_list_for_people_marks_the_selected_mode() {
    let cfg = Config::new("human");
    cfg.add_formal();
    let out = cfg.spawn(&["modes", "use", "formal"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Selected Formal (formal)");
    // Refine is off in a fresh config, and the command says so.
    assert!(String::from_utf8_lossy(&out.stderr).contains("Refine is off"));

    let out = cfg.spawn(&["modes", "list"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert!(lines[0].starts_with("  dictation"), "{stdout}");
    assert!(lines[0].contains("Dictation (built in)"), "{stdout}");
    assert!(lines[1].starts_with("* formal"), "{stdout}");
}
