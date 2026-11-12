//! One ask of a scenario, made the way the app and `openwhisper ask
//! run` / `agent run` make it: the saved settings, the rule pass, the
//! facts about the computer, the Vision trigger, the Agent settings
//! with the app in front, then [`AskPlan::send_and_record`]. What
//! comes back is what a person would see ([`Seen`]).
//!
//! Two things differ from an ask in the app, and both keep the run
//! away from anything real:
//!
//! - the screen is always a stand-in ([`Screen`]): a blank image when
//!   the scenario expects no capture (so a capture that should not
//!   happen is counted, and still sees nothing), a synthetic note, or
//!   a made-up platform that refuses;
//! - a run with tools is refused unless its only connection is the
//!   test fixture.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use openwhisper_core::agent::{AgentRun, FrontApp, fixture};
use openwhisper_core::ask::{
    AskEvent, AskImage, AskInput, AskPlan, AskStep, CancelFlag, ConfirmDecision, ScreenState, StandIn, VisionChoice,
    answer_confirm,
};
use openwhisper_core::settings::{AgentConnectionKind, AgentSettings, AskSettings, RefineSettings};
use openwhisper_core::transcript;
use openwhisper_core::vision::testimg::{self, Line, Scene};
use openwhisper_core::vision::{
    AppSelector, DisplayInfo, FrontApp as ScreenApp, PermissionState, Platform, PlatformError, PointRect, RawCapture,
};

use super::Seen;
use super::env::Env;

/// What stands in for the screen in one ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Nothing to see: a blank window. For an ask that must not take
    /// a screenshot at all.
    Blank,
    /// A note with a fact on it.
    Note,
    /// A note that points at the thread of the fixture.
    ReviewNote,
    /// The process may not record the screen.
    NoPermission,
    /// An app on the never-capture list is in front: its id and name.
    Protected(&'static str, &'static str),
    /// Every display is off.
    NoDisplay,
}

/// The fact on [`Screen::Note`], as an answer would give it.
pub const NOTE_FACTS: &[&str] = &["friday", "9:30", "dentist"];

fn scene(screen: &Screen) -> Scene {
    let lines = match screen {
        Screen::Note => vec![Line::Big("Reminder"), Line::Gap, Line::Text("Dentist on Friday at 9:30")],
        Screen::ReviewNote => {
            vec![Line::Big("Reminder"), Line::Gap, Line::Text("Release review moved. Ask Anna which day.")]
        }
        _ => Vec::new(),
    };
    Scene { title: "Notes", lines }
}

/// A platform that is not this computer's: it has the permission or
/// not, one made-up app in front, and no display to capture. Every
/// step of a real capture runs against it
/// ([`StandIn::platform`]); no pixel ever leaves it.
struct MadeUpPlatform {
    permission: PermissionState,
    front: ScreenApp,
    /// How many times a capture was asked of it.
    asked: AtomicUsize,
}

impl Platform for MadeUpPlatform {
    fn permission(&self) -> PermissionState {
        self.permission
    }

    fn frontmost_apps(&self) -> Result<Vec<ScreenApp>, String> {
        Ok(vec![self.front.clone()])
    }

    fn window_rects(&self, _apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
        Ok(Vec::new())
    }

    fn capture(&self, _exclude: AppSelector<'_>, _at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Err(PlatformError::NoDisplay)
    }

    fn target_display(&self) -> Result<DisplayInfo, PlatformError> {
        Err(PlatformError::NoDisplay)
    }
}

/// The stand-in for `screen`, and whether a screenshot can leave it
/// at all (an image can, a platform that refuses cannot).
fn stand_in(screen: &Screen) -> Result<(StandIn, bool), String> {
    let platform = |permission, id: &str, name: &str| {
        let made_up = MadeUpPlatform {
            permission,
            front: ScreenApp { id: id.to_string(), name: name.to_string(), pid: 4242 },
            asked: AtomicUsize::new(0),
        };
        (StandIn::platform(Arc::new(made_up)), false)
    };
    Ok(match screen {
        Screen::Blank | Screen::Note | Screen::ReviewNote => {
            let data = testimg::png(&scene(screen)).map_err(|e| format!("could not draw the stand-in screen: {e}"))?;
            (StandIn::new(AskImage { media_type: "image/png".into(), data }), true)
        }
        Screen::NoPermission => platform(PermissionState::Denied, "com.example.editor", "Editor"),
        Screen::Protected(id, name) => platform(PermissionState::Granted, id, name),
        Screen::NoDisplay => platform(PermissionState::Granted, "com.example.editor", "Editor"),
    })
}

/// One ask of a scenario.
#[derive(Clone, Debug)]
pub struct Asking {
    /// The question as heard or typed. The rule pass runs on it, as
    /// for every spoken question.
    pub question: String,
    /// The app in front: id and name. `None` when the scenario has
    /// none.
    pub front: Option<(&'static str, &'static str)>,
    pub screen: Screen,
    /// Use the Ask and Refine settings exactly as saved, without the
    /// overrides of the run: the scenario set the backend up itself.
    pub as_saved: bool,
    /// The log of the fixture, when the scenario has one.
    pub fixture_log: Option<PathBuf>,
    /// Cancel this long after the first piece of the answer.
    pub cancel_after_first_text: Option<Duration>,
}

impl Asking {
    pub fn new(question: &str) -> Self {
        Self {
            question: question.to_string(),
            front: None,
            screen: Screen::Blank,
            as_saved: false,
            fixture_log: None,
            cancel_after_first_text: None,
        }
    }
}

/// Refuse a run with tools whose connections are anything but the
/// test fixture: a scenario never starts, lists or calls a real
/// connection.
pub fn only_the_fixture(agent: &AgentSettings, exe: &std::path::Path) -> Result<(), String> {
    for connection in &agent.connections {
        let custom = connection.custom.as_ref();
        let is_fixture = connection.kind == AgentConnectionKind::Custom
            && custom.is_some_and(|c| {
                std::path::Path::new(&c.command) == exe
                    && c.url.is_empty()
                    && c.args.first().map(String::as_str) == Some("agent")
                    && c.args.get(1).map(String::as_str) == Some("fake-server")
            });
        if !is_fixture {
            return Err(format!(
                "the connection \"{}\" is not the test fixture; a scenario uses no other connection",
                connection.name
            ));
        }
    }
    Ok(())
}

/// A tool call as the panel shows it.
fn step_line(step: &AskStep) -> String {
    format!("{} on {}: {}", step.tool, step.connection, step.state.as_str())
}

/// Ask, and report what a person would see. Never panics and never
/// returns early without saying why: a scenario that cannot be set up
/// comes back as [`Seen::broken`].
pub fn ask(env: &Env, asking: &Asking) -> Seen {
    let started = Instant::now();
    let (ask, refine) = if asking.as_saved {
        env.ask_and_refine_as_saved()
    } else {
        match env.ask_and_refine() {
            Ok(both) => both,
            Err(why) => return Seen::broken(why),
        }
    };
    ask_with(env, asking, &ask, &refine, started)
}

/// [`ask`] with the settings given, and the clock started by the
/// caller (a spoken scenario starts it before the transcription).
pub fn ask_with(env: &Env, asking: &Asking, ask: &AskSettings, refine: &RefineSettings, started: Instant) -> Seen {
    // The rule pass of a spoken question, as `ask run` makes it.
    let question = transcript::process(&asking.question).trim().to_string();
    if question.is_empty() {
        return Seen::broken("the question is empty after the rule pass");
    }
    let vision = env.vision();
    let agent = env.agent();
    if let Err(why) = only_the_fixture(&agent, &env.exe) {
        return Seen::broken(why);
    }
    let (stand_in, yields_images) = match stand_in(&asking.screen) {
        Ok(made) => made,
        Err(why) => return Seen::broken(why),
    };

    // The app in front, and the tools of this ask when Agent is on:
    // what the app does for an ask.
    let front = asking.front.map(|(id, name)| FrontApp::new(id, name));
    let run = AgentRun::for_ask(&agent, refine.backend, front.as_ref());
    let plan = AskPlan::for_app(ask, refine, asking.front.map(|(_, name)| name))
        .with_vision(&vision, VisionChoice::Settings)
        .with_agent(run)
        .with_screen_stand_in(stand_in.clone());

    let cancel = CancelFlag::new();
    let cancelled_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let mut seen = Seen { question: question.clone(), ..Seen::default() };
    let mut steps: Vec<AskStep> = Vec::new();
    let mut screen = (ScreenState::None, String::new());
    let mut canceller_started = false;
    let mut sink = |event: AskEvent| match event {
        AskEvent::Text(_) => {
            if let Some(after) = asking.cancel_after_first_text.filter(|_| !canceller_started) {
                canceller_started = true;
                // The person presses the cancel key a moment into
                // the answer.
                let (cancel, cancelled_at) = (cancel.clone(), cancelled_at.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(after);
                    *cancelled_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
                    cancel.cancel();
                });
            }
        }
        AskEvent::Searching { .. } => seen.searched = true,
        AskEvent::Step(step) => match steps.iter_mut().find(|s| s.id == step.id) {
            Some(existing) => *existing = step,
            None => steps.push(step),
        },
        // No scenario has a tool that asks first. Should one ask, the
        // person of a scenario says no.
        AskEvent::Confirm(confirm) => {
            answer_confirm(&confirm.request_id, ConfirmDecision::Deny);
        }
        AskEvent::Screen { state, notice } if state != ScreenState::Capturing => screen = (state, notice),
        _ => {}
    };
    let mut backend = plan.backend();
    // The same call `ask run` and `agent run` make. No database is
    // registered and history is not asked for, so it leaves nothing
    // behind.
    let result = plan.send_and_record(backend.as_mut(), &AskInput::text(question), &mut sink, &cancel, false);
    // The CLI goes, and the fixture under it: the log is complete.
    drop(backend);
    let ended = Instant::now();

    seen.elapsed_ms = ended.duration_since(started).as_millis() as u64;
    seen.cancel_to_end_ms = cancelled_at
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .map(|at| ended.saturating_duration_since(at).as_millis() as u64);
    seen.captures = if yields_images { stand_in.taken() } else { 0 };
    seen.calls = asking
        .fixture_log
        .as_deref()
        .map(|log| fixture::read_log(log).into_iter().map(|call| call.tool).collect())
        .unwrap_or_default();
    match result {
        Ok(reply) => {
            seen.answer = reply.text;
            seen.notice = reply.notice;
            seen.suggestion = reply.suggestion.map(|s| s.as_str().to_string()).unwrap_or_default();
            seen.screen = reply.screen.as_str().to_string();
            seen.screen_notice = reply.screen_notice;
            seen.steps = reply.steps.iter().map(step_line).collect();
            seen.context_source = reply.context_source.label();
        }
        Err(e) => {
            seen.error = e.to_string();
            seen.screen = screen.0.as_str().to_string();
            seen.screen_notice = screen.1;
            seen.steps = steps.iter().map(step_line).collect();
            seen.context_source = plan.routed_to().map_or("none".to_string(), |name| format!("connection:{name}"));
        }
    }
    seen
}

/// What a plain ask leaves on [`Seen`] besides the answer: for the
/// ideal outcomes of the offline self-test.
pub fn answered(question: &str, answer: &str) -> Seen {
    Seen {
        question: question.to_string(),
        answer: answer.to_string(),
        screen: "none".to_string(),
        context_source: "none".to_string(),
        elapsed_ms: 3_000,
        ..Seen::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwhisper_core::settings::{AgentConnection, CustomServer};

    fn connection(kind: AgentConnectionKind, command: &str, args: &[&str]) -> AgentConnection {
        AgentConnection {
            name: "teamchat".into(),
            kind,
            enabled: true,
            custom: Some(CustomServer {
                command: command.into(),
                args: args.iter().map(|a| (*a).to_string()).collect(),
                ..CustomServer::default()
            }),
            ..AgentConnection::default()
        }
    }

    #[test]
    fn only_the_fixture_may_be_a_connection_of_a_scenario() {
        let exe = std::path::Path::new("/made-up/openwhisper");
        let with = |connection| AgentSettings { connections: vec![connection], ..AgentSettings::default() };
        let fixture = connection(AgentConnectionKind::Custom, "/made-up/openwhisper", &["agent", "fake-server", "--log", "x"]);
        assert_eq!(only_the_fixture(&with(fixture.clone()), exe), Ok(()));
        assert_eq!(only_the_fixture(&AgentSettings::default(), exe), Ok(()));
        // A connection of the person's Claude Code, another program,
        // this binary with another command, a URL: all refused.
        let mut remote = fixture.clone();
        remote.custom.as_mut().unwrap().url = "https://example.com/mcp".into();
        for other in [
            connection(AgentConnectionKind::Claude, "/made-up/openwhisper", &["agent", "fake-server"]),
            connection(AgentConnectionKind::Custom, "/usr/bin/some-server", &["agent", "fake-server"]),
            connection(AgentConnectionKind::Custom, "/made-up/openwhisper", &["agent", "run"]),
            AgentConnection { custom: None, ..fixture.clone() },
            remote,
        ] {
            let why = only_the_fixture(&with(other), exe).unwrap_err();
            assert!(why.contains("is not the test fixture"), "{why}");
        }
    }

    #[test]
    fn a_platform_that_refuses_never_yields_an_image() {
        for screen in [Screen::NoPermission, Screen::Protected("com.example.vault", "Vault"), Screen::NoDisplay] {
            let (_, yields) = stand_in(&screen).unwrap();
            assert!(!yields, "{screen:?}");
        }
        for screen in [Screen::Blank, Screen::Note, Screen::ReviewNote] {
            let (stand_in, yields) = stand_in(&screen).unwrap();
            assert!(yields && stand_in.taken() == 0, "{screen:?}");
        }
        // What the notes say is what the checks look for.
        assert!(scene(&Screen::Note).texts().iter().any(|t| t.contains("Dentist on Friday at 9:30")));
        assert!(scene(&Screen::Blank).lines.is_empty());
    }
}
