//! The everyday group: the questions a person asks on an ordinary
//! day. The weather with web search on and off, what day it is, which
//! app they are in, which computer this is, the same three with the
//! facts about the computer switched off, a question in Danish, and
//! an answer without long dashes.
//!
//! Every scenario writes the Ask settings the person would have and
//! asks through [`asking::ask`]. The place is typed ("Oslo"); nothing
//! uses location services. The app in front is made up and handed in
//! the way `ask run --front-app` hands it in.

use chrono::Datelike;
use openwhisper_core::ask::context::{self, DeviceContext};
use openwhisper_core::settings::ContextSettings;

use super::asking::{self, Asking, answered};
use super::checks::{
    ASKS_WHERE, CANNOT, USUAL, has_any, has_none, has_temperature, in_danish, names_a_day, no_long_dashes,
    plain_answer,
};
use super::env::{Env, Person};
use super::{Group, Host, Samples, Scenario, Seen};

/// Where the person of these scenarios says they are.
const PLACE: &str = "Oslo";
/// The app they are working in. Made up.
const APP: (&str, &str) = ("com.example.harbornotes", "Harbor Notes");

const WEATHER: &str = "what is the weather like today";
const WHAT_DAY: &str = "what day is it today";
const WHICH_APP: &str = "which app am i in right now";
const WHAT_COMPUTER: &str = "what computer is this";
const DANISH: &str = "hvad er forskellen p\u{e5} vejr og klima";
const TOWER: &str = "in two sentences tell me about the eiffel tower and mention in passing when it was built";

const DANISH_WEEKDAYS: [&str; 7] = ["mandag", "tirsdag", "onsdag", "torsdag", "fredag", "l\u{f8}rdag", "s\u{f8}ndag"];

/// Today's weekday as an answer may name it: in English and in
/// Danish.
fn weekday_now() -> [String; 2] {
    let now = chrono::Local::now();
    let at = now.weekday().num_days_from_monday() as usize;
    [now.format("%A").to_string(), DANISH_WEEKDAYS[at].to_string()]
}

/// What names this computer in an answer: its name, its model id and
/// its chip, as far as the app could read them.
fn computer_facts(device: &DeviceContext) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    let mut add = |fact: &str| {
        let fact = fact.trim();
        if fact.chars().count() >= 3 && !facts.iter().any(|f| f.eq_ignore_ascii_case(fact)) {
            facts.push(fact.to_string());
        }
    };
    // "Name (size), ModelId": the id has a comma of its own in it.
    let mut parts = device.computer.split(", ");
    add(parts.next().unwrap_or_default().split('(').next().unwrap_or_default());
    if let Some(model_id) = parts.last() {
        add(model_id);
    }
    add(&device.chip);
    add(device.chip.trim_start_matches("Apple "));
    facts
}

/// What `openwhisper ask context` reports about this computer, with
/// every group on.
fn this_computer() -> Vec<String> {
    computer_facts(&context::gather(&ContextSettings::default(), None))
}

/// The version in "macOS 26.0" or "Windows 11 (26100)": the first
/// word that starts with a digit and has a dot in it.
fn version_in(os: &str) -> Option<&str> {
    os.split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()) && word.contains('.'))
}

/// What the system says about itself to a program that asks
/// (`uname -s`, `uname -r`): the name and the release of its kernel.
/// Empty where there is no `uname`.
fn kernel() -> Vec<String> {
    ["-s", "-r"]
        .iter()
        .filter_map(|flag| std::process::Command::new("uname").arg(flag).output().ok())
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|text| text.chars().count() >= 3)
        .collect()
}

/// Everything that would give this computer away in an answer that
/// was told nothing about it: what names it ([`this_computer`]), the
/// version of its operating system, and its kernel.
fn nothing_of_this_computer() -> Vec<String> {
    let device = context::gather(&ContextSettings::default(), None);
    let mut facts = computer_facts(&device);
    facts.extend(version_in(&device.os).map(str::to_string));
    for fact in kernel() {
        if !facts.contains(&fact) {
            facts.push(fact);
        }
    }
    facts
}

fn nothing_about_the_computer() -> ContextSettings {
    ContextSettings { date_time: false, computer: false, front_app: false, locale: false, place: String::new() }
}

/// Write the settings of `person` and ask.
fn asked(env: &Env, person: Person, asking: Asking) -> Seen {
    if let Err(why) = env.person(person) {
        return Seen::broken(why);
    }
    asking::ask(env, &asking)
}

// --- the weather ----------------------------------------------------

fn run_weather_on(env: &Env) -> Seen {
    let mut person = Person::default();
    person.ask.web_search = true;
    person.ask.context.place = PLACE.to_string();
    asked(env, person, Asking::new(WEATHER))
}

/// Ways of saying that the search came back without a forecast.
const FOUND_NONE: &[&str] = &[
    "couldn't find",
    "could not find",
    "couldn't get",
    "could not get",
    "unable to find",
    "unable to get",
    "wasn't able to",
    "was not able to",
    "didn't find",
    "did not find",
    "no reliable",
    "kunne ikke finde",
];

fn check_weather_on(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    if !seen.searched {
        return Err("the model did not search the web, so the forecast is not from today".to_string());
    }
    has_any(&seen.answer, &[PLACE]).map_err(|_| format!("the answer does not name {PLACE}"))?;
    // What is usual in November is not a forecast, with or without
    // a temperature in it.
    has_none(&seen.answer, FOUND_NONE).map_err(|why| format!("it searched and has no forecast for today: {why}"))?;
    if !has_temperature(&seen.answer) {
        return Err("the answer has no temperature (a number with a degree sign or the word degrees)".to_string());
    }
    has_none(&seen.answer, ASKS_WHERE).map_err(|why| format!("it asks where the person is although they said: {why}"))?;
    if !seen.suggestion.is_empty() {
        return Err("the hint to switch web search on is shown although it is on".to_string());
    }
    Ok(())
}

fn run_weather_off(env: &Env) -> Seen {
    let mut person = Person::default();
    person.ask.context.place = PLACE.to_string();
    asked(env, person, Asking::new(WEATHER))
}

/// Ways of stating today's weather as something that was looked up.
const STATES_IT: &[&str] = &[
    "it is currently",
    "it's currently",
    "right now it is",
    "right now it's",
    "today's forecast is",
    "the forecast for today is",
    "the forecast today is",
];

fn check_weather_off(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    if seen.searched {
        return Err("the model searched the web although web search is off".to_string());
    }
    if seen.suggestion != "web-search" {
        return Err("the hint that points to the web search setting is missing".to_string());
    }
    // What is usual in November may be said; a forecast may not.
    if has_temperature(&seen.answer) && has_any(&seen.answer, USUAL).is_err() {
        return Err("it states a temperature as if it had looked it up".to_string());
    }
    has_none(&seen.answer, STATES_IT).map_err(|why| format!("it states a forecast it cannot have: {why}"))?;
    has_none(&seen.answer, ASKS_WHERE).map_err(|why| format!("it asks where the person is although they said: {why}"))
}

// --- the facts about the computer ----------------------------------

fn run_what_day(env: &Env) -> Seen {
    let before = weekday_now();
    let mut seen = asked(env, Person::default(), Asking::new(WHAT_DAY));
    // A run across midnight has two right answers.
    seen.facts = before.into_iter().chain(weekday_now()).collect();
    seen.facts.dedup();
    seen
}

/// The answer has one of the facts the scenario gathered.
fn has_a_fact(seen: &Seen, what: &str) -> Result<(), String> {
    if seen.facts.is_empty() {
        return Err(format!("{what} could not be read on this computer, so there is nothing to compare with"));
    }
    let facts: Vec<&str> = seen.facts.iter().map(String::as_str).collect();
    has_any(&seen.answer, &facts).map_err(|_| format!("the answer does not say {what} ({})", seen.facts.join(" / ")))
}

fn check_what_day(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    has_a_fact(seen, "today's weekday")
}

fn run_which_app(env: &Env) -> Seen {
    let mut seen = asked(env, Person::default(), Asking { front: Some(APP), ..Asking::new(WHICH_APP) });
    seen.facts = vec![APP.1.to_string()];
    seen
}

fn check_which_app(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    has_a_fact(seen, "the app in front")
}

fn run_what_computer(env: &Env) -> Seen {
    let mut seen = asked(env, Person::default(), Asking::new(WHAT_COMPUTER));
    seen.facts = this_computer();
    seen
}

fn check_what_computer(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    has_a_fact(seen, "which computer this is")
}

// --- the same, switched off -----------------------------------------

fn switched_off(env: &Env, asking: Asking) -> Seen {
    let mut person = Person::default();
    person.ask.context = nothing_about_the_computer();
    asked(env, person, asking)
}

/// The answer says that it cannot know.
fn says_it_cannot_know(seen: &Seen) -> Result<(), String> {
    has_any(&seen.answer, CANNOT).map_err(|_| "the answer does not say that it cannot know".to_string())
}

fn run_off_day(env: &Env) -> Seen {
    switched_off(env, Asking::new(WHAT_DAY))
}

fn check_off_day(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    if let Some(day) = names_a_day(&seen.answer) {
        return Err(format!("it names a day (\"{day}\") although it was not told the date"));
    }
    says_it_cannot_know(seen)
}

fn run_off_app(env: &Env) -> Seen {
    // The app is in front all the same: the switch is what keeps its
    // name from the model.
    let mut seen = switched_off(env, Asking { front: Some(APP), ..Asking::new(WHICH_APP) });
    seen.facts = vec![APP.1.to_string()];
    seen
}

/// None of the facts the scenario gathered is in the answer.
fn has_no_fact(seen: &Seen, what: &str) -> Result<(), String> {
    let facts: Vec<&str> = seen.facts.iter().map(String::as_str).collect();
    has_none(&seen.answer, &facts).map_err(|why| format!("{what} reached the model although that fact is switched off: {why}"))
}

fn check_off_app(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    has_no_fact(seen, "the app in front")?;
    says_it_cannot_know(seen)
}

fn run_off_computer(env: &Env) -> Seen {
    let mut seen = switched_off(env, Asking::new(WHAT_COMPUTER));
    seen.facts = nothing_of_this_computer();
    seen
}

fn check_off_computer(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    has_no_fact(seen, "which computer this is")?;
    says_it_cannot_know(seen)
}

// --- Danish, and no long dashes -------------------------------------

fn run_danish(env: &Env) -> Seen {
    asked(env, Person::default(), Asking::new(DANISH))
}

fn check_danish(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    in_danish(&seen.answer)?;
    has_any(&seen.answer, &["klima"]).map_err(|_| "the answer is not about the question (no \"klima\")".to_string())
}

fn run_no_long_dashes(env: &Env) -> Seen {
    // The switch is on for a person who never touched it.
    asked(env, Person::default(), Asking::new(TOWER))
}

fn check_no_long_dashes(seen: &Seen) -> Result<(), String> {
    plain_answer(seen)?;
    no_long_dashes(&seen.answer)?;
    has_any(&seen.answer, &["1889", "1887"]).map_err(|_| "the answer does not say when the tower was built".to_string())
}

// --- for the offline self-test --------------------------------------

fn with_facts(seen: Seen, facts: &[&str]) -> Seen {
    Seen { facts: facts.iter().map(|f| (*f).to_string()).collect(), ..seen }
}

fn saying(ideal: &Seen, answer: &str) -> Seen {
    Seen { answer: answer.to_string(), ..ideal.clone() }
}

/// The mistakes every plain ask must be caught at.
fn common(ideal: &Seen) -> Vec<(&'static str, Seen)> {
    vec![
        ("no answer", Seen { answer: String::new(), error: "no answer in time".into(), ..ideal.clone() }),
        ("a screenshot was taken", Seen { captures: 1, screen: "included".into(), ..ideal.clone() }),
        ("a tool was called", Seen { steps: vec!["read_thread on teamchat: done".into()], ..ideal.clone() }),
    ]
}

fn samples_weather_on() -> Samples {
    let ideal = Seen {
        searched: true,
        elapsed_ms: 11_000,
        ..answered(WEATHER, "In Oslo it is about 3\u{b0}C and overcast today, with light rain in the afternoon.")
    };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("no forecast in the answer", saying(&ideal, "Oslo has a cool autumn day ahead.")),
        ("another place", saying(&ideal, "In Bergen it is about 6\u{b0}C and raining today.")),
        (
            "a search without a forecast, and what is usual in its place",
            saying(&ideal, "I couldn't find a reliable forecast for Oslo today. In mid November it is usually around 0 to 5 \u{b0}C."),
        ),
        ("asks where the person is", saying(&ideal, "Which city are you in? In Oslo it is 3\u{b0}C.")),
        ("no web search was made", Seen { searched: false, ..ideal.clone() }),
        ("the hint is shown although web search is on", Seen { suggestion: "web-search".into(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_weather_off() -> Samples {
    let ideal = Seen {
        suggestion: "web-search".into(),
        ..answered(
            WEATHER,
            "I can't check today's weather in Oslo from here. In mid November it is usually around 0 to 4 degrees there.",
        )
    };
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("the hint is missing", Seen { suggestion: String::new(), ..ideal.clone() }),
        ("a made-up forecast", saying(&ideal, "In Oslo it is 3\u{b0}C and overcast today.")),
        ("a forecast stated as fact", saying(&ideal, "It's currently cloudy in Oslo with light rain.")),
        ("asks where the person is", saying(&ideal, "I can't check the weather. Which city are you in?")),
        ("a web search although it is off", Seen { searched: true, ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_what_day() -> Samples {
    let ideal = with_facts(answered(WHAT_DAY, "Today is Thursday, 12 November 2026."), &["Thursday", "torsdag"]);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("the wrong day", saying(&ideal, "Today is Wednesday.")),
        ("says it cannot know", saying(&ideal, "I don't have access to today's date.")),
        ("nothing to compare with", Seen { facts: Vec::new(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_which_app() -> Samples {
    let ideal = with_facts(answered(WHICH_APP, "You are in Harbor Notes."), &[APP.1]);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("says it cannot know", saying(&ideal, "I can't see which app you are using.")),
        ("another app", saying(&ideal, "You are in Editor.")),
    ]);
    Samples { ideal, mistakes }
}

fn samples_what_computer() -> Samples {
    let facts = ["Northwind Book Pro", "Book14,2", "Northwind N3"];
    let ideal = with_facts(answered(WHAT_COMPUTER, "A Northwind Book Pro with a Northwind N3 chip and 32 GB of memory."), &facts);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("says it cannot know", saying(&ideal, "I have no way to tell which computer you are on.")),
        ("another computer", saying(&ideal, "This is a Tidewater Desk Mini.")),
        ("nothing to compare with", Seen { facts: Vec::new(), ..ideal.clone() }),
    ]);
    Samples { ideal, mistakes }
}

fn samples_off_day() -> Samples {
    let ideal = answered(WHAT_DAY, "I don't have access to the current date, so I can't tell you what day it is.");
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("a confident weekday", saying(&ideal, "Today is Thursday.")),
        ("a confident date", saying(&ideal, "I can't be sure, but it is November 12.")),
        ("does not say that it cannot know", saying(&ideal, "Every day is a good day.")),
    ]);
    Samples { ideal, mistakes }
}

fn samples_off_app() -> Samples {
    let ideal = with_facts(answered(WHICH_APP, "I can't see which app you are in."), &[APP.1]);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("the app is named", saying(&ideal, "You are in Harbor Notes.")),
        ("the app is named with a hedge", saying(&ideal, "I can't be sure, but it looks like Harbor Notes.")),
        ("does not say that it cannot know", saying(&ideal, "You are in a text editor.")),
    ]);
    Samples { ideal, mistakes }
}

fn samples_off_computer() -> Samples {
    let facts = ["Northwind Book Pro", "Book14,2", "Northwind N3", "26.0", "Darwin", "25.0.0"];
    let ideal = with_facts(answered(WHAT_COMPUTER, "I don't have any information about the computer you are using."), &facts);
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("the computer is named", saying(&ideal, "This is a Northwind Book Pro.")),
        (
            "the system and its version are named",
            saying(&ideal, "I can't tell the exact model, but the system reports macOS (Darwin 25.0.0)."),
        ),
        ("the chip is named", saying(&ideal, "I can't say for sure, but it has a Northwind N3 chip.")),
        ("does not say that it cannot know", saying(&ideal, "It is a laptop.")),
    ]);
    Samples { ideal, mistakes }
}

fn samples_danish() -> Samples {
    let ideal = answered(
        DANISH,
        "Vejr er det, der sker i atmosf\u{e6}ren lige nu og de n\u{e6}ste dage. Klima er vejret set over mange \u{e5}r.",
    );
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("answered in English", saying(&ideal, "Weather is what happens now. Klima, the climate, is the long run average.")),
        ("not about the question", saying(&ideal, "Det er et godt sp\u{f8}rgsm\u{e5}l, som der er mange svar p\u{e5}.")),
    ]);
    Samples { ideal, mistakes }
}

fn samples_no_long_dashes() -> Samples {
    let ideal = answered(
        TOWER,
        "The Eiffel Tower is a wrought iron tower in Paris, finished in 1889 for the World's Fair. It is one of the \
most visited monuments in the world.",
    );
    let mut mistakes = common(&ideal);
    mistakes.extend([
        ("an em dash", saying(&ideal, "The Eiffel Tower \u{2014} built in 1889 \u{2014} stands in Paris.")),
        ("an en dash", saying(&ideal, "The Eiffel Tower was built 1887\u{2013}1889 in Paris.")),
        ("no year", saying(&ideal, "The Eiffel Tower is a wrought iron tower in Paris.")),
    ]);
    Samples { ideal, mistakes }
}

// --- the group ------------------------------------------------------

fn always(_host: &Host) -> Option<String> {
    None
}

fn needs_claude_for_web(host: &Host) -> Option<String> {
    host.openai.then(|| "web search, and the hint that points to it, are for the Claude backend".to_string())
}

pub fn scenarios() -> Vec<Scenario> {
    let plain = |name, situation, expect, run, check, samples| Scenario {
        name,
        group: Group::Everyday,
        situation,
        expect,
        limit_ms: 20_000,
        setup_ms: 0,
        live_model: true,
        skip: always,
        run,
        check,
        samples,
    };
    vec![
        Scenario {
            // A search alone takes ten seconds and more.
            limit_ms: 45_000,
            skip: needs_claude_for_web,
            ..plain(
                "everyday-weather-web-on",
                "Web search is on and the place is typed (Oslo). They ask what the weather is like today.",
                "A forecast for Oslo with a temperature, from a web search. No hint.",
                run_weather_on,
                check_weather_on,
                samples_weather_on,
            )
        },
        Scenario {
            skip: needs_claude_for_web,
            ..plain(
                "everyday-weather-web-off",
                "Web search is off and the place is typed (Oslo). They ask what the weather is like today.",
                "No made-up forecast, and the hint that points to the web search setting.",
                run_weather_off,
                check_weather_off,
                samples_weather_off,
            )
        },
        plain(
            "everyday-what-day",
            "They ask what day it is.",
            "Today's weekday, from the date the app tells the model.",
            run_what_day,
            check_what_day,
            samples_what_day,
        ),
        plain(
            "everyday-which-app",
            "They are in an app called Harbor Notes and ask which app they are in.",
            "The name of that app.",
            run_which_app,
            check_which_app,
            samples_which_app,
        ),
        plain(
            "everyday-what-computer",
            "They ask what computer this is.",
            "The computer or its chip, as `openwhisper ask context` reports it.",
            run_what_computer,
            check_what_computer,
            samples_what_computer,
        ),
        plain(
            "everyday-context-off-day",
            "Every fact about the computer is switched off. They ask what day it is.",
            "It says it cannot know, and names no day.",
            run_off_day,
            check_off_day,
            samples_off_day,
        ),
        plain(
            "everyday-context-off-app",
            "Every fact about the computer is switched off. They are in Harbor Notes and ask which app they are in.",
            "It says it cannot know, and the name of the app is not in the answer.",
            run_off_app,
            check_off_app,
            samples_off_app,
        ),
        plain(
            "everyday-context-off-computer",
            "Every fact about the computer is switched off. They ask what computer this is.",
            "It says it cannot know, and nothing about this computer is in the answer.",
            run_off_computer,
            check_off_computer,
            samples_off_computer,
        ),
        plain(
            "everyday-danish",
            "They ask in Danish what the difference between weather and climate is.",
            "An answer in Danish.",
            run_danish,
            check_danish,
            samples_danish,
        ),
        plain(
            "everyday-no-long-dashes",
            "They ask for two sentences about the Eiffel Tower with an aside on when it was built.",
            "An answer with the year and without an em dash or an en dash.",
            run_no_long_dashes,
            check_no_long_dashes,
            samples_no_long_dashes,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_computer_is_named_by_its_name_its_model_and_its_chip() {
        let device = DeviceContext {
            computer: "Northwind Book Pro (14-inch), Book14,2".into(),
            chip: "Apple N3 Max".into(),
            ..DeviceContext::default()
        };
        assert_eq!(computer_facts(&device), ["Northwind Book Pro", "Book14,2", "Apple N3 Max", "N3 Max"]);
        // What could not be read is not a fact, and nothing is said twice.
        let bare = DeviceContext { computer: "Desk".into(), chip: "Desk".into(), ..DeviceContext::default() };
        assert_eq!(computer_facts(&bare), ["Desk"]);
        assert!(computer_facts(&DeviceContext::default()).is_empty());
    }

    #[test]
    fn the_version_of_a_system_is_the_word_with_digits_and_a_dot() {
        assert_eq!(version_in("macOS 26.0"), Some("26.0"));
        assert_eq!(version_in("macOS 26.6.2 (build)"), Some("26.6.2"));
        assert_eq!(version_in("Windows 11 (10.0.26100)"), Some("10.0.26100"));
        assert_eq!(version_in("Windows 11"), None);
        assert_eq!(version_in(""), None);
        // On a system with `uname`, the kernel is two facts; none is empty.
        assert!(kernel().iter().all(|fact| fact.chars().count() >= 3));
        if cfg!(unix) {
            assert_eq!(kernel().len(), 2);
        }
    }

    #[test]
    fn todays_weekday_comes_in_both_languages() {
        let [english, danish] = weekday_now();
        let at = super::super::checks::WEEKDAYS.iter().position(|day| day.eq_ignore_ascii_case(&english));
        assert_eq!(at.map(|at| DANISH_WEEKDAYS[at]), Some(danish.as_str()));
    }

    #[test]
    fn the_questions_have_no_screen_phrase_in_them() {
        let vision = openwhisper_core::settings::VisionSettings {
            trigger: openwhisper_core::settings::VisionTrigger::Phrases,
            ..Default::default()
        };
        for question in [WEATHER, WHAT_DAY, WHICH_APP, WHAT_COMPUTER, DANISH, TOWER] {
            assert_eq!(
                openwhisper_core::vision::decide(&vision, question),
                openwhisper_core::vision::Decision::NoCapture,
                "{question}"
            );
        }
    }
}
