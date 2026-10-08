//! A capture for a game Vosh has no codes for, built from the line you
//! point at.
//!
//! Digit runs become `(-?\d+)`, runs of spaces become ` +`, everything
//! else is escaped, and the line is anchored at both ends. So a prompt the
//! game pads to one width, whose spaces at the end shrink as a number
//! grows, matches at every width. A line that ends on its own text or on
//! spaces settles, as an Aabahran prompt that ends on its space does, so a
//! partial that matches is your prompt at once, and a partial split before
//! its end waits for the rest. One that ends on a number waits for a line
//! end, GA or EOR, since a read could split the number.
//!
//! Each number takes a name from the letters right after it, or after the
//! pair it ends, where `a/b` is a value and its max: `h`, `hp` and `hit`
//! read Health, `m`, `mn`, `ma`, `mana`, `sp` and `mp` read Mana, and `v`,
//! `mv`, `mov`, `move`, `st` and `ep` read Moves. Any other number is
//! `n1`, `n2` and so on, for you to rename. A number you leave out is
//! matched but not read, and a line with no number read is a recognizer
//! only, which is all a design drawn from GMCP needs. One line only in
//! this build.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::config::RegexCapture;
use crate::values;

/// One number in the line, as the card marks and names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Number {
    /// Byte range in the line, a minus sign included.
    pub span: [usize; 2],
    /// As the line shows it.
    pub text: String,
    /// The value it reads into, empty when you left it out.
    pub name: String,
    /// The name Vosh suggests from the letters after it.
    pub suggested: String,
    /// The label of `name`, the name itself for one Vosh does not know,
    /// and empty when you left it out.
    pub label: String,
    /// It is the max of the number before it, as in `100/120hp`.
    pub max: bool,
}

/// A capture built from a line, with its numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generic {
    pub capture: RegexCapture,
    pub numbers: Vec<Number>,
}

/// What the line is made of, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// The number with this index.
    Number(usize),
    /// A run of spaces.
    Spaces,
    /// Anything else, as a byte range of the line.
    Text(usize, usize),
}

/// The value a run of letters after a number names, and its max.
fn vital(letters: &str) -> Option<(&'static str, &'static str)> {
    match letters.to_ascii_lowercase().as_str() {
        "h" | "hp" | "hit" => Some(("hp", "maxhp")),
        "m" | "mn" | "ma" | "mana" | "sp" | "mp" => Some(("mana", "maxmana")),
        "v" | "mv" | "mov" | "move" | "st" | "ep" => Some(("move", "maxmove")),
        _ => None,
    }
}

/// The tokens of `line` and its numbers with the names Vosh suggests.
fn lex(line: &str) -> (Vec<Token>, Vec<Number>) {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut numbers: Vec<Number> = Vec::new();
    let mut text_start: Option<usize> = None;
    let mut i = 0;
    let flush = |tokens: &mut Vec<Token>, start: &mut Option<usize>, end: usize| {
        if let Some(from) = start.take() {
            tokens.push(Token::Text(from, end));
        }
    };
    while i < bytes.len() {
        let b = bytes[i];
        // A minus sign right before digits belongs to the number, unless
        // it follows a letter or a digit, as in `a-5` or `3-4`.
        let signed = b == b'-'
            && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
            && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric());
        if b.is_ascii_digit() || signed {
            flush(&mut tokens, &mut text_start, i);
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(Token::Number(numbers.len()));
            numbers.push(Number {
                span: [start, i],
                text: line[start..i].to_string(),
                name: String::new(),
                suggested: String::new(),
                label: String::new(),
                max: false,
            });
            continue;
        }
        if b == b' ' {
            flush(&mut tokens, &mut text_start, i);
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            tokens.push(Token::Spaces);
            continue;
        }
        text_start.get_or_insert(i);
        i += 1;
    }
    flush(&mut tokens, &mut text_start, bytes.len());
    suggest(line, &tokens, &mut numbers);
    (tokens, numbers)
}

/// Name each number from the letters after it or after its pair, and
/// mark the max of each pair. A value already named takes `n` and a
/// count, as any number with no letters after it does.
fn suggest(line: &str, tokens: &[Token], numbers: &mut [Number]) {
    let bytes = line.as_bytes();
    // A number, a slash and a number, with nothing between, is a pair.
    for window in tokens.windows(3) {
        if let [Token::Number(a), Token::Text(from, to), Token::Number(b)] = window {
            if &line[*from..*to] == "/" && *b == *a + 1 {
                numbers[*b].max = true;
            }
        }
    }
    let letters_after = |end: usize| -> &str {
        let len = bytes[end..]
            .iter()
            .take_while(|b| b.is_ascii_alphabetic())
            .count();
        &line[end..end + len]
    };
    let mut used: Vec<&str> = Vec::new();
    let mut names: Vec<Option<&'static str>> = vec![None; numbers.len()];
    let mut k = 0;
    while k < numbers.len() {
        let pair = numbers.get(k + 1).is_some_and(|n| n.max);
        let last = if pair { k + 1 } else { k };
        if let Some((value, max)) = vital(letters_after(numbers[last].span[1])) {
            if !used.contains(&value) {
                used.push(value);
                names[k] = Some(value);
                if pair {
                    names[k + 1] = Some(max);
                }
            }
        }
        k = last + 1;
    }
    let mut count = 0;
    for (number, name) in numbers.iter_mut().zip(names) {
        number.suggested = match name {
            Some(name) => name.to_string(),
            None => {
                count += 1;
                format!("n{count}")
            }
        };
    }
}

/// True when `name` can name a group in a pattern.
fn group_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The label the card shows for `name`.
fn label(name: &str) -> String {
    values::entry(name).map_or_else(|| name.to_string(), |e| e.label.to_string())
}

/// The capture for `line`, the plain text of the line you pointed at,
/// with `names` for its numbers in order: a name reads the number into
/// that value, and an empty name leaves it out. A number with no entry
/// takes the name Vosh suggests. It settles when the line ends on its own
/// text or on spaces, so a partial that matches is your prompt at once,
/// and not when it ends on a number.
pub fn from_line(line: &str, names: &[String]) -> Generic {
    let (tokens, mut numbers) = lex(line);
    let mut pattern = String::from("^");
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut named: Vec<String> = Vec::new();
    let mut groups = 0;
    for token in &tokens {
        match *token {
            Token::Number(k) => {
                let number = &mut numbers[k];
                let name = names
                    .get(k)
                    .map_or(number.suggested.as_str(), String::as_str)
                    .trim()
                    .to_string();
                if name.is_empty() {
                    pattern.push_str(r"-?\d+");
                } else {
                    groups += 1;
                    if group_name(&name) && !named.contains(&name) {
                        let _ = write!(pattern, r"(?<{name}>-?\d+)");
                        named.push(name.clone());
                    } else {
                        // A name no group can carry, or one a group
                        // already carries, goes by the group's number.
                        pattern.push_str(r"(-?\d+)");
                        map.insert(groups.to_string(), name.clone());
                    }
                }
                number.label = if name.is_empty() {
                    String::new()
                } else {
                    label(&name)
                };
                number.name = name;
            }
            Token::Spaces => pattern.push_str(" +"),
            Token::Text(from, to) => pattern.push_str(&regex::escape(&line[from..to])),
        }
    }
    pattern.push('$');
    let settle = matches!(tokens.last(), Some(Token::Spaces | Token::Text(..)));
    Generic {
        capture: RegexCapture {
            lines: vec![pattern],
            settle,
            names: map,
            seen_at: None,
            source: None,
        },
        numbers,
    }
}
