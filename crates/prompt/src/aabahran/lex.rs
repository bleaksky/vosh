//! A PROMPT setting as the game stores it, and the two passes the game
//! prints it in.
//!
//! [`normalize`] does what the game does to a setting you type:
//! `read_from_buffer` reads the line, and `do_prompt` and `do_fprompt`
//! (`act_info.c:2083-2146`) store what it read. A setting the game
//! sends in Char.Prompt, or shows after `Current prompt:`, is already
//! stored that way, so Vosh reads it as it came and skips this step.
//!
//! [`pass_one`] reads the stored setting as `bust_a_prompt`
//! (`comm.c:1801-1944`) does, into characters it copies, breaks and
//! value codes. [`pass_two`] reads each run of copied characters as
//! `send_to_char` (`comm.c:6583-6649`) and `process_color` do, into text
//! and colors that take no cell.

use std::ops::Range;

use super::codes::{chars, Code, Edges};
use super::colors::{self, Color};
use super::{CompileError, Warning, WarningKind, Which, Who};

/// What `prompt all` sets (`act_info.c:2098`).
pub const PROMPT_ALL: &str = "%n%P%C<%hhp %mm %vmv> ";

/// The most characters of a line the game reads, `MIL - 3`
/// (`read_from_buffer`, `comm.c:1486-1495`). Past them it says "Line too
/// long." and runs the ones it read.
pub const LINE: usize = 253;

/// The most characters of a setting the game keeps when you type it:
/// what the line holds after `prompt ` or `fprompt `. `do_prompt` would
/// keep 255, but the line never brings it that many.
pub fn keeps(which: Which) -> usize {
    let command = match which {
        Which::Prompt => "prompt ",
        Which::Fight => "fprompt ",
    };
    LINE - command.len()
}

/// A setting you typed, as the game stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    pub warnings: Vec<Warning>,
}

/// Store a setting you typed as the game does. Vosh trims the ends of
/// every command it sends. The game reads the line through
/// [`read_line`], and skips the spaces after the command word
/// (`one_argument`, `interp.c:1704-1727`), so no space before the
/// setting reaches `do_prompt`. That turns each `~` into `-`
/// (`smash_tilde`) and adds one space unless the setting ends in `%c` in
/// any case (`str_suffix`). `prompt all` is the stock prompt and
/// `fprompt off` clears the fight prompt. A setting that is empty or
/// only spaces stays empty, which the game draws as its fallback prompt.
///
/// `prompt off` turns prompts off and sets nothing, so the caller
/// handles it before it gets here.
pub fn normalize(typed: &str, which: Which, who: Who) -> Normalized {
    let empty = || Normalized {
        text: String::new(),
        warnings: Vec::new(),
    };
    let keeps = keeps(which);
    let (read, cut) = read_line(typed.trim(), keeps, who);
    let argument = read.trim_start_matches(' ');
    match which {
        Which::Prompt if argument == "all" => {
            return Normalized {
                text: PROMPT_ALL.to_string(),
                warnings: Vec::new(),
            };
        }
        Which::Fight if argument.eq_ignore_ascii_case("off") => return empty(),
        _ => {}
    }
    if argument.is_empty() {
        return empty();
    }
    let mut warnings = Vec::new();
    if cut {
        let at = argument.len();
        warnings.push(Warning::new(
            WarningKind::Cut,
            which,
            at..at,
            format!(
                "The game keeps the first {keeps} characters of your prompt. Vosh reads the same {keeps}."
            ),
        ));
    }
    let mut text = argument.replace('~', "-");
    if !ends_in_break(&text) {
        text.push(' ');
    }
    Normalized { text, warnings }
}

/// What the game reads of a setting you type, as `read_from_buffer`
/// (`comm.c:1474-1500`) reads the line, and whether the line ran out of
/// room. It keeps printable ASCII alone and at most `room` characters of
/// it. For anyone it does not keep backticks for, it drops each backtick
/// and the character after it, and those take no room.
fn read_line(typed: &str, room: usize, who: Who) -> (String, bool) {
    let mut read = String::new();
    let mut count = 0;
    let mut chars = typed.chars();
    while let Some(c) = chars.next() {
        if count >= room {
            return (read, true);
        }
        if c == '`' && !who.keeps_backticks {
            chars.next();
        } else if c == ' ' || c.is_ascii_graphic() {
            read.push(c);
            count += 1;
        }
    }
    (read, false)
}

/// True when a setting ends in `%c` or `%C`, as `str_suffix("%c", …)`
/// tests it.
fn ends_in_break(text: &str) -> bool {
    text.len() >= 2 && text.as_bytes()[text.len() - 2..].eq_ignore_ascii_case(b"%c")
}

/// What pass one reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// A character the game copies as it is. A backtick among them starts
    /// a color in pass two.
    Lit(char),
    /// `%c`, a line end.
    Break,
    /// `%C`, a line end only while your opponent fights someone in your
    /// group.
    TankBreak,
    /// A value code.
    Code(Code),
}

/// A token and the bytes of the setting it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    pub token: Token,
    pub span: Range<usize>,
}

/// A setting read by pass one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassOne {
    pub tokens: Vec<Lexed>,
    pub warnings: Vec<Warning>,
}

const LONE_PERCENT: &str =
    "Your prompt ends in a lone %, which swallows the space the game adds. Remove it in the game.";

/// Read a stored setting as `bust_a_prompt` does. `%l` and a character
/// copy a backtick and that character, `%L` two backticks and `%%` a
/// percent sign. `%f` or `%j` with no digit prints nothing and leaves the
/// next character alone. `%` and any other character prints nothing and
/// takes that character, so a `%` at the end swallows the space the game
/// adds, which warns.
pub fn pass_one(setting: &str, which: Which) -> PassOne {
    let mut tokens = Vec::new();
    let mut warnings = Vec::new();
    let mut push = |token, span| tokens.push(Lexed { token, span });
    let mut it = setting.char_indices().peekable();
    while let Some((at, c)) = it.next() {
        if c != '%' {
            push(Token::Lit(c), at..at + c.len_utf8());
            continue;
        }
        let Some((code_at, letter)) = it.next() else {
            // The game reads on past the end of the setting.
            warnings.push(lone_percent(which, at..at + 1));
            break;
        };
        let end = code_at + letter.len_utf8();
        match letter {
            'c' => push(Token::Break, at..end),
            'C' => push(Token::TankBreak, at..end),
            'l' => {
                // The backtick and whatever character follows. At the very
                // end the game copies the backtick alone.
                let (x, end) = match it.next() {
                    Some((x_at, x)) => (Some(x), x_at + x.len_utf8()),
                    None => (None, end),
                };
                push(Token::Lit('`'), at..end);
                if let Some(x) = x {
                    push(Token::Lit(x), at..end);
                }
            }
            'L' => {
                push(Token::Lit('`'), at..end);
                push(Token::Lit('`'), at..end);
            }
            '%' => push(Token::Lit('%'), at..end),
            'f' | 'j' => {
                if let Some(&(digit_at, digit)) = it.peek() {
                    if let Some(code) = Code::with_digit(letter, digit) {
                        it.next();
                        push(Token::Code(code), at..digit_at + 1);
                    }
                }
            }
            _ => match Code::from_letter(letter) {
                Some(code) => push(Token::Code(code), at..end),
                None if letter == ' ' && end == setting.len() => {
                    warnings.push(lone_percent(which, at..end));
                }
                None => {}
            },
        }
    }
    PassOne { tokens, warnings }
}

fn lone_percent(which: Which, span: Range<usize>) -> Warning {
    Warning::new(WarningKind::LonePercent, which, span, LONE_PERCENT.into())
}

/// What the game prints, after pass two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    /// A character that takes one cell.
    Text(char),
    /// A color, which takes no cell.
    Color(Color),
    /// A line end.
    Break,
    /// A value code.
    Code(Code),
}

/// A piece and the bytes of the setting it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub piece: Piece,
    pub span: Range<usize>,
}

/// Read each run of copied characters as `send_to_char` does. A backtick
/// and a color code make a color, `(NNN)` and `)NNN(` a 256 color, `-`
/// a tilde and `=` a backtick, and any other character prints as
/// itself. A backtick that ends the setting or a line drops.
///
/// Pass one's `%C` reaches here as a line end, so the caller drops it
/// first for a shape where it prints nothing. `edges` says what each code
/// can print in the shape, and whether it can print nothing there.
///
/// A run that ends in a backtick right before a code takes the first
/// character printed after it. When that is always one `process_color`
/// prints as it is, such as the `[` that `%e` starts with, the backtick
/// just drops. Otherwise it can take the code's first character as a
/// color, which no pattern can follow, and so can a run that ends in part
/// of a 256 color the code's digits would finish. That is the error.
pub fn pass_two(
    tokens: &[Lexed],
    which: Which,
    edges: &dyn Fn(Code) -> Edges,
) -> Result<Vec<Placed>, CompileError> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lexed) = tokens.get(i) {
        let piece = match lexed.token {
            Token::Lit(_) => {
                let start = i;
                while matches!(
                    tokens.get(i),
                    Some(Lexed {
                        token: Token::Lit(_),
                        ..
                    })
                ) {
                    i += 1;
                }
                run(&tokens[start..i], &tokens[i..], which, edges, &mut out)?;
                continue;
            }
            Token::Break | Token::TankBreak => Piece::Break,
            Token::Code(code) => Piece::Code(code),
        };
        out.push(Placed {
            piece,
            span: lexed.span.clone(),
        });
        i += 1;
    }
    Ok(out)
}

/// The characters of a code's text that `process_color` prints as they
/// are after a backtick.
const PLAIN: u8 = chars::BRACKET | chars::SPACE | chars::APOS;

/// True when a character prints as it is after a backtick, so the
/// backtick before it just drops. A color code, `-`, `=`, and the `(`
/// that can start a 256 color do not.
fn plain(c: char) -> bool {
    colors::index(c).is_none() && !matches!(c, '-' | '=' | '(')
}

/// True when the first character printed from `rest` on, the tokens after
/// a run that ends in a lone backtick, is always one that prints as it
/// is, so the backtick drops whatever the codes print. A code that can
/// print nothing hands the question to what follows it. A line end or
/// the end of the setting takes the backtick as the game drops it there.
fn takes_plain(rest: &[Lexed], edges: &dyn Fn(Code) -> Edges) -> bool {
    match rest.first().map(|l| l.token) {
        None | Some(Token::Break | Token::TankBreak) => true,
        Some(Token::Lit(c)) => plain(c),
        Some(Token::Code(code)) => {
            let prints = edges(code);
            prints.first & !PLAIN == 0 && (!prints.nullable || takes_plain(&rest[1..], edges))
        }
    }
}

/// One run of copied characters, and the tokens after it.
fn run(
    run: &[Lexed],
    rest: &[Lexed],
    which: Which,
    edges: &dyn Fn(Code) -> Edges,
    out: &mut Vec<Placed>,
) -> Result<(), CompileError> {
    let next = rest.first();
    let chars: Vec<(char, Range<usize>)> = run
        .iter()
        .filter_map(|l| match l.token {
            Token::Lit(c) => Some((c, l.span.clone())),
            _ => None,
        })
        .collect();
    let next_code = next.and_then(|l| match l.token {
        Token::Code(code) => Some((code, l.span.clone())),
        _ => None,
    });
    let swallows = |from: usize| {
        let (code, span) = next_code.clone().expect("a code follows");
        CompileError::runs_into(which, &code.written(), from..span.end)
    };
    let n = chars.len();
    let mut k = 0;
    while k < n {
        let (c, span) = chars[k].clone();
        let mut place = |piece, end: usize| {
            out.push(Placed {
                piece,
                span: span.start..end,
            });
        };
        if c != '`' {
            place(Piece::Text(c), span.end);
            k += 1;
            continue;
        }
        let Some((a, a_span)) = chars.get(k + 1).cloned() else {
            // A backtick at the end of the run takes the first character
            // printed after it.
            if next_code.is_some() && !takes_plain(rest, edges) {
                return Err(swallows(span.start));
            }
            k += 1;
            continue;
        };
        if a == '(' || a == ')' {
            let close = if a == '(' { ')' } else { '(' };
            let digits: String = chars[k + 2..]
                .iter()
                .take(3)
                .map(|(d, _)| *d)
                .take_while(char::is_ascii_digit)
                .collect();
            if digits.len() == 3 && chars.get(k + 5).is_some_and(|(c, _)| *c == close) {
                let number: u16 = digits.parse().expect("three digits");
                let color = if a == '(' {
                    Color::Fg256(number)
                } else {
                    Color::Bg256(number)
                };
                place(Piece::Color(color), chars[k + 5].1.end);
                k += 6;
                continue;
            }
            // The run ends inside the color, and the code after it can
            // print the digits that finish it.
            let ends_inside = digits.len() < 3 && k + 2 + digits.len() == n;
            let digit_next = next_code
                .as_ref()
                .is_some_and(|(code, _)| edges(*code).first & chars::DIGIT != 0);
            if ends_inside && digit_next {
                return Err(swallows(span.start));
            }
        }
        let piece = match a {
            '-' => Piece::Text('~'),
            '=' => Piece::Text('`'),
            _ => colors::index(a).map_or(Piece::Text(a), |index| {
                Piece::Color(Color::Table { index, code: a })
            }),
        };
        place(piece, a_span.end);
        k += 2;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mortal, whose backticks the game drops.
    const MORTAL: Who = Who {
        immortal: false,
        mobile: false,
        keeps_backticks: false,
    };

    /// An immortal with trust 55 or more, whose backticks the game keeps.
    const TRUSTED: Who = Who {
        immortal: true,
        mobile: false,
        keeps_backticks: true,
    };

    fn stored(typed: &str) -> String {
        normalize(typed, Which::Prompt, MORTAL).text
    }

    #[test]
    fn the_game_adds_one_space_after_the_trailing_ones_go() {
        assert_eq!(
            stored("[%h/%Hhp %m/%Mmn %v/%Vmv]"),
            "[%h/%Hhp %m/%Mmn %v/%Vmv] "
        );
        assert_eq!(stored("<%hhp>   "), "<%hhp> ");
    }

    #[test]
    fn the_game_skips_the_spaces_before_the_setting() {
        // The game skips the spaces after the command word, and Vosh
        // trims the ends of every command it sends.
        assert_eq!(stored("  <%hhp>"), "<%hhp> ");
        assert_eq!(stored(" <%hhp %mm %vmv> "), "<%hhp %mm %vmv> ");
        // Spaces inside the setting stay.
        assert_eq!(stored("<%hhp>  <%mm>"), "<%hhp>  <%mm> ");
    }

    #[test]
    fn a_setting_that_ends_in_a_break_gets_no_space() {
        assert_eq!(
            stored("%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"),
            "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c"
        );
        assert_eq!(stored("[%h]%C"), "[%h]%C");
        assert_eq!(stored("[%h]%c  "), "[%h]%c");
        // `%%c` ends in `%c` as the game compares, though it prints a
        // percent sign and a c.
        assert_eq!(stored("100%%c"), "100%%c");
    }

    #[test]
    fn prompt_all_is_the_stock_prompt() {
        assert_eq!(stored("all"), PROMPT_ALL);
        // The spaces around it never reach the game.
        assert_eq!(stored(" all "), PROMPT_ALL);
        assert_eq!(stored("all  "), PROMPT_ALL);
        // The game compares exactly, so anything else is a setting.
        assert_eq!(stored("All"), "All ");
        assert_eq!(normalize("all", Which::Fight, MORTAL).text, "all ");
    }

    #[test]
    fn fprompt_off_clears_the_fight_prompt() {
        assert_eq!(normalize("off", Which::Fight, MORTAL).text, "");
        assert_eq!(normalize("OFF", Which::Fight, MORTAL).text, "");
        assert_eq!(normalize(" off ", Which::Fight, MORTAL).text, "");
        assert_eq!(normalize("off!", Which::Fight, MORTAL).text, "off! ");
    }

    #[test]
    fn a_tilde_becomes_a_dash() {
        assert_eq!(stored("~%h~"), "-%h- ");
    }

    #[test]
    fn an_empty_setting_stays_empty() {
        assert_eq!(
            normalize("", Which::Prompt, MORTAL),
            normalize("   ", Which::Prompt, MORTAL)
        );
        assert_eq!(stored(""), "");
        assert_eq!(stored("   "), "");
        assert_eq!(normalize("", Which::Fight, MORTAL).text, "");
    }

    #[test]
    fn the_game_reads_the_first_253_characters_of_the_line() {
        // `prompt ` takes 7 of them, so 246 are left for the setting.
        let typed = format!("{}%h", "x".repeat(300));
        let got = normalize(&typed, Which::Prompt, MORTAL);
        assert_eq!(got.text, format!("{} ", "x".repeat(246)));
        assert_eq!(
            got.warnings,
            [Warning::new(
                WarningKind::Cut,
                Which::Prompt,
                246..246,
                "The game keeps the first 246 characters of your prompt. Vosh reads the same 246."
                    .into(),
            )]
        );
        // A setting of 250 is cut too, where the game cuts it.
        let typed = format!("<%hhp>{}", "y".repeat(244));
        let got = normalize(&typed, Which::Prompt, MORTAL);
        assert_eq!(got.text, format!("<%hhp>{} ", "y".repeat(240)));
        assert_eq!(got.warnings.len(), 1);
        // Exactly 246 is kept whole.
        let typed = "y".repeat(246);
        let got = normalize(&typed, Which::Prompt, MORTAL);
        assert_eq!(got.text, format!("{typed} "));
        assert!(got.warnings.is_empty());
        // `fprompt ` takes 8, which leaves 245.
        let typed = "y".repeat(246);
        let got = normalize(&typed, Which::Fight, MORTAL);
        assert_eq!(got.text, format!("{} ", "y".repeat(245)));
        assert_eq!(
            got.warnings[0].text,
            "The game keeps the first 245 characters of your prompt. Vosh reads the same 245."
        );
        assert!(normalize(&"y".repeat(245), Which::Fight, MORTAL)
            .warnings
            .is_empty());
        // A cut that ends in spaces keeps them as the game does, and the
        // game adds its own.
        let typed = format!("{}{}zz", "w".repeat(240), " ".repeat(10));
        let got = normalize(&typed, Which::Prompt, MORTAL);
        assert_eq!(got.text, format!("{}{}", "w".repeat(240), " ".repeat(7)));
        assert_eq!(got.warnings[0].span, 246..246);
    }

    #[test]
    fn a_mortal_loses_each_backtick_and_the_character_after_it() {
        let mortal = |typed: &str| normalize(typed, Which::Prompt, MORTAL).text;
        let trusted = |typed: &str| normalize(typed, Which::Prompt, TRUSTED).text;
        // What the game prints for a mortal is what Vosh reads.
        assert_eq!(mortal("`(240)[%h/%Hhp]"), "240)[%h/%Hhp] ");
        assert_eq!(mortal("`1%h``hp [%p] >"), "%hhp [%p] > ");
        assert_eq!(mortal("[`-%h`=]"), "[%h] ");
        // A backtick at the end goes alone.
        assert_eq!(mortal("<%h>`"), "<%h> ");
        // What it leaves before the setting counts as the spaces there.
        assert_eq!(mortal("`x <%h>"), "<%h> ");
        // %l is how a mortal writes a color, and it stays.
        assert_eq!(mortal("%l1%h%L"), "%l1%h%L ");
        // Trust 55 keeps them.
        assert_eq!(trusted("`(240)[%h/%Hhp]"), "`(240)[%h/%Hhp] ");
        assert_eq!(trusted("`1%h``hp [%p] >"), "`1%h``hp [%p] > ");
    }

    #[test]
    fn the_game_reads_only_printable_ascii() {
        let mortal = |typed: &str| normalize(typed, Which::Prompt, MORTAL).text;
        assert_eq!(mortal("<%h>\t<%m>"), "<%h><%m> ");
        assert_eq!(mortal("<%h é>"), "<%h > ");
        assert_eq!(mortal("é<%h>"), "<%h> ");
    }

    #[test]
    fn the_line_counts_only_what_the_game_keeps() {
        // Ten backtick pairs a mortal loses take no room on the line.
        let typed = format!("{}{}", "`1".repeat(10), "x".repeat(246));
        let got = normalize(&typed, Which::Prompt, MORTAL);
        assert_eq!(got.text, format!("{} ", "x".repeat(246)));
        assert!(got.warnings.is_empty());
        // The same line from someone who keeps them runs out of room.
        let got = normalize(&typed, Which::Prompt, TRUSTED);
        assert_eq!(got.text, format!("{}{} ", "`1".repeat(10), "x".repeat(226)));
        assert_eq!(got.warnings.len(), 1);
    }

    /// The Char.Prompt fixtures, which carry settings as the game stores
    /// them. The fight prompt's backticks mean an immortal with trust 55
    /// or more set it, since the game drops them for anyone else.
    const CHAR_PROMPT: [&str; 3] = [
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt.gmcp"),
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt-off.gmcp"),
        include_str!("../../../../fixtures/gmcp/aabahran/char-prompt-fight.gmcp"),
    ];

    #[test]
    fn a_setting_the_game_stored_is_already_normal() {
        for file in CHAR_PROMPT {
            let (package, json) = file.trim_end().split_once(' ').expect("a package and JSON");
            assert_eq!(package, "Char.Prompt");
            let data: serde_json::Value = serde_json::from_str(json).expect("JSON");
            for (key, which) in [("prompt", Which::Prompt), ("fprompt", Which::Fight)] {
                let setting = data[key].as_str().expect("a string");
                let got = normalize(setting, which, TRUSTED);
                assert_eq!(got.text, setting, "{key} in {file}");
                assert!(got.warnings.is_empty(), "{key} in {file}");
            }
        }
    }

    fn tokens(setting: &str) -> Vec<Token> {
        pass_one(setting, Which::Prompt)
            .tokens
            .into_iter()
            .map(|l| l.token)
            .collect()
    }

    fn lits(text: &str) -> Vec<Token> {
        text.chars().map(Token::Lit).collect()
    }

    #[test]
    fn pass_one_reads_as_bust_a_prompt_does() {
        use Token::{Break, Code as C, Lit, TankBreak};
        let cases: Vec<(&str, Vec<Token>)> = vec![
            ("ab", lits("ab")),
            ("%c", vec![Break]),
            ("%C", vec![TankBreak]),
            ("%h", vec![C(Code::Hp)]),
            ("%l1", lits("`1")),
            ("%l%h", lits("`%h")),
            ("%L", lits("``")),
            ("%%", lits("%")),
            ("%%h", lits("%h")),
            ("%f1", vec![C(Code::Slot(1))]),
            ("%f0", vec![C(Code::Slot(0))]),
            ("%j3", vec![C(Code::Moon(3))]),
            // %f and %j with no digit print nothing and leave the next
            // character to be read.
            ("%fx", lits("x")),
            ("%j%h", vec![C(Code::Hp)]),
            // Any other letter prints nothing and goes with the %.
            ("%qx", lits("x")),
            ("a%yb", lits("ab")),
            ("50% hp", lits("50hp")),
            (
                "%n%P%C[%h]%c",
                vec![
                    C(Code::Tank),
                    C(Code::TankBar),
                    TankBreak,
                    Lit('['),
                    C(Code::Hp),
                    Lit(']'),
                    Break,
                ],
            ),
        ];
        for (setting, want) in cases {
            assert_eq!(tokens(setting), want, "{setting}");
        }
    }

    #[test]
    fn pass_one_keeps_where_each_token_came_from() {
        let read = pass_one("<%l1%h%f1x>", Which::Prompt);
        let spans: Vec<Range<usize>> = read.tokens.iter().map(|l| l.span.clone()).collect();
        assert_eq!(spans, [0..1, 1..4, 1..4, 4..6, 6..9, 9..10, 10..11]);
    }

    #[test]
    fn a_lone_percent_at_the_end_warns() {
        for (setting, span, count) in [("<%h> %", 5..6, 4), ("<%h>% ", 4..6, 3)] {
            let read = pass_one(setting, Which::Fight);
            assert_eq!(read.tokens.len(), count, "{setting}");
            assert_eq!(
                read.warnings,
                [Warning::new(
                    WarningKind::LonePercent,
                    Which::Fight,
                    span,
                    "Your prompt ends in a lone %, which swallows the space the game adds. Remove it in the game.".into(),
                )],
                "{setting}"
            );
        }
        // A percent and a space in the middle take the space quietly.
        assert!(pass_one("50% hp ", Which::Prompt).warnings.is_empty());
    }

    fn printed(setting: &str) -> Result<Vec<Piece>, CompileError> {
        let read = pass_one(setting, Which::Prompt);
        pass_two(&read.tokens, Which::Prompt, &|code| {
            code.edges(Who::default())
        })
        .map(|placed| placed.into_iter().map(|p| p.piece).collect())
    }

    fn table(index: u8, code: char) -> Piece {
        Piece::Color(Color::Table { index, code })
    }

    fn text(t: &str) -> Vec<Piece> {
        t.chars().map(Piece::Text).collect()
    }

    #[test]
    fn pass_two_reads_colors_as_send_to_char_does() {
        use Piece::{Color as Col, Text};
        let cases: Vec<(&str, Vec<Piece>)> = vec![
            // `(NNN) and `)NNN( are 256 colors.
            ("%l(240)x", vec![Col(Color::Fg256(240)), Text('x')]),
            ("%l)017(x", vec![Col(Color::Bg256(17)), Text('x')]),
            ("%l(300)", vec![Col(Color::Fg256(300))]),
            // A table code.
            ("%l1x", vec![table(1, '1'), Text('x')]),
            ("%L", vec![table(0, '`')]),
            ("%l$", vec![table(12, '$')]),
            // - and = print a tilde and a backtick.
            ("%l-%l=", text("~`")),
            // Any other character prints as itself.
            ("%l[x", text("[x")),
            ("%l(x", text("(x")),
            // Not a whole 256 color. ( prints, and ) is a table color.
            ("%l(24", text("(24")),
            ("%l(2400)", text("(2400)")),
            ("%l)24", [vec![table(12, ')')], text("24")].concat()),
            // %l takes the next character whatever it is, so %l%h is a
            // color and an h.
            ("%l%h", [vec![table(13, '%')], text("h")].concat()),
            // A backtick at the end drops.
            ("ab%l", text("ab")),
            // A lone backtick before a line end drops too.
            ("a`%cb", [text("a"), vec![Piece::Break], text("b")].concat()),
        ];
        for (setting, want) in cases {
            assert_eq!(printed(setting), Ok(want), "{setting}");
        }
    }

    #[test]
    fn the_help_example_reads_as_the_game_colors_it() {
        let got = printed("<%l!%h%Lhp %l6%m%Lm %l3%v%Lmv>").unwrap();
        let want = [
            text("<"),
            vec![table(9, '!'), Piece::Code(Code::Hp), table(0, '`')],
            text("hp "),
            vec![table(6, '6'), Piece::Code(Code::Mana), table(0, '`')],
            text("m "),
            vec![table(3, '3'), Piece::Code(Code::Move), table(0, '`')],
            text("mv>"),
        ]
        .concat();
        assert_eq!(got, want);
    }

    #[test]
    fn a_color_that_runs_into_a_code_is_an_error() {
        let error = |setting: &str| printed(setting).expect_err(setting);
        let err = error("<`%h>");
        assert_eq!(err.code, "%h");
        assert_eq!(err.span, 1..4);
        assert_eq!(
            err.text,
            "A color code runs into %h. Put a space between them in the game."
        );
        assert_eq!(err.to_string(), err.text);
        // A 256 color the code's digits would finish.
        assert_eq!(error("%l(%h").code, "%h");
        assert_eq!(error("%l(2%f1").code, "%f1");
        assert_eq!(error("%l)24%g").code, "%g");
        // Across a code that prints nothing in pass one.
        assert_eq!(error("`%q%x").code, "%x");
        assert_eq!(error("`%n").code, "%n");
        // A code that cannot print a digit leaves the 256 color unmade.
        assert_eq!(
            printed("%l(%S"),
            Ok(vec![Piece::Text('('), Piece::Code(Code::Pos)])
        );
        // Three digits wait for a close no code prints.
        assert!(printed("%l(240%h").is_ok());
        // A whole color right before a code is fine.
        assert!(printed("%l1%h").is_ok());
    }

    #[test]
    fn a_backtick_before_a_code_that_starts_with_a_bracket_drops() {
        // %e always starts with [, which a backtick prints as it is, so
        // the game prints the same as with no backtick.
        assert_eq!(
            printed("<`%e>"),
            Ok([text("<"), vec![Piece::Code(Code::Exits)], text(">")].concat())
        );
        assert_eq!(
            printed("[`%p]"),
            Ok([text("["), vec![Piece::Code(Code::TankPct)], text("]")].concat())
        );
        // A code that can start with a color code letter still runs in.
        assert_eq!(printed("`%S").expect_err("%S").code, "%S");
    }

    #[test]
    fn pieces_keep_where_they_came_from() {
        let read = pass_one("a%l(240)b%h", Which::Prompt);
        let placed = pass_two(&read.tokens, Which::Prompt, &|code| {
            code.edges(Who::default())
        })
        .unwrap();
        let spans: Vec<Range<usize>> = placed.iter().map(|p| p.span.clone()).collect();
        // The color spans %l and the four characters after it.
        assert_eq!(spans, [0..1, 1..8, 8..9, 9..11]);
    }
}
