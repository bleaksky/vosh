//! The shapes your PROMPT settings compile to.
//!
//! A shape is one way the game can print your prompt, as a pattern per
//! line. `%c` always ends a line. `%C`, `%n`, `%p` and `%P` print only
//! while your opponent fights someone in your group, so a setting with
//! `%C` compiles to two shapes, Normal without them and Tank with them.
//! A setting without `%C` prints the same lines either way, so its one
//! shape reads `%n`, `%p` and `%P` as optional. A fight prompt compiles
//! the same way, and the game prints it only in a fight. The AFK prompt
//! and the fallback for an empty setting come from outside the code loop
//! and have shapes of their own.
//!
//! The first line of every shape starts with the immortal prefix the game
//! sends before the prompt (`comm.c:1780-1787`). A complete line is
//! anchored at both ends and allows trailing spaces. The final segment a
//! setting leaves with no line end reads two ways: as a partial with
//! nothing after it, its trailing spaces as ` +`, and as a complete line,
//! for the partial the next pulse's line end completes in the same read.
//! A shape settles, so its partial is the prompt at once, when that
//! segment ends in a character you wrote.

use std::collections::BTreeSet;

use regex::Regex;
use serde::Serialize;

use super::codes::{Code, Edges};
use super::lex::{self, Lexed, Piece, Placed, Token};
use super::{CompileError, Warning, WarningKind, Which, Who};

/// The Wizi and Incog levels the game prints before the first line.
pub(crate) const PREFIX: &str = r"(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?";

/// `<Nhp Nm Nmv>` and your `prefix` setting, the prompt the game prints
/// for an empty setting (`comm.c:1793-1799`).
const FALLBACK: &str = r"<(?<hp>-?\d+)hp (?<mana>-?\d+)m (?<move>-?\d+)mv>.*";

/// The prompt the game prints while you are away (`comm.c:1788-1792`).
const AFK: &str = "<AFK>";

/// Which way the game printed the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    /// No one in your group is tanking your opponent, so `%n %p %P %C`
    /// print nothing.
    Normal,
    /// Your opponent fights someone in your group. `%p` and `%P` print
    /// nothing under lamented tears, so they are optional.
    Tank,
    /// The setting has no `%C`, so both ways print the same lines, and
    /// `%n %p %P` are optional.
    Either,
    /// `<AFK> ` while you are away.
    Afk,
    /// `<Nhp Nm Nmv>` for an empty setting.
    Fallback,
}

/// The patterns for one line of a shape.
#[derive(Debug, Clone)]
pub struct ShapeLine {
    /// Reads the line with its line end taken off, or a partial a GA or
    /// EOR ended.
    pub line: Regex,
    /// Reads the final segment as a partial with nothing after it. Only
    /// the last line of a setting that ends with no line end has one.
    pub partial: Option<Regex>,
    /// `line` with a group around what every code prints, named `c` and
    /// the code's place in [`Shape::codes`], read or not. It matches
    /// exactly what `line` matches, so the card can mark each code's text
    /// in a prompt, the ones Vosh cannot tell apart among them. It is
    /// compiled only when the card asks.
    pub marker: String,
}

/// A code one of a shape's lines prints, with where it sits in the
/// setting and whether Vosh reads its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeCode {
    pub code: Code,
    pub span: std::ops::Range<usize>,
    pub read: bool,
}

/// What one code printed in a prompt a shape read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShapeMark {
    /// The line it is on, top line first.
    pub line: usize,
    /// Its bytes in that line.
    pub bytes: std::ops::Range<usize>,
    /// The code, or None for the immortal prefix's levels and the
    /// fallback prompt's values, which `name` names.
    pub code: Option<ShapeCode>,
    /// The group's name in the shape: the value a read code fills, or
    /// `wizi`, `incog`, `hp`, `mana` or `move` outside the code loop.
    pub name: Option<String>,
}

/// One way the game prints your prompt.
#[derive(Debug, Clone)]
pub struct Shape {
    pub kind: ShapeKind,
    pub which: Which,
    /// Top line first. The last is the line the drawn prompt always
    /// replaces.
    pub lines: Vec<ShapeLine>,
    /// A partial the last line reads is the prompt at once, since the
    /// setting's final segment ends in a character you wrote.
    pub settle: bool,
    /// Every code the lines print, in the order they print them.
    pub codes: Vec<ShapeCode>,
}

impl Shape {
    /// What each code printed in `lines`, a prompt this shape read: one
    /// mark per code that printed anything, and one for each level of
    /// the immortal prefix, in the order the lines print them. None when
    /// the shape does not read `lines`.
    pub(crate) fn marks(&self, lines: &[&str]) -> Option<Vec<ShapeMark>> {
        if lines.len() != self.lines.len() {
            return None;
        }
        let mut out = Vec::new();
        for (index, (text, line)) in lines.iter().zip(&self.lines).enumerate() {
            let marker = Regex::new(&line.marker).ok()?;
            let found = marker.captures(text)?;
            let mut marks: Vec<ShapeMark> = marker
                .capture_names()
                .flatten()
                .filter_map(|name| {
                    let m = found.name(name)?;
                    if m.is_empty() {
                        return None;
                    }
                    let code = name
                        .strip_prefix('c')
                        .and_then(|n| n.parse::<usize>().ok())
                        .and_then(|n| self.codes.get(n))
                        .cloned();
                    let name = match &code {
                        Some(code) if code.read => code.code.name().map(str::to_string),
                        Some(_) => None,
                        None => Some(name.to_string()),
                    };
                    Some(ShapeMark {
                        line: index,
                        bytes: m.range(),
                        code,
                        name,
                    })
                })
                .collect();
            marks.sort_by_key(|m| (m.bytes.start, m.bytes.end));
            out.extend(marks);
        }
        Some(out)
    }

    /// The names this shape reads, in the order its lines print them,
    /// the immortal prefix left out.
    pub(crate) fn names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for line in &self.lines {
            for name in line.line.capture_names().flatten() {
                if name != "wizi" && name != "incog" && !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }
}

/// Where a setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// As the game stores it: Char.Prompt, a `Current prompt:` line, or a
    /// saved capture. Vosh reads it as it is.
    Stored,
    /// You typed or pasted it, so Vosh stores it as `do_prompt` would
    /// first.
    Typed,
}

/// Your two settings compiled.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The PROMPT setting as the game stores it.
    pub prompt: String,
    /// The fight prompt setting as the game stores it, empty when none is
    /// set.
    pub fprompt: String,
    /// The prompt's shapes, then the fight prompt's, then AFK.
    pub shapes: Vec<Shape>,
    pub warnings: Vec<Warning>,
}

impl Compiled {
    /// The names one setting reads, in the order it prints them.
    pub fn reads(&self, which: Which) -> Vec<&str> {
        let mut names = Vec::new();
        for shape in self.shapes.iter().filter(|s| s.which == which) {
            for name in shape.names() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }
}

/// Compile your PROMPT and fight prompt settings into the shapes Vosh
/// recognizes your prompt by. A setting you typed is stored as the game
/// would store it first. `who` decides what `%u` and `%s` print.
pub fn compile(
    prompt: &str,
    fprompt: &str,
    origin: Origin,
    who: Who,
) -> Result<Compiled, CompileError> {
    let mut warnings = Vec::new();
    let (prompt, fprompt) = match origin {
        Origin::Stored => (prompt.to_string(), fprompt.to_string()),
        Origin::Typed => {
            let p = lex::normalize(prompt, Which::Prompt, who);
            let f = lex::normalize(fprompt, Which::Fight, who);
            warnings.extend(p.warnings);
            warnings.extend(f.warnings);
            (p.text, f.text)
        }
    };
    let mut shapes = Vec::new();
    if prompt.is_empty() {
        shapes.push(fallback());
    } else {
        shapes.extend(setting(&prompt, Which::Prompt, who, &mut warnings)?);
    }
    if !fprompt.is_empty() {
        shapes.extend(setting(&fprompt, Which::Fight, who, &mut warnings)?);
    }
    shapes.push(afk());
    warnings.sort_by(|a, b| {
        (a.which, a.span.start, a.span.end, a.kind).cmp(&(
            b.which,
            b.span.start,
            b.span.end,
            b.kind,
        ))
    });
    warnings.dedup();
    Ok(Compiled {
        prompt,
        fprompt,
        shapes,
        warnings,
    })
}

/// How a code reads in one shape.
#[derive(Debug, Clone, Copy)]
struct Use {
    /// Its group holds the value.
    capture: bool,
    /// The game may print nothing for it in this shape.
    optional: bool,
}

/// The shapes of one setting.
fn setting(
    text: &str,
    which: Which,
    who: Who,
    warnings: &mut Vec<Warning>,
) -> Result<Vec<Shape>, CompileError> {
    let read = lex::pass_one(text, which);
    warnings.extend(read.warnings);
    let tokens = read.tokens;

    // The first use of each field reads it. Codes are keyed by where they
    // start in the setting.
    let mut first: BTreeSet<&str> = BTreeSet::new();
    let mut reads_first: BTreeSet<usize> = BTreeSet::new();
    for lexed in &tokens {
        let Token::Code(code) = lexed.token else {
            continue;
        };
        match code {
            Code::Pacify if !who.immortal => warnings.push(Warning::new(
                WarningKind::PacifyMortal,
                which,
                lexed.span.clone(),
                "Only immortals get a value for %u. For anyone else the game repeats the text of the code before it, so Vosh cannot read this part.".into(),
            )),
            Code::Lang if who.mobile => warnings.push(Warning::new(
                WarningKind::LangMobile,
                which,
                lexed.span.clone(),
                "While you control a mobile, %s repeats the text of the code before it.".into(),
            )),
            _ => {}
        }
        let Some(name) = code.name() else {
            continue;
        };
        if first.insert(name) {
            reads_first.insert(lexed.span.start);
        } else {
            warnings.push(Warning::new(
                WarningKind::Twice,
                which,
                lexed.span.clone(),
                format!(
                    "Your prompt shows {} twice. Vosh reads the first one.",
                    code.label()
                ),
            ));
        }
    }

    let kinds: &[ShapeKind] = if tokens.iter().any(|l| l.token == Token::TankBreak) {
        &[ShapeKind::Normal, ShapeKind::Tank]
    } else {
        &[ShapeKind::Either]
    };
    let mut shapes = Vec::new();
    for &kind in kinds {
        let kept: Vec<Lexed> = tokens
            .iter()
            .filter(|l| match l.token {
                Token::TankBreak => kind == ShapeKind::Tank,
                Token::Code(code) if code.is_tank() => kind != ShapeKind::Normal,
                _ => true,
            })
            .cloned()
            .collect();
        let optional =
            |code: Code| code.is_tank() && (kind == ShapeKind::Either || code != Code::Tank);
        let edges = |code: Code| {
            let edges = code.edges(who);
            if optional(code) {
                edges.nullable()
            } else {
                edges
            }
        };
        let pieces = lex::pass_two(&kept, which, &edges)?;
        let lines = split(&pieces);
        let mut unread = BTreeSet::new();
        for line in &lines {
            for (a, b) in run_together(line, &edges) {
                unread.insert(a.span.start);
                unread.insert(b.span.start);
                let (Piece::Code(ca), Piece::Code(cb)) = (a.piece, b.piece) else {
                    continue;
                };
                warnings.push(Warning::new(
                    WarningKind::RunTogether,
                    which,
                    a.span.start..b.span.end,
                    format!(
                        "Vosh cannot tell where {} ends and {} begins. Put a space between them in the game.",
                        ca.label(),
                        cb.label()
                    ),
                ));
            }
        }
        let uses = |placed: &Placed, code: Code| Use {
            capture: code.readable(who)
                && reads_first.contains(&placed.span.start)
                && !unread.contains(&placed.span.start),
            optional: optional(code),
        };
        let shape = build(kind, which, &lines, who, &uses);
        let literal = pieces
            .iter()
            .filter(|p| matches!(p.piece, Piece::Text(_)))
            .count();
        if shape.names().is_empty() && literal < 4 {
            warnings.push(Warning::new(
                WarningKind::Short,
                which,
                0..text.len(),
                "This prompt is short enough to match other lines. Vosh can draw over them by mistake."
                    .into(),
            ));
        }
        shapes.push(shape);
    }
    Ok(shapes)
}

/// The pieces split into lines at each line end. The final segment is
/// the last entry, empty when the setting ends in a line end.
fn split(pieces: &[Placed]) -> Vec<Vec<&Placed>> {
    let mut lines = vec![Vec::new()];
    for placed in pieces {
        if placed.piece == Piece::Break {
            lines.push(Vec::new());
        } else {
            lines.last_mut().expect("a line").push(placed);
        }
    }
    lines
}

/// Pairs of codes in one line with nothing between them that tells
/// where the first ends. A code that can print nothing lets the one
/// before it run into the one after it.
fn run_together<'a>(
    line: &[&'a Placed],
    edges: &dyn Fn(Code) -> Edges,
) -> Vec<(&'a Placed, &'a Placed)> {
    let mut pairs = Vec::new();
    for (i, a) in line.iter().enumerate() {
        let Piece::Code(ca) = a.piece else {
            continue;
        };
        for b in &line[i + 1..] {
            match b.piece {
                Piece::Color(_) => {}
                Piece::Code(cb) => {
                    if edges(ca).runs_into(edges(cb)) {
                        pairs.push((*a, *b));
                    }
                    if !edges(cb).nullable {
                        break;
                    }
                }
                Piece::Text(_) | Piece::Break => break,
            }
        }
    }
    pairs
}

/// Whether a line is the last and how it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// Its line end taken off. Trailing spaces are optional.
    Line,
    /// A partial with nothing after it. Its trailing spaces are ` +`.
    Partial,
}

fn build(
    kind: ShapeKind,
    which: Which,
    lines: &[Vec<&Placed>],
    who: Who,
    uses: &dyn Fn(&Placed, Code) -> Use,
) -> Shape {
    let (last, whole) = lines.split_last().expect("at least the final segment");
    // A final segment that prints nothing leaves the last whole line as
    // the prompt's last line.
    let open = whole.is_empty() || last.iter().any(|p| !matches!(p.piece, Piece::Color(_)));
    let mut codes: Vec<ShapeCode> = Vec::new();
    let mut out: Vec<ShapeLine> = whole
        .iter()
        .enumerate()
        .map(|(i, line)| ShapeLine {
            line: regex(line, i == 0, End::Line, who, uses),
            partial: None,
            marker: marker(line, i == 0, who, uses, &mut codes),
        })
        .collect();
    if open {
        let first = whole.is_empty();
        out.push(ShapeLine {
            line: regex(last, first, End::Line, who, uses),
            partial: Some(regex(last, first, End::Partial, who, uses)),
            marker: marker(last, first, who, uses, &mut codes),
        });
    }
    let settle = open
        && matches!(
            last.iter()
                .rev()
                .find(|p| !matches!(p.piece, Piece::Color(_)))
                .map(|p| p.piece),
            Some(Piece::Text(_))
        );
    Shape {
        kind,
        which,
        lines: out,
        settle,
        codes,
    }
}

/// The pattern for one line.
fn regex(
    line: &[&Placed],
    first: bool,
    end: End,
    who: Who,
    uses: &dyn Fn(&Placed, Code) -> Use,
) -> Regex {
    let pattern = source(line, first, end, who, uses, &mut |_, code, usage| {
        usage
            .capture
            .then(|| code.name())
            .flatten()
            .map(str::to_string)
    });
    Regex::new(&pattern).expect("a compiled shape is a valid pattern")
}

/// The pattern for one line with every code in a group of its own,
/// named `c` and its place in `codes`, which it adds the line's codes
/// to. See [`ShapeLine::marker`].
fn marker(
    line: &[&Placed],
    first: bool,
    who: Who,
    uses: &dyn Fn(&Placed, Code) -> Use,
    codes: &mut Vec<ShapeCode>,
) -> String {
    source(
        line,
        first,
        End::Line,
        who,
        uses,
        &mut |placed, code, usage| {
            codes.push(ShapeCode {
                code,
                span: placed.span.clone(),
                read: usage.capture && code.name().is_some(),
            });
            Some(format!("c{}", codes.len() - 1))
        },
    )
}

/// The source of the pattern for one line. `name` gives the group each
/// code's value goes in, or None for no group.
fn source(
    line: &[&Placed],
    first: bool,
    end: End,
    who: Who,
    uses: &dyn Fn(&Placed, Code) -> Use,
    name: &mut dyn FnMut(&Placed, Code, Use) -> Option<String>,
) -> String {
    // Trailing spaces, past any colors among them, come off the text.
    let mut cut = line.len();
    let mut spaces = 0;
    for (i, placed) in line.iter().enumerate().rev() {
        match placed.piece {
            Piece::Color(_) => {}
            Piece::Text(' ') => {
                spaces += 1;
                cut = i;
            }
            _ => break,
        }
    }
    let mut re = String::from("^");
    if first {
        re.push_str(PREFIX);
    }
    let mut text = String::new();
    for placed in &line[..cut] {
        match placed.piece {
            Piece::Text(c) => text.push(c),
            Piece::Color(_) | Piece::Break => {}
            Piece::Code(code) => {
                re.push_str(&regex::escape(&text));
                text.clear();
                let usage = uses(placed, code);
                let group = name(placed, code, usage);
                let pattern = code.pattern(who).regex(group.as_deref());
                if usage.optional {
                    re.push_str("(?:");
                    re.push_str(&pattern);
                    re.push_str(")?");
                } else {
                    re.push_str(&pattern);
                }
            }
        }
    }
    re.push_str(&regex::escape(&text));
    re.push_str(match end {
        End::Line => " *$",
        End::Partial if spaces > 0 => " +$",
        End::Partial => "$",
    });
    re
}

/// The one line the game prints while you are away.
fn afk() -> Shape {
    fixed(ShapeKind::Afk, AFK)
}

/// The prompt the game prints for an empty setting.
fn fallback() -> Shape {
    fixed(ShapeKind::Fallback, FALLBACK)
}

/// A shape of one partial line from outside the code loop, which ends in
/// a space.
fn fixed(kind: ShapeKind, body: &str) -> Shape {
    let pattern = |tail: &str| {
        Regex::new(&format!("^{PREFIX}{body}{tail}")).expect("a fixed shape is a valid pattern")
    };
    let line = if kind == ShapeKind::Fallback {
        pattern("$")
    } else {
        pattern(" *$")
    };
    Shape {
        kind,
        which: Which::Prompt,
        lines: vec![ShapeLine {
            marker: line.as_str().to_string(),
            line,
            partial: Some(pattern(" $")),
        }],
        settle: true,
        codes: Vec::new(),
    }
}
