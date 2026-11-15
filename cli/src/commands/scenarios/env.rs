//! Where a run of the scenarios lives: a scratch config folder, and
//! the settings of each scenario in it, written the way the settings
//! panes write them and read back the way the commands read them.
//!
//! A run never reads or writes a real OpenWhisper folder: not its
//! settings, not its dictionary, not its history, not its database.
//! [`refuse_real`] is checked before the folder is used, and the
//! folder must be empty or new.

use std::path::{Component, Path, PathBuf};

use openwhisper_core::dictionary;
use openwhisper_core::settings::{
    self, AgentSettings, AskSettings, ModesSettings, RefineSettings, VisionSettings,
};

use super::super::BUNDLE_ID;
use super::super::ask::{self as ask_cmd, Overrides};

/// The config folder of the development build.
const DEV_BUNDLE_ID: &str = "com.openwhisper.app.dev";

/// Every folder a real OpenWhisper keeps its settings, dictionary,
/// history or database in on this computer: the released app's and
/// the development build's, under the config folder and under the
/// data folder of the platform.
pub fn real_config_dirs() -> Vec<PathBuf> {
    let mut real: Vec<PathBuf> = Vec::new();
    for base in [dirs::config_dir(), dirs::data_dir()].into_iter().flatten() {
        for id in [BUNDLE_ID, DEV_BUNDLE_ID] {
            let dir = base.join(id);
            if !real.contains(&dir) {
                real.push(dir);
            }
        }
    }
    real
}

/// `path` made absolute, with links resolved as far as it exists: the
/// longest part that is there is resolved by the file system, the
/// rest is appended as written (without `.` and `..`).
fn resolved(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut tidy = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                tidy.pop();
            }
            other => tidy.push(other.as_os_str()),
        }
    }
    let mut there = tidy.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = there.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (there.file_name().map(|name| name.to_os_string()), there.parent().map(Path::to_path_buf)) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                there = parent;
            }
            _ => return tidy,
        }
    }
}

/// `inner` is `outer` or lies inside it. Names are compared ignoring
/// case: the folders in question are on file systems that do.
fn is_inside(inner: &Path, outer: &Path) -> bool {
    let inner: Vec<String> = inner.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    let outer: Vec<String> = outer.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    !outer.is_empty() && inner.len() >= outer.len() && inner[..outer.len()] == outer[..]
}

/// Refuse `dir` as the config folder of a run when it is one of the
/// `real` folders, lies inside one, or holds one. Looks at paths
/// only: nothing in a real folder is opened.
pub fn refuse_real(dir: &Path, real: &[PathBuf]) -> Result<(), String> {
    let wanted = resolved(dir);
    for folder in real {
        let folder = resolved(folder);
        if is_inside(&wanted, &folder) || is_inside(&folder, &wanted) {
            return Err(format!(
                "refusing to run: {} is a real OpenWhisper folder, or holds one ({}). The scenarios write settings \
of their own and only ever run in a scratch folder. Leave --config-dir out, or name an empty folder elsewhere.",
                dir.display(),
                folder.display()
            ));
        }
    }
    Ok(())
}

/// The settings a person has, block by block. The default is what a
/// person has who switched Ask on and touched nothing else.
#[derive(Clone, Debug)]
pub struct Person {
    pub ask: AskSettings,
    pub refine: RefineSettings,
    pub vision: VisionSettings,
    pub agent: AgentSettings,
    pub modes: ModesSettings,
}

impl Default for Person {
    fn default() -> Self {
        Self {
            ask: AskSettings { enabled: true, ..AskSettings::default() },
            refine: RefineSettings::default(),
            vision: VisionSettings::default(),
            agent: AgentSettings::default(),
            modes: ModesSettings::default(),
        }
    }
}

/// The scratch folder of one run.
#[derive(Debug)]
pub struct Env {
    pub dir: PathBuf,
    /// Made by this run, and removed at its end. A folder the caller
    /// named stays.
    own: bool,
    pub overrides: Overrides,
    /// This binary: the test fixture is its `agent fake-server`.
    pub exe: PathBuf,
}

/// What stands in for an API key, so that no scenario reads the
/// keychain.
const NO_KEY: &str = "not-a-real-key";

impl Env {
    /// Make the scratch folder (`wanted`, or a new one in the temp
    /// folder), refuse it when it is a real one or has anything in
    /// it, and point this process at it.
    pub fn make(wanted: Option<&Path>, overrides: Overrides) -> Result<Self, String> {
        let dir = match wanted {
            Some(dir) => dir.to_path_buf(),
            None => {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                std::env::temp_dir().join(format!("openwhisper-scenarios-{}-{stamp}", std::process::id()))
            }
        };
        refuse_real(&dir, &real_config_dirs())?;
        if std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some()) {
            return Err(format!(
                "refusing to run: {} has files in it. The scenarios write settings of their own; name an empty \
folder, or leave --config-dir out.",
                dir.display()
            ));
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not make the scratch folder {}: {e}", dir.display()))?;
        let dir = dir.canonicalize().unwrap_or(dir);
        // Once more, now that the folder exists and every link in its
        // path can be resolved.
        refuse_real(&dir, &real_config_dirs())?;
        // Everything in the CLI that works out a config folder gets
        // this one, and the key of an OpenAI-compatible endpoint is
        // never looked up in the keychain.
        //
        // SAFETY: this runs at the start of `scenarios run`, on the
        // main thread, before any other thread of this process
        // exists.
        unsafe {
            std::env::set_var("OPENWHISPER_CONFIG_DIR", &dir);
            if std::env::var_os(openwhisper_core::refine::secrets::ENV_API_KEY).is_none_or(|key| key.is_empty()) {
                std::env::set_var(openwhisper_core::refine::secrets::ENV_API_KEY, NO_KEY);
            }
        }
        let env = Self {
            dir,
            own: wanted.is_none(),
            overrides,
            exe: std::env::current_exe().map_err(|e| format!("could not find this binary: {e}"))?,
        };
        // A dictionary of the person's own, made up, where the app
        // would have theirs.
        let dictionary_path = env.dir.join(dictionary::FILE_NAME);
        std::fs::write(&dictionary_path, "{\n  \"terms\": [\n    { \"term\": \"Brimley\", \"heard_as\": [\"brimly\"] }\n  ]\n}\n")
            .map_err(|e| format!("could not write the scratch dictionary: {e}"))?;
        dictionary::set_path(&dictionary_path);
        Ok(env)
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    /// Write the settings of `person` as the only settings there are.
    pub fn person(&self, person: Person) -> Result<(), String> {
        let path = self.settings_path();
        let _ = std::fs::remove_file(&path);
        settings::save_refine_settings(&path, person.refine)?;
        settings::save_modes_settings(&path, person.modes)?;
        settings::save_ask_settings(&path, person.ask)?;
        settings::save_vision_settings(&path, person.vision)?;
        settings::save_agent_settings(&path, person.agent)?;
        Ok(())
    }

    /// The Ask and Refine settings as the commands load them: what
    /// is saved, with the overrides of this run on top.
    pub fn ask_and_refine(&self) -> Result<(AskSettings, RefineSettings), String> {
        ask_cmd::load_from(&self.settings_path(), &self.overrides).map_err(|e| e.to_string())
    }

    /// The Ask and Refine settings exactly as saved: for a scenario
    /// whose point is the backend it set up itself.
    pub fn ask_and_refine_as_saved(&self) -> (AskSettings, RefineSettings) {
        let path = self.settings_path();
        (settings::load_ask_settings(&path), settings::load_refine_settings(&path))
    }

    pub fn vision(&self) -> VisionSettings {
        settings::load_vision_settings(&self.settings_path())
    }

    pub fn agent(&self) -> AgentSettings {
        settings::load_agent_settings(&self.settings_path())
    }

    pub fn modes(&self) -> ModesSettings {
        settings::load_modes_settings(&self.settings_path())
    }

    /// A file of this run, by name.
    pub fn file(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Write a shell script that stands in for a command line tool,
    /// and return its path.
    #[cfg(unix)]
    pub fn script(&self, name: &str, body: &str) -> Result<PathBuf, String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = self.dir.join("bin");
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
        let path = dir.join(name);
        std::fs::write(&path, body).map_err(|e| format!("could not write {}: {e}", path.display()))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("could not make {} runnable: {e}", path.display()))?;
        Ok(path)
    }

    #[cfg(not(unix))]
    pub fn script(&self, name: &str, _body: &str) -> Result<PathBuf, String> {
        Err(format!("{name} is a shell script, and this system has no shell to run it"))
    }

    /// Remove the folder, when this run made it.
    pub fn remove(&self) {
        if self.own {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Why a scenario that needs a shell script as a stand-in cannot run
/// here; `None` where it can.
pub fn needs_a_shell() -> Option<String> {
    if cfg!(unix) {
        None
    } else {
        Some("needs a shell script as a stand-in for the Claude CLI, which this system cannot run".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/made-up/Application Support/com.openwhisper.app"),
            PathBuf::from("/made-up/Application Support/com.openwhisper.app.dev"),
        ]
    }

    #[test]
    fn a_real_folder_is_refused_and_so_is_anything_in_it_or_around_it() {
        for refused in [
            "/made-up/Application Support/com.openwhisper.app",
            "/made-up/Application Support/com.openwhisper.app/",
            "/made-up/Application Support/com.openwhisper.app.dev",
            "/made-up/Application Support/com.openwhisper.app/history",
            "/made-up/Application Support/com.openwhisper.app/a/b/c",
            // Around it: the settings would land next to the real folder.
            "/made-up/Application Support",
            "/made-up",
            // However it is spelled.
            "/made-up/Application Support/COM.OpenWhisper.App",
            "/made-up/Application Support/x/../com.openwhisper.app",
            "/made-up/Application Support/./com.openwhisper.app.dev/",
        ] {
            let why = refuse_real(Path::new(refused), &real()).expect_err(refused);
            assert!(why.starts_with("refusing to run: "), "{why}");
            assert!(why.contains("a real OpenWhisper folder"), "{why}");
            assert!(!why.contains(['\u{2014}', '\u{2013}']), "{why}");
        }
    }

    #[test]
    fn a_folder_elsewhere_is_accepted() {
        for accepted in [
            "/tmp/openwhisper-scenarios-1-2",
            "/made-up/Application Support/com.openwhisper.app.scratch",
            "/made-up/Application Support/com.openwhisper.application",
            "/made-up/Application Support/another.app",
            "/elsewhere/com.openwhisper.app",
        ] {
            assert_eq!(refuse_real(Path::new(accepted), &real()), Ok(()), "{accepted}");
        }
    }

    #[test]
    fn the_real_folders_of_this_computer_are_the_app_and_the_dev_build() {
        let real = real_config_dirs();
        // Nothing here opens them: these are names only.
        if let Some(base) = dirs::config_dir() {
            assert!(real.contains(&base.join("com.openwhisper.app")), "{real:?}");
            assert!(real.contains(&base.join("com.openwhisper.app.dev")), "{real:?}");
            assert!(refuse_real(&base.join("com.openwhisper.app"), &real).is_err());
            assert!(refuse_real(&base.join("com.openwhisper.app.dev").join("history"), &real).is_err());
        }
        assert!(refuse_real(&std::env::temp_dir().join("openwhisper-scenarios-test"), &real).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_real_folder_is_seen_through() {
        let base = std::env::temp_dir().join(format!("openwhisper-scenarios-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target = base.join("config").join("com.openwhisper.app");
        std::fs::create_dir_all(&target).unwrap();
        let link = base.join("looks-harmless");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let real = vec![target.clone()];
        assert!(refuse_real(&link, &real).is_err());
        assert!(refuse_real(&link.join("deeper"), &real).is_err());
        assert!(refuse_real(&base.join("scratch"), &real).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_person_who_only_switched_ask_on_has_the_defaults() {
        let person = Person::default();
        assert!(person.ask.enabled && !person.ask.web_search);
        assert!(!person.agent.enabled && person.agent.connections.is_empty());
        assert!(person.modes.custom.is_empty());
    }
}
