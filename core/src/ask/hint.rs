//! Marker words at the start of an answer.
//!
//! Some things only the model can tell: that a question could not be
//! answered properly without the web, or that it cannot be answered
//! without a look at the screen. The prompt asks it to say so in a way
//! a program can read: a marker word at the very start of its reply.
//! A marker is for the app, never for the person, so it must not
//! reach the panel, a terminal or the history.
//!
//! There are two of them, and one ask can know both:
//!
//! - [`super::prompt::NEEDS_WEB_SEARCH`], a [`Form::Line`] marker: a
//!   first line that is exactly the word, then the answer. The line
//!   is taken off and the answer streams.
//! - [`super::NEED_SCREEN`], a [`Form::Reply`] marker: the word is the
//!   whole reply. Nothing of such a reply is shown; the ask captures
//!   the screen and sends a second turn.
//!
//! An answer streams, and a marker can arrive split over several
//! pieces. [`FirstLine`] therefore holds back the start of the text
//! for as long as it could still become one of the markers it was
//! given, and no longer: text that cannot be any of them is passed on
//! in the same call it arrived in. The two words begin alike ("NEED",
//! then `_` or `S`), which is why one filter looks for all of them at
//! once: what is held is judged against every marker, and it is let
//! go only when none of them is possible any more. [`MarkerSink`] is
//! the same thing around an event sink.
//!
//! The rules of a [`Form::Line`] marker:
//!
//! - it counts only as the first line of the answer, alone on that
//!   line (blanks after it and a `\r\n` line end are fine, and so are
//!   blank lines before it), spelled exactly;
//! - the marker line, its line break and blank space right after it
//!   are removed, and the rest streams as it comes. The rest is looked
//!   at again for the markers that are left, so a reply that is the
//!   line marker and then the reply marker is recognised as both;
//! - the marker anywhere else is ordinary text and is left alone, and
//!   so is a first line that only begins like it.
//!
//! The rules of a [`Form::Reply`] marker:
//!
//! - it counts at the start of the reply. Models dress a bare word
//!   up, so upper and lower case are the same, and quotes, backticks,
//!   asterisks and brackets around it are looked through;
//! - whatever follows it is not shown: the reply is a request, not an
//!   answer;
//! - a longer word that begins like it ("NEED_SCREENSHOTS") is
//!   ordinary text. That is known with the next character, so the
//!   word alone is held until that character or the end of the reply.
//!
//! Nothing in here knows what a marker means.

use super::AskEvent;

/// How a marker is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// A first line that is exactly the word; the answer follows.
    Line,
    /// The whole reply is the word.
    Reply,
}

/// A marker word to look for. `word` is a single word of letters,
/// digits and underscores, without blanks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub word: &'static str,
    pub form: Form,
}

/// What the text held so far is, for one marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Could still become the marker.
    Maybe,
    /// Is not the marker and cannot become it.
    No,
    /// Is the marker. For a line marker, the answer begins at this
    /// byte of the held text.
    Found(usize),
}

impl Marker {
    pub const fn line(word: &'static str) -> Self {
        Self { word, form: Form::Line }
    }

    pub const fn reply(word: &'static str) -> Self {
        Self { word, form: Form::Reply }
    }

    /// `held` is the start of the text. `at_end`: no more will come.
    fn judge(&self, held: &str, at_end: bool) -> Verdict {
        match self.form {
            Form::Line => self.judge_line(held, at_end),
            Form::Reply => self.judge_reply(held, at_end),
        }
    }

    fn judge_line(&self, held: &str, at_end: bool) -> Verdict {
        let text = held.trim_start();
        let Some(rest) = text.strip_prefix(self.word) else {
            // Shorter than the marker and the same so far: wait for
            // more. Anything else is not the marker.
            return if !at_end && self.word.starts_with(text) { Verdict::Maybe } else { Verdict::No };
        };
        // `rest` is what follows the marker on its line and after.
        match rest.split_once('\n') {
            Some((line, after)) if line.trim().is_empty() => Verdict::Found(held.len() - after.len()),
            None if rest.trim().is_empty() => {
                if at_end {
                    Verdict::Found(held.len())
                } else {
                    Verdict::Maybe
                }
            }
            // More on the marker's line ("NEEDS_WEB_SEARCHING"): a
            // word that begins like the marker.
            _ => Verdict::No,
        }
    }

    fn judge_reply(&self, held: &str, at_end: bool) -> Verdict {
        let probe = undecorated(held);
        let n = self.word.len();
        match probe.get(..n) {
            // Shorter than the marker (or cut inside a character,
            // which the marker does not have): a start of it, or not.
            None => {
                let start = probe.len() < n && probe.is_ascii() && self.word[..probe.len()].eq_ignore_ascii_case(probe);
                if start && !at_end { Verdict::Maybe } else { Verdict::No }
            }
            Some(head) if head.eq_ignore_ascii_case(self.word) => match probe[n..].chars().next() {
                // "NEED_SCREENSHOTS are..." is a word, not the marker.
                Some(c) if c.is_alphanumeric() || c == '_' => Verdict::No,
                Some(_) => Verdict::Found(held.len()),
                // The word and nothing after it yet: it is the marker
                // unless a letter follows.
                None if at_end => Verdict::Found(held.len()),
                None => Verdict::Maybe,
            },
            Some(_) => Verdict::No,
        }
    }
}

/// `text` without what a model may wrap a bare word in.
fn undecorated(text: &str) -> &str {
    text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '*' | '_' | '<' | '[' | '('))
}

/// Decides, as the text of an answer arrives, whether it begins with
/// one of `markers`, and takes the marker off.
#[derive(Debug)]
pub struct FirstLine {
    /// The markers not found yet.
    markers: Vec<Marker>,
    /// The words that were found, in the order they came.
    found: Vec<&'static str>,
    state: State,
    /// The start of the answer, while it could still be a marker.
    held: String,
    /// A line marker was taken off and the answer after it has not
    /// begun: blank space before it is dropped.
    after_line: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Too little text to tell.
    Holding,
    /// Decided: the text is an answer. Everything passes.
    Passing,
    /// Decided: the reply is a [`Form::Reply`] marker. Nothing passes.
    Dropping,
}

impl FirstLine {
    /// A filter for one answer. Without markers everything passes.
    pub fn new(markers: &[Marker]) -> Self {
        let state = if markers.is_empty() { State::Passing } else { State::Holding };
        Self { markers: markers.to_vec(), found: Vec::new(), state, held: String::new(), after_line: false }
    }

    /// The next piece of the answer. Returns the text to pass on,
    /// which is empty while the start is held back.
    pub fn push(&mut self, piece: &str) -> String {
        match self.state {
            State::Passing => piece.to_string(),
            State::Dropping => String::new(),
            State::Holding => {
                self.held.push_str(piece);
                self.decide(false)
            }
        }
    }

    /// The answer is over. Returns what was still held back and is
    /// not a marker.
    pub fn finish(&mut self) -> String {
        if self.state == State::Holding { self.decide(true) } else { String::new() }
    }

    /// The answer began with the marker `word`. Known as soon as the
    /// marker is complete: at the end of its line, or with the first
    /// character after a [`Form::Reply`] marker.
    pub fn found(&self, word: &str) -> bool {
        self.found.contains(&word)
    }

    /// The markers the answer began with, in the order they came.
    pub fn all_found(&self) -> &[&'static str] {
        &self.found
    }

    /// Look at what is held. `at_end`: no more text will come.
    fn decide(&mut self, at_end: bool) -> String {
        loop {
            let mut waiting = false;
            let mut hit = None;
            for (at, marker) in self.markers.iter().enumerate() {
                match marker.judge(&self.held, at_end) {
                    Verdict::Maybe => waiting = true,
                    Verdict::Found(rest) if hit.is_none() => hit = Some((at, rest)),
                    Verdict::Found(_) | Verdict::No => {}
                }
            }
            // While one marker is still possible nothing is settled:
            // not that the text is an answer, and not that it is
            // another marker either.
            if waiting {
                return String::new();
            }
            let Some((at, rest)) = hit else {
                return self.release(at_end);
            };
            let marker = self.markers.remove(at);
            self.found.push(marker.word);
            match marker.form {
                Form::Reply => {
                    self.state = State::Dropping;
                    self.held.clear();
                    return String::new();
                }
                // The line is off. What follows it is the start of
                // the answer, or one of the markers that are left.
                Form::Line => {
                    self.held.drain(..rest);
                    self.after_line = true;
                }
            }
        }
    }

    /// Not a marker: pass on what is held. As it came, or without the
    /// blank space that followed a marker line.
    fn release(&mut self, at_end: bool) -> String {
        if !self.after_line {
            self.state = State::Passing;
            return std::mem::take(&mut self.held);
        }
        let text = self.held.trim_start().to_string();
        self.held.clear();
        if !text.is_empty() || at_end {
            self.state = State::Passing;
        }
        text
    }
}

/// A whole answer at once: the markers it began with, and the answer
/// without them (empty for a [`Form::Reply`] marker).
pub fn split(markers: &[Marker], answer: &str) -> (Vec<&'static str>, String) {
    let mut line = FirstLine::new(markers);
    let mut text = line.push(answer);
    text.push_str(&line.finish());
    (line.found, text)
}

/// An event sink with a [`FirstLine`] in front of it: text events go
/// through the filter, every other event passes as it is. A marker
/// can be given an event ([`MarkerSink::announce`]) that is sent once
/// when the marker is recognised, before any text that follows it.
pub struct MarkerSink<'a> {
    line: FirstLine,
    sink: &'a mut dyn FnMut(AskEvent),
    announce: Vec<(&'static str, AskEvent)>,
    /// How many of the found markers the sink has been told about.
    told: usize,
}

impl<'a> MarkerSink<'a> {
    pub fn new(markers: &[Marker], sink: &'a mut dyn FnMut(AskEvent)) -> Self {
        Self { line: FirstLine::new(markers), sink, announce: Vec::new(), told: 0 }
    }

    /// Send `event` to the sink when the marker `word` is found.
    pub fn announce(mut self, word: &'static str, event: AskEvent) -> Self {
        self.announce.push((word, event));
        self
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
    /// Returns the markers the answer began with.
    ///
    /// For an answer that came to its end. When the backend failed or
    /// was cancelled midway, drop the filter instead: what it still
    /// holds is then at most the start of a marker, and that is not
    /// shown either.
    pub fn finish(mut self) -> Vec<&'static str> {
        let text = self.line.finish();
        self.forward(text);
        self.line.found
    }

    /// The answer began with the marker `word`, as far as it has
    /// arrived.
    pub fn found(&self, word: &str) -> bool {
        self.line.found(word)
    }

    fn forward(&mut self, text: String) {
        while let Some(word) = self.line.all_found().get(self.told).copied() {
            self.told += 1;
            if let Some(at) = self.announce.iter().position(|(w, _)| *w == word) {
                let (_, event) = self.announce.remove(at);
                (self.sink)(event);
            }
        }
        if !text.is_empty() {
            (self.sink)(AskEvent::Text(text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two markers an ask knows, as the ask uses them.
    const WEB: &str = super::super::prompt::NEEDS_WEB_SEARCH;
    const SCREEN: &str = super::super::NEED_SCREEN;
    const LINE: &[Marker] = &[Marker::line(WEB)];
    const REPLY: &[Marker] = &[Marker::reply(SCREEN)];
    const BOTH: &[Marker] = &[Marker::reply(SCREEN), Marker::line(WEB)];

    /// Feed `pieces` one by one to a filter for `markers`; what came
    /// out per piece, what came out at the end, and what was found.
    fn feed_to(markers: &[Marker], pieces: &[&str]) -> (Vec<String>, String, Vec<&'static str>) {
        let mut line = FirstLine::new(markers);
        let out = pieces.iter().map(|p| line.push(p)).collect();
        let rest = line.finish();
        (out, rest, line.all_found().to_vec())
    }

    /// [`feed_to`] for the web search marker alone; whether it was
    /// found.
    fn feed(pieces: &[&str]) -> (Vec<String>, String, bool) {
        let (out, rest, found) = feed_to(LINE, pieces);
        (out, rest, found == [WEB])
    }

    /// The whole text that came out, and whether the web search
    /// marker was found.
    fn through(pieces: &[&str]) -> (String, bool) {
        let (out, rest, found) = feed(pieces);
        (out.concat() + &rest, found)
    }

    /// The whole text that came out of a filter for `markers`, and
    /// what was found.
    fn through_to(markers: &[Marker], pieces: &[&str]) -> (String, Vec<&'static str>) {
        let (out, rest, found) = feed_to(markers, pieces);
        (out.concat() + &rest, found)
    }

    /// [`split`] for the web search marker alone.
    fn split_line(answer: &str) -> (bool, String) {
        let (found, text) = split(LINE, answer);
        (found == [WEB], text)
    }

    // --- a line marker ------------------------------------------------

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
        let mut line = FirstLine::new(LINE);
        assert_eq!(line.push("NEEDS_WEB_SEARCH"), "");
        // It could still become "NEEDS_WEB_SEARCHING".
        assert!(!line.found(WEB));
        assert_eq!(line.push("\n"), "");
        assert!(line.found(WEB));
        assert_eq!(line.all_found(), [WEB]);
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
        let mut line = FirstLine::new(LINE);
        assert_eq!(line.push("N"), "");
        assert_eq!(line.push("o."), "No.");
        assert_eq!(line.push(" NEEDS_WEB_SEARCH"), " NEEDS_WEB_SEARCH");
    }

    #[test]
    fn a_whole_answer_splits_the_same_way() {
        assert_eq!(split_line("NEEDS_WEB_SEARCH\nIt is sunny."), (true, "It is sunny.".to_string()));
        assert_eq!(split_line("NEEDS_WEB_SEARCH"), (true, String::new()));
        assert_eq!(split_line("It is sunny."), (false, "It is sunny.".to_string()));
        assert_eq!(split_line("NEEDS"), (false, "NEEDS".to_string()));
        assert_eq!(split_line(""), (false, String::new()));
    }

    #[test]
    fn another_marker_works_the_same() {
        let other = &[Marker::line("NEED_SCREEN")];
        assert_eq!(split(other, "NEED_SCREEN\nShow me."), (vec!["NEED_SCREEN"], "Show me.".to_string()));
        assert_eq!(split(other, "NEEDS_WEB_SEARCH\nNo."), (vec![], "NEEDS_WEB_SEARCH\nNo.".to_string()));
        // Without markers there is nothing to look for.
        assert_eq!(split(&[], "NEEDS_WEB_SEARCH\nNo."), (vec![], "NEEDS_WEB_SEARCH\nNo.".to_string()));
        let mut none = FirstLine::new(&[]);
        assert_eq!((none.push("NEED"), none.finish()), ("NEED".to_string(), String::new()));
    }

    // --- a reply marker -----------------------------------------------

    /// What came out of a filter for the screen marker alone, and
    /// whether the reply was that marker.
    fn through_reply(pieces: &[&str]) -> (String, bool) {
        let (out, found) = through_to(REPLY, pieces);
        (out, found == [SCREEN])
    }

    #[test]
    fn the_reply_marker_never_passes_however_it_is_cut() {
        for cut in 0..=SCREEN.len() {
            let (a, b) = SCREEN.split_at(cut);
            assert_eq!(through_reply(&[a, b]), (String::new(), true), "cut at {cut}");
        }
        let letters: Vec<String> = SCREEN.chars().map(String::from).collect();
        let letters: Vec<&str> = letters.iter().map(String::as_str).collect();
        assert_eq!(through_reply(&letters), (String::new(), true));
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
            assert_eq!(through_reply(&[wrapped]), (String::new(), true), "{wrapped}");
            assert_eq!(split(REPLY, wrapped), (vec![SCREEN], String::new()), "{wrapped}");
        }
        // Whatever follows the marker in later pieces stays back too.
        assert_eq!(through_reply(&["NEED_", "SCREEN", " because", " I cannot see it"]), (String::new(), true));
    }

    #[test]
    fn an_answer_passes_whole_as_soon_as_it_is_clear_it_is_one() {
        assert_eq!(through_reply(&["Four", "."]), ("Four.".to_string(), false));
        // Starts like the marker, then is not: nothing is lost.
        assert_eq!(through_reply(&["N", "E", "ED", " more sleep."]), ("NEED more sleep.".to_string(), false));
        assert_eq!(
            through_reply(&["Need", "_SCREENSHOTS are", " handy."]),
            ("Need_SCREENSHOTS are handy.".to_string(), false)
        );
        // Also when the longer word is cut right after the marker:
        // the word alone settles nothing, the next character does.
        let (out, rest, found) = feed_to(REPLY, &["NEED_SCREEN", "SHOTS are", " handy."]);
        assert_eq!(out, ["", "NEED_SCREENSHOTS are", " handy."]);
        assert_eq!((rest.as_str(), found.is_empty()), ("", true));
        // A short answer that is a start of the marker comes out at
        // the end of the turn.
        assert_eq!(through_reply(&["NE"]), ("NE".to_string(), false));
        assert_eq!(through_reply(&["N\u{e9}", "e"]), ("N\u{e9}e".to_string(), false));
        assert_eq!(through_reply(&["", ""]), (String::new(), false));
        for not_it in ["", "I NEED_SCREEN", "NEED_SCREENS", "The screen. NEED_SCREEN"] {
            assert_eq!(split(REPLY, not_it), (vec![], not_it.to_string()), "{not_it}");
        }
    }

    #[test]
    fn the_first_piece_decides_when_it_is_long_enough() {
        let mut line = FirstLine::new(REPLY);
        assert_eq!(line.push("The capital "), "The capital ");
        assert_eq!(line.push("NEED_SCREEN"), "NEED_SCREEN");
        assert_eq!(line.finish(), "");
        assert!(!line.found(SCREEN));
    }

    #[test]
    fn the_reply_marker_is_known_with_the_character_after_it_or_at_the_end() {
        let mut line = FirstLine::new(REPLY);
        assert_eq!(line.push("NEED_SCR"), "");
        assert_eq!(line.push("EEN"), "");
        // It could still become "NEED_SCREENSHOTS".
        assert!(!line.found(SCREEN));
        assert_eq!(line.push("."), "");
        assert!(line.found(SCREEN));
        assert_eq!(line.push(" Then more."), "");
        assert_eq!(line.finish(), "");
        // The end of the reply settles it as well.
        let mut line = FirstLine::new(REPLY);
        assert_eq!(line.push("NEED_SCREEN"), "");
        assert_eq!(line.finish(), "");
        assert!(line.found(SCREEN));
    }

    // --- both at once -------------------------------------------------

    /// Replies, what the person may see of each, and the markers each
    /// begins with, for an ask that knows both markers.
    const REPLIES: &[(&str, &str, &[&str])] = &[
        // The screen marker: the whole reply, nothing is shown.
        ("NEED_SCREEN", "", &[SCREEN]),
        ("NEED_SCREEN\n", "", &[SCREEN]),
        ("`NEED_SCREEN`", "", &[SCREEN]),
        ("need_screen.", "", &[SCREEN]),
        ("  NEED_SCREEN because I cannot see it", "", &[SCREEN]),
        ("NEED_SCREEN\nNEEDS_WEB_SEARCH\nIt is sunny.", "", &[SCREEN]),
        // The web search marker: its line goes, the answer stays.
        ("NEEDS_WEB_SEARCH", "", &[WEB]),
        ("NEEDS_WEB_SEARCH\nIt is sunny.", "It is sunny.", &[WEB]),
        ("\n NEEDS_WEB_SEARCH \r\n\n  It is sunny.\n", "It is sunny.\n", &[WEB]),
        ("NEEDS_WEB_SEARCH\nNEED_SCREENSHOTS would help.", "NEED_SCREENSHOTS would help.", &[WEB]),
        ("NEEDS_WEB_SEARCH\nNEED more data.", "NEED more data.", &[WEB]),
        // Both: the web line first, and the rest is the screen marker.
        ("NEEDS_WEB_SEARCH\nNEED_SCREEN", "", &[WEB, SCREEN]),
        ("NEEDS_WEB_SEARCH \r\n\n`need_screen`\n", "", &[WEB, SCREEN]),
        // Neither: every character comes out.
        ("NEED", "NEED", &[]),
        ("NEEDS", "NEEDS", &[]),
        ("NEED_", "NEED_", &[]),
        ("NEED more sleep.", "NEED more sleep.", &[]),
        ("NEEDS more salt.", "NEEDS more salt.", &[]),
        ("NEED_SCREENSHOTS are handy.", "NEED_SCREENSHOTS are handy.", &[]),
        ("NEEDS_WEB_SEARCHING is not a word.", "NEEDS_WEB_SEARCHING is not a word.", &[]),
        ("NEEDS_WEB_SEARCH I cannot check that.", "NEEDS_WEB_SEARCH I cannot check that.", &[]),
        ("needs_web_search\nOslo.", "needs_web_search\nOslo.", &[]),
        ("NEEDS_SCREEN", "NEEDS_SCREEN", &[]),
        ("NEED_WEB_SEARCH\nOslo.", "NEED_WEB_SEARCH\nOslo.", &[]),
        ("The capital is Oslo. NEED_SCREEN", "The capital is Oslo. NEED_SCREEN", &[]),
        ("Oslo.\nNEEDS_WEB_SEARCH\n", "Oslo.\nNEEDS_WEB_SEARCH\n", &[]),
        ("\n  Oslo.\n", "\n  Oslo.\n", &[]),
        ("N\u{e9}e Hansen.", "N\u{e9}e Hansen.", &[]),
        ("", "", &[]),
    ];

    /// Every way to cut `text` into three pieces (empty ones
    /// included, which covers two pieces and one).
    fn cuts(text: &str) -> Vec<[&str; 3]> {
        let at: Vec<usize> = (0..=text.len()).filter(|i| text.is_char_boundary(*i)).collect();
        let mut all = Vec::new();
        for (n, &a) in at.iter().enumerate() {
            for &b in &at[n..] {
                all.push([&text[..a], &text[a..b], &text[b..]]);
            }
        }
        all
    }

    #[test]
    fn with_both_markers_a_reply_comes_out_the_same_however_it_is_cut() {
        for (reply, shown, found) in REPLIES {
            // At once.
            assert_eq!(split(BOTH, reply), (found.to_vec(), shown.to_string()), "{reply:?}");
            // In every three pieces.
            for pieces in cuts(reply) {
                assert_eq!(through_to(BOTH, &pieces), (shown.to_string(), found.to_vec()), "{pieces:?}");
            }
            // One character at a time.
            let chars: Vec<String> = reply.chars().map(String::from).collect();
            let pieces: Vec<&str> = chars.iter().map(String::as_str).collect();
            assert_eq!(through_to(BOTH, &pieces), (shown.to_string(), found.to_vec()), "{reply:?} by character");
            // The order the markers are given in does not matter.
            let turned = [Marker::line(WEB), Marker::reply(SCREEN)];
            assert_eq!(split(&turned, reply), (found.to_vec(), shown.to_string()), "{reply:?} turned");
        }
    }

    #[test]
    fn with_both_markers_no_piece_that_comes_out_holds_a_marker_or_the_start_of_one() {
        // Stronger than "the text is right in the end": while a
        // marked reply streams, nothing is passed on before the
        // marker is off, in any cut. So the panel never shows the
        // start of a marker and takes it back.
        for (reply, shown, _) in REPLIES.iter().filter(|(_, _, found)| !found.is_empty()) {
            for pieces in cuts(reply) {
                let (out, rest, _) = feed_to(BOTH, &pieces);
                let mut so_far = String::new();
                for piece in out.iter().chain([&rest]) {
                    so_far.push_str(piece);
                    assert!(shown.starts_with(&so_far), "{pieces:?} showed {so_far:?}");
                }
            }
        }
    }

    #[test]
    fn with_both_markers_text_is_let_go_the_moment_it_can_be_neither() {
        // What is held after each character of a reply: nothing, or
        // everything so far. It is held exactly as long as one of the
        // two words is still possible.
        let held_until = |reply: &str| {
            let mut line = FirstLine::new(BOTH);
            let mut fed = String::new();
            for c in reply.chars() {
                fed.push(c);
                if !line.push(&c.to_string()).is_empty() {
                    return fed;
                }
            }
            fed + &line.finish()
        };
        // The two words are the same for four characters.
        assert_eq!(held_until("NEED more sleep."), "NEED ");
        assert_eq!(held_until("NEEX"), "NEEX");
        assert_eq!(held_until("No."), "No");
        // Then they part: after "NEEDS" only the web marker is left,
        // after "NEED_" only the screen marker.
        assert_eq!(held_until("NEEDS more salt."), "NEEDS ");
        assert_eq!(held_until("NEEDS_SCREEN"), "NEEDS_S");
        assert_eq!(held_until("NEEDS_WEB_SEARCHING"), "NEEDS_WEB_SEARCHI");
        assert_eq!(held_until("NEEDS_WEB_SEARCH is"), "NEEDS_WEB_SEARCH i");
        assert_eq!(held_until("NEED_WEB_SEARCH"), "NEED_W");
        assert_eq!(held_until("NEED_SCREENSHOTS"), "NEED_SCREENS");
        // The line marker is exact, the reply marker is not: lower
        // case is held only while it can be the screen marker.
        assert_eq!(held_until("needs a nap"), "needs");
        assert_eq!(held_until("need a nap"), "need ");
        // Ordinary text is never held, whatever comes later.
        let mut line = FirstLine::new(BOTH);
        assert_eq!(line.push("Oslo. "), "Oslo. ");
        assert_eq!(line.push("NEED_SCREEN"), "NEED_SCREEN");
        assert_eq!(line.push("\nNEEDS_WEB_SEARCH\n"), "\nNEEDS_WEB_SEARCH\n");
        assert_eq!(line.finish(), "");
        assert!(line.all_found().is_empty());
    }

    #[test]
    fn after_the_web_line_the_answer_streams_and_the_screen_marker_is_still_caught() {
        // The answer after the line marker: passed on as it comes.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB", "_SEARCH\n", "\n", "It is ", "sunny."]);
        assert_eq!(out, ["", "", "", "It is ", "sunny."]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB]));
        // The screen marker after it, in pieces: nothing is shown.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB_SEARCH\nNE", "ED_SC", "REEN", "\n"]);
        assert_eq!(out, ["", "", "", ""]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB, SCREEN]));
        // Text that begins like it is held for a moment, then shown.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB_SEARCH\n", "NEED", " an umbrella."]);
        assert_eq!(out, ["", "", "NEED an umbrella."]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB]));
        // The web marker is looked for once: a second such line is
        // part of the answer.
        assert_eq!(
            split(BOTH, "NEEDS_WEB_SEARCH\nNEEDS_WEB_SEARCH\nOslo."),
            (vec![WEB], "NEEDS_WEB_SEARCH\nOslo.".to_string())
        );
    }

    // --- around a sink ------------------------------------------------

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
        let mut filter = MarkerSink::new(LINE, &mut sink).announce(WEB, found());
        filter.event(text("NEEDS_WEB"));
        filter.event(text("_SEARCH\n"));
        assert!(filter.found(WEB));
        filter.event(text("It is "));
        filter.event(AskEvent::SearchDone);
        filter.event(text("sunny."));
        assert_eq!(filter.finish(), [WEB]);
        assert_eq!(seen, [found(), text("It is "), AskEvent::SearchDone, text("sunny.")]);
    }

    #[test]
    fn the_sink_of_an_answer_without_the_marker_sees_what_the_backend_sent() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(LINE, &mut sink).announce(WEB, found());
        filter.event(text("Fo"));
        filter.event(AskEvent::Searching { query: "ferry".into() });
        filter.event(text("ur."));
        assert!(filter.finish().is_empty());
        assert_eq!(seen, [text("Fo"), AskEvent::Searching { query: "ferry".into() }, text("ur.")]);
    }

    #[test]
    fn the_sink_of_a_marker_only_answer_gets_the_found_event_at_the_end() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(LINE, &mut sink).announce(WEB, found());
        filter.event(text("NEEDS_WEB_SEARCH"));
        assert_eq!(filter.finish(), [WEB]);
        assert_eq!(seen, [found()]);
    }

    #[test]
    fn held_text_that_is_not_the_marker_comes_out_at_the_end() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(LINE, &mut sink).announce(WEB, found());
        filter.event(text("NEEDS"));
        assert!(filter.finish().is_empty());
        assert_eq!(seen, [text("NEEDS")]);
    }

    #[test]
    fn a_filter_that_is_dropped_shows_nothing_of_what_it_held() {
        // A turn that failed or was cancelled midway: what is held
        // then may be the start of a marker.
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(BOTH, &mut sink).announce(WEB, found());
        filter.event(text("NEEDS_WEB_SE"));
        drop(filter);
        assert!(seen.is_empty(), "{seen:?}");
    }

    #[test]
    fn other_events_always_pass() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(REPLY, &mut sink);
        let searching = AskEvent::Searching { query: "ferry timetable".into() };
        filter.event(searching.clone());
        filter.event(text("NEED_SCR"));
        filter.event(AskEvent::SearchDone);
        filter.event(text("EEN"));
        assert_eq!(filter.finish(), [SCREEN]);
        assert_eq!(seen, [searching, AskEvent::SearchDone]);
    }

    #[test]
    fn one_sink_for_both_markers_announces_the_ones_it_was_given_an_event_for() {
        let run = |pieces: &[&str]| {
            let mut seen = Vec::new();
            let mut sink = |event: AskEvent| seen.push(event);
            let mut filter = MarkerSink::new(BOTH, &mut sink).announce(WEB, found());
            for piece in pieces {
                filter.event(text(piece));
            }
            let found = filter.finish();
            (seen, found)
        };
        // The screen marker has no event: nothing at all reaches the
        // sink.
        assert_eq!(run(&["NEED_", "SCREEN\n", "Show me."]), (vec![], vec![SCREEN]));
        assert_eq!(run(&["NEEDS_WEB", "_SEARCH\nNo."]), (vec![found(), text("No.")], vec![WEB]));
        // Both: the event of the web marker, and no text.
        assert_eq!(run(&["NEEDS_WEB_SEARCH\n", "NEED_SC", "REEN"]), (vec![found()], vec![WEB, SCREEN]));
        assert_eq!(run(&["Neither."]), (vec![text("Neither.")], vec![]));
        assert_eq!(run(&["NEED", "S must."]), (vec![text("NEEDS must.")], vec![]));
    }
}
