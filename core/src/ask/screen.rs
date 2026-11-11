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

use super::prompt::ScreenPrompt;
use super::AskImage;
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

    /// What the system prompt says about the screen, for an ask that
    /// does (`has_image`) or does not go out with a screenshot.
    ///
    /// For every trigger but one this follows [`AskVision::expected`]:
    /// the prompt for what the settings alone say will happen, and
    /// the prompt for a screenshot when there is one after all (the
    /// caller brought an image).
    ///
    /// The phrases trigger is the exception, because there the answer
    /// to "will there be a screenshot" is only known once the
    /// question is. It gets one prompt for both outcomes
    /// ([`ScreenPrompt::IfAttached`]), so the prompt a backend is
    /// warmed with (`has_image` false, before the question) is the
    /// prompt the question is sent with (`has_image` true when a
    /// phrase matched), and the warmed backend is the one that
    /// answers. With two prompts, the asks that include the screen in
    /// the default setting were exactly the ones that paid for a cold
    /// start.
    pub(super) fn prompt(&self, has_image: bool) -> ScreenPrompt {
        use crate::settings::VisionTrigger;
        let phrases = self.choice == VisionChoice::Settings
            && cfg!(feature = "vision")
            && self.settings.trigger == VisionTrigger::Phrases;
        if phrases {
            return ScreenPrompt::IfAttached;
        }
        match self.expected() {
            Step::AskModel => ScreenPrompt::OnRequest,
            Step::Capture => ScreenPrompt::Attached,
            Step::NoCapture if has_image => ScreenPrompt::Attached,
            Step::NoCapture => ScreenPrompt::None,
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

/// Take the screenshot for the ask `token` belongs to. `name_app`:
/// the line for the model may name a protected app that is in front
/// ([`crate::vision::NoScreenshot::for_model_with`]); the sentence for
/// the person always does.
#[cfg(feature = "vision")]
pub(super) fn take(token: &AskCaptureToken, settings: &VisionSettings, name_app: bool) -> Shot {
    match crate::vision::capture_for_ask(token, settings) {
        Ok(shot) => Shot::Image(shot.into_ask_image()),
        Err(no) => Shot::Missing {
            state: if no.is_blocked() { ScreenState::Blocked } else { ScreenState::Unavailable },
            notice: no.message(),
            for_model: no.for_model_with(name_app),
        },
    }
}

#[cfg(not(feature = "vision"))]
pub(super) fn take(_token: &AskCaptureToken, _settings: &VisionSettings, _name_app: bool) -> Shot {
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
///
/// It never reaches the panel: the text of a reply is held back until
/// it is clear that the reply is not this word ([`super::hint`], which
/// looks for it as a [`super::hint::Form::Reply`] marker, together
/// with the web search marker when the ask knows that one too).
pub const NEED_SCREEN: &str = "NEED_SCREEN";

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_prompt_follows_the_trigger_and_phrases_has_one_for_both_outcomes() {
        use crate::settings::VisionTrigger;
        let with = |trigger: VisionTrigger, choice: VisionChoice| AskVision {
            choice,
            settings: VisionSettings { trigger, ..VisionSettings::default() },
        };
        // (without an image, with one)
        let both = |vision: &AskVision| (vision.prompt(false), vision.prompt(true));
        // No Vision for this plan, or switched off: the prompt from
        // before Vision, and the screenshot prompt for an image the
        // caller brought.
        for trigger in VisionTrigger::ALL {
            assert_eq!(both(&with(trigger, VisionChoice::None)), (ScreenPrompt::None, ScreenPrompt::Attached));
            // A forced capture is expected, whatever the trigger.
            assert_eq!(both(&with(trigger, VisionChoice::Force)), (ScreenPrompt::Attached, ScreenPrompt::Attached));
        }
        let follow = |trigger: VisionTrigger| both(&with(trigger, VisionChoice::Settings));
        assert_eq!(follow(VisionTrigger::Off), (ScreenPrompt::None, ScreenPrompt::Attached));
        #[cfg(feature = "vision")]
        {
            assert_eq!(follow(VisionTrigger::Always), (ScreenPrompt::Attached, ScreenPrompt::Attached));
            assert_eq!(follow(VisionTrigger::Smart), (ScreenPrompt::OnRequest, ScreenPrompt::OnRequest));
            // Phrases: the same prompt before the question is known,
            // without a match and with one. That is what lets the
            // warmed backend answer.
            assert_eq!(follow(VisionTrigger::Phrases), (ScreenPrompt::IfAttached, ScreenPrompt::IfAttached));
            // The decision per question is untouched by this.
            let phrases = with(VisionTrigger::Phrases, VisionChoice::Settings);
            assert_eq!(phrases.decide("what is on my screen"), Step::Capture);
            assert_eq!(phrases.decide("two plus two"), Step::NoCapture);
            assert_eq!(phrases.expected(), Step::NoCapture);
        }
        // A build that cannot capture never promises a screenshot.
        #[cfg(not(feature = "vision"))]
        for trigger in VisionTrigger::ALL {
            assert_eq!(follow(trigger), (ScreenPrompt::None, ScreenPrompt::Attached));
        }
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
            // Named, not the default: the trigger nobody chose is
            // another one on Windows.
            let phrases = VisionSettings { trigger: VisionTrigger::Phrases, ..VisionSettings::default() };
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
