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
//! - [`super::prompt::NEEDS_WEB_SEARCH`], a [`Form::Line`] marker: the
//!   reply begins with the word, then the answer. The word is taken
//!   off and the answer streams.
//! - [`super::NEED_SCREEN`], a [`Form::Reply`] marker: the word is the
//!   whole reply. Nothing of such a reply is shown; the ask captures
//!   the screen and sends a second turn.
//!
//! # How a marker may be written
//!
//! The prompt asks for the bare word. Models dress it up anyway, so a
//! marker is recognised behind and in front of what Markdown and a
//! polite model put around a word. The same rules for both markers:
//!
//! - **Case** does not matter (`needs_web_search`, `Need_Screen`).
//! - **In front of the word**, all of this is looked through: blank
//!   space and blank lines, a byte order mark, emphasis (`*`, `_`),
//!   backticks, straight and curly quotes, opening brackets and braces
//!   (`(`, `[`, `{`, `<`), and the signs that begin a list item, a
//!   quote or a heading (`-`, `*`, `>`, `#`). A code fence counts as
//!   backticks, and the language tag on its line is skipped
//!   (` ```text `, then the word on the next line).
//! - **Behind the word**, on its line: emphasis, backticks, quotes,
//!   closing brackets and braces (`)`, `]`, `}`, `>`), and the
//!   punctuation `.`, `:`, `!`.
//! - **The word itself is whole.** A longer word that begins like it
//!   (`NEED_SCREENSHOTS`, `NEEDS_WEB_SEARCHING`, `NEED_SCREEN_2`) is
//!   ordinary text. That is known with the character after the word,
//!   so the word alone is held until that character or the end of the
//!   reply.
//! - The word must begin within the first [`START_MAX`] characters of
//!   the text it is looked for in.
//!
//! What then happens depends on the form.
//!
//! A [`Form::Line`] marker counts at the start of the reply only:
//!
//! - the word, what is wrapped around it, the rest of the blank space
//!   on its line, its line break, blank lines after it and the closing
//!   line of a code fence it stood in are removed;
//! - the answer may follow on the next line or on the same line
//!   (`NEEDS_WEB_SEARCH I cannot check that.`): the marker goes, the
//!   rest is shown. A dash or colon between the two goes as well;
//! - a model that writes the marker twice, on two lines, has both
//!   removed;
//! - what follows is looked at again for the other markers, so a
//!   reply that is the line marker and then the reply marker is
//!   recognised as both;
//! - the word anywhere else is ordinary text and is left alone.
//!
//! A [`Form::Reply`] marker counts in two places:
//!
//! - at the start of the reply. Whatever follows it is not shown: the
//!   reply is a request, not an answer;
//! - after one short sentence of preamble ("I need to see the
//!   screen.", a line break, the word). This is the tight case: the
//!   first line is at most [`PREAMBLE_MAX`] characters, the word is
//!   alone on the last line that is not blank (nothing but what may
//!   be wrapped around it), and the whole reply is at most
//!   [`REPLY_MAX`] characters. Nothing of such a reply is shown, the
//!   preamble included.
//!
//! # What is held back, and for how long
//!
//! An answer streams, and a marker can arrive split over several
//! pieces. [`FirstLine`] holds back the start of the text for as long
//! as it could still become one of the markers it was given, and no
//! longer: text that cannot be any of them is passed on in the same
//! call it arrived in. The two words begin alike ("NEED", then `_` or
//! `S`), which is why one filter looks for all of them at once: what
//! is held is judged against every marker, and it is let go only when
//! none of them is possible any more. [`MarkerSink`] is the same thing
//! around an event sink.
//!
//! With a line marker alone (web search switched off, the default
//! Vision trigger), ordinary text is held for the characters that
//! could still stand in front of a marker: an answer that begins with
//! a letter is let go with that letter, one that begins with a list
//! sign or a quote with the first letter after it. The worst case is
//! text made of nothing but such signs and the start of the word:
//! [`START_MAX`] characters, the word, and the character that settles
//! it, [`LINE_HOLD_MAX`] in all.
//!
//! With a reply marker (the smart trigger) the preamble form costs
//! more, and that is a price paid on purpose: the first line of an
//! answer is held until it is clear that it is not a preamble. That
//! is when the line passes [`PREAMBLE_MAX`] characters, or when the
//! first character after its line break is not the start of the
//! marker, or when the reply ends. So a person sees an answer of one
//! short line appear whole, and a longer answer begin a moment later
//! than it otherwise would; they never see "I need to see the screen"
//! appear and vanish again. The worst case is [`REPLY_MAX`]
//! characters: a short first line followed by blank lines and signs
//! up to that length.
//!
//! Nothing in here knows what a marker means.

use super::AskEvent;

/// The furthest into a text a marker word may begin, in characters:
/// room for a byte order mark, blank lines, a quote sign, emphasis and
/// a code fence with its language tag.
pub const START_MAX: usize = 48;

/// The longest first line that still counts as a preamble before a
/// [`Form::Reply`] marker, in characters.
pub const PREAMBLE_MAX: usize = 80;

/// The longest reply that can be a preamble and a [`Form::Reply`]
/// marker, in characters. Also the most text a filter with such a
/// marker ever holds back.
pub const REPLY_MAX: usize = 200;

/// The most text a filter with only [`Form::Line`] markers holds back
/// for a word of `word_chars` characters: everything that may stand in
/// front of the word, the word, and the one character that tells it
/// from a longer word.
pub const fn line_hold_max(word_chars: usize) -> usize {
    START_MAX + word_chars + 1
}

/// [`line_hold_max`] for the web search marker, the one line marker
/// an ask knows.
pub const LINE_HOLD_MAX: usize = line_hold_max(super::prompt::NEEDS_WEB_SEARCH.len());

/// The longest language tag of a code fence that is skipped.
const TAG_MAX: usize = 16;

/// How a marker is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// The reply begins with the word; the answer follows.
    Line,
    /// The whole reply is the word.
    Reply,
}

/// A marker word to look for. `word` is a single word of ASCII
/// letters, digits and underscores that begins and ends with a letter
/// or a digit, without blanks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub word: &'static str,
    pub form: Form,
}

/// What may stand in front of a marker word. See the module docs.
fn is_lead(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '\u{feff}' | '*' | '_' | '`' | '"' | '\'' | '\u{201c}' | '\u{201d}' | '\u{2018}' | '\u{2019}' | '(' | '['
                | '{' | '<' | '-' | '>' | '#'
        )
}

/// What may stand behind a marker word on its line, blank space
/// aside. See the module docs.
fn is_trail(c: char) -> bool {
    matches!(
        c,
        '\u{feff}' | '*' | '_' | '`' | '"' | '\'' | '\u{201c}' | '\u{201d}' | '\u{2018}' | '\u{2019}' | ')' | ']' | '}'
            | '>' | '.' | ':' | '!'
    )
}

/// A character of a code fence's language tag (`text`, `c++`, `c#`).
fn is_tag(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '.' | '-' | '_')
}

/// How a text begins, for one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    /// With a start of the word and nothing else (also: with nothing).
    Partial,
    /// With the word, and nothing after it that settles whether a
    /// longer word begins here.
    Open,
    /// With the word, whole.
    Word,
    /// With something else.
    Other,
}

/// How `text` begins, for `word`.
fn head(text: &str, word: &str) -> Head {
    let n = word.len();
    let Some(start) = text.get(..n) else {
        // Shorter than the word, or cut inside a character (which the
        // word does not have).
        let partial = text.len() < n && text.is_ascii() && word[..text.len()].eq_ignore_ascii_case(text);
        return if partial { Head::Partial } else { Head::Other };
    };
    if !start.eq_ignore_ascii_case(word) {
        return Head::Other;
    }
    // "NEED_SCREENSHOTS" and "NEED_SCREEN_2" are longer words; an
    // underscore that is followed by anything else is emphasis.
    let after = text[n..].trim_start_matches('_');
    match after.chars().next() {
        None => Head::Open,
        Some(c) if c.is_alphanumeric() => Head::Other,
        Some(_) => Head::Word,
    }
}

/// Where `word` stands at the start of `text`, behind what may be
/// wrapped around a marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    /// Could still become the word.
    Maybe,
    /// Is not the word and cannot become it.
    No,
    /// The word ends at this byte of `text`. `fence`: it stands in a
    /// code fence that was opened in front of it.
    Found { end: usize, fence: bool },
}

/// The byte at which the signs in front of a word end, and whether a
/// code fence is among them and is the last of them.
fn lead_of(text: &str) -> (usize, bool, bool) {
    let at = text.find(|c: char| !is_lead(c)).unwrap_or(text.len());
    let signs = &text[..at];
    (at, signs.contains("```"), signs.ends_with("```"))
}

/// Look for `word` at the start of `text`. `at_end`: no more text will
/// come.
fn at_start(text: &str, word: &str, at_end: bool) -> Start {
    let (at, fence, fence_last) = lead_of(text);
    if text[..at].chars().count() > START_MAX {
        return Start::No;
    }
    let settle = |head: Head, from: usize, fence: bool| match head {
        Head::Word => Some(Start::Found { end: from + word.len(), fence }),
        Head::Open if at_end => Some(Start::Found { end: from + word.len(), fence }),
        Head::Open => Some(Start::Maybe),
        Head::Partial if !at_end => Some(Start::Maybe),
        Head::Partial => Some(Start::No),
        Head::Other => None,
    };
    if let Some(start) = settle(head(&text[at..], word), at, fence) {
        return start;
    }
    // Not the word. Right after a code fence it may be the fence's
    // language tag, with the word on the next line.
    if !fence_last {
        return Start::No;
    }
    let tag = text[at..].find(|c: char| !is_tag(c)).unwrap_or(text.len() - at);
    if tag == 0 || tag > TAG_MAX {
        return Start::No;
    }
    let line_end = text[at + tag..].trim_start_matches([' ', '\t', '\r']);
    if line_end.is_empty() {
        // The tag's line is not over yet.
        return if at_end { Start::No } else { Start::Maybe };
    }
    if !line_end.starts_with('\n') {
        return Start::No;
    }
    let body = text.len() - line_end.len() + 1;
    let (inner, _, _) = lead_of(&text[body..]);
    if text[..body + inner].chars().count() > START_MAX {
        return Start::No;
    }
    settle(head(&text[body + inner..], word), body + inner, true).unwrap_or(Start::No)
}

/// What the text held so far is, for one marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// Could still become the marker.
    Maybe,
    /// Is not the marker and cannot become it.
    No,
    /// Is the marker. The word ends at byte `end` of the held text
    /// (what follows a [`Form::Reply`] marker does not matter).
    /// `fence`: the word stands in a code fence that is still open.
    Found { end: usize, fence: bool },
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
        match (self.form, at_start(held, self.word, at_end)) {
            (_, Start::Found { end, fence }) => Verdict::Found { end, fence },
            (_, Start::Maybe) => Verdict::Maybe,
            (Form::Line, Start::No) => Verdict::No,
            (Form::Reply, Start::No) => self.judge_after_preamble(held, at_end),
        }
    }

    /// The second place a [`Form::Reply`] marker counts: alone on the
    /// last line of a short reply, after one short line of preamble.
    fn judge_after_preamble(&self, held: &str, at_end: bool) -> Verdict {
        if held.chars().count() > REPLY_MAX {
            return Verdict::No;
        }
        let text = held.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        let Some((first, rest)) = text.split_once('\n') else {
            // Still on the first line: a preamble while it is short.
            let short = text.chars().count() <= PREAMBLE_MAX;
            return if short && !at_end { Verdict::Maybe } else { Verdict::No };
        };
        if first.trim_end().chars().count() > PREAMBLE_MAX {
            return Verdict::No;
        }
        match at_start(rest, self.word, at_end) {
            Start::No => Verdict::No,
            Start::Maybe => Verdict::Maybe,
            Start::Found { end, .. } => {
                // Alone on the last line that is not blank: nothing
                // may follow but what is wrapped around the word.
                let alone = rest[end..].chars().all(|c| c.is_whitespace() || is_trail(c));
                match (alone, at_end) {
                    (false, _) => Verdict::No,
                    (true, true) => Verdict::Found { end: held.len(), fence: false },
                    (true, false) => Verdict::Maybe,
                }
            }
        }
    }
}

/// Whether the whole reply `text` is the [`Form::Reply`] marker
/// `word`, read exactly as a [`FirstLine`] for that marker reads it.
/// For code that judges a finished reply (the eval sets).
pub fn is_reply_marker(word: &'static str, text: &str) -> bool {
    !split(&[Marker::reply(word)], text).0.is_empty()
}

/// Decides, as the text of an answer arrives, whether it begins with
/// one of `markers`, and takes the marker off.
#[derive(Debug)]
pub struct FirstLine {
    markers: Vec<Marker>,
    /// The words that were found, in the order they came, each once.
    found: Vec<&'static str>,
    state: State,
    /// The start of the answer, while it could still be a marker.
    held: String,
    /// What is left to take off after a line marker.
    after: After,
    /// The line marker stood in a code fence whose closing line has
    /// not come yet.
    fence: bool,
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

/// Where the text is, relative to a line marker that was taken off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After {
    /// No line marker was found (yet).
    Nothing,
    /// On the marker's line, right behind the word.
    Word,
    /// Past the marker's line, or past its word and the signs behind
    /// it: blank space in front of the answer is dropped.
    Line,
}

/// A dash or colon that a model puts between a marker and the answer
/// on the same line.
fn is_joint(c: char) -> bool {
    matches!(c, '-' | '\u{2013}' | '\u{2014}' | ':')
}

impl FirstLine {
    /// A filter for one answer. Without markers everything passes.
    pub fn new(markers: &[Marker]) -> Self {
        let state = if markers.is_empty() { State::Passing } else { State::Holding };
        Self {
            markers: markers.to_vec(),
            found: Vec::new(),
            state,
            held: String::new(),
            after: After::Nothing,
            fence: false,
        }
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
    /// marker is complete: with the first character after the word
    /// that is not part of a longer one, or at the end of the reply.
    pub fn found(&self, word: &str) -> bool {
        self.found.contains(&word)
    }

    /// The markers the answer began with, in the order they came.
    pub fn all_found(&self) -> &[&'static str] {
        &self.found
    }

    fn note(&mut self, word: &'static str) {
        if !self.found.contains(&word) {
            self.found.push(word);
        }
    }

    /// Take off what follows a line marker's word on its line: the
    /// signs wrapped around it, blank space, one dash or colon that
    /// joins it to an answer on the same line, and the line break.
    /// `false` when more text is needed to tell where that ends.
    fn finish_marker_line(&mut self, at_end: bool) -> bool {
        let cut = self
            .held
            .find(|c: char| !(is_trail(c) || matches!(c, ' ' | '\t' | '\r')))
            .unwrap_or(self.held.len());
        if cut == self.held.len() && !at_end {
            return false;
        }
        let rest = &self.held[cut..];
        let mut end = cut;
        if let Some(joint) = rest.chars().next().filter(|c| is_joint(*c)) {
            let behind = &rest[joint.len_utf8()..];
            match behind.chars().next() {
                // The joint may be the last thing so far.
                None if !at_end => return false,
                Some(' ' | '\t') | None => end += joint.len_utf8(),
                // "-5 degrees": part of the answer.
                Some(_) => {}
            }
        }
        // A fence that closes on the marker's own line is closed.
        if self.held[..end].contains("```") {
            self.fence = false;
        }
        self.held.drain(..end);
        if self.held.starts_with('\n') {
            self.held.drain(..1);
        }
        self.after = After::Line;
        true
    }

    /// Drop the blank space in front of the answer that follows a
    /// line marker, and the closing line of the code fence the marker
    /// stood in. `false` when more text is needed to tell.
    fn skip_to_answer(&mut self, at_end: bool) -> bool {
        loop {
            let blank = self.held.len() - self.held.trim_start().len();
            self.held.drain(..blank);
            if !self.fence {
                break;
            }
            if self.held.len() < 3 && "```".starts_with(self.held.as_str()) && !at_end {
                // Nothing yet, or the start of the closing fence.
                return false;
            }
            self.fence = false;
            if self.held.starts_with("```") {
                let ticks = self.held.len() - self.held.trim_start_matches('`').len();
                self.held.drain(..ticks);
            }
        }
        // The answer has not begun: keep waiting for it.
        !self.held.is_empty() || at_end
    }

    /// Look at what is held. `at_end`: no more text will come.
    fn decide(&mut self, at_end: bool) -> String {
        loop {
            if self.after == After::Word && !self.finish_marker_line(at_end) {
                return String::new();
            }
            if self.after == After::Line && !self.skip_to_answer(at_end) {
                return String::new();
            }
            let mut waiting = false;
            let mut line = None;
            let mut reply = None;
            for marker in &self.markers {
                match (marker.judge(&self.held, at_end), marker.form) {
                    (Verdict::Maybe, _) => waiting = true,
                    (Verdict::No, _) => {}
                    (Verdict::Found { end, fence }, Form::Line) => {
                        line.get_or_insert((marker.word, end, fence));
                    }
                    (Verdict::Found { .. }, Form::Reply) => {
                        reply.get_or_insert(marker.word);
                    }
                }
            }
            // A line marker is taken off the moment it is whole, also
            // while another marker is still possible for the same
            // text: what follows it is judged again without it.
            if let Some((word, end, fence)) = line {
                self.note(word);
                self.held.drain(..end);
                self.after = After::Word;
                self.fence = fence;
                continue;
            }
            // While one marker is still possible nothing is settled:
            // not that the text is an answer, and not that it is
            // another marker either.
            if waiting {
                return String::new();
            }
            if let Some(word) = reply {
                self.note(word);
                self.state = State::Dropping;
                self.held.clear();
                return String::new();
            }
            // Not a marker: pass on what is held.
            self.state = State::Passing;
            return std::mem::take(&mut self.held);
        }
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

    /// `reply` one character at a time through a filter for `markers`:
    /// how many characters had arrived when the first text came out
    /// (all of them when nothing came out before the end).
    fn held_for(markers: &[Marker], reply: &str) -> usize {
        let mut line = FirstLine::new(markers);
        for (n, c) in reply.chars().enumerate() {
            if !line.push(&c.to_string()).is_empty() {
                return n + 1;
            }
        }
        reply.chars().count()
    }

    /// The start of `reply` that had arrived when the first text came
    /// out.
    fn held_until(markers: &[Marker], reply: &str) -> String {
        reply.chars().take(held_for(markers, reply)).collect()
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
        assert_eq!(through(&["**NEEDS_WEB_SEARCH**"]), (String::new(), true));
        assert_eq!(through(&["```\nNEEDS_WEB_SEARCH\n```\n"]), (String::new(), true));
    }

    #[test]
    fn the_marker_is_known_with_the_character_after_it() {
        let mut line = FirstLine::new(LINE);
        assert_eq!(line.push("NEEDS_WEB_SEARCH"), "");
        // It could still become "NEEDS_WEB_SEARCHING".
        assert!(!line.found(WEB));
        assert_eq!(line.push("\n"), "");
        assert!(line.found(WEB));
        assert_eq!(line.all_found(), [WEB]);
        // Any character that is not part of a longer word settles it.
        for after in [" ", ".", "*", "`", ":", "\r"] {
            let mut line = FirstLine::new(LINE);
            assert_eq!(line.push("NEEDS_WEB_SEARCH"), "");
            assert_eq!(line.push(after), "", "{after:?}");
            assert!(line.found(WEB), "{after:?}");
        }
        // An underscore does not: "NEEDS_WEB_SEARCH_2" is a word,
        // "_NEEDS_WEB_SEARCH_" is the marker in italics.
        let mut line = FirstLine::new(LINE);
        assert_eq!(line.push("_NEEDS_WEB_SEARCH_"), "");
        assert!(!line.found(WEB));
        assert_eq!(line.push("\n"), "");
        assert!(line.found(WEB));
    }

    #[test]
    fn the_marker_anywhere_but_the_start_is_ordinary_text() {
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
        // Longer words.
        for word in ["NEEDS_WEB_SEARCHING is not a word.", "NEEDS_WEB_SEARCH_2 is a name.", "needs_web_searches"] {
            assert_eq!(through(&[word]), (word.to_string(), false), "{word}");
        }
    }

    #[test]
    fn the_answer_may_follow_the_marker_on_its_own_line() {
        let shown = ("I cannot check that.".to_string(), true);
        assert_eq!(through(&["NEEDS_WEB_SEARCH", " I cannot check that."]), shown);
        assert_eq!(through(&["NEEDS_WEB_SEARCH: I cannot check that."]), shown);
        assert_eq!(through(&["NEEDS_WEB_SEARCH - I cannot check that."]), shown);
        assert_eq!(through(&["NEEDS_WEB_SEARCH \u{2014} I cannot check that."]), shown);
        assert_eq!(through(&["**NEEDS_WEB_SEARCH** I cannot ", "check that."]), shown);
        assert_eq!(through(&["`NEEDS_WEB_SEARCH`. I cannot check that."]), shown);
        // A dash that belongs to the answer stays.
        assert_eq!(through(&["NEEDS_WEB_SEARCH -5 degrees, I think."]), ("-5 degrees, I think.".to_string(), true));
        assert_eq!(through(&["NEEDS_WEB_SEARCH\n- one\n- two"]), ("- one\n- two".to_string(), true));
        // What is wrapped around the answer's own first word stays.
        assert_eq!(through(&["NEEDS_WEB_SEARCH\n**Roughly** 5 degrees."]), ("**Roughly** 5 degrees.".to_string(), true));
        assert_eq!(through(&["NEEDS_WEB_SEARCH\n\"Mild\", they say."]), ("\"Mild\", they say.".to_string(), true));
    }

    #[test]
    fn a_dressed_up_marker_is_still_the_marker() {
        let sunny = "It is sunny.";
        for marker in [
            "needs_web_search",
            "Needs_Web_Search",
            "**NEEDS_WEB_SEARCH**",
            "*NEEDS_WEB_SEARCH*",
            "__NEEDS_WEB_SEARCH__",
            "_NEEDS_WEB_SEARCH_",
            "`NEEDS_WEB_SEARCH`",
            "\"NEEDS_WEB_SEARCH\"",
            "'NEEDS_WEB_SEARCH'",
            "\u{201c}NEEDS_WEB_SEARCH\u{201d}",
            "\u{2018}NEEDS_WEB_SEARCH\u{2019}",
            "(NEEDS_WEB_SEARCH)",
            "[NEEDS_WEB_SEARCH]",
            "{NEEDS_WEB_SEARCH}",
            "<NEEDS_WEB_SEARCH>",
            "> NEEDS_WEB_SEARCH",
            "# NEEDS_WEB_SEARCH",
            "## NEEDS_WEB_SEARCH",
            "- NEEDS_WEB_SEARCH",
            "* NEEDS_WEB_SEARCH",
            "NEEDS_WEB_SEARCH.",
            "NEEDS_WEB_SEARCH:",
            "NEEDS_WEB_SEARCH!",
            "\u{feff}NEEDS_WEB_SEARCH",
            "\u{feff}\n > **`needs_web_search`**.",
            "```NEEDS_WEB_SEARCH```",
        ] {
            let reply = format!("{marker}\n{sunny}");
            assert_eq!(split_line(&reply), (true, sunny.to_string()), "{reply:?}");
            assert_eq!(split_line(marker), (true, String::new()), "{marker:?}");
            // Twice, on two lines: both go.
            let twice = format!("{marker}\n{marker}\n{sunny}");
            assert_eq!(split_line(&twice), (true, sunny.to_string()), "{twice:?}");
        }
        // In a code fence, with or without a language tag: the fence's
        // closing line goes with it.
        for fenced in [
            "```\nNEEDS_WEB_SEARCH\n```\nIt is sunny.",
            "```text\nNEEDS_WEB_SEARCH\n```\nIt is sunny.",
            "```plaintext \r\nneeds_web_search\r\n```\r\n\r\nIt is sunny.",
            "\u{feff}```\nNEEDS_WEB_SEARCH\n```\nIt is sunny.",
        ] {
            assert_eq!(split_line(fenced), (true, sunny.to_string()), "{fenced:?}");
        }
        // A fence that belongs to the answer stays.
        assert_eq!(
            split_line("NEEDS_WEB_SEARCH\n```sh\nping example.org\n```"),
            (true, "```sh\nping example.org\n```".to_string())
        );
        // Code that is not the marker is code.
        for code in ["```python\nprint(1)\n```", "```\nls\n```", "```text\nNEEDS_WEB_SEARCHING\n```"] {
            assert_eq!(split_line(code), (false, code.to_string()), "{code:?}");
        }
    }

    #[test]
    fn text_that_cannot_be_the_marker_is_never_held() {
        let mut line = FirstLine::new(LINE);
        assert_eq!(line.push("N"), "");
        assert_eq!(line.push("o."), "No.");
        assert_eq!(line.push(" NEEDS_WEB_SEARCH"), " NEEDS_WEB_SEARCH");
    }

    #[test]
    fn with_a_line_marker_alone_text_is_let_go_with_the_first_character_that_settles_it() {
        let until = |reply: &str| held_until(LINE, reply);
        // A letter that is not the word's: at once.
        assert_eq!(until("Oslo."), "O");
        assert_eq!(until("No."), "No");
        assert_eq!(until("NEED more sleep."), "NEED ");
        assert_eq!(until("NEEDS more salt."), "NEEDS ");
        assert_eq!(until("needs a nap"), "needs ");
        assert_eq!(until("NEEDS_WEB_SEARCHING"), "NEEDS_WEB_SEARCHI");
        assert_eq!(until("NEED_SCREEN"), "NEED_");
        // Signs a marker could stand behind: the first letter after
        // them.
        assert_eq!(until("- one\n- two"), "- o");
        assert_eq!(until("**Bold** start"), "**B");
        assert_eq!(until("\"Mild\", they say."), "\"M");
        assert_eq!(until("\n\n  Oslo."), "\n\n  O");
        assert_eq!(until("# Heading"), "# H");
        // A code fence: its tag, and the first character of the code.
        assert_eq!(until("```python\nprint(1)"), "```python\np");
        assert_eq!(until("```\nls"), "```\nl");
        // A tag that is too long to be one, or a line that goes on.
        assert_eq!(until("```aaaaaaaaaaaaaaaaaaaa"), "```aaaaaaaaaaaaaaaaa");
        assert_eq!(until("```two words"), "```two w");
    }

    #[test]
    fn with_a_line_marker_alone_the_most_that_is_ever_held_is_the_signs_the_word_and_one_more() {
        assert_eq!(LINE_HOLD_MAX, START_MAX + WEB.len() + 1);
        assert_eq!(LINE_HOLD_MAX, 65);
        // The worst there is: as many signs as may stand in front of
        // a marker, the word, and a letter that makes it a longer one.
        let worst = format!("{}{WEB}X", "*".repeat(START_MAX));
        assert_eq!(held_for(LINE, &worst), LINE_HOLD_MAX);
        assert_eq!(split_line(&worst), (false, worst.clone()));
        // One sign more, and it is let go with that sign.
        assert_eq!(held_for(LINE, &"*".repeat(START_MAX + 20)), START_MAX + 1);
        assert_eq!(held_for(LINE, &" ".repeat(300)), START_MAX + 1);
        // The same budget covers a fence and its tag.
        let fenced = format!("```{}\n{}{WEB}X", "a".repeat(TAG_MAX), " ".repeat(START_MAX - 3 - TAG_MAX - 1));
        assert_eq!(held_for(LINE, &fenced), LINE_HOLD_MAX);
        // Whatever the text is made of, never more than that.
        for filler in ["*", "_", "`", ">", "#", "-", "\"", "(", "\n", "\u{feff}", "```\n", "> ", "N", "NEEDS_", "a"] {
            let text = filler.repeat(400);
            assert!(held_for(LINE, &text) <= LINE_HOLD_MAX, "{filler:?} was held for {}", held_for(LINE, &text));
            assert_eq!(split_line(&text), (false, text.clone()), "{filler:?}");
        }
        // And the marker is still found right at the edge.
        let at_the_edge = format!("{}{WEB}\nOslo.", " ".repeat(START_MAX));
        assert_eq!(split_line(&at_the_edge), (true, "Oslo.".to_string()));
        let past_it = format!("{}{WEB}\nOslo.", " ".repeat(START_MAX + 1));
        assert_eq!(split_line(&past_it), (false, past_it.clone()));
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
            "Need_Screen!",
            "\"NEED_SCREEN\"",
            "**NEED_SCREEN**",
            "_NEED_SCREEN_",
            "__NEED_SCREEN__",
            "\u{201c}NEED_SCREEN\u{201d}",
            "\u{2018}NEED_SCREEN\u{2019}",
            "> NEED_SCREEN",
            "# NEED_SCREEN",
            "- NEED_SCREEN",
            "* NEED_SCREEN",
            "{NEED_SCREEN}",
            "[NEED_SCREEN]",
            "(NEED_SCREEN)",
            "<NEED_SCREEN>",
            "NEED_SCREEN:",
            "\u{feff}NEED_SCREEN",
            "```\nNEED_SCREEN\n```",
            "```text\nNEED_SCREEN\n```\n",
            "```plaintext\r\nneed_screen\r\n```",
            "```NEED_SCREEN```",
            "NEED_SCREEN\n\nI cannot see your screen.",
        ] {
            assert_eq!(through_reply(&[wrapped]), (String::new(), true), "{wrapped:?}");
            assert_eq!(split(REPLY, wrapped), (vec![SCREEN], String::new()), "{wrapped:?}");
            assert!(is_reply_marker(SCREEN, wrapped), "{wrapped:?}");
        }
        // Whatever follows the marker in later pieces stays back too.
        assert_eq!(through_reply(&["NEED_", "SCREEN", " because", " I cannot see it"]), (String::new(), true));
    }

    #[test]
    fn one_short_sentence_before_the_marker_is_a_request_for_the_screen_too() {
        for reply in [
            "I need to see the screen.\nNEED_SCREEN",
            "I need to see the screen.\n\nNEED_SCREEN\n",
            "I need to see your screen to answer that.\r\n`NEED_SCREEN`\r\n",
            "  I can't tell without a look at it.\n**need_screen**.",
            "Jeg skal se din sk\u{e6}rm for at kunne svare.\n\n> NEED_SCREEN",
            "Let me look.\n```\nNEED_SCREEN\n```",
            "Let me look.\n```text\nNEED_SCREEN\n```\n",
            "\u{feff}Let me look.\n\u{201c}NEED_SCREEN\u{201d}",
        ] {
            assert_eq!(split(REPLY, reply), (vec![SCREEN], String::new()), "{reply:?}");
            assert!(is_reply_marker(SCREEN, reply), "{reply:?}");
            // However it arrives, nothing of it is shown: not the
            // preamble either.
            let chars: Vec<String> = reply.chars().map(String::from).collect();
            let pieces: Vec<&str> = chars.iter().map(String::as_str).collect();
            let (out, rest, found) = feed_to(REPLY, &pieces);
            assert!(out.iter().all(String::is_empty) && rest.is_empty(), "{reply:?} showed {out:?} {rest:?}");
            assert_eq!(found, [SCREEN], "{reply:?}");
        }
        // The first line as long as it may be, and one character more.
        let long = "a".repeat(PREAMBLE_MAX);
        assert!(is_reply_marker(SCREEN, &format!("{long}\nNEED_SCREEN")));
        assert!(!is_reply_marker(SCREEN, &format!("{long}a\nNEED_SCREEN")));
        // The whole reply as long as it may be, and one more.
        let padded = |blank: usize| format!("Let me look.\n{}NEED_SCREEN", " ".repeat(blank));
        let room = START_MAX;
        assert!(is_reply_marker(SCREEN, &padded(room)));
        assert!(!is_reply_marker(SCREEN, &padded(room + 1)));
        let tail = |blank: usize| format!("Let me look.\nNEED_SCREEN{}", "\n".repeat(blank));
        let fits = REPLY_MAX - "Let me look.\nNEED_SCREEN".len();
        assert!(is_reply_marker(SCREEN, &tail(fits)));
        assert!(!is_reply_marker(SCREEN, &tail(fits + 1)));
    }

    #[test]
    fn the_preamble_form_is_tight() {
        for answer in [
            // The marker is not alone on its line.
            "Sure.\nNEED_SCREEN is the word the app looks for.",
            "Sure.\nI NEED_SCREEN",
            // Something follows the marker's line.
            "I need to see the screen.\nNEED_SCREEN\nPlease share it.",
            // Two lines before it.
            "One.\nTwo.\nNEED_SCREEN",
            // A longer word.
            "Sure.\nNEED_SCREENSHOTS",
            "Sure.\nNEED_SCREEN_2",
            // On the same line as the sentence.
            "I need to see the screen. NEED_SCREEN",
            "The capital is Oslo. NEED_SCREEN",
            "I NEED_SCREEN",
            "NEED_SCREENS",
            "",
        ] {
            assert_eq!(split(REPLY, answer), (vec![], answer.to_string()), "{answer:?}");
            assert!(!is_reply_marker(SCREEN, answer), "{answer:?}");
        }
    }

    #[test]
    fn an_answer_comes_out_whole() {
        assert_eq!(through_reply(&["Four", "."]), ("Four.".to_string(), false));
        // Starts like the marker, then is not: nothing is lost.
        assert_eq!(through_reply(&["N", "E", "ED", " more sleep."]), ("NEED more sleep.".to_string(), false));
        assert_eq!(
            through_reply(&["Need", "_SCREENSHOTS are", " handy."]),
            ("Need_SCREENSHOTS are handy.".to_string(), false)
        );
        assert_eq!(through_reply(&["NEED_SCREEN", "SHOTS are", " handy.\nThey ", "help."]).0, "NEED_SCREENSHOTS are handy.\nThey help.");
        assert_eq!(through_reply(&["NE"]), ("NE".to_string(), false));
        assert_eq!(through_reply(&["N\u{e9}", "e"]), ("N\u{e9}e".to_string(), false));
        assert_eq!(through_reply(&["", ""]), (String::new(), false));
    }

    #[test]
    fn with_a_reply_marker_the_first_line_is_held_until_it_cannot_be_a_preamble() {
        let until = |reply: &str| held_until(REPLY, reply);
        // A first line that goes on past what a preamble may be: let
        // go with the character that makes it too long.
        let long = "The capital of Norway is Oslo, a city at the head of a fjord, and it has been the capital since 1814.";
        assert_eq!(until(long).chars().count(), PREAMBLE_MAX + 1);
        // A short first line: let go with the first character after
        // its line break that is not the start of the marker.
        assert_eq!(until("It is Oslo.\nThe city lies at a fjord."), "It is Oslo.\nT");
        assert_eq!(until("It is Oslo.\n\nNorway's capital."), "It is Oslo.\n\nNo");
        assert_eq!(until("Steps:\n- open the lid"), "Steps:\n- o");
        assert_eq!(until("Sure.\nNEED_SCREENSHOTS help."), "Sure.\nNEED_SCREENS");
        assert_eq!(until("Sure.\nNEED_SCREEN is a word."), "Sure.\nNEED_SCREEN i");
        // An answer of one short line comes out when the reply ends.
        let (out, rest, found) = feed_to(REPLY, &["It is ", "Oslo."]);
        assert_eq!((out, rest.as_str(), found.is_empty()), (vec![String::new(), String::new()], "It is Oslo.", true));
        // Once text passes, everything passes, whatever comes later.
        let mut line = FirstLine::new(REPLY);
        assert_eq!(line.push(long), long);
        assert_eq!(line.push("\nNEED_SCREEN"), "\nNEED_SCREEN");
        assert_eq!(line.finish(), "");
        assert!(!line.found(SCREEN));
    }

    #[test]
    fn with_a_reply_marker_never_more_than_a_short_reply_is_held() {
        assert_eq!((PREAMBLE_MAX, REPLY_MAX), (80, 200));
        // The worst there is: a short line, then nothing but blank
        // space and signs for as long as a marker could still follow.
        for filler in [" ", "\n", "*", "> ", "Hi.\n", "a", "NEED_", "N", "\u{feff}", "```\n"] {
            let text = filler.repeat(600);
            let held = held_for(REPLY, &text);
            assert!(held <= REPLY_MAX + 1, "{filler:?} was held for {held} characters");
            assert_eq!(split(REPLY, &text), (vec![], text.clone()), "{filler:?}");
        }
        // Blank space in front of everything is the case that goes
        // all the way.
        assert_eq!(held_for(REPLY, &" ".repeat(600)), REPLY_MAX + 1);
        assert_eq!(held_for(BOTH, &" ".repeat(600)), REPLY_MAX + 1);
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
        // After a preamble it is the end of the reply that settles it.
        let mut line = FirstLine::new(REPLY);
        assert_eq!(line.push("Let me look.\nNEED_SCREEN\n"), "");
        assert!(!line.found(SCREEN));
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
        ("_NEED_SCREEN_", "", &[SCREEN]),
        ("__NEED_SCREEN__", "", &[SCREEN]),
        ("\u{201c}NEED_SCREEN\u{201d}", "", &[SCREEN]),
        ("> NEED_SCREEN", "", &[SCREEN]),
        ("# NEED_SCREEN", "", &[SCREEN]),
        ("- NEED_SCREEN", "", &[SCREEN]),
        ("{NEED_SCREEN}", "", &[SCREEN]),
        ("\u{feff}NEED_SCREEN", "", &[SCREEN]),
        ("```\nNEED_SCREEN\n```", "", &[SCREEN]),
        ("```text\nNEED_SCREEN\n```\n", "", &[SCREEN]),
        ("  NEED_SCREEN because I cannot see it", "", &[SCREEN]),
        ("NEED_SCREEN\nNEEDS_WEB_SEARCH\nIt is sunny.", "", &[SCREEN]),
        // After one short sentence.
        ("I need to see the screen.\nNEED_SCREEN", "", &[SCREEN]),
        ("I need to see the screen.\n\n**NEED_SCREEN**\n", "", &[SCREEN]),
        ("Let me look.\n```\nneed_screen\n```", "", &[SCREEN]),
        // The web search marker: it goes, the answer stays.
        ("NEEDS_WEB_SEARCH", "", &[WEB]),
        ("NEEDS_WEB_SEARCH\nIt is sunny.", "It is sunny.", &[WEB]),
        ("\n NEEDS_WEB_SEARCH \r\n\n  It is sunny.\n", "It is sunny.\n", &[WEB]),
        ("NEEDS_WEB_SEARCH\nNEED_SCREENSHOTS would help.", "NEED_SCREENSHOTS would help.", &[WEB]),
        ("NEEDS_WEB_SEARCH\nNEED more data.", "NEED more data.", &[WEB]),
        ("**NEEDS_WEB_SEARCH**\nIt is sunny.", "It is sunny.", &[WEB]),
        ("`needs_web_search`\nIt is sunny.", "It is sunny.", &[WEB]),
        ("Needs_Web_Search.\nIt is sunny.", "It is sunny.", &[WEB]),
        ("NEEDS_WEB_SEARCH:\n\nIt is sunny.", "It is sunny.", &[WEB]),
        ("\u{feff}> NEEDS_WEB_SEARCH!\nIt is sunny.", "It is sunny.", &[WEB]),
        ("```\nNEEDS_WEB_SEARCH\n```\nIt is sunny.", "It is sunny.", &[WEB]),
        ("```text\nNEEDS_WEB_SEARCH\n```\n\nIt is sunny.\n", "It is sunny.\n", &[WEB]),
        ("NEEDS_WEB_SEARCH I cannot check that.", "I cannot check that.", &[WEB]),
        ("NEEDS_WEB_SEARCH - I cannot check that.", "I cannot check that.", &[WEB]),
        ("*NEEDS_WEB_SEARCH*: I cannot check that.", "I cannot check that.", &[WEB]),
        ("NEEDS_WEB_SEARCH\nNEEDS_WEB_SEARCH\nOslo.", "Oslo.", &[WEB]),
        ("NEEDS_WEB_SEARCH\n\n`NEEDS_WEB_SEARCH`\nOslo.", "Oslo.", &[WEB]),
        ("NEEDS_WEB_SEARCH\n```sh\nping example.org\n```", "```sh\nping example.org\n```", &[WEB]),
        // Both: the web marker first, and the rest is the screen marker.
        ("NEEDS_WEB_SEARCH\nNEED_SCREEN", "", &[WEB, SCREEN]),
        ("NEEDS_WEB_SEARCH \r\n\n`need_screen`\n", "", &[WEB, SCREEN]),
        ("**NEEDS_WEB_SEARCH**\n**NEED_SCREEN**", "", &[WEB, SCREEN]),
        ("NEEDS_WEB_SEARCH NEED_SCREEN", "", &[WEB, SCREEN]),
        ("NEEDS_WEB_SEARCH\nI need to see the screen.\nNEED_SCREEN", "", &[WEB, SCREEN]),
        // Neither: every character comes out.
        ("NEED", "NEED", &[]),
        ("NEEDS", "NEEDS", &[]),
        ("NEED_", "NEED_", &[]),
        ("NEED more sleep.", "NEED more sleep.", &[]),
        ("NEEDS more salt.", "NEEDS more salt.", &[]),
        ("NEED_SCREENSHOTS are handy.", "NEED_SCREENSHOTS are handy.", &[]),
        ("NEED_SCREEN_2 is a name.", "NEED_SCREEN_2 is a name.", &[]),
        ("NEEDS_WEB_SEARCHING is not a word.", "NEEDS_WEB_SEARCHING is not a word.", &[]),
        ("NEEDS_SCREEN", "NEEDS_SCREEN", &[]),
        ("NEED_WEB_SEARCH\nOslo.", "NEED_WEB_SEARCH\nOslo.", &[]),
        ("The capital is Oslo. NEED_SCREEN", "The capital is Oslo. NEED_SCREEN", &[]),
        ("Oslo.\nNEEDS_WEB_SEARCH\n", "Oslo.\nNEEDS_WEB_SEARCH\n", &[]),
        ("Oslo.\nNEED_SCREEN is not a word I know.", "Oslo.\nNEED_SCREEN is not a word I know.", &[]),
        ("One.\nTwo.\nNEED_SCREEN", "One.\nTwo.\nNEED_SCREEN", &[]),
        ("Let me look.\nNEED_SCREEN\nPlease share it.", "Let me look.\nNEED_SCREEN\nPlease share it.", &[]),
        ("```python\nprint(1)\n```", "```python\nprint(1)\n```", &[]),
        ("- one\n- two", "- one\n- two", &[]),
        ("**Oslo** is the capital.", "**Oslo** is the capital.", &[]),
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
    fn each_marker_alone_reads_its_replies_the_same_however_they_are_cut() {
        // The same table through a filter that knows one marker only:
        // the replies that marker has no part in must come out whole,
        // and the ones that are only that marker as with both.
        for (reply, shown, found) in REPLIES {
            let (markers, word): (&[Marker], &str) = match found {
                [w] if *w == WEB => (LINE, WEB),
                [w] if *w == SCREEN => (REPLY, SCREEN),
                [] => (LINE, WEB),
                _ => continue,
            };
            // A reply that is the screen marker and goes on with the
            // web marker is only the screen marker to both filters.
            let expected = if found.is_empty() { (reply.to_string(), vec![]) } else { (shown.to_string(), vec![word]) };
            for pieces in cuts(reply) {
                assert_eq!(through_to(markers, &pieces), expected, "{pieces:?}");
            }
            if found.is_empty() {
                for pieces in cuts(reply) {
                    assert_eq!(through_to(REPLY, &pieces), (reply.to_string(), vec![]), "{pieces:?} (reply marker)");
                }
            }
        }
    }

    #[test]
    fn with_both_markers_no_piece_that_comes_out_holds_a_marker_or_the_start_of_one() {
        // Stronger than "the text is right in the end": while a
        // marked reply streams, nothing is passed on before the
        // marker is off, in any cut. So the panel never shows the
        // start of a marker, or a preamble, and takes it back.
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
    fn with_both_markers_the_first_line_waits_and_what_cannot_be_a_marker_goes() {
        let until = |reply: &str| held_until(BOTH, reply);
        // One short line: until the reply ends.
        assert_eq!(until("NEED more sleep."), "NEED more sleep.");
        assert_eq!(until("No."), "No.");
        // Two lines: with the first character of the second that is
        // not the start of the screen marker.
        assert_eq!(until("NEEDS more salt.\nAnd pepper."), "NEEDS more salt.\nA");
        assert_eq!(until("Oslo.\nNEEDS_WEB_SEARCH\n"), "Oslo.\nNEEDS");
        assert_eq!(until("Oslo.\nNEED_WEB_SEARCH"), "Oslo.\nNEED_W");
        // The web marker is off with the character after it, and the
        // answer behind it waits like any first line.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB_SEARCH", "\n", "It is ", "sunny.\n", "Bring ", "a hat."]);
        assert_eq!(out, ["", "", "", "", "It is sunny.\nBring ", "a hat."]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB]));
        // Ordinary text is never held again once it passes.
        let long = "a".repeat(PREAMBLE_MAX + 1);
        let mut line = FirstLine::new(BOTH);
        assert_eq!(line.push(&long), long);
        assert_eq!(line.push("NEED_SCREEN"), "NEED_SCREEN");
        assert_eq!(line.push("\nNEEDS_WEB_SEARCH\n"), "\nNEEDS_WEB_SEARCH\n");
        assert_eq!(line.finish(), "");
        assert!(line.all_found().is_empty());
    }

    #[test]
    fn after_the_web_marker_the_screen_marker_is_still_caught() {
        // The screen marker after it, in pieces: nothing is shown.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB_SEARCH\nNE", "ED_SC", "REEN", "\n"]);
        assert_eq!(out, ["", "", "", ""]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB, SCREEN]));
        // Text that begins like it comes out whole.
        let (out, rest, found) = feed_to(BOTH, &["NEEDS_WEB_SEARCH\n", "NEED", " an umbrella."]);
        assert_eq!((out.concat() + &rest, found), ("NEED an umbrella.".to_string(), vec![WEB]));
        // With the web marker alone the answer streams as it comes.
        let (out, rest, found) = feed_to(LINE, &["NEEDS_WEB", "_SEARCH\n", "\n", "It is ", "sunny."]);
        assert_eq!(out, ["", "", "", "It is ", "sunny."]);
        assert_eq!((rest.as_str(), found), ("", vec![WEB]));
        // A model that writes the web marker twice has both removed,
        // and it is reported once.
        assert_eq!(split(BOTH, "NEEDS_WEB_SEARCH\nNEEDS_WEB_SEARCH\nOslo."), (vec![WEB], "Oslo.".to_string()));
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
    fn a_marker_written_twice_is_announced_once() {
        let mut seen = Vec::new();
        let mut sink = |event: AskEvent| seen.push(event);
        let mut filter = MarkerSink::new(LINE, &mut sink).announce(WEB, found());
        for piece in ["**NEEDS_WEB_SEARCH**\n", "needs_web_search\n", "It is sunny."] {
            filter.event(text(piece));
        }
        assert_eq!(filter.finish(), [WEB]);
        assert_eq!(seen, [found(), text("It is sunny.")]);
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
        // then may be the start of a marker, or a preamble to one.
        for held in ["NEEDS_WEB_SE", "I need to see the screen.\nNEED_SCR"] {
            let mut seen = Vec::new();
            let mut sink = |event: AskEvent| seen.push(event);
            let mut filter = MarkerSink::new(BOTH, &mut sink).announce(WEB, found());
            filter.event(text(held));
            drop(filter);
            assert!(seen.is_empty(), "{seen:?}");
        }
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
        assert_eq!(run(&["Let me look.\n", "NEED_", "SCREEN"]), (vec![], vec![SCREEN]));
        assert_eq!(run(&["NEEDS_WEB", "_SEARCH\nNo."]), (vec![found(), text("No.")], vec![WEB]));
        // Both: the event of the web marker, and no text.
        assert_eq!(run(&["NEEDS_WEB_SEARCH\n", "NEED_SC", "REEN"]), (vec![found()], vec![WEB, SCREEN]));
        assert_eq!(run(&["Neither."]), (vec![text("Neither.")], vec![]));
        assert_eq!(run(&["NEED", "S must."]), (vec![text("NEEDS must.")], vec![]));
    }
}
