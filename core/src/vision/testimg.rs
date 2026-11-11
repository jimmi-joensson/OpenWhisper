//! Synthetic screenshots for the Vision eval sets ([`super::eval`]).
//!
//! An eval needs images whose content is known exactly and belongs to
//! nobody: a real screenshot is neither. So the pictures are drawn
//! here, in memory, from a small bitmap font: a plain desktop colour,
//! one light window with a title bar, and a few lines of text. Every
//! name and every line in them is made up.
//!
//! Nothing here reads the screen and nothing here touches the disk:
//! a [`Scene`] becomes an RGBA buffer and then a PNG in a `Vec<u8>`,
//! through the same encoder a real capture uses
//! ([`super::paint::encode_png`]).
//!
//! The font is 5 pixels wide and 7 high (9 for letters that reach
//! under the line), drawn three or four times enlarged, which a vision
//! model reads without trouble.

use super::paint;

/// Size of every synthetic screenshot, in pixels.
pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 800;

const DESKTOP: [u8; 3] = [52, 104, 136];
const WINDOW: [u8; 3] = [250, 250, 250];
const TITLE_BAR: [u8; 3] = [218, 219, 224];
const INK: [u8; 3] = [24, 24, 30];
const BUTTON: [u8; 3] = [38, 110, 220];
const BUTTON_INK: [u8; 3] = [255, 255, 255];

const WIN_X: u32 = 90;
const WIN_Y: u32 = 70;
const WIN_W: u32 = 1100;
const WIN_H: u32 = 660;
const TITLE_H: u32 = 48;
const MARGIN: u32 = 34;

/// Columns of a glyph, and the rows a line of text takes (7 for the
/// letter, 2 under the line, 3 of air).
const GLYPH_W: u32 = 5;
const LINE_ROWS: u32 = 12;

/// One line in a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// Body text. Wrapped at word boundaries when it is too long.
    Text(&'static str),
    /// Larger text, for a dialog's headline.
    Big(&'static str),
    /// An empty line.
    Gap,
    /// A button with this label.
    Button(&'static str),
}

/// One synthetic screenshot: a window with a title and some lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// The window's title, shown in its title bar.
    pub title: &'static str,
    pub lines: Vec<Line>,
}

impl Scene {
    /// Every piece of text in the scene.
    pub fn texts(&self) -> Vec<&'static str> {
        let mut out = vec![self.title];
        for line in &self.lines {
            match line {
                Line::Text(t) | Line::Big(t) | Line::Button(t) => out.push(t),
                Line::Gap => {}
            }
        }
        out
    }
}

/// The font. Each glyph is its rows from the top, `/` between them,
/// `#` for ink. Seven rows; nine for a letter with a descender.
const FONT: &[(char, &str)] = &[
    (' ', "...../...../...../...../...../...../....."),
    ('A', ".###./#...#/#...#/#####/#...#/#...#/#...#"),
    ('B', "####./#...#/#...#/####./#...#/#...#/####."),
    ('C', ".###./#...#/#..../#..../#..../#...#/.###."),
    ('D', "####./#...#/#...#/#...#/#...#/#...#/####."),
    ('E', "#####/#..../#..../####./#..../#..../#####"),
    ('F', "#####/#..../#..../####./#..../#..../#...."),
    ('G', ".###./#...#/#..../#.###/#...#/#...#/.####"),
    ('H', "#...#/#...#/#...#/#####/#...#/#...#/#...#"),
    ('I', ".###./..#../..#../..#../..#../..#../.###."),
    ('J', "..###/...#./...#./...#./...#./#..#./.##.."),
    ('K', "#...#/#..#./#.#../##.../#.#../#..#./#...#"),
    ('L', "#..../#..../#..../#..../#..../#..../#####"),
    ('M', "#...#/##.##/#.#.#/#.#.#/#...#/#...#/#...#"),
    ('N', "#...#/##..#/#.#.#/#..##/#...#/#...#/#...#"),
    ('O', ".###./#...#/#...#/#...#/#...#/#...#/.###."),
    ('P', "####./#...#/#...#/####./#..../#..../#...."),
    ('Q', ".###./#...#/#...#/#...#/#.#.#/#..#./.##.#"),
    ('R', "####./#...#/#...#/####./#.#../#..#./#...#"),
    ('S', ".####/#..../#..../.###./....#/....#/####."),
    ('T', "#####/..#../..#../..#../..#../..#../..#.."),
    ('U', "#...#/#...#/#...#/#...#/#...#/#...#/.###."),
    ('V', "#...#/#...#/#...#/#...#/#...#/.#.#./..#.."),
    ('W', "#...#/#...#/#...#/#.#.#/#.#.#/##.##/#...#"),
    ('X', "#...#/#...#/.#.#./..#../.#.#./#...#/#...#"),
    ('Y', "#...#/#...#/.#.#./..#../..#../..#../..#.."),
    ('Z', "#####/....#/...#./..#../.#.../#..../#####"),
    ('a', "...../...../.###./....#/.####/#...#/.####"),
    ('b', "#..../#..../#.##./##..#/#...#/#...#/####."),
    ('c', "...../...../.###./#..../#..../#...#/.###."),
    ('d', "....#/....#/.##.#/#..##/#...#/#...#/.####"),
    ('e', "...../...../.###./#...#/#####/#..../.###."),
    ('f', "..##./.#..#/.#.../###../.#.../.#.../.#..."),
    ('g', "...../...../.####/#...#/#...#/#...#/.####/....#/.###."),
    ('h', "#..../#..../#.##./##..#/#...#/#...#/#...#"),
    ('i', "..#../...../.##../..#../..#../..#../.###."),
    ('j', "...#./...../..##./...#./...#./...#./...#./#..#./.##.."),
    ('k', "#..../#..../#..#./#.#../##.../#.#../#..#."),
    ('l', ".##../..#../..#../..#../..#../..#../.###."),
    ('m', "...../...../##.#./#.#.#/#.#.#/#.#.#/#.#.#"),
    ('n', "...../...../#.##./##..#/#...#/#...#/#...#"),
    ('o', "...../...../.###./#...#/#...#/#...#/.###."),
    ('p', "...../...../####./#...#/#...#/#...#/####./#..../#...."),
    ('q', "...../...../.####/#...#/#...#/#...#/.####/....#/....#"),
    ('r', "...../...../#.##./##..#/#..../#..../#...."),
    ('s', "...../...../.####/#..../.###./....#/####."),
    ('t', ".#.../.#.../###../.#.../.#.../.#..#/..##."),
    ('u', "...../...../#...#/#...#/#...#/#..##/.##.#"),
    ('v', "...../...../#...#/#...#/#...#/.#.#./..#.."),
    ('w', "...../...../#...#/#...#/#.#.#/#.#.#/.#.#."),
    ('x', "...../...../#...#/.#.#./..#../.#.#./#...#"),
    ('y', "...../...../#...#/#...#/#...#/#...#/.####/....#/.###."),
    ('z', "...../...../#####/...#./..#../.#.../#####"),
    ('0', ".###./#...#/#..##/#.#.#/##..#/#...#/.###."),
    ('1', "..#../.##../..#../..#../..#../..#../.###."),
    ('2', ".###./#...#/....#/...#./..#../.#.../#####"),
    ('3', ".###./#...#/....#/..##./....#/#...#/.###."),
    ('4', "...#./..##./.#.#./#..#./#####/...#./...#."),
    ('5', "#####/#..../####./....#/....#/#...#/.###."),
    ('6', "..##./.#.../#..../####./#...#/#...#/.###."),
    ('7', "#####/....#/...#./..#../.#.../.#.../.#..."),
    ('8', ".###./#...#/#...#/.###./#...#/#...#/.###."),
    ('9', ".###./#...#/#...#/.####/....#/...#./.##.."),
    ('.', "...../...../...../...../...../.##../.##.."),
    (',', "...../...../...../...../...../.##../.##../..#../.#..."),
    (':', "...../...../.##../.##../...../.##../.##.."),
    (';', "...../...../.##../.##../...../.##../.##../..#../.#..."),
    ('!', "..#../..#../..#../..#../..#../...../..#.."),
    ('?', ".###./#...#/....#/...#./..#../...../..#.."),
    ('\'', "..#../..#../.#.../...../...../...../....."),
    ('"', ".#.#./.#.#./...../...../...../...../....."),
    ('-', "...../...../...../#####/...../...../....."),
    ('_', "...../...../...../...../...../...../#####"),
    ('/', "....#/....#/...#./..#../.#.../#..../#...."),
    ('(', "...#./..#../.#.../.#.../.#.../..#../...#."),
    (')', ".#.../..#../...#./...#./...#./..#../.#..."),
    ('[', ".###./.#.../.#.../.#.../.#.../.#.../.###."),
    (']', ".###./...#./...#./...#./...#./...#./.###."),
    ('<', "...#./..#../.#.../#..../.#.../..#../...#."),
    ('>', ".#.../..#../...#./....#/...#./..#../.#..."),
    ('=', "...../...../#####/...../#####/...../....."),
    ('+', "...../..#../..#../#####/..#../..#../....."),
    ('*', "...../#.#.#/.###./#####/.###./#.#.#/....."),
    ('#', ".#.#./.#.#./#####/.#.#./#####/.#.#./.#.#."),
    ('@', ".###./#...#/#.###/#.#.#/#.###/#..../.####"),
    ('%', "##..#/##..#/...#./..#../.#.../#..##/#..##"),
    ('&', ".##../#..#./#.#../.#.../#.#.#/#..#./.##.#"),
    ('$', "..#../.####/#.#../.###./..#.#/####./..#.."),
    ('æ', "...../...../##.#./..#.#/.####/#.#../.#.##"),
    ('ø', "...../....#/.###./#..##/#.#.#/##..#/.###./#..../....."),
    ('å', ".##../.##../.###./....#/.####/#...#/.####"),
    ('Æ', ".####/#.#../#.#../####./#.#../#.#../#.###"),
    ('Ø', ".###./#..##/#..##/#.#.#/##..#/##..#/.###."),
    ('Å', "..#../.#.#./..#../.###./#...#/#####/#...#"),
];

/// What is drawn for a character the font does not have: a hollow box.
const UNKNOWN: &str = "#####/#...#/#...#/#...#/#...#/#...#/#####";

fn glyph(c: char) -> Option<&'static str> {
    FONT.iter().find(|(known, _)| *known == c).map(|(_, rows)| *rows)
}

/// The font has this character.
pub fn has_glyph(c: char) -> bool {
    glyph(c).is_some()
}

/// An RGBA picture being drawn.
struct Canvas {
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(colour: [u8; 3]) -> Self {
        let pixel = [colour[0], colour[1], colour[2], 255];
        Self { rgba: pixel.iter().copied().cycle().take(WIDTH as usize * HEIGHT as usize * 4).collect() }
    }

    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, colour: [u8; 3]) {
        for py in y.min(HEIGHT)..(y + h).min(HEIGHT) {
            for px in x.min(WIDTH)..(x + w).min(WIDTH) {
                let i = (py * WIDTH + px) as usize * 4;
                self.rgba[i..i + 3].copy_from_slice(&colour);
            }
        }
    }

    /// Draw `text` with its top left corner at `(x, y)`, each font
    /// pixel `scale` pixels wide. Returns the x after the last letter.
    fn text(&mut self, text: &str, x: u32, y: u32, scale: u32, colour: [u8; 3]) -> u32 {
        let mut pen = x;
        for c in text.chars() {
            let rows = glyph(c).unwrap_or(UNKNOWN);
            for (row, bits) in rows.split('/').enumerate() {
                for (col, bit) in bits.chars().enumerate() {
                    if bit == '#' {
                        self.rect(pen + col as u32 * scale, y + row as u32 * scale, scale, scale, colour);
                    }
                }
            }
            pen += (GLYPH_W + 1) * scale;
        }
        pen
    }
}

/// `text` cut into lines of at most `max` characters, at spaces. A
/// word longer than a line stands on a line of its own.
fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let fits = line.chars().count() + 1 + word.chars().count() <= max;
        if line.is_empty() {
            line.push_str(word);
        } else if fits {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    lines.push(line);
    lines
}

/// Draw the scene: [`WIDTH`] x [`HEIGHT`] pixels, RGBA, rows from the
/// top.
pub fn render(scene: &Scene) -> Vec<u8> {
    let mut canvas = Canvas::new(DESKTOP);
    canvas.rect(WIN_X, WIN_Y, WIN_W, WIN_H, WINDOW);
    canvas.rect(WIN_X, WIN_Y, WIN_W, TITLE_H, TITLE_BAR);
    // The three window buttons.
    for (n, colour) in [[236, 106, 94], [244, 191, 79], [97, 197, 84]].into_iter().enumerate() {
        canvas.rect(WIN_X + 18 + n as u32 * 26, WIN_Y + 16, 16, 16, colour);
    }
    canvas.text(scene.title, WIN_X + 120, WIN_Y + 12, 3, INK);

    let left = WIN_X + MARGIN;
    let room = WIN_W - 2 * MARGIN;
    let mut y = WIN_Y + TITLE_H + MARGIN;
    for line in &scene.lines {
        match line {
            Line::Gap => y += LINE_ROWS * 3,
            Line::Text(text) | Line::Big(text) => {
                let scale = if matches!(line, Line::Big(_)) { 4 } else { 3 };
                let max = (room / ((GLYPH_W + 1) * scale)) as usize;
                for part in wrap(text, max) {
                    canvas.text(&part, left, y, scale, INK);
                    y += LINE_ROWS * scale;
                }
            }
            Line::Button(label) => {
                let width = (label.chars().count() as u32 + 4) * (GLYPH_W + 1) * 3;
                canvas.rect(left, y, width, 48, BUTTON);
                canvas.text(label, left + 2 * (GLYPH_W + 1) * 3, y + 13, 3, BUTTON_INK);
                y += 48 + 12;
            }
        }
    }
    canvas.rgba
}

/// The scene as a PNG file, in memory.
pub fn png(scene: &Scene) -> Result<Vec<u8>, String> {
    let mut rgba = render(scene);
    let png = paint::encode_png(&rgba, WIDTH, HEIGHT);
    rgba.fill(0);
    png
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene {
            title: "Notes - Shopping",
            lines: vec![
                Line::Big("Saturday"),
                Line::Text("Milk, bread and coffee filters."),
                Line::Gap,
                Line::Button("OK"),
            ],
        }
    }

    #[test]
    fn every_glyph_is_five_wide_and_seven_or_nine_high() {
        for (c, rows) in FONT.iter().copied().chain([('?', UNKNOWN)]) {
            let rows: Vec<&str> = rows.split('/').collect();
            assert!(rows.len() == 7 || rows.len() == 9, "{c}: {} rows", rows.len());
            for row in rows {
                assert_eq!(row.chars().count(), 5, "{c}: row \"{row}\"");
                assert!(row.chars().all(|bit| bit == '#' || bit == '.'), "{c}: row \"{row}\"");
            }
        }
        // No character twice, and every one but the space has ink.
        for (n, (c, rows)) in FONT.iter().enumerate() {
            assert!(!FONT[..n].iter().any(|(earlier, _)| earlier == c), "{c} is in the font twice");
            assert_eq!(rows.contains('#'), *c != ' ', "{c}");
        }
        // Letters that look alike are not the same drawing.
        for (a, b) in [('O', '0'), ('Ø', '0'), ('l', '1'), ('I', 'l'), ('S', '5'), ('B', '8'), ('ø', 'o'), ('å', 'a')] {
            assert_ne!(glyph(a), glyph(b), "{a} and {b}");
        }
    }

    #[test]
    fn the_font_covers_what_english_and_danish_text_needs() {
        let needed = "ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 .,:;!?'\"-_/()[]<>=+*#@%&$ æøåÆØÅ";
        for c in needed.chars() {
            assert!(has_glyph(c), "no glyph for {c}");
        }
        assert!(!has_glyph('\u{2014}'));
        assert!(!has_glyph('\n'));
    }

    #[test]
    fn long_text_wraps_at_words() {
        assert_eq!(wrap("one two three", 7), vec!["one two", "three"]);
        assert_eq!(wrap("one two three", 100), vec!["one two three"]);
        assert_eq!(wrap("a verylongword b", 6), vec!["a", "verylongword", "b"]);
        assert_eq!(wrap("", 10), vec![""]);
    }

    fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 3] {
        let i = (y * WIDTH + x) as usize * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    #[test]
    fn a_scene_is_a_window_with_ink_on_a_plain_desktop() {
        let rgba = render(&scene());
        assert_eq!(rgba.len(), WIDTH as usize * HEIGHT as usize * 4);
        assert!(rgba.chunks_exact(4).all(|px| px[3] == 255));
        assert_eq!(pixel(&rgba, 5, 5), DESKTOP);
        assert_eq!(pixel(&rgba, WIDTH - 5, HEIGHT - 5), DESKTOP);
        assert_eq!(pixel(&rgba, WIN_X + WIN_W - 10, WIN_Y + 10), TITLE_BAR);
        assert_eq!(pixel(&rgba, WIN_X + WIN_W - 10, WIN_Y + WIN_H - 10), WINDOW);
        let count = |colour: [u8; 3]| rgba.chunks_exact(4).filter(|px| px[..3] == colour).count();
        // The title and three lines of text are there, and the button.
        assert!(count(INK) > 3000, "{}", count(INK));
        assert!(count(BUTTON) > 3000, "{}", count(BUTTON));
        assert!(count(BUTTON_INK) > 100, "{}", count(BUTTON_INK));
        // More text is more ink; a different text is a different picture.
        let mut longer = scene();
        longer.lines.push(Line::Text("And oat biscuits."));
        assert!(render(&longer).chunks_exact(4).filter(|px| px[..3] == INK).count() > count(INK));
        // Text never leaves the window.
        let mut wide = scene();
        wide.lines = vec![Line::Text(
            "this line is far too long to fit in the window in one piece and so it has to be wrapped onto a second line",
        )];
        let wide = render(&wide);
        for y in 0..HEIGHT {
            for x in (0..WIN_X).chain(WIN_X + WIN_W..WIDTH) {
                assert_eq!(pixel(&wide, x, y), DESKTOP, "ink outside the window at {x}, {y}");
            }
        }
    }

    #[test]
    fn a_scene_encodes_as_a_png_of_the_stated_size() {
        let png = png(&scene()).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // The IHDR chunk: width and height, big endian.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), WIDTH);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), HEIGHT);
        // Flat colours compress well: far below what a model accepts.
        assert!(png.len() < 400_000, "{} bytes", png.len());
        assert_eq!(png, super::png(&scene()).unwrap(), "the same scene is the same file");
    }
}
