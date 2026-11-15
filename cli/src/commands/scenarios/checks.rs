//! What the checks of the scenarios are made of. Loose about wording
//! (a model words every answer differently), strict about facts and
//! about what must not happen: a tool call, a screenshot, a forecast
//! nobody could know.
//!
//! Everything here is text in, verdict out. Nothing asks a model.

use super::Seen;

/// Lower case, with typographic apostrophes made plain, so "can’t"
/// and "Can't" are both "can't".
pub fn plain(text: &str) -> String {
    text.to_lowercase().replace(['\u{2019}', '\u{2018}'], "'")
}

/// At least one of `needles` is in the text, whatever its case.
pub fn has_any(text: &str, needles: &[&str]) -> Result<(), String> {
    let lower = plain(text);
    if needles.iter().any(|n| lower.contains(&plain(n))) {
        Ok(())
    } else {
        Err(format!("it has none of: {}", needles.join(", ")))
    }
}

/// None of `needles` is in the text, whatever its case.
pub fn has_none(text: &str, needles: &[&str]) -> Result<(), String> {
    let lower = plain(text);
    match needles.iter().find(|n| lower.contains(&plain(n))) {
        Some(found) => Err(format!("it says \"{found}\"")),
        None => Ok(()),
    }
}

/// Ways of saying "I cannot know that" or "I cannot see that", in
/// English and in Danish.
pub const CANNOT: &[&str] = &[
    "can't",
    "cannot",
    "can not",
    "couldn't",
    "could not",
    "unable",
    "not able",
    "don't have",
    "do not have",
    "don't know",
    "do not know",
    "no access",
    "no way to",
    "not sure",
    "wasn't",
    "was not",
    "isn't available",
    "is not available",
    "kan ikke",
    "har ikke",
    "ved ikke",
];

/// Ways of marking a figure as what is usual, not as a forecast.
pub const USUAL: &[&str] = &[
    "typical",
    "usually",
    "generally",
    "normally",
    "on average",
    "average",
    "tends to",
    "tend to",
    "likely",
    "probably",
    "expect",
    "this time of year",
    "plejer",
    "typisk",
    "normalt",
];

/// Ways of asking the person where they are.
pub const ASKS_WHERE: &[&str] = &[
    "which city",
    "what city",
    "which town",
    "which location",
    "what location",
    "what's your location",
    "what is your location",
    "where are you",
    "tell me where",
    "let me know where",
];

/// The weekdays in English, Monday first.
pub const WEEKDAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The text states a temperature: a number with a degree sign, or
/// with the word "degrees" (Danish: "grader"), a few characters on.
pub fn has_temperature(text: &str) -> bool {
    let lower = plain(text);
    let chars: Vec<char> = lower.chars().collect();
    let words_after = ["degrees", "degree", "grader", "celsius", "fahrenheit"];
    for (at, c) in chars.iter().enumerate() {
        if !c.is_ascii_digit() {
            continue;
        }
        // The end of this number.
        let mut end = at;
        while end < chars.len() && (chars[end].is_ascii_digit() || matches!(chars[end], '.' | ',')) {
            end += 1;
        }
        let after: String = chars[end..chars.len().min(end + 12)].iter().collect();
        let after = after.trim_start();
        if after.starts_with('\u{b0}') || words_after.iter().any(|w| after.starts_with(w)) {
            return true;
        }
    }
    false
}

/// The text names a weekday or a day of a month ("12 November",
/// "November 12"): a date somebody could act on.
pub fn names_a_day(text: &str) -> Option<String> {
    let lower = plain(text);
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if let Some(day) = WEEKDAYS.iter().find(|day| words.contains(day)) {
        return Some((*day).to_string());
    }
    let is_day_number = |w: &str| {
        let digits = w.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        w.len() <= 4 && digits.parse::<u8>().is_ok_and(|n| (1..=31).contains(&n))
    };
    for pair in words.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        // "May" is also a verb: only next to a number of a day.
        if (MONTHS.contains(&a) && is_day_number(b)) || (is_day_number(a) && MONTHS.contains(&b)) {
            return Some(format!("{a} {b}"));
        }
    }
    for three in words.windows(3) {
        if is_day_number(three[0]) && three[1] == "of" && MONTHS.contains(&three[2]) {
            return Some(three.join(" "));
        }
    }
    None
}

/// No em dash, en dash or horizontal bar.
pub fn no_long_dashes(text: &str) -> Result<(), String> {
    match text.chars().find(|c| matches!(c, '\u{2014}' | '\u{2013}' | '\u{2015}')) {
        Some(dash) => Err(format!("it has a long dash (U+{:04X})", dash as u32)),
        None => Ok(()),
    }
}

/// The text reads as Danish: several of the commonest Danish words
/// are in it, and none of the commonest English ones.
pub fn in_danish(text: &str) -> Result<(), String> {
    const DANISH: &[&str] = &[
        "er", "og", "det", "ikke", "på", "af", "til", "som", "der", "har", "kan", "eller", "når", "også", "fra",
        "ved", "hvor", "hedder", "i", "en", "den",
    ];
    not_english(text)?;
    let lower = plain(text);
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).collect();
    let danish = DANISH.iter().filter(|d| words.contains(d)).count();
    if danish >= 2 { Ok(()) } else { Err(format!("only {danish} common Danish words: it does not read as Danish")) }
}

/// None of the commonest English words is in the text. For an answer
/// that may be a single Danish word, which [`in_danish`] cannot tell
/// from any other language.
pub fn not_english(text: &str) -> Result<(), String> {
    const ENGLISH: &[&str] = &["the", "is", "are", "and", "of", "that", "which", "when", "this", "an"];
    let lower = plain(text);
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).collect();
    match ENGLISH.iter().find(|e| words.contains(e)) {
        Some(english) => Err(format!("it reads as English (\"{english}\")")),
        None => Ok(()),
    }
}

/// There is an answer, and no error.
pub fn answered(seen: &Seen) -> Result<(), String> {
    if !seen.error.is_empty() {
        return Err(format!("no answer: {}", seen.error));
    }
    if seen.answer.trim().is_empty() {
        return Err("the answer is empty".to_string());
    }
    Ok(())
}

/// No tool was called: nothing in the fixture's log, no step on the
/// panel.
pub fn no_tool_call(seen: &Seen) -> Result<(), String> {
    if !seen.calls.is_empty() {
        return Err(format!("a tool call reached the connection: {}", seen.calls.join(", ")));
    }
    if !seen.steps.is_empty() {
        return Err(format!("a tool call was made: {}", seen.steps.join("; ")));
    }
    Ok(())
}

/// No screenshot left the stand-in for the screen, and the ask does
/// not say that one went out.
pub fn no_capture(seen: &Seen) -> Result<(), String> {
    if seen.captures != 0 {
        return Err(format!("a screenshot was taken ({} times) where none may be", seen.captures));
    }
    if seen.screen == "included" {
        return Err("the ask says a screenshot went out".to_string());
    }
    Ok(())
}

/// A plain ask that answered: no tool, no screenshot, no notice the
/// person did not expect.
pub fn plain_answer(seen: &Seen) -> Result<(), String> {
    answered(seen)?;
    no_tool_call(seen)?;
    no_capture(seen)?;
    if seen.context_source != "none" {
        return Err(format!("the ask looked at {}", seen.context_source));
    }
    Ok(())
}

/// There is no answer, the reason is a plain sentence, and it has
/// one of `words` in it.
pub fn failed_with(seen: &Seen, words: &[&str]) -> Result<(), String> {
    if !seen.answer.is_empty() {
        return Err(format!("there is an answer where none can be: {}", seen.answer));
    }
    if seen.error.trim().is_empty() {
        return Err("it failed without a message".to_string());
    }
    plain_message(&seen.error)?;
    has_any(&seen.error, words).map_err(|why| format!("the message is \"{}\": {why}", seen.error))
}

/// A message a person can read: one or two sentences, no stack
/// trace, no JSON, no code.
pub fn plain_message(message: &str) -> Result<(), String> {
    if message.chars().count() > 400 {
        return Err(format!("the message is {} characters long", message.chars().count()));
    }
    if message.lines().count() > 3 {
        return Err("the message has more than three lines".to_string());
    }
    has_none(message, &["{\"", "panicked", "backtrace", "stack trace", "os error", "errno", "0x"])
        .map_err(|why| format!("the message is not plain: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temperature_is_a_number_with_a_degree_sign_or_the_word() {
        for yes in [
            "It is 3\u{b0}C in Oslo.",
            "Around 3 \u{b0}C today.",
            "Highs near 41 degrees.",
            "Omkring 4 grader i dag.",
            "about 2.5\u{b0} this afternoon",
            "minus 3 degrees Celsius",
        ] {
            assert!(has_temperature(yes), "{yes}");
        }
        for no in [
            "I can't check the weather from here.",
            "It is 14:32.",
            "There are 360 of them.",
            "Oslo has about 700 000 people.",
            "To a great degree, yes.",
        ] {
            assert!(!has_temperature(no), "{no}");
        }
    }

    #[test]
    fn a_day_is_a_weekday_or_a_day_of_a_month() {
        assert_eq!(names_a_day("Today is Thursday.").as_deref(), Some("thursday"));
        assert_eq!(names_a_day("It is the 12th of November").as_deref(), Some("12th of november"));
        assert_eq!(names_a_day("It is November 12th.").as_deref(), Some("november 12th"));
        assert_eq!(names_a_day("It is 12 November 2026.").as_deref(), Some("12 november"));
        assert_eq!(names_a_day("I don't have access to today's date."), None);
        assert_eq!(names_a_day("You may check the clock in the menu bar."), None);
        assert_eq!(names_a_day("My training data ends in 2025."), None);
    }

    #[test]
    fn danish_is_told_from_english() {
        assert!(in_danish("Hovedstaden i Danmark hedder K\u{f8}benhavn.").is_ok());
        assert!(in_danish("Det er Stockholm, som ligger p\u{e5} \u{f8}stkysten.").is_ok());
        assert!(in_danish("The capital of Denmark is Copenhagen.").is_err());
        assert!(in_danish("Stockholm.").is_err());
    }

    #[test]
    fn a_plain_message_is_short_and_has_no_code_in_it() {
        assert!(plain_message("Claude Code is not installed on this computer.").is_ok());
        assert!(plain_message("no answer in time").is_ok());
        assert!(plain_message("{\"type\":\"error\"}").is_err());
        assert!(plain_message("connect failed: os error 61").is_err());
        assert!(plain_message(&"word ".repeat(100)).is_err());
        assert!(plain_message("a\nb\nc\nd").is_err());
    }

    #[test]
    fn what_must_not_happen_is_caught() {
        let quiet = Seen { answer: "Oslo.".into(), context_source: "none".into(), screen: "none".into(), ..Seen::default() };
        assert!(plain_answer(&quiet).is_ok());
        assert!(plain_answer(&Seen { captures: 1, ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { screen: "included".into(), ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { calls: vec!["read_thread".into()], ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { steps: vec!["read_thread on teamchat: done".into()], ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { context_source: "screen".into(), ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { error: "no answer in time".into(), answer: String::new(), ..quiet.clone() }).is_err());
        assert!(plain_answer(&Seen { answer: "  ".into(), ..quiet }).is_err());

        let failed = Seen { error: "Claude Code is not installed on this computer.".into(), ..Seen::default() };
        assert!(failed_with(&failed, &["not installed"]).is_ok());
        assert!(failed_with(&failed, &["signed in"]).is_err());
        assert!(failed_with(&Seen { answer: "Oslo.".into(), ..failed.clone() }, &["not installed"]).is_err());
        assert!(failed_with(&Seen::default(), &["not installed"]).is_err());
    }
}
