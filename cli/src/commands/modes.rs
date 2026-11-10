//! `openwhisper modes`: the modes of the Refine step (TASK-96). A
//! mode is a name plus instructions, optionally with its own model
//! and effort; "Dictation" is built in and the rest are the user's.
//! Reads and edits the same `modes` block of `settings.json` as the
//! desktop app, through the same core functions
//! (`openwhisper_core::modes`).
//!
//! Subcommands:
//! - `list`, `show <id>`: what modes there are.
//! - `add`, `edit <id>`, `delete <id>`: the user's own modes.
//! - `use <id>`, `cycle`: which mode is selected.
//! - `resolve [--app APP_ID]`: which mode a dictation into that app
//!   would run in, and why.
//! - `app list | set | unset`: a default mode per app. App ids are
//!   what `openwhisper focus` prints; `app list --candidates` shows
//!   the apps a default can be given to (the ones with a default, the
//!   ones in the history, the ones that are open), like the app's
//!   Modes pane.
//!
//! Nothing here calls a model. To run text through a mode, use
//! `openwhisper refine run --mode <id>`.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use clap::{Args as ClapArgs, Subcommand};
use openwhisper_core::modes::{self, ModeChoice, ModeEdit, ModeReason, ModeView};
use openwhisper_core::settings::{
    self, ModesSettings, RefineBackendKind, RefineEffort, RefineSettings, app_key,
};
use openwhisper_core::{focus, history};

#[derive(ClapArgs, Debug)]
pub struct ModesArgs {
    #[command(subcommand)]
    command: ModesCommand,
}

#[derive(Subcommand, Debug)]
enum ModesCommand {
    /// Show every mode; the selected one is marked.
    List,
    /// Print one mode in full.
    Show(IdArgs),
    /// Make a new mode.
    Add(AddArgs),
    /// Change a mode you made. Its id stays the same.
    Edit(EditArgs),
    /// Delete a mode you made. Apps lose it as their default, and if
    /// it was selected, Dictation is selected instead.
    Delete(IdArgs),
    /// Select the mode dictations run in.
    Use(IdArgs),
    /// Select the next mode in the list (what the cycle shortcut does).
    Cycle,
    /// Show which mode a dictation would run in, and why.
    Resolve(ResolveArgs),
    /// A default mode per app.
    App(AppArgs),
}

#[derive(ClapArgs, Debug)]
struct IdArgs {
    /// Mode id, as printed by `modes list` (for example `formal`).
    id: String,
}

#[derive(ClapArgs, Debug)]
struct AddArgs {
    /// Name of the mode, as shown in the app.
    #[arg(long)]
    name: String,
    /// What the model should do with what you say, in your own words.
    #[arg(long)]
    instructions: String,
    /// Model for this mode. Left out, it uses the Refine model.
    #[arg(long)]
    model: Option<String>,
    /// Effort for this mode: minimal, low, medium, high, xhigh or max.
    /// Left out, it uses the Refine effort.
    #[arg(long)]
    effort: Option<String>,
    /// The Refine backend --model and --effort are for: claude-cli or
    /// openai-compatible. Left out: the backend Refine uses now. A
    /// mode's model and effort only apply while that backend is the
    /// one in use.
    #[arg(long)]
    backend: Option<String>,
}

#[derive(ClapArgs, Debug)]
struct EditArgs {
    /// Mode id, as printed by `modes list`.
    id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    instructions: Option<String>,
    #[arg(long, conflicts_with = "clear_model")]
    model: Option<String>,
    #[arg(long, conflicts_with = "clear_effort")]
    effort: Option<String>,
    /// Go back to the Refine model.
    #[arg(long)]
    clear_model: bool,
    /// Go back to the Refine effort.
    #[arg(long)]
    clear_effort: bool,
    /// The Refine backend --model and --effort are for: claude-cli or
    /// openai-compatible. Left out: the backend Refine uses now.
    #[arg(long)]
    backend: Option<String>,
}

#[derive(ClapArgs, Debug)]
struct ResolveArgs {
    /// The app the dictation goes into (see `openwhisper focus`).
    /// Without it: the selected mode.
    #[arg(long, value_name = "APP_ID")]
    app: Option<String>,
}

#[derive(ClapArgs, Debug)]
struct AppArgs {
    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Subcommand, Debug)]
enum AppCommand {
    /// Show the apps that have a default mode.
    List(AppListArgs),
    /// Give an app a default mode.
    Set(AppSetArgs),
    /// Remove an app's default mode.
    Unset(AppIdArgs),
}

#[derive(ClapArgs, Debug)]
struct AppListArgs {
    /// Show every app a default can be given to instead: the apps
    /// that have one, then the apps in the history, then the apps
    /// that are open now.
    #[arg(long)]
    candidates: bool,
}

#[derive(ClapArgs, Debug)]
struct AppSetArgs {
    /// App id: the bundle id on macOS, the exe name on Windows.
    app_id: String,
    /// Mode id, as printed by `modes list`.
    mode_id: String,
    /// Name of the app to show in lists.
    #[arg(long)]
    name: Option<String>,
}

#[derive(ClapArgs, Debug)]
struct AppIdArgs {
    app_id: String,
}

pub fn run(args: ModesArgs, json: bool) -> Result<()> {
    match args.command {
        ModesCommand::List => list(json),
        ModesCommand::Show(a) => show(&a.id, json),
        ModesCommand::Add(a) => add(a, json),
        ModesCommand::Edit(a) => edit(a, json),
        ModesCommand::Delete(a) => delete(&a.id, json),
        ModesCommand::Use(a) => select(&a.id, json),
        ModesCommand::Cycle => cycle(json),
        ModesCommand::Resolve(a) => resolve(a, json),
        ModesCommand::App(a) => match a.command {
            AppCommand::List(a) if a.candidates => app_candidates(json),
            AppCommand::List(_) => app_list(json),
            AppCommand::Set(a) => app_set(a, json),
            AppCommand::Unset(a) => app_unset(&a.app_id, json),
        },
    }
}

fn settings_path() -> Result<PathBuf> {
    Ok(super::config_dir()?.join("settings.json"))
}

/// The saved modes block and the Refine settings (the built-in mode's
/// instructions live there).
fn load() -> Result<(PathBuf, ModesSettings, RefineSettings)> {
    let path = settings_path()?;
    let modes = settings::load_modes_settings(&path);
    let refine = settings::load_refine_settings(&path);
    Ok((path, modes, refine))
}

fn save(path: &Path, next: ModesSettings) -> Result<ModesSettings> {
    settings::save_modes_settings(path, next).map_err(|e| anyhow!("save modes: {e}"))
}

fn changed_hint() {
    eprintln!("a running app picks this up when it next reads its settings (on restart at the latest)");
}

/// Modes only do something while Refine is on. Said once, on stderr,
/// where it matters: when a mode is selected or looked up.
fn refine_off_hint(refine: &RefineSettings) {
    if !refine.enabled {
        eprintln!(
            "Refine is off, so dictations are typed as transcribed whatever the mode (openwhisper refine config --enabled true)"
        );
    }
}

fn parse_effort(text: &str) -> Result<RefineEffort> {
    RefineEffort::parse(text)
        .ok_or_else(|| anyhow!("unknown effort \"{text}\"; use minimal, low, medium, high, xhigh or max"))
}

fn parse_backend(text: &str) -> Result<RefineBackendKind> {
    RefineBackendKind::parse(text)
        .ok_or_else(|| anyhow!("unknown backend \"{text}\"; use claude-cli or openai-compatible"))
}

/// The backend a model or effort given on the command line is for:
/// `--backend` when it was passed, else the backend Refine uses.
/// `None` when neither a model nor an effort is given: then there is
/// nothing a backend could belong to, and `--backend` on its own is
/// a mistake worth saying.
fn backend_for_values(
    backend: Option<&str>,
    names_a_value: bool,
    active: RefineBackendKind,
) -> Result<Option<RefineBackendKind>> {
    match (backend, names_a_value) {
        (Some(text), true) => Ok(Some(parse_backend(text)?)),
        (Some(_), false) => bail!("--backend says which backend --model and --effort are for; pass one of them with it"),
        (None, true) => Ok(Some(active)),
        (None, false) => Ok(None),
    }
}

fn find(modes: &ModesSettings, refine: &RefineSettings, id: &str) -> Result<ModeView> {
    modes::get(modes, refine, id).ok_or_else(|| anyhow!("There is no mode with the id \"{id}\"."))
}

/// Model and effort of a mode for a table cell; empty when it follows
/// the Refine settings. When they are for another backend than the
/// one Refine uses (`backend`), the cell says so: they are not what a
/// dictation in the mode runs with right now.
fn own_settings(mode: &ModeView, backend: RefineBackendKind) -> String {
    let own = match (mode.model.as_deref(), mode.effort) {
        (Some(model), Some(effort)) => format!("{model}, {} effort", effort.as_str()),
        (Some(model), None) => model.to_string(),
        (None, Some(effort)) => format!("{} effort", effort.as_str()),
        (None, None) => return String::new(),
    };
    match mode.override_backend() {
        Some(for_backend) if for_backend != backend => {
            format!("{own} (for {}, not in use: same as Refine)", for_backend.as_str())
        }
        _ => own,
    }
}

/// One row of `modes list`.
fn list_row(mode: &ModeView, active: &str, backend: RefineBackendKind) -> String {
    let name = if mode.builtin { format!("{} (built in)", mode.name) } else { mode.name.clone() };
    format!(
        "{} {:<28} {:<42} {}",
        if mode.id == active { "*" } else { " " },
        mode.id,
        name,
        own_settings(mode, backend)
    )
    .trim_end()
    .to_string()
}

/// "selected" or "default for Mail".
fn reason_label(choice: &ModeChoice) -> String {
    match choice.reason {
        ModeReason::AppDefault => format!("default for {}", choice.app_name),
        _ => "selected".to_string(),
    }
}

/// A mode as JSON, with whether it is the selected one and whether
/// its own model and effort are in use with the backend Refine uses
/// (`overrides_apply`; false also when it has none).
fn mode_json(mode: &ModeView, modes: &ModesSettings, backend: RefineBackendKind) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(mode)?;
    value["selected"] = serde_json::json!(mode.id == modes.active);
    value["overrides_apply"] = serde_json::json!(mode.overrides_apply(backend));
    Ok(value)
}

fn print_mode(mode: &ModeView, modes: &ModesSettings, backend: RefineBackendKind) {
    println!("id            {}", mode.id);
    println!("name          {}{}", mode.name, if mode.builtin { " (built in)" } else { "" });
    println!("selected      {}", if mode.id == modes.active { "yes" } else { "no" });
    if !mode.builtin {
        println!("model         {}", mode.model.as_deref().unwrap_or("(same as Refine)"));
        println!(
            "effort        {}",
            mode.effort.map_or("(same as Refine)", RefineEffort::as_str)
        );
        if let Some(for_backend) = mode.override_backend() {
            println!(
                "for backend   {}{}",
                for_backend.as_str(),
                if for_backend == backend {
                    String::new()
                } else {
                    format!(" (Refine uses {} now, so this mode runs with the Refine model and effort)", backend.as_str())
                }
            );
        }
    }
    let apps: Vec<&str> = modes
        .app_defaults
        .iter()
        .filter(|d| d.mode == mode.id)
        .map(|d| if d.app_name.is_empty() { d.app_id.as_str() } else { d.app_name.as_str() })
        .collect();
    if !apps.is_empty() {
        println!("default for   {}", apps.join(", "));
    }
    println!();
    if !mode.instructions.is_empty() {
        println!("{}", mode.instructions);
    } else if mode.builtin {
        println!("(the default cleanup: no Refine instructions are set)");
    } else {
        println!("(no instructions: cleans up like Dictation)");
    }
}

fn list(json: bool) -> Result<()> {
    let (_, modes, refine) = load()?;
    let all = modes::list(&modes, &refine);
    if json {
        let rows: Vec<serde_json::Value> =
            all.iter().map(|m| mode_json(m, &modes, refine.backend)).collect::<Result<_>>()?;
        let value = serde_json::json!({ "active": modes.active, "modes": rows });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    for mode in &all {
        println!("{}", list_row(mode, &modes.active, refine.backend));
    }
    eprintln!("* = selected");
    refine_off_hint(&refine);
    Ok(())
}

fn show(id: &str, json: bool) -> Result<()> {
    let (_, modes, refine) = load()?;
    let mode = find(&modes, &refine, id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&mode_json(&mode, &modes, refine.backend)?)?);
    } else {
        print_mode(&mode, &modes, refine.backend);
    }
    Ok(())
}

/// Print the outcome of a command that changed or selected a mode.
fn report(mode: &ModeView, modes: &ModesSettings, refine: &RefineSettings, what: &str, json: bool) -> Result<()> {
    if json {
        let value = serde_json::json!({ "active": modes.active, "mode": mode_json(mode, modes, refine.backend)? });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{what} {} ({})", mode.name, mode.id);
        changed_hint();
    }
    Ok(())
}

fn add(args: AddArgs, json: bool) -> Result<()> {
    let (path, current, refine) = load()?;
    let effort = args.effort.as_deref().map(parse_effort).transpose()?;
    let backend =
        backend_for_values(args.backend.as_deref(), args.model.is_some() || effort.is_some(), refine.backend)?;
    let next = modes::add(&current, &args.name, &args.instructions, args.model.as_deref(), effort, backend)
        .map_err(|e| anyhow!(e))?;
    let saved = save(&path, next)?;
    let Some(id) = saved.custom.last().map(|m| m.id.clone()) else {
        bail!("the mode was not stored");
    };
    report(&find(&saved, &refine, &id)?, &saved, &refine, "Added", json)
}

/// The changes `modes edit` asks for. `active` is the backend Refine
/// uses: a model or effort given without `--backend` is for that one.
/// The clear flags carry no backend, so they clear what is stored,
/// whichever backend it was for.
fn edit_from(args: &EditArgs, active: RefineBackendKind) -> Result<ModeEdit> {
    let effort = match (&args.effort, args.clear_effort) {
        (Some(text), _) => Some(Some(parse_effort(text)?)),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let model = match (&args.model, args.clear_model) {
        (Some(model), _) => Some(Some(model.clone())),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let names_a_value = args.model.is_some() || args.effort.is_some();
    Ok(ModeEdit {
        name: args.name.clone(),
        instructions: args.instructions.clone(),
        model,
        effort,
        backend: backend_for_values(args.backend.as_deref(), names_a_value, active)?,
    })
}

fn edit(args: EditArgs, json: bool) -> Result<()> {
    let (path, current, refine) = load()?;
    let changes = edit_from(&args, refine.backend)?;
    if changes == ModeEdit::default() {
        bail!("nothing to change: pass --name, --instructions, --model, --effort, --clear-model or --clear-effort");
    }
    let next = modes::edit(&current, &args.id, &changes).map_err(|e| anyhow!(e))?;
    let saved = save(&path, next)?;
    report(&find(&saved, &refine, &args.id)?, &saved, &refine, "Changed", json)
}

fn delete(id: &str, json: bool) -> Result<()> {
    let (path, current, _) = load()?;
    let next = modes::delete(&current, id).map_err(|e| anyhow!(e))?;
    let saved = save(&path, next)?;
    if json {
        println!("{}", serde_json::json!({ "deleted": id, "active": saved.active }));
    } else {
        println!("Deleted the mode {id}.");
        if current.active == id {
            println!("It was the selected mode; Dictation is selected now.");
        }
        changed_hint();
    }
    Ok(())
}

fn select(id: &str, json: bool) -> Result<()> {
    let (path, current, refine) = load()?;
    let next = modes::select(&current, id).map_err(|e| anyhow!(e))?;
    let saved = save(&path, next)?;
    report(&find(&saved, &refine, &saved.active)?, &saved, &refine, "Selected", json)?;
    if !json {
        refine_off_hint(&refine);
    }
    Ok(())
}

fn cycle(json: bool) -> Result<()> {
    let (path, current, refine) = load()?;
    let saved = save(&path, modes::cycle(&current))?;
    report(&find(&saved, &refine, &saved.active)?, &saved, &refine, "Selected", json)?;
    if !json {
        if saved.custom.is_empty() {
            eprintln!("there is only the built-in mode, so there is nothing to cycle to");
        }
        refine_off_hint(&refine);
    }
    Ok(())
}

fn resolve(args: ResolveArgs, json: bool) -> Result<()> {
    let (_, modes, refine) = load()?;
    let choice = modes::resolve(&modes, &refine, args.app.as_deref().map(|app| (app, "")));
    if json {
        println!("{}", serde_json::to_string_pretty(&choice)?);
        return Ok(());
    }
    println!("mode    {} ({})", choice.mode.name, choice.mode.id);
    println!("reason  {}", reason_label(&choice));
    refine_off_hint(&refine);
    Ok(())
}

fn app_list(json: bool) -> Result<()> {
    let (_, modes, refine) = load()?;
    let name_of = |id: &str| modes::get(&modes, &refine, id).map(|m| m.name).unwrap_or_default();
    if json {
        let rows: Vec<serde_json::Value> = modes
            .app_defaults
            .iter()
            .map(|d| {
                serde_json::json!({
                    "app_id": d.app_id,
                    "app_name": d.app_name,
                    "mode": d.mode,
                    "mode_name": name_of(&d.mode),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if modes.app_defaults.is_empty() {
        println!("No app has a default mode. Every dictation runs in the selected mode.");
        return Ok(());
    }
    for d in &modes.app_defaults {
        println!(
            "{:<36} {:<20} {} ({})",
            d.app_id,
            if d.app_name.is_empty() { "-" } else { &d.app_name },
            name_of(&d.mode),
            d.mode
        );
    }
    Ok(())
}

/// `modes app list --candidates`: the apps a default can be given
/// to, from the same core function the app's Modes pane uses.
fn app_candidates(json: bool) -> Result<()> {
    let (_, modes, refine) = load()?;
    // The history folder may not exist (history is off by default);
    // then it lists no apps.
    let seen = super::history::history_dir().map(|dir| history::apps(&dir)).unwrap_or_default();
    let apps = modes::apps_for_defaults(&modes, &seen, &focus::running_apps());
    let default_of = |app_id: &str| modes.app_defaults.iter().find(|d| d.app_id == app_id);
    let name_of = |id: &str| modes::get(&modes, &refine, id).map(|m| m.name).unwrap_or_default();
    if json {
        let rows: Vec<serde_json::Value> = apps
            .iter()
            .map(|app| {
                let default = default_of(&app.id);
                serde_json::json!({
                    "app_id": app.id,
                    "app_name": app.name,
                    "mode": default.map(|d| d.mode.clone()),
                    "mode_name": default.map(|d| name_of(&d.mode)),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if apps.is_empty() {
        println!("No apps to offer: none is open, none is in the history and none has a default mode.");
        return Ok(());
    }
    for app in &apps {
        let default = match default_of(&app.id) {
            Some(d) => format!("{} ({})", name_of(&d.mode), d.mode),
            None => "-".to_string(),
        };
        println!("{:<36} {:<24} {}", app.id, app.name, default);
    }
    eprintln!("set a default with: openwhisper modes app set <app id> <mode id>");
    Ok(())
}

fn app_set(args: AppSetArgs, json: bool) -> Result<()> {
    let (path, current, refine) = load()?;
    let next = modes::set_app_default(
        &current,
        &args.app_id,
        args.name.as_deref().unwrap_or(""),
        &args.mode_id,
    )
    .map_err(|e| anyhow!(e))?;
    let saved = save(&path, next)?;
    let app_id = app_key(&args.app_id);
    let Some(entry) = saved.app_defaults.iter().find(|d| d.app_id == app_id) else {
        bail!("the default was not stored");
    };
    let mode = find(&saved, &refine, &entry.mode)?;
    if json {
        let value = serde_json::json!({
            "app_id": entry.app_id,
            "app_name": entry.app_name,
            "mode": entry.mode,
            "mode_name": mode.name,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "Dictations into {} run in {} ({}).",
            if entry.app_name.is_empty() { &entry.app_id } else { &entry.app_name },
            mode.name,
            mode.id
        );
        changed_hint();
    }
    Ok(())
}

fn app_unset(app_id: &str, json: bool) -> Result<()> {
    let (path, current, _) = load()?;
    let next = modes::clear_app_default(&current, app_id).map_err(|e| anyhow!(e))?;
    save(&path, next)?;
    if json {
        println!("{}", serde_json::json!({ "removed": app_key(app_id) }));
    } else {
        println!("{} follows the selected mode again.", app_id.trim());
        changed_hint();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::settings::{BUILTIN_MODE_ID, Mode};

    fn view(id: &str, name: &str, model: Option<&str>, effort: Option<RefineEffort>) -> ModeView {
        ModeView {
            id: id.into(),
            name: name.into(),
            instructions: "Rewrite it.".into(),
            model: model.map(str::to_string),
            effort,
            backend: (model.is_some() || effort.is_some()).then_some(CLAUDE),
            builtin: id == BUILTIN_MODE_ID,
        }
    }

    const CLAUDE: RefineBackendKind = RefineBackendKind::ClaudeCli;
    const OPENAI: RefineBackendKind = RefineBackendKind::OpenaiCompatible;

    fn edit_args(id: &str) -> EditArgs {
        EditArgs {
            id: id.into(),
            name: None,
            instructions: None,
            model: None,
            effort: None,
            clear_model: false,
            clear_effort: false,
            backend: None,
        }
    }

    #[test]
    fn efforts_parse_and_unknown_ones_name_the_choices() {
        assert_eq!(parse_effort("High").unwrap(), RefineEffort::High);
        let err = parse_effort("turbo").unwrap_err().to_string();
        assert!(err.contains("turbo") && err.contains("xhigh"), "{err}");
    }

    #[test]
    fn list_rows_mark_the_selected_mode_and_the_built_in_one() {
        let dictation = view(BUILTIN_MODE_ID, "Dictation", None, None);
        let formal = view("formal", "Formal", Some("opus"), Some(RefineEffort::High));
        assert_eq!(list_row(&dictation, "formal", CLAUDE), format!("  {:<28} Dictation (built in)", "dictation"));
        let row = list_row(&formal, "formal", CLAUDE);
        assert!(row.starts_with("* formal "), "{row}");
        assert!(row.ends_with("opus, high effort"), "{row}");
    }

    #[test]
    fn a_model_for_the_other_backend_is_shown_as_not_in_use() {
        let formal = view("formal", "Formal", Some("opus"), Some(RefineEffort::High));
        assert_eq!(
            own_settings(&formal, OPENAI),
            "opus, high effort (for claude-cli, not in use: same as Refine)"
        );
        // Stored before modes kept a backend: for the Claude CLI.
        let old = ModeView { backend: None, ..formal.clone() };
        assert_eq!(own_settings(&old, CLAUDE), "opus, high effort");
        assert!(own_settings(&old, OPENAI).contains("for claude-cli"));
        let modes = ModesSettings::default();
        assert_eq!(mode_json(&formal, &modes, CLAUDE).unwrap()["overrides_apply"], true);
        assert_eq!(mode_json(&formal, &modes, OPENAI).unwrap()["overrides_apply"], false);
        assert_eq!(mode_json(&formal, &modes, OPENAI).unwrap()["backend"], "claude-cli");
    }

    #[test]
    fn a_model_or_effort_is_for_the_backend_in_use_unless_one_is_named() {
        let mut args = edit_args("formal");
        args.model = Some("small-model".into());
        assert_eq!(edit_from(&args, OPENAI).unwrap().backend, Some(OPENAI));
        assert_eq!(edit_from(&args, CLAUDE).unwrap().backend, Some(CLAUDE));
        args.backend = Some("openai-compatible".into());
        assert_eq!(edit_from(&args, CLAUDE).unwrap().backend, Some(OPENAI));
        args.backend = Some("telepathy".into());
        assert!(edit_from(&args, CLAUDE).unwrap_err().to_string().contains("telepathy"));

        // Nothing a backend could be for: none is stored, and naming
        // one alone is refused.
        let mut args = edit_args("formal");
        args.name = Some("Business tone".into());
        args.clear_model = true;
        assert_eq!(edit_from(&args, OPENAI).unwrap().backend, None);
        args.backend = Some("claude-cli".into());
        let err = edit_from(&args, OPENAI).unwrap_err().to_string();
        assert!(err.contains("--model and --effort"), "{err}");
    }

    #[test]
    fn a_modes_own_model_and_effort_are_shown_only_when_set() {
        assert_eq!(own_settings(&view("a", "A", None, None), CLAUDE), "");
        assert_eq!(own_settings(&view("a", "A", Some("haiku"), None), CLAUDE), "haiku");
        assert_eq!(own_settings(&view("a", "A", None, Some(RefineEffort::Medium)), CLAUDE), "medium effort");
    }

    #[test]
    fn edit_flags_become_changes_and_clear_flags_hand_the_value_back() {
        assert_eq!(edit_from(&edit_args("formal"), CLAUDE).unwrap(), ModeEdit::default());

        let mut args = edit_args("formal");
        args.name = Some("Business tone".into());
        args.model = Some("opus".into());
        args.effort = Some("medium".into());
        let changes = edit_from(&args, CLAUDE).unwrap();
        assert_eq!(changes.name.as_deref(), Some("Business tone"));
        assert_eq!(changes.instructions, None);
        assert_eq!(changes.model, Some(Some("opus".to_string())));
        assert_eq!(changes.effort, Some(Some(RefineEffort::Medium)));

        let mut args = edit_args("formal");
        args.clear_model = true;
        args.clear_effort = true;
        let changes = edit_from(&args, CLAUDE).unwrap();
        assert_eq!((changes.model, changes.effort), (Some(None), Some(None)));

        let mut args = edit_args("formal");
        args.effort = Some("turbo".into());
        assert!(edit_from(&args, CLAUDE).is_err());
    }

    #[test]
    fn reasons_read_as_a_sentence_fragment() {
        let selected = ModeChoice {
            mode: view("formal", "Formal", None, None),
            reason: ModeReason::Selected,
            app_id: String::new(),
            app_name: String::new(),
        };
        assert_eq!(reason_label(&selected), "selected");
        let by_app = ModeChoice {
            reason: ModeReason::AppDefault,
            app_id: "com.example.mail".into(),
            app_name: "Mail".into(),
            ..selected
        };
        assert_eq!(reason_label(&by_app), "default for Mail");
    }

    #[test]
    fn mode_json_says_whether_the_mode_is_selected() {
        let modes = ModesSettings {
            active: "formal".into(),
            custom: vec![Mode { id: "formal".into(), name: "Formal".into(), ..Mode::default() }],
            app_defaults: vec![],
        };
        let value = mode_json(&view("formal", "Formal", None, None), &modes, CLAUDE).unwrap();
        assert_eq!(value["selected"], true);
        assert_eq!(value["id"], "formal");
        assert_eq!(value["builtin"], false);
        let value = mode_json(&view(BUILTIN_MODE_ID, "Dictation", None, None), &modes, CLAUDE).unwrap();
        assert_eq!(value["selected"], false);
        assert_eq!(value["builtin"], true);
    }
}
