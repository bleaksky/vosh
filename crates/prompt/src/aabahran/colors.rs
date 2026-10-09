//! Backtick colors as the game sends them, and the reverse.
//!
//! `send_to_char` (`comm.c:6583-6649`) turns a backtick and the character
//! after it into color through `process_color` (`comm.c:1957-2049`),
//! which writes `ESC[0` then an entry of `color_table` (`ansi.h:156-224`)
//! then `m`. A backtick with `(NNN)` or `)NNN(` after it is a 256 color
//! foreground or background (`process_color_256`, `comm.c:2056-2091`).
//!
//! [`rebuild`] runs that backwards. The game shows your PROMPT setting
//! through `send_to_char`, so a line such as `Current prompt:` carries
//! colors where you typed backtick codes. Vosh writes each color back as
//! the first code `process_color` lists for it, so the field shows what
//! you typed, or a code the game turns into the same bytes. The writing
//! card reads your description and a note's text back the same way.

/// `color_table` in `ansi.h`. The game writes `ESC[0`, the entry and `m`.
const TABLE: [&str; 65] = [
    ";0", // 0, normal
    ";31",
    ";32",
    ";33",
    ";34",
    ";35",
    ";36",
    ";37",
    ";30", // 8, black, which no code reaches
    ";1;31",
    ";1;32",
    ";1;33",
    ";1;34",
    ";1;35",
    ";1;36",
    ";1;37",
    ";1;30", // 16
    ";5;31",
    ";5;32",
    ";5;33",
    ";5;34",
    ";5;35",
    ";5;36",
    ";5;37",
    ";5;30", // 24, which no code reaches
    ";1;5;31",
    ";1;5;32",
    ";1;5;33",
    ";1;5;34",
    ";1;5;35",
    ";1;5;36",
    ";1;5;37",
    ";1;5;30", // 32
    ";4;31",
    ";4;32",
    ";4;33",
    ";4;34",
    ";4;35",
    ";4;36",
    ";4;37",
    ";4;30", // 40, which no code reaches
    ";1;4;31",
    ";1;4;32",
    ";1;4;33",
    ";1;4;34",
    ";1;4;35",
    ";1;4;36",
    ";1;4;37",
    ";1;4;30", // 48
    ";5;4;31",
    ";5;4;32",
    ";5;4;33",
    ";5;4;34",
    ";5;4;35",
    ";5;4;36",
    ";5;4;37",
    ";5;4;30", // 56, which no code reaches
    ";1;5;4;31",
    ";1;5;4;32",
    ";1;5;4;33",
    ";1;5;4;34",
    ";1;5;4;35",
    ";1;5;4;36",
    ";1;5;4;37",
    ";1;5;4;30", // 64
];

/// The codes `process_color` knows and the table index each one writes,
/// in the order its switch lists them. Where two codes write the same
/// color, the first one here is the one [`rebuild`] writes back, which
/// keeps `9` for bold purple rather than `%`, a character the PROMPT
/// reads as the start of a code.
const CODES: [(char, u8); 67] = [
    ('`', 0),
    ('1', 1),
    ('2', 2),
    ('3', 3),
    ('4', 4),
    ('5', 5),
    ('6', 6),
    ('7', 7),
    ('8', 16),
    ('9', 13),
    ('0', 12),
    ('!', 9),
    ('@', 10),
    ('#', 11),
    ('$', 12),
    (')', 12),
    ('%', 13),
    ('^', 14),
    ('&', 15),
    ('*', 16),
    ('q', 17),
    ('w', 18),
    ('e', 19),
    ('r', 20),
    ('t', 21),
    ('y', 22),
    ('u', 23),
    ('i', 32),
    ('Q', 25),
    ('W', 26),
    ('E', 27),
    ('R', 28),
    ('T', 29),
    ('Y', 30),
    ('U', 31),
    ('I', 32),
    ('a', 33),
    ('s', 34),
    ('d', 35),
    ('f', 36),
    ('g', 37),
    ('h', 38),
    ('j', 39),
    ('k', 48),
    ('A', 41),
    ('S', 42),
    ('D', 43),
    ('F', 44),
    ('G', 45),
    ('H', 46),
    ('J', 47),
    ('K', 48),
    ('z', 49),
    ('x', 50),
    ('c', 51),
    ('v', 52),
    ('b', 53),
    ('n', 54),
    ('m', 55),
    (',', 64),
    ('Z', 57),
    ('X', 58),
    ('C', 59),
    ('V', 60),
    ('B', 61),
    ('N', 62),
    ('M', 63),
];

/// A color the game writes for a backtick code. It takes no cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Color {
    /// A `color_table` entry, and the code you wrote for it.
    Table { index: u8, code: char },
    /// `` `(NNN) ``, a 256 color foreground. A number above 255 sends
    /// nothing.
    Fg256(u16),
    /// `` `)NNN( ``, a 256 color background, the same way.
    Bg256(u16),
}

impl Color {
    /// The SGR the game writes for it with 256 color on, or None when it
    /// writes nothing. Test only.
    #[cfg(test)]
    pub(crate) fn sgr(self) -> Option<String> {
        match self {
            Self::Table { index, .. } => sgr(index),
            Self::Fg256(n) => (n <= 255).then(|| format!("\x1b[38;5;{n}m")),
            Self::Bg256(n) => (n <= 255).then(|| format!("\x1b[48;5;{n}m")),
        }
    }
}

/// The table index `process_color` writes for a code, or None when the
/// character after the backtick is not a color and prints as itself.
/// `-` and `=` print `~` and a backtick, so they are not colors either.
/// `<` writes the same color as `,` but sits after the table in the
/// switch, so it is looked up on its own.
pub(crate) fn index(code: char) -> Option<u8> {
    if code == '<' {
        return Some(64);
    }
    CODES.iter().find(|(c, _)| *c == code).map(|(_, i)| *i)
}

/// The bytes the game writes for a table index.
pub(crate) fn sgr(index: u8) -> Option<String> {
    TABLE
        .get(usize::from(index))
        .map(|entry| format!("\x1b[0{entry}m"))
}

/// The code [`rebuild`] writes for a table index.
fn code(index: u8) -> Option<char> {
    CODES.iter().find(|(_, i)| *i == index).map(|(c, _)| *c)
}

/// Your PROMPT setting with backtick codes, from the text the game
/// showed it as, colors included. Each color becomes the first code
/// that writes it, and 256 colors become `` `(NNN) `` or `` `)NNN( ``.
/// With 256 color off the game writes the nearest of the sixteen, which
/// comes back as the 256 code under 16 that writes the same bytes. A `~`
/// comes back as `` `- `` and a backtick as `` `= ``, since the game
/// keeps neither in a setting any other way. Any other escape drops.
pub fn rebuild(shown: &str) -> String {
    let mut out = String::with_capacity(shown.len());
    let mut chars = shown.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                if chars.next_if_eq(&'[').is_none() {
                    // A lone escape, or one that is not a CSI. Drop it and
                    // the character that names it.
                    chars.next();
                    continue;
                }
                let mut params = String::new();
                let mut last = None;
                for p in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&p) {
                        last = Some(p);
                        break;
                    }
                    params.push(p);
                }
                if last == Some('m') {
                    if let Some(code) = backtick(&params) {
                        out.push_str(&code);
                    }
                }
            }
            '~' => out.push_str("`-"),
            '`' => out.push_str("`="),
            other => out.push(other),
        }
    }
    out
}

/// The backtick code for the parameters of one SGR, or None when no code
/// writes it.
fn backtick(params: &str) -> Option<String> {
    if let Some(entry) = params.strip_prefix('0') {
        if let Some(index) = TABLE.iter().position(|e| *e == entry) {
            return code(index as u8).map(|c| format!("`{c}"));
        }
    }
    let fields: Vec<u16> = params
        .split(';')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let (open, n, close) = match fields.as_slice() {
        [38, 5, n] if *n <= 255 => ('(', *n, ')'),
        [48, 5, n] if *n <= 255 => (')', *n, '('),
        // `process_color_256` without 256 color on writes the nearest of
        // the sixteen, and a 256 code under 16 writes exactly that.
        [n @ 30..=37] => ('(', n - 30, ')'),
        [n @ 90..=97] => ('(', n - 90 + 8, ')'),
        [n @ 40..=47] => (')', n - 40, '('),
        [n @ 100..=107] => (')', n - 100 + 8, '('),
        _ => return None,
    };
    Some(format!("`{open}{n:03}{close}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_writes_its_table_entry() {
        assert_eq!(sgr(0).as_deref(), Some("\x1b[0;0m"));
        assert_eq!(index('1').and_then(sgr).as_deref(), Some("\x1b[0;31m"));
        assert_eq!(index('!').and_then(sgr).as_deref(), Some("\x1b[0;1;31m"));
        assert_eq!(index('8').and_then(sgr).as_deref(), Some("\x1b[0;1;30m"));
        assert_eq!(index('q').and_then(sgr).as_deref(), Some("\x1b[0;5;31m"));
        assert_eq!(index('a').and_then(sgr).as_deref(), Some("\x1b[0;4;31m"));
        assert_eq!(index('A').and_then(sgr).as_deref(), Some("\x1b[0;1;4;31m"));
        assert_eq!(index('z').and_then(sgr).as_deref(), Some("\x1b[0;5;4;31m"));
        assert_eq!(
            index('Z').and_then(sgr).as_deref(),
            Some("\x1b[0;1;5;4;31m")
        );
        assert_eq!(
            index(',').and_then(sgr).as_deref(),
            Some("\x1b[0;1;5;4;30m")
        );
        assert_eq!(index('<'), Some(64));
        // Codes that share a color.
        for (a, b) in [('0', '$'), ('$', ')'), ('9', '%'), ('8', '*')] {
            assert_eq!(index(a), index(b), "{a} and {b}");
        }
        assert_eq!(index('i'), index('I'));
        assert_eq!(index('k'), index('K'));
        // Not colors.
        for c in ['-', '=', '(', '[', ' ', 'o', 'l', 'L', 'p', 'P'] {
            assert_eq!(index(c), None, "{c}");
        }
    }

    #[test]
    fn a_256_color_writes_its_number_up_to_255() {
        assert_eq!(Color::Fg256(240).sgr().as_deref(), Some("\x1b[38;5;240m"));
        assert_eq!(Color::Bg256(17).sgr().as_deref(), Some("\x1b[48;5;17m"));
        assert_eq!(Color::Fg256(300).sgr(), None);
    }

    #[test]
    fn the_shown_setting_comes_back_as_you_typed_it() {
        // `fprompt `1%h``hp [%p] > ` as the game shows it.
        assert_eq!(
            rebuild("\x1b[0;31m%h\x1b[0;0mhp [%p] > "),
            "`1%h``hp [%p] > "
        );
        // Codes with no backtick show as typed.
        assert_eq!(
            rebuild("%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"),
            "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"
        );
        // A 256 color, foreground and background.
        assert_eq!(rebuild("\x1b[38;5;240m(\x1b[48;5;17mx"), "`(240)(`)017(x");
        // `-` and `=` show as a tilde and a backtick.
        assert_eq!(rebuild("a~b`c"), "a`-b`=c");
    }

    #[test]
    fn a_color_two_codes_write_comes_back_as_the_first() {
        assert_eq!(rebuild("\x1b[0;1;34m"), "`0");
        assert_eq!(rebuild("\x1b[0;1;35m%h"), "`9%h");
        assert_eq!(rebuild("\x1b[0;1;30m"), "`8");
        assert_eq!(rebuild("\x1b[0;1;5;30m"), "`i");
        assert_eq!(rebuild("\x1b[0;1;4;30m"), "`k");
        assert_eq!(rebuild("\x1b[0;1;5;4;30m"), "`,");
    }

    #[test]
    fn a_256_color_the_game_sent_as_sixteen_comes_back_the_same() {
        // 256 color off: `(240) writes ESC[37m, and `(007) writes the
        // same bytes.
        assert_eq!(rebuild("\x1b[37m(Wizi 60)"), "`(007)(Wizi 60)");
        assert_eq!(rebuild("\x1b[91m"), "`(009)");
        assert_eq!(rebuild("\x1b[44m"), "`)004(");
        assert_eq!(rebuild("\x1b[107m"), "`)015(");
    }

    #[test]
    fn every_color_rebuilds_to_a_code_that_writes_it_again() {
        for (c, i) in CODES {
            let shown = sgr(i).unwrap();
            let back = rebuild(&shown);
            let again = back.strip_prefix('`').and_then(|b| b.chars().next());
            assert_eq!(again.and_then(index), Some(i), "{c} came back as {back}");
        }
    }

    #[test]
    fn other_escapes_drop() {
        assert_eq!(rebuild("a\x1b[2Kb"), "ab");
        assert_eq!(rebuild("a\x1b[1mb"), "ab");
        assert_eq!(rebuild("a\x1b[38;5;300mb"), "ab");
        assert_eq!(rebuild("a\x1b]x"), "ax");
        assert_eq!(rebuild("a\x1b"), "a");
    }
}
