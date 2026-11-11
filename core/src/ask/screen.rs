//! The screen in an ask (TASK-98): what an ask knows about a
//! screenshot, and the one place a screenshot can be asked for.
//!
//! The capture itself is [`crate::vision`] (cargo feature `vision`).
//! This file is the ask's side of it and is always compiled with
//! `ask`; without `vision` an ask simply never has a screenshot.
//!
//! **Only an ask can capture.** [`AskCaptureToken`] is what
//! [`crate::vision::capture_for_ask`] demands, and its constructors
//! are visible in `crate::ask` only. Nothing else in this crate, and
//! nothing outside it, can make one: not the dictation pipeline, not
//! a shell, not a timer. That is checked by the compiler. A token is
//! made right before the capture, is not `Clone`, and is checked
//! again when it is used: the ask it was made for must still be
//! running ([`AskCaptureToken::is_live`]).

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

use super::{AskEvent, AskImage};
use crate::settings::VisionSettings;

/// What became of the screen in one ask. On [`super::AskSnapshot`]
/// for the pill and the panel, and on [`super::AskReply`].
#[non_exhaustive]
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScreenState {
    /// No screenshot in this ask (so far).
    #[default]
    None,
    /// The screen is being captured right now.
    Capturing,
    /// A screenshot was sent with the question.
    Included,
    /// No screenshot, because an app on the never-capture list is in
    /// front. The notice says which.
    Blocked,
    /// A screenshot was wanted and could not be taken. The notice
    /// says why.
    Unavailable,
}

impl ScreenState {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Capturing => "capturing",
            Self::Included => "included",
            Self::Blocked => "blocked",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Whether an ask may include the screen.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VisionChoice {
    /// Never, whatever the settings say. What a plan starts with.
    #[default]
    None,
    /// As the Vision settings say ([`VisionSettings::trigger`]).
    Settings,
    /// Take a screenshot for this ask (`openwhisper ask run
    /// --screen`). The never-capture list and every other safeguard
    /// still apply.
    Force,
}

/// The Vision part of an [`super::AskPlan`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct AskVision {
    pub choice: VisionChoice,
    pub settings: VisionSettings,
}

/// What to do about the screen for one question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    NoCapture,
    Capture,
    /// Text first; the model may ask for the screen.
    AskModel,
}

impl AskVision {
    /// The decision for this question. Made here, on the machine.
    pub(super) fn decide(&self, question: &str) -> Step {
        match self.choice {
            VisionChoice::None => Step::NoCapture,
            VisionChoice::Force => Step::Capture,
            VisionChoice::Settings => from_settings(&self.settings, question),
        }
    }

    /// What the settings alone say will happen, before there is a
    /// question: for the prompt a backend is warmed with.
    pub(super) fn expected(&self) -> Step {
        use crate::settings::VisionTrigger;
        match self.choice {
            VisionChoice::None => Step::NoCapture,
            VisionChoice::Force => Step::Capture,
            VisionChoice::Settings if !cfg!(feature = "vision") => Step::NoCapture,
            VisionChoice::Settings => match self.settings.trigger {
                VisionTrigger::Off | VisionTrigger::Phrases => Step::NoCapture,
                VisionTrigger::Smart => Step::AskModel,
                VisionTrigger::Always => Step::Capture,
            },
        }
    }
}

#[cfg(feature = "vision")]
fn from_settings(settings: &VisionSettings, question: &str) -> Step {
    use crate::vision::Decision;
    match crate::vision::decide(settings, question) {
        Decision::NoCapture => Step::NoCapture,
        Decision::Capture { .. } => Step::Capture,
        Decision::AskModel => Step::AskModel,
    }
}

#[cfg(not(feature = "vision"))]
fn from_settings(_settings: &VisionSettings, _question: &str) -> Step {
    Step::NoCapture
}

/// What a capture came to.
#[cfg_attr(not(feature = "vision"), allow(dead_code))]
pub(super) enum Shot {
    Image(AskImage),
    Missing {
        /// [`ScreenState::Blocked`] or [`ScreenState::Unavailable`].
        state: ScreenState,
        /// For the person.
        notice: String,
        /// For the model.
        for_model: String,
    },
}

/// Take the screenshot for the ask `token` belongs to.
#[cfg(feature = "vision")]
pub(super) fn take(token: &AskCaptureToken, settings: &VisionSettings) -> Shot {
    match crate::vision::capture_for_ask(token, settings) {
        Ok(shot) => Shot::Image(shot.into_ask_image()),
        Err(no) => Shot::Missing {
            state: if no.is_blocked() { ScreenState::Blocked } else { ScreenState::Unavailable },
            notice: no.message(),
            for_model: no.for_model(),
        },
    }
}

#[cfg(not(feature = "vision"))]
pub(super) fn take(_token: &AskCaptureToken, _settings: &VisionSettings) -> Shot {
    Shot::Missing {
        state: ScreenState::Unavailable,
        notice: "No screenshot was taken: this build cannot capture the screen.".to_string(),
        for_model: "No screenshot was taken, because this version of the app cannot capture the screen.".to_string(),
    }
}

// --- the token -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    /// The app's one ask with this id.
    Session(u64),
    /// An ask outside the app's session ([`super::AskPlan::send`]).
    OneShot,
    #[cfg(all(test, feature = "vision"))]
    Test,
}

/// Proof that a capture is asked for by an ask. See the module docs.
#[derive(Debug)]
pub struct AskCaptureToken {
    // Read by `is_live`, which only a build that can capture calls.
    #[cfg_attr(not(feature = "vision"), allow(dead_code))]
    origin: Origin,
}

/// How many one-shot asks are between their start and their end
/// right now.
static ONE_SHOTS: AtomicUsize = AtomicUsize::new(0);

/// Held by a one-shot ask for as long as it runs.
pub(super) struct OneShot(());

impl OneShot {
    pub(super) fn begin() -> Self {
        ONE_SHOTS.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Drop for OneShot {
    fn drop(&mut self) {
        ONE_SHOTS.fetch_sub(1, Ordering::SeqCst);
    }
}

impl AskCaptureToken {
    /// For the app's ask `id`, made by the code that runs it.
    pub(super) fn for_session(id: u64) -> Self {
        Self { origin: Origin::Session(id) }
    }

    /// For an ask outside the app's session, made inside
    /// [`super::AskPlan::send`].
    pub(super) fn for_one_shot() -> Self {
        Self { origin: Origin::OneShot }
    }

    /// The ask this token was made for is still running: the app's
    /// ask with that id is thinking or answering, or a one-shot ask
    /// is under way. A capture is refused otherwise.
    #[cfg_attr(not(feature = "vision"), allow(dead_code))]
    pub(crate) fn is_live(&self) -> bool {
        match self.origin {
            Origin::Session(id) => super::is_running(id),
            Origin::OneShot => ONE_SHOTS.load(Ordering::SeqCst) > 0,
            #[cfg(all(test, feature = "vision"))]
            Origin::Test => true,
        }
    }

    /// A token that is always live, for unit tests of the capture
    /// layers. Does not exist outside tests.
    #[cfg(all(test, feature = "vision"))]
    pub(crate) fn for_test() -> Self {
        Self { origin: Origin::Test }
    }

    /// A token of an ask that is not running, for unit tests.
    #[cfg(test)]
    pub(crate) fn dead_for_test() -> Self {
        Self { origin: Origin::Session(u64::MAX) }
    }
}

// --- the marker ----------------------------------------------------

/// What the model replies, and nothing else, when it cannot answer
/// without the screen ([`crate::settings::VisionTrigger::Smart`]).
pub const NEED_SCREEN: &str = "NEED_SCREEN";

/// `text` without what a model may wrap a bare word in.
fn undecorated(text: &str) -> &str {
    text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '*' | '_' | '<' | '[' | '('))
}

/// How a reply relates to the marker, as far as it has arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// Could still become the marker.
    Maybe,
    /// Starts with the marker.
    Marker,
    /// Is something else.
    Text,
}

fn seen(text: &str) -> Seen {
    let probe = undecorated(text);
    let n = NEED_SCREEN.len();
    match probe.get(..n) {
        // Shorter than the marker (or cut inside a character, which
        // the marker does not have): a prefix of it, or not.
        None => {
            if probe.len() < n && probe.is_ascii() && NEED_SCREEN[..probe.len()].eq_ignore_ascii_case(probe) {
                Seen::Maybe
            } else {
                Seen::Text
            }
        }
        Some(head) if head.eq_ignore_ascii_case(NEED_SCREEN) => {
            // "NEED_SCREENSHOTS are..." is a word, not the marker.
            match probe[n..].chars().next() {
                Some(c) if c.is_alphanumeric() || c == '_' => Seen::Text,
                _ => Seen::Marker,
            }
        }
        Some(_) => Seen::Text,
    }
}

/// The reply is the marker: the model wants the screen.
pub(super) fn is_marker(reply: &str) -> bool {
    seen(reply) == Seen::Marker
}

/// Stands between a backend and the sink while the model may answer
/// with the marker, so that the marker never reaches the panel. Text
/// is held back until it is clear what the reply is: the marker can
/// arrive in pieces. Once it is clear that the reply is an answer,
/// everything held is passed on and the gate is open.
pub(super) struct MarkerGate {
    held: String,
    state: Seen,
}

impl MarkerGate {
    pub(super) fn new() -> Self {
        Self { held: String::new(), state: Seen::Maybe }
    }

    /// The event to pass on for `event`, if any.
    pub(super) fn pass(&mut self, event: AskEvent) -> Option<AskEvent> {
        let AskEvent::Text(piece) = event else {
            return Some(event);
        };
        match self.state {
            Seen::Text => Some(AskEvent::Text(piece)),
            // The reply began with the marker: nothing of it is shown.
            Seen::Marker => None,
            Seen::Maybe => {
                self.held.push_str(&piece);
                self.state = seen(&self.held);
                match self.state {
                    Seen::Text => Some(AskEvent::Text(std::mem::take(&mut self.held))),
                    _ => None,
                }
            }
        }
    }

    /// The reply began with the marker.
    pub(super) fn saw_marker(&self) -> bool {
        self.state == Seen::Marker
    }

    /// The turn is over and the reply was not the marker: what is
    /// still held (a short answer that happens to begin like it).
    pub(super) fn flush(&mut self) -> Option<AskEvent> {
        let held = std::mem::take(&mut self.held);
        self.state = Seen::Text;
        (!held.is_empty()).then_some(AskEvent::Text(held))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> AskEvent {
        AskEvent::Text(s.to_string())
    }

    /// Feed `pieces` through a gate; what came out, and whether the
    /// reply was the marker.
    fn through(pieces: &[&str]) -> (String, bool) {
        let mut gate = MarkerGate::new();
        let mut out = String::new();
        for piece in pieces {
            if let Some(AskEvent::Text(t)) = gate.pass(text(piece)) {
                out.push_str(&t);
            }
        }
        let whole: String = pieces.concat();
        let marker = gate.saw_marker() || is_marker(&whole);
        if !marker {
            if let Some(AskEvent::Text(t)) = gate.flush() {
                out.push_str(&t);
            }
        }
        (out, marker)
    }

    #[test]
    fn the_marker_never_passes_however_it_is_cut() {
        for cut in 0..=NEED_SCREEN.len() {
            let (a, b) = NEED_SCREEN.split_at(cut);
            assert_eq!(through(&[a, b]), (String::new(), true), "cut at {cut}");
        }
        let letters: Vec<String> = NEED_SCREEN.chars().map(String::from).collect();
        let letters: Vec<&str> = letters.iter().map(String::as_str).collect();
        assert_eq!(through(&letters), (String::new(), true));
        for wrapped in [
            "NEED_SCREEN\n",
            "  NEED_SCREEN  ",
            "`NEED_SCREEN`",
            "NEED_SCREEN.",
            "need_screen",
            "\"NEED_SCREEN\"",
            "**NEED_SCREEN**",
            "NEED_SCREEN\n\nI cannot see your screen.",
        ] {
            assert_eq!(through(&[wrapped]), (String::new(), true), "{wrapped}");
            assert!(is_marker(wrapped));
        }
        // Whatever follows the marker in later pieces stays back too.
        assert_eq!(through(&["NEED_", "SCREEN", " because", " I cannot see it"]), (String::new(), true));
    }

    #[test]
    fn an_answer_passes_whole_as_soon_as_it_is_clear_it_is_one() {
        assert_eq!(through(&["Four", "."]), ("Four.".to_string(), false));
        // Starts like the marker, then is not: nothing is lost.
        assert_eq!(through(&["N", "E", "ED", " more sleep."]), ("NEED more sleep.".to_string(), false));
        assert_eq!(through(&["Need", "_SCREENSHOTS are", " handy."]), ("Need_SCREENSHOTS are handy.".to_string(), false));
        // A short answer that is a prefix of the marker comes out at
        // the end of the turn.
        assert_eq!(through(&["NE"]), ("NE".to_string(), false));
        assert_eq!(through(&["Né", "e"]), ("Née".to_string(), false));
        assert_eq!(through(&["", ""]), (String::new(), false));
        assert!(!is_marker(""));
        assert!(!is_marker("I NEED_SCREEN"));
        assert!(!is_marker("NEED_SCREENS"));
    }

    #[test]
    fn the_first_piece_decides_when_it_is_long_enough() {
        let mut gate = MarkerGate::new();
        assert_eq!(gate.pass(text("The capital ")), Some(text("The capital ")));
        assert_eq!(gate.pass(text("NEED_SCREEN")), Some(text("NEED_SCREEN")));
        assert!(!gate.saw_marker());
        assert_eq!(gate.flush(), None);
    }

    #[test]
    fn other_events_always_pass() {
        let mut gate = MarkerGate::new();
        let searching = AskEvent::Searching { query: "ferry timetable".into() };
        assert_eq!(gate.pass(searching.clone()), Some(searching));
        assert_eq!(gate.pass(text("NEED_SCR")), None);
        assert_eq!(gate.pass(AskEvent::SearchDone), Some(AskEvent::SearchDone));
        assert_eq!(gate.pass(text("EEN")), None);
        assert!(gate.saw_marker());
    }

    #[test]
    fn a_token_is_live_only_while_its_ask_runs() {
        let _lock = crate::settings::CACHE_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // No ask with this id is running.
        assert!(!AskCaptureToken::for_session(u64::MAX).is_live());
        assert!(!AskCaptureToken::dead_for_test().is_live());
        // A one-shot token counts between the start and the end of a
        // one-shot ask. (Other tests run one-shot asks at the same
        // time, so "not live outside one" is checked on the counter
        // going back down, in the ask tests.)
        let token = AskCaptureToken::for_one_shot();
        let ask = OneShot::begin();
        assert!(token.is_live());
        assert!(ONE_SHOTS.load(Ordering::SeqCst) >= 1);
        drop(ask);
    }

    #[test]
    fn states_and_choices_have_stable_ids() {
        for (state, id) in [
            (ScreenState::None, "none"),
            (ScreenState::Capturing, "capturing"),
            (ScreenState::Included, "included"),
            (ScreenState::Blocked, "blocked"),
            (ScreenState::Unavailable, "unavailable"),
        ] {
            assert_eq!(state.as_str(), id);
            assert_eq!(serde_json::to_string(&state).unwrap(), format!("\"{id}\""));
        }
        assert_eq!(ScreenState::default(), ScreenState::None);
        assert_eq!(VisionChoice::default(), VisionChoice::None);
    }

    #[test]
    fn the_choice_overrides_the_settings_both_ways() {
        use crate::settings::VisionTrigger;
        let always = VisionSettings { trigger: VisionTrigger::Always, ..VisionSettings::default() };
        let off = VisionSettings { trigger: VisionTrigger::Off, ..VisionSettings::default() };
        let none = AskVision { choice: VisionChoice::None, settings: always.clone() };
        assert_eq!((none.decide("look at my screen"), none.expected()), (Step::NoCapture, Step::NoCapture));
        let force = AskVision { choice: VisionChoice::Force, settings: off.clone() };
        assert_eq!((force.decide("two plus two"), force.expected()), (Step::Capture, Step::Capture));
        #[cfg(feature = "vision")]
        {
            let follow = |settings: &VisionSettings| AskVision { choice: VisionChoice::Settings, settings: settings.clone() };
            assert_eq!(follow(&off).decide("look at my screen"), Step::NoCapture);
            assert_eq!(follow(&always).decide("two plus two"), Step::Capture);
            let phrases = VisionSettings::default();
            assert_eq!(follow(&phrases).decide("what is on my screen"), Step::Capture);
            assert_eq!(follow(&phrases).decide("two plus two"), Step::NoCapture);
            // Before the question is known, a phrase is not expected.
            assert_eq!(follow(&phrases).expected(), Step::NoCapture);
            let smart = VisionSettings { trigger: VisionTrigger::Smart, ..VisionSettings::default() };
            assert_eq!(follow(&smart).decide("two plus two"), Step::AskModel);
            assert_eq!(follow(&smart).expected(), Step::AskModel);
            assert_eq!(follow(&always).expected(), Step::Capture);
        }
    }
}
