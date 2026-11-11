//! Long dashes out of text a model wrote.
//!
//! OpenWhisper's default is that text it produces contains no em dash
//! and no en dash ([`crate::settings::RefineSettings::no_long_dashes`]).
//! The prompts of Refine and Ask ask the model for that; a prompt is
//! not a guarantee, so this is the net under it. It is applied to what
//! a model returned and to nothing else: the transcript as it was
//! recognized is never touched.
//!
//! The rule, for an em dash (U+2014), an en dash (U+2013) and a
//! horizontal bar (U+2015), which counts as an em dash:
//!
//! - between two letters or digits, with no space on either side, it
//!   becomes a hyphen: a range stays a range (`10-12`) and a joined
//!   pair stays joined;
//! - anywhere else it becomes a hyphen with a space on each side that
//!   has text: the spaces that are there are kept, a missing one is
//!   added, and none is added at the start or the end of the text;
//! - several of them in a row count as one.
//!
//! Nothing else changes. The minus sign (U+2212), the plain hyphen and
//! every other character pass as they are.
//!
//! An answer streams, so the rule is a small state machine over
//! characters ([`Normalizer`]): the text that comes out depends on the
//! characters alone, never on how they were cut into pieces. A dash at
//! the end of a piece is held back until the next character shows what
//! follows it.

/// Em dash, en dash or horizontal bar.
fn is_long_dash(c: char) -> bool {
    matches!(c, '\u{2014}' | '\u{2013}' | '\u{2015}')
}

/// `text` holds a long dash. For checks and tests.
pub fn has_long_dash(text: &str) -> bool {
    text.chars().any(is_long_dash)
}

/// Replaces long dashes in text that arrives in pieces. Feed every
/// piece to [`Normalizer::push`] and call [`Normalizer::finish`] at
/// the end; the pieces returned, in order, are the text.
#[derive(Debug, Default)]
pub struct Normalizer {
    /// The last character passed on. `None` at the start of the text.
    before: Option<char>,
    /// A long dash was read and is not written yet: what it becomes
    /// depends on the character after it.
    pending: bool,
}

impl Normalizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The next piece of the text. Returns what can be passed on now,
    /// which is everything but a dash the piece ends with.
    pub fn push(&mut self, piece: &str) -> String {
        let mut out = String::with_capacity(piece.len() + 2);
        for c in piece.chars() {
            if is_long_dash(c) {
                self.pending = true;
                continue;
            }
            if self.pending {
                self.pending = false;
                self.write_dash(Some(c), &mut out);
            }
            out.push(c);
            self.before = Some(c);
        }
        out
    }

    /// The text is over. Returns a dash that was still held back.
    pub fn finish(&mut self) -> String {
        let mut out = String::new();
        if self.pending {
            self.pending = false;
            self.write_dash(None, &mut out);
        }
        out
    }

    fn write_dash(&mut self, after: Option<char>, out: &mut String) {
        let joined = self.before.is_some_and(char::is_alphanumeric) && after.is_some_and(char::is_alphanumeric);
        if joined {
            out.push('-');
        } else {
            if self.before.is_some_and(|c| !c.is_whitespace()) {
                out.push(' ');
            }
            out.push('-');
            if after.is_some_and(|c| !c.is_whitespace()) {
                out.push(' ');
            }
        }
        self.before = Some('-');
    }
}

/// `text` with its long dashes replaced. See the module docs for the
/// rule. Text without one comes back unchanged.
pub fn normalize(text: &str) -> String {
    if !has_long_dash(text) {
        return text.to_string();
    }
    let mut normalizer = Normalizer::new();
    let mut out = normalizer.push(text);
    out.push_str(&normalizer.finish());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EM: char = '\u{2014}';
    const EN: char = '\u{2013}';
    const BAR: char = '\u{2015}';

    /// `text` with `{em}`, `{en}` and `{bar}` filled in.
    fn with(text: &str) -> String {
        text.replace("{em}", &EM.to_string()).replace("{en}", &EN.to_string()).replace("{bar}", &BAR.to_string())
    }

    fn fixed(text: &str) -> String {
        normalize(&with(text))
    }

    #[test]
    fn an_en_dash_between_digits_is_a_hyphen() {
        assert_eq!(fixed("10{en}12"), "10-12");
        assert_eq!(fixed("open 9{en}17 on weekdays"), "open 9-17 on weekdays");
        assert_eq!(fixed("(2019{en}2021)"), "(2019-2021)");
        // An em dash between digits is no different.
        assert_eq!(fixed("pages 4{em}6"), "pages 4-6");
    }

    #[test]
    fn a_dash_with_spaces_around_it_keeps_them() {
        assert_eq!(fixed("the report {em} which is late {em} needs a review"), "the report - which is late - needs a review");
        assert_eq!(fixed("one {en} two"), "one - two");
        assert_eq!(fixed("from 10 {en} 12"), "from 10 - 12");
        // Whatever blank is there stays as it is.
        assert_eq!(fixed("one\u{00A0}{em} two"), "one\u{00A0}- two");
        assert_eq!(fixed("one  {em}  two"), "one  -  two");
    }

    #[test]
    fn a_dash_between_word_characters_without_spaces_is_a_hyphen() {
        assert_eq!(fixed("the report{em}which is late"), "the report-which is late");
        assert_eq!(fixed("Aarhus{en}Odense"), "Aarhus-Odense");
        assert_eq!(fixed("A{en}Z"), "A-Z");
        assert_eq!(fixed("mix3{em}d"), "mix3-d");
    }

    #[test]
    fn a_dash_with_a_space_on_one_side_only_gets_the_other() {
        assert_eq!(fixed("wait{em} what"), "wait - what");
        assert_eq!(fixed("wait {em}what"), "wait - what");
        // Next to punctuation it is not between word characters.
        assert_eq!(fixed("\"so\"{em}she said"), "\"so\" - she said");
        assert_eq!(fixed("really?{em}yes"), "really? - yes");
        assert_eq!(fixed("yes{em}(mostly)"), "yes - (mostly)");
    }

    #[test]
    fn a_dash_at_the_start_or_the_end_gets_no_space_on_the_outside() {
        assert_eq!(fixed("{em} hello"), "- hello");
        assert_eq!(fixed("{em}hello"), "- hello");
        assert_eq!(fixed("hello {em}"), "hello -");
        assert_eq!(fixed("hello{em}"), "hello -");
        assert_eq!(fixed("{em}"), "-");
        // The start of a line is like the start of the text: a list
        // written with dashes becomes a list written with hyphens.
        assert_eq!(fixed("Two things:\n{em} one\n{en} two"), "Two things:\n- one\n- two");
    }

    #[test]
    fn a_horizontal_bar_is_an_em_dash() {
        assert_eq!(fixed("one {bar} two"), "one - two");
        assert_eq!(fixed("one{bar}two"), "one-two");
        assert_eq!(fixed("{bar} said she"), "- said she");
    }

    #[test]
    fn dashes_in_a_row_count_as_one() {
        assert_eq!(fixed("one{em}{em}two"), "one-two");
        assert_eq!(fixed("one {em}{en} two"), "one - two");
    }

    #[test]
    fn danish_text_keeps_its_letters() {
        assert_eq!(fixed("Det er {en} som sagt {en} for sent."), "Det er - som sagt - for sent.");
        assert_eq!(fixed("æbler{em}pærer"), "æbler-pærer");
        assert_eq!(fixed("på{em} og så"), "på - og så");
        assert_eq!(fixed("kl. 8{en}16 i Århus"), "kl. 8-16 i Århus");
        assert_eq!(fixed("Søren {em} ikke Jørgen {em} kommer på fredag."), "Søren - ikke Jørgen - kommer på fredag.");
    }

    #[test]
    fn nothing_else_is_touched() {
        // The minus sign, the hyphen and their neighbours in Unicode.
        let plain = "5 \u{2212} 3 = 2, well-known, a - b, non\u{2011}breaking, figure\u{2012}dash, x\u{2010}y";
        assert_eq!(normalize(plain), plain);
        assert!(!has_long_dash(plain));
        let text = "  Hello,  world.\n\nTwo  lines. ";
        assert_eq!(normalize(text), text);
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn has_long_dash_knows_the_three() {
        for dash in [EM, EN, BAR] {
            assert!(has_long_dash(&format!("a{dash}b")));
        }
        assert!(!has_long_dash("a-b"));
    }

    #[test]
    fn the_result_does_not_depend_on_how_the_text_was_cut() {
        let texts = [
            "the report {em} which is late {em} needs a review",
            "open 9{en}17, closed 12{en}13{em}always",
            "{em} start and end {em}",
            "Det er {en} som sagt {en} for sent{bar}",
            "one{em}{em}two {em}{en} three",
            "wait{em} what {em}now",
        ];
        for text in texts {
            let text = with(text);
            let whole = normalize(&text);
            assert!(!has_long_dash(&whole), "{whole}");
            let chars: Vec<char> = text.chars().collect();
            // Cut after every character, and at every single place.
            let mut one_by_one = Normalizer::new();
            let mut out: String = chars.iter().map(|c| one_by_one.push(&c.to_string())).collect();
            out.push_str(&one_by_one.finish());
            assert_eq!(out, whole, "{text}");
            for cut in 0..=chars.len() {
                let (head, tail): (String, String) = (chars[..cut].iter().collect(), chars[cut..].iter().collect());
                let mut n = Normalizer::new();
                let mut out = n.push(&head);
                out.push_str(&n.push(&tail));
                out.push_str(&n.finish());
                assert_eq!(out, whole, "{text} cut at {cut}");
            }
        }
    }

    #[test]
    fn a_dash_at_the_end_of_a_piece_waits_for_what_follows() {
        let mut n = Normalizer::new();
        assert_eq!(n.push(&with("10{en}")), "10");
        assert_eq!(n.push("12"), "-12");
        assert_eq!(n.push(&with(" and so {em}")), " and so ");
        assert_eq!(n.finish(), "-");
        // Nothing is held after that.
        assert_eq!(n.finish(), "");
    }
}
