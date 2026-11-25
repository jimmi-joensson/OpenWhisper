//! A marker on the first line of an answer.
//!
//! Some things only the model can tell: that a question could not be
//! answered properly without the web, say. The prompt asks it to say
//! so in a way a program can read: a first line that is exactly a
//! marker word ([`super::prompt::NEEDS_WEB_SEARCH`]), then the answer.
//! The marker is for the app, never for the person, so it must not
//! reach the panel, a terminal or the history.
//!
//! An answer streams, and the marker can arrive split over several
//! pieces. [`FirstLine`] therefore holds back the start of the text
//! for as long as it could still become the marker line, and no
//! longer: text that cannot be the marker is passed on in the same
//! call it arrived in, and nothing is held past the first line break.
//! [`MarkerSink`] is the same thing around an event sink.
//!
//! The rules:
//!
//! - the marker counts only as the first line of the answer, alone on
//!   that line (blanks after it and a `\r\n` line end are fine, and so
//!   are blank lines before it);
//! - the marker line, its line break and blank space right after it
//!   are removed, and the rest streams as it comes;
//! - the marker anywhere else is ordinary text and is left alone, and
//!   so is a first line that only begins like it.
//!
//! Nothing in here knows which marker it looks for or what it means,
//! so two filters can sit around one sink, each with its own marker.

use super::AskEvent;

/// Decides, as the text of an answer arrives, whether its first line
/// is `marker`, and takes that line off.
#[derive(Debug)]
pub struct FirstLine {
    marker: &'static str,
    state: State,
    /// The start of the answer, while it could still be the marker
    /// line.
    held: String,
    found: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Too little text to tell.
    Holding,
    /// The marker line is off; blank space after it is dropped until
    /// the answer begins.
    Skipping,
    /// Decided. Everything passes.
    Passing,
}

impl FirstLine {
    /// A filter for one answer. `marker` is a single word without
    /// blanks.
    pub fn new(marker: &'static str) -> Self {
        Self { marker, state: State::Holding, held: String::new(), found: false }
    }

    /// The next piece of the answer. Returns the text to pass on,
    /// which is empty while the start is held back.
    pub fn push(&mut self, piece: &str) -> String {
        match self.state {
            State::Passing => piece.to_string(),
            State::Skipping => {
                let rest = piece.trim_start();
                if !rest.is_empty() {
                    self.state = State::Passing;
                }
                rest.to_string()
            }
            State::Holding => {
                self.held.push_str(piece);
                self.decide(false)
            }
        }
    }

    /// The answer is over. Returns what was still held back and is
    /// not the marker.
    pub fn finish(&mut self) -> String {
        let rest = if self.state == State::Holding { self.decide(true) } else { String::new() };
        self.state = State::Passing;
        rest
    }

    /// The answer began with the marker line. Known as soon as that
    /// line is complete.
    pub fn found(&self) -> bool {
        self.found
    }

    /// Look at what is held. `at_end`: no more text will come.
    fn decide(&mut self, at_end: bool) -> String {
        let text = self.held.trim_start();
        let Some(rest) = text.strip_prefix(self.marker) else {
            // Shorter than the marker and the same so far: wait for
            // more. Anything else is not the marker.
            if !at_end && self.marker.starts_with(text) {
                return String::new();
            }
            return self.release();
        };
        // `rest` is what follows the marker on its line and after.
        match rest.split_once('\n') {
            Some((line, after)) if line.trim().is_empty() => {
                let after = after.trim_start().to_string();
                self.take(if after.is_empty() { State::Skipping } else { State::Passing });
                after
            }
            None if rest.trim().is_empty() => {
                if at_end {
                    self.take(State::Passing);
                }
                String::new()
            }
            // More on the marker's line ("NEEDS_WEB_SEARCHING"): a
            // word that begins like the marker.
            _ => self.release(),
        }
    }

    /// The marker line was there.
    fn take(&mut self, next: State) {
        self.found = true;
        self.state = next;
        self.held.clear();
    }

    /// Not the marker: pass on everything held, as it came.
    fn release(&mut self) -> String {
        self.state = State::Passing;
        std::mem::take(&mut self.held)
    }
}

/// A whole answer at once: whether it began with the `marker` line,
/// and the answer without it.
pub fn split(marker: &'static str, answer: &str) -> (bool, String) {
    let mut line = FirstLine::new(marker);
    let mut text = line.push(answer);
    text.push_str(&line.finish());
    (line.found(), text)
}

/// An event sink with a [`FirstLine`] in front of it: text events go
/// through the filter, every other event passes as it is. When the
/// marker line is recognised, `found` is sent once, before any text
/// that follows the marker.
///
/// It is a sink like the one it wraps (`&mut |event| filter.event(event)`),
/// so filters stack.
pub struct MarkerSink<'a> {
    line: FirstLine,
    sink: &'a mut dyn FnMut(AskEvent),
    found: Option<AskEvent>,
}

impl<'a> MarkerSink<'a> {
    pub fn new(marker: &'static str, found: AskEvent, sink: &'a mut dyn FnMut(AskEvent)) -> Self {
        Self { line: FirstLine::new(marker), sink, found: Some(found) }
    }

    /// The next event of the answer.
    pub fn event(&mut self, event: AskEvent) {
        match event {
            AskEvent::Text(piece) => {
                let text = self.line.push(&piece);
                self.forward(text);
            }
            other => (self.sink)(other),
        }
    }

    /// The answer is over: pass on text that was still held back.
    /// Returns whether the answer began with the marker line.
    pub fn finish(&mut self) -> bool {
        let text = self.line.finish();
        self.forward(text);
        self.line.found()
    }

    fn forward(&mut self, text: String) {
        if self.line.found()
            && let Some(found) = self.found.take()
        {
            (self.sink)(found);
        }
        if !text.is_empty() {
            (self.sink)(AskEvent::Text(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = "NEEDS_WEB_SEARCH";

    /// Feed `pieces` one by one; what came out per piece, what came
    /// out at the end, and whether the marker was found.
    fn feed(pieces: &[&str]) -> (Vec<String>, String, bool) {
        let mut line = FirstLine::new(MARKER);
        let out = pieces.iter().map(|p| line.push(p)).collect();
        let rest = line.finish();
        (out, rest, line.found())
    }

    /// The whole text that came out, and whether the marker was found.
    fn through(pieces: &[&str]) -> (String, bool) {
        let (out, rest, found) = feed(pieces);
        (out.concat() + &rest, found)
    }

    #[test]
    fn an_answer_without_the_marker_passes_piece_by_piece_as_it_came() {
        let (out, rest, found) = feed(&["The capital ", "of Norway ", "is Oslo."]);
        assert_eq!(out, ["The capital ", "of Norway ", "is Oslo."]);
        assert_eq!((rest.as_str(), found), ("", false));
        // Also one that starts with blank space.
        assert_eq!(through(&["\n  Oslo.", "\n"]), ("\n  Oslo.\n".to_string(), false));
    }

    #[test]
    fn the_marker_line_is_removed_and_the_rest_streams() {
        let (out, rest, found) = feed(&["NEEDS_WEB_SEARCH\nI cannot ", "check that ", "from here."]);
        assert_eq!(out, ["I cannot ", "check that ", "from here."]);
        assert_eq!((rest.as_str(), found), ("", true));
    }

    #[test]
    fn the_marker_can_arrive_split_over_pieces() {
        let (out, rest, found) = feed(&["NEE", "DS_WEB", "_SEAR", "CH", "\n", "It is ", "sunny."]);
        assert_eq!(out, ["", "", "", "", "", "It is ", "sunny."]);
        assert_eq!((rest.as_str(), found), ("", true));
        // One character at a time.
        let text = "NEEDS_WEB_SEARCH\nIt is sunny.";
        let chars: Vec<String> = text.chars().map(String::from).collect();
        let pieces: Vec<&str> = chars.iter().map(String::as_str).collect();
        assert_eq!(through(&pieces), ("It is sunny.".to_string(), true));
    }

    #[test]
    fn blanks_after_the_marker_a_crlf_and_blank_lines_around_it_are_fine() {
        assert_eq!(through(&["NEEDS_WEB_SEARCH  \t\r\nIt is sunny."]), ("It is sunny.".to_string(), true));
        assert_eq!(through(&["NEEDS_WEB_SEARCH\r", "\nIt is sunny."]), ("It is sunny.".to_string(), true));
        assert_eq!(through(&["\n NEEDS_WEB_SEARCH\n", "\n\n", "  It is ", "sunny.\n"]), ("It is sunny.\n".to_string(), true));
        // Blank space inside the answer is kept.
        assert_eq!(through(&["NEEDS_WEB_SEARCH\nOne.", "\n\n", "Two."]), ("One.\n\nTwo.".to_string(), true));
    }

    #[test]
    fn an_answer_that_is_only_the_marker_leaves_nothing() {
        assert_eq!(through(&["NEEDS_WEB_SEARCH"]), (String::new(), true));
        assert_eq!(through(&["NEEDS_WEB", "_SEARCH", "\n"]), (String::new(), true));
        assert_eq!(through(&["NEEDS_WEB_SEARCH \r\n\n"]), (String::new(), true));
    }

    #[test]
    fn the_marker_is_known_as_soon_as_its_line_is_complete() {
        let mut line = FirstLine::new(MARKER);
        assert_eq!(line.push("NEEDS_WEB_SEARCH"), "");
        // It could still become "NEEDS_WEB_SEARCHING".
        assert!(!line.found());
        assert_eq!(line.push("\n"), "");
        assert!(line.found());
    }

    #[test]
    fn the_marker_anywhere_but_the_first_line_is_ordinary_text() {
        let text = "I cannot check that.\nNEEDS_WEB_SEARCH\nSorry.";
        assert_eq!(through(&[text]), (text.to_string(), false));
        let (out, _, found) = feed(&["Note: ", "NEEDS_WEB_SEARCH", "\n", "is a marker."]);
        assert_eq!(out, ["Note: ", "NEEDS_WEB_SEARCH", "\n", "is a marker."]);
        assert!(!found);
    }

    #[test]
    fn a_first_line_that_only_begins_like_the_marker_is_not_eaten() {
        // A word that is the start of the marker: held only until the
        // next character settles it.
        let (out, rest, found) = feed(&["NEEDS", " more salt."]);
        assert_eq!(out, ["", "NEEDS more salt."]);
        assert_eq!((rest.as_str(), found), ("", false));
        // Never past the first line break.
        let (out, _, found) = feed(&["NEEDS", "\n", "more salt."]);
        assert_eq!(out, ["", "NEEDS\n", "more salt."]);
        assert!(!found);
        // The whole answer is such a word: it comes out at the end.
        assert_eq!(feed(&["NEEDS"]), (vec![String::new()], "NEEDS".to_string(), false));
        // A longer word, and text on the marker's own line.
        assert!(!through(&["NEEDS_WEB_SEARCHING is not a word."]).1);
        let same_line = "NEEDS_WEB_SEARCH I cannot check that.";
        assert_eq!(through(&["NEEDS_WEB_SEARCH", " I cannot check that."]), (same_line.to_string(), false));
        // Case matters: the marker is exact.
        assert_eq!(through(&["needs_web_search\nOslo."]), ("needs_web_search\nOslo.".to_string(), false));
    }

    #[test]
    fn text_that_cannot_be_the_marker_is_never_held() {
        let mut line = FirstLine::new(MARKER);
        assert_eq!(line.push("N"), "");
        assert_eq!(line.push("o."), "No.");
        assert_eq!(line.push(" NEEDS_WEB_SEARCH"), " NEEDS_WEB_SEARCH");
    }

    #[test]
    fn a_whole_answer_splits_the_same_way() {
        assert_eq!(split(MARKER, "NEEDS_WEB_SEARCH\nIt is sunny."), (true, "It is sunny.".to_string()));
        assert_eq!(split(MARKER, "NEEDS_WEB_SEARCH"), (true, String::new()));
        assert_eq!(split(MARKER, "It is sunny."), (false, "It is sunny.".to_string()));
        assert_eq!(split(MARKER, "NEEDS"), (false, "NEEDS".to_string()));
        assert_eq!(split(MARKER, ""), (false, String::new()));
    }

    #[test]
    fn another_marker_works_the_same() {
        assert_eq!(split("NEED_SCREEN", "NEED_SCREEN\nShow me."), (true, "Show me.".to_string()));
        assert_eq!(split("NEED_SCREEN", "NEEDS_WEB_SEARCH\nNo."), (false, "NEEDS_WEB_SEARCH\nNo.".to_string()));
    }

    fn found() -> AskEvent {
        AskEvent::Searching { query: "found".into() }
    }

    fn text(s: &str) -> AskEvent {
        AskEvent::Text(s.into())
    }

    #[test]
    fn the_sink_gets_the_found_event_once_then_the_text_without_the_marker() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(MARKER, found(), &mut sink);
        filter.event(text("NEEDS_WEB"));
        filter.event(text("_SEARCH\n"));
        filter.event(text("It is "));
        filter.event(AskEvent::SearchDone);
        filter.event(text("sunny."));
        assert!(filter.finish());
        assert_eq!(seen, [found(), text("It is "), AskEvent::SearchDone, text("sunny.")]);
    }

    #[test]
    fn the_sink_of_an_answer_without_the_marker_sees_what_the_backend_sent() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(MARKER, found(), &mut sink);
        filter.event(text("Fo"));
        filter.event(AskEvent::Searching { query: "ferry".into() });
        filter.event(text("ur."));
        assert!(!filter.finish());
        assert_eq!(seen, [text("Fo"), AskEvent::Searching { query: "ferry".into() }, text("ur.")]);
    }

    #[test]
    fn the_sink_of_a_marker_only_answer_gets_the_found_event_at_the_end() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(MARKER, found(), &mut sink);
        filter.event(text("NEEDS_WEB_SEARCH"));
        assert!(filter.finish());
        assert_eq!(seen, [found()]);
    }

    #[test]
    fn held_text_that_is_not_the_marker_comes_out_at_the_end() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(MARKER, found(), &mut sink);
        filter.event(text("NEEDS"));
        assert!(!filter.finish());
        assert_eq!(seen, [text("NEEDS")]);
    }

    #[test]
    fn two_filters_stack_each_with_its_own_marker() {
        let run = |pieces: &[&str]| {
            let mut seen = Vec::new();
            let mut sink = |event: AskEvent| seen.push(event);
            let mut web = MarkerSink::new(MARKER, found(), &mut sink);
            let mut inner = |event: AskEvent| web.event(event);
            let mut screen = MarkerSink::new("NEED_SCREEN", AskEvent::SearchDone, &mut inner);
            for piece in pieces {
                screen.event(text(piece));
            }
            let screen_found = screen.finish();
            let web_found = web.finish();
            (seen, screen_found, web_found)
        };
        assert_eq!(run(&["NEED_", "SCREEN\n", "Show me."]), (vec![AskEvent::SearchDone, text("Show me.")], true, false));
        assert_eq!(run(&["NEEDS_WEB", "_SEARCH\nNo."]), (vec![found(), text("No.")], false, true));
        assert_eq!(run(&["Neither."]), (vec![text("Neither.")], false, false));
    }
}
