//! Recognizing your prompt, by a pattern you point at or write or by
//! Aabahran's codes, and the capture triggers older builds used, read as
//! a `kind = "regex"` capture.
//!
//! [`settle`] decides whether a partial line the last pattern matches is
//! the prompt at once. [`from_trigger`] reads a capture trigger, the kind
//! `#prompt {regex}` wrote, into a capture the profile holds.
//! [`Recognizer`] is a capture compiled for the stage, a regex capture or
//! Aabahran's codes, which reads a line, a partial or a block of lines as
//! your prompt and says whether a partial can still become one. It reads
//! Aabahran's codes through each [`Shape`], whose reading lives here so
//! the shapes module needs nothing from this one.
//!
//! `Recognizer::marks`, which only the candidates view reads, lives in
//! [`crate::card::candidates`], so this module never imports the card.

pub mod generic;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use regex_automata::hybrid::dfa::{Cache, DFA};
use regex_automata::{Anchored, Input};
use regex_syntax::hir::{HirKind, Look};

use crate::aabahran::{Compiled, Origin, Shape, ShapeKind, ShapeLine, Which, Who};
use crate::config::{CaptureConfig, CaptureSource, RegexCapture};

/// True when a partial line `pattern` matches is the prompt at once. That
/// holds only when the pattern is anchored at both ends and its last item
/// is a literal, so a match cannot stop on a prefix of a longer line.
/// Otherwise the prompt waits for a line end, GA or EOR, as a capture
/// trigger always did. A pattern that does not parse never settles.
pub fn settle(pattern: &str) -> bool {
    let Ok(hir) = regex_syntax::parse(pattern) else {
        return false;
    };
    let HirKind::Concat(items) = hir.kind() else {
        return false;
    };
    let [first, .., last_item, end] = items.as_slice() else {
        return false;
    };
    starts(first.kind()) && ends(end.kind()) && matches!(last_item.kind(), HirKind::Literal(_))
}

/// True when a stored settle flag can hold for `pattern`: anchored at both
/// ends, with a last item that is a literal or a literal one or more
/// times, such as the run of spaces ` +` a line you point at ends in
/// ([`generic`]). Either way a match ends on that literal, so it
/// cannot stop on a prefix of a longer line. [`settle`] derives the flag
/// for a pattern you write, and only from a plain literal.
fn can_settle(pattern: &str) -> bool {
    let Ok(hir) = regex_syntax::parse(pattern) else {
        return false;
    };
    let HirKind::Concat(items) = hir.kind() else {
        return false;
    };
    let [first, .., last_item, end] = items.as_slice() else {
        return false;
    };
    let literal = match last_item.kind() {
        HirKind::Literal(_) => true,
        HirKind::Repetition(rep) => rep.min >= 1 && matches!(rep.sub.kind(), HirKind::Literal(_)),
        _ => false,
    };
    starts(first.kind()) && ends(end.kind()) && literal
}

fn starts(kind: &HirKind) -> bool {
    matches!(
        kind,
        HirKind::Look(Look::Start | Look::StartLF | Look::StartCRLF)
    )
}

fn ends(kind: &HirKind) -> bool {
    matches!(kind, HirKind::Look(Look::End | Look::EndLF | Look::EndCRLF))
}

/// Why a trigger that calls `mud.set_prompt_var` stays a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotACapture {
    /// Its script does more than hand the pattern's groups to
    /// `mud.set_prompt_var`, or reads a group the pattern lacks.
    ScriptDoesMore,
    /// It reads the prompt with more than one pattern, or with none.
    Patterns,
    /// Its pattern does not compile, so it never matched anything.
    BadPattern,
}

/// One line of a capture trigger's script,
/// `mud.set_prompt_var("NAME", captures[N])`.
fn script_line() -> &'static Regex {
    static LINE: OnceLock<Regex> = OnceLock::new();
    LINE.get_or_init(|| {
        Regex::new(
            r#"^\s*mud\s*\.\s*set_prompt_var\s*\(\s*(?:"([^"\\]*)"|'([^'\\]*)')\s*,\s*captures\s*\[\s*(\d+)\s*\]\s*\)\s*;?\s*$"#,
        )
        .expect("the script line pattern compiles")
    })
}

/// A capture trigger as a `kind = "regex"` capture. `patterns` are the
/// trigger's enabled patterns and `body` its script. The trigger fits
/// when it has one pattern and its script is only
/// `mud.set_prompt_var("NAME", captures[N])` lines, where `captures[1]` is
/// the whole match and group k is `captures[k+1]`. The pattern is copied
/// as it is, unanchored if it was, so what matched before matches now.
/// A named group the script never read is left out, so the capture reads
/// exactly what the trigger did.
pub fn from_trigger(patterns: &[&str], body: &str) -> Result<RegexCapture, NotACapture> {
    let [pattern] = patterns else {
        return Err(NotACapture::Patterns);
    };
    let regex = Regex::new(pattern).map_err(|_| NotACapture::BadPattern)?;
    let groups: Vec<Option<&str>> = regex.capture_names().collect();

    // Group number to the variable the script handed it to.
    let mut read: BTreeMap<usize, String> = BTreeMap::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let found = script_line()
            .captures(line)
            .ok_or(NotACapture::ScriptDoesMore)?;
        let var = found
            .get(1)
            .or_else(|| found.get(2))
            .map_or("", |m| m.as_str())
            .to_string();
        let index: usize = found[3].parse().map_err(|_| NotACapture::ScriptDoesMore)?;
        // captures[1] is the whole match, and captures[k+1] group k.
        let group = index.checked_sub(1).ok_or(NotACapture::ScriptDoesMore)?;
        if var.is_empty() || group == 0 || group >= groups.len() {
            return Err(NotACapture::ScriptDoesMore);
        }
        if read.get(&group).is_some_and(|other| *other != var) {
            return Err(NotACapture::ScriptDoesMore);
        }
        read.insert(group, var);
    }
    if read.is_empty() {
        return Err(NotACapture::ScriptDoesMore);
    }

    let mut names = BTreeMap::new();
    for (group, name) in groups.iter().enumerate().skip(1) {
        match (read.get(&group), name) {
            (Some(var), Some(name)) if var == name => {}
            (Some(var), Some(name)) => {
                names.insert((*name).to_string(), var.clone());
            }
            (Some(var), None) => {
                names.insert(group.to_string(), var.clone());
            }
            (None, Some(name)) => {
                names.insert((*name).to_string(), String::new());
            }
            (None, None) => {}
        }
    }
    Ok(RegexCapture {
        lines: vec![(*pattern).to_string()],
        settle: settle(pattern),
        names,
        seen_at: None,
        source: Some(CaptureSource::Migrated),
    })
}

/// A capture compiled for the stage. It reads a line, a partial the game
/// has not ended yet, or a block of lines, as your prompt, and hands each
/// group to the variable it feeds.
///
/// A `kind = "regex"` capture reads one line. Aabahran's codes compile
/// into the shapes of [`crate::aabahran::shapes`], and a shape may span
/// lines, so the stage holds the lines that start one until the rest
/// arrives.
#[derive(Debug, Clone)]
pub struct Recognizer {
    pub(crate) reader: Reader,
    /// A lazy DFA per line of each shape, in shape order, to tell whether
    /// a partial can still become that line. A regex capture has one.
    prefixes: Vec<Vec<Prefix>>,
}

/// Whether text can still grow into a match of one pattern.
#[derive(Debug, Clone)]
struct Prefix {
    dfa: DFA,
    cache: Cache,
    /// The pattern starts with `^`, so a mismatch at the start is final.
    anchored: bool,
}

impl Prefix {
    fn new(pattern: &str) -> Option<Self> {
        let dfa = DFA::new(pattern).ok()?;
        let cache = dfa.create_cache();
        Some(Self {
            dfa,
            cache,
            anchored: anchored_start(pattern),
        })
    }

    /// True while `text` is a prefix of some line the pattern matches:
    /// the DFA state after it is not dead. An unanchored pattern can
    /// match further on, so it never dies.
    fn live(&mut self, text: &str) -> bool {
        let mode = if self.anchored {
            Anchored::Yes
        } else {
            Anchored::No
        };
        let input = Input::new(text).anchored(mode);
        let Ok(mut state) = self.dfa.start_state_forward(&mut self.cache, &input) else {
            return false;
        };
        for &byte in text.as_bytes() {
            match self.dfa.next_state(&mut self.cache, state, byte) {
                Ok(next) if !next.is_dead() && !next.is_quit() => state = next,
                _ => return false,
            }
        }
        true
    }
}

/// True when a pattern starts with `^` or `\A`.
fn anchored_start(pattern: &str) -> bool {
    let Ok(hir) = regex_syntax::parse(pattern) else {
        return false;
    };
    match hir.kind() {
        HirKind::Concat(items) => items.first().is_some_and(|first| starts(first.kind())),
        kind => starts(kind),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Reader {
    Regex {
        line: Regex,
        /// Group number and the variable it feeds, in group order.
        groups: Vec<(usize, String)>,
        /// A partial the pattern matches is the prompt at once.
        settle: bool,
    },
    Codes(Box<Compiled>),
}

/// A prompt the recognizer read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recognized {
    /// Each group the shape has, by variable. A group that printed
    /// nothing reads as an empty string.
    pub values: BTreeMap<String, String>,
    /// A partial the last line reads is the whole prompt, and a line end
    /// after it follows the prompt rather than ending it.
    pub settle: bool,
    /// The game's away prompt, which shows as sent.
    pub afk: bool,
    /// The groups each line reads, top line first, so the stage knows
    /// what a line above the last one carries.
    pub lines: Vec<Vec<String>>,
}

impl Recognizer {
    /// Compile a capture for a mortal. See [`Recognizer::compile_for`].
    /// Test only. The tests in `tests/` reach it through the `testkit`
    /// feature.
    #[cfg(any(test, feature = "testkit"))]
    pub fn compile(capture: &CaptureConfig) -> Option<Self> {
        Self::compile_for(capture, Who::default())
    }

    /// Compile a capture. `who` decides what `%u` and `%s` print. None
    /// when it recognizes nothing: no capture, a regex capture with no
    /// line or more than one, a pattern that does not compile, or codes
    /// the game cannot print in a way Vosh can follow.
    pub fn compile_for(capture: &CaptureConfig, who: Who) -> Option<Self> {
        let reader = match capture {
            CaptureConfig::None => return None,
            CaptureConfig::Aabahran(codes) => {
                let compiled =
                    crate::aabahran::compile(&codes.prompt, &codes.fprompt, Origin::Stored, who)
                        .ok()?;
                Reader::Codes(Box::new(compiled))
            }
            CaptureConfig::Regex(capture) => regex_reader(capture)?,
        };
        let prefixes = match &reader {
            Reader::Regex { line, .. } => vec![Prefix::new(line.as_str()).into_iter().collect()],
            Reader::Codes(compiled) => compiled
                .shapes
                .iter()
                .map(|shape| {
                    shape
                        .lines
                        .iter()
                        .filter_map(|line| Prefix::new(line.line.as_str()))
                        .collect()
                })
                .collect(),
        };
        Some(Self { reader, prefixes })
    }

    /// True when `partial`, after the held lines `held`, can still grow
    /// into your prompt, so the stage holds it a moment rather than paint
    /// it raw. A partial that no shape can become paints at once.
    pub(crate) fn live(&mut self, held: &[&str], partial: &str) -> bool {
        match &self.reader {
            Reader::Regex { .. } => {
                held.is_empty()
                    && self
                        .prefixes
                        .first_mut()
                        .and_then(|lines| lines.first_mut())
                        .is_some_and(|prefix| prefix.live(partial))
            }
            Reader::Codes(compiled) => {
                for (shape, prefixes) in compiled.shapes.iter().zip(self.prefixes.iter_mut()) {
                    if shape.lines.len() <= held.len() || prefixes.len() != shape.lines.len() {
                        continue;
                    }
                    let heads_match = held
                        .iter()
                        .zip(&shape.lines)
                        .all(|(text, line)| line.line.is_match(text));
                    if heads_match && prefixes[held.len()].live(partial) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// The compiled codes, for a capture that reads them. Test only.
    #[cfg(test)]
    pub(crate) fn codes(&self) -> Option<&Compiled> {
        match &self.reader {
            Reader::Codes(compiled) => Some(compiled),
            Reader::Regex { .. } => None,
        }
    }

    /// The groups each line of each way the game prints your prompt
    /// reads, top line first, and whether that way is the away prompt,
    /// which always shows as sent. A regex capture has one way, one line.
    pub(crate) fn shapes(&self) -> Vec<(Vec<Vec<String>>, bool)> {
        let names = |re: &Regex| -> Vec<String> {
            re.capture_names().flatten().map(str::to_string).collect()
        };
        match &self.reader {
            Reader::Regex { line, .. } => vec![(vec![names(line)], false)],
            Reader::Codes(compiled) => compiled
                .shapes
                .iter()
                .map(|shape| {
                    (
                        shape.lines.iter().map(|l| names(&l.line)).collect(),
                        shape.kind == ShapeKind::Afk,
                    )
                })
                .collect(),
        }
    }

    /// Every value a prompt this capture reads can fill, each once, the
    /// immortal prefix's among them.
    pub(crate) fn reads(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |name: &str| {
            if !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        };
        match &self.reader {
            Reader::Regex { groups, .. } => groups.iter().for_each(|(_, var)| add(var)),
            Reader::Codes(compiled) => {
                for shape in &compiled.shapes {
                    for line in &shape.lines {
                        line.line.capture_names().flatten().for_each(&mut add);
                    }
                }
            }
        }
        out
    }

    /// True when a partial some shape reads is the prompt at once. Test
    /// only.
    #[cfg(test)]
    pub(crate) fn settles(&self) -> bool {
        match &self.reader {
            Reader::Regex { settle, .. } => *settle,
            Reader::Codes(compiled) => compiled.shapes.iter().any(|s| s.settle),
        }
    }

    /// Read a complete line, or a partial a GA or EOR ended, as your
    /// prompt.
    pub fn line(&self, plain: &str) -> Option<Recognized> {
        self.read(&[plain])
    }

    /// Read a partial the game has not ended as your prompt, which only a
    /// capture that settles does. A partial split before its last
    /// character fails the anchored match and waits for the next read.
    pub fn partial(&self, plain: &str) -> Option<Recognized> {
        self.read_partial(&[plain])
    }

    /// Read `lines` as one whole prompt, top line first. The last line
    /// ended with a line end, a GA or an EOR.
    pub fn read(&self, lines: &[&str]) -> Option<Recognized> {
        match &self.reader {
            Reader::Regex {
                line,
                groups,
                settle,
            } => {
                let [plain] = lines else {
                    return None;
                };
                regex_read(line, groups, *settle, plain)
            }
            Reader::Codes(compiled) => {
                shapes_in_order(compiled).find_map(|shape| Some(found(shape, shape.read(lines)?)))
            }
        }
    }

    /// Read `lines` as one whole prompt whose last line is a partial the
    /// game has not ended. Only a shape that settles reads one.
    pub fn read_partial(&self, lines: &[&str]) -> Option<Recognized> {
        match &self.reader {
            Reader::Regex {
                line,
                groups,
                settle,
            } => {
                let [plain] = lines else {
                    return None;
                };
                if !*settle {
                    return None;
                }
                regex_read(line, groups, true, plain)
            }
            Reader::Codes(compiled) => shapes_in_order(compiled)
                .find_map(|shape| Some(found(shape, shape.read_partial(lines)?))),
        }
    }

    /// True when `lines` are the top lines of a shape with more lines,
    /// so the stage holds them for the rest.
    pub(crate) fn starts(&self, lines: &[&str]) -> bool {
        let Reader::Codes(compiled) = &self.reader else {
            return false;
        };
        compiled.shapes.iter().any(|shape| {
            shape.lines.len() > lines.len()
                && lines
                    .iter()
                    .zip(&shape.lines)
                    .all(|(text, line)| line.line.is_match(text))
        })
    }
}

/// The shapes to try, the away prompt first, since the game prints it in
/// place of every other while you are away.
pub(crate) fn shapes_in_order(compiled: &Compiled) -> impl Iterator<Item = &Shape> {
    let afk = compiled.shapes.iter().filter(|s| s.kind == ShapeKind::Afk);
    let rest = compiled.shapes.iter().filter(|s| s.kind != ShapeKind::Afk);
    afk.chain(rest)
}

// A shape reads lines into a Recognized, so its reading lives with the
// recognizer, and the shapes module needs nothing from the capture.
impl Shape {
    /// Read whole lines as this shape, the last one without its line end
    /// or ended by a GA or EOR. Every name the shape reads is in the
    /// values, empty where the game printed nothing. The AFK shape reads
    /// `afk`.
    pub fn read(&self, lines: &[&str]) -> Option<Recognized> {
        self.read_with(lines, |line| Some(&line.line))
    }

    /// Read lines whose last one is a partial the game has not ended.
    /// Only a shape that settles reads one, and a partial split before
    /// its last character does not match.
    pub fn read_partial(&self, lines: &[&str]) -> Option<Recognized> {
        if !self.settle {
            return None;
        }
        self.read_with(lines, |line| line.partial.as_ref())
    }

    fn read_with<'a>(
        &'a self,
        lines: &[&str],
        last: impl Fn(&'a ShapeLine) -> Option<&'a Regex>,
    ) -> Option<Recognized> {
        if lines.len() != self.lines.len() {
            return None;
        }
        let mut values = BTreeMap::new();
        let count = lines.len();
        for (i, (text, line)) in lines.iter().zip(&self.lines).enumerate() {
            let re = if i + 1 == count {
                last(line)?
            } else {
                &line.line
            };
            let found = re.captures(text)?;
            for name in re.capture_names().flatten() {
                let value = found.name(name).map_or("", |m| m.as_str());
                values.insert(name.to_string(), value.to_string());
            }
        }
        if self.kind == ShapeKind::Afk {
            values.insert("afk".to_string(), "1".to_string());
        }
        Some(Recognized {
            values,
            ..Recognized::default()
        })
    }
}

/// What a shape read, with what the shape itself says: whether it
/// settles, whether it is the away prompt, the groups on each line, and
/// `fight` when the game prints it only in a fight.
fn found(shape: &Shape, mut read: Recognized) -> Recognized {
    let tank_named = read.values.get("tank").is_some_and(|t| !t.is_empty());
    if shape.kind == ShapeKind::Tank || shape.which == Which::Fight || tank_named {
        read.values.insert("fight".to_string(), "1".to_string());
    }
    read.settle = shape.settle;
    read.afk = shape.kind == ShapeKind::Afk;
    read.lines = shape
        .lines
        .iter()
        .map(|line| {
            line.line
                .capture_names()
                .flatten()
                .map(str::to_string)
                .collect()
        })
        .collect();
    read
}

/// The values a regex capture fills, each group under the variable it
/// feeds, in the order the pattern reads them, each once. A group mapped
/// to an empty name fills nothing, and a capture that reads nothing
/// fills nothing (see [`Recognizer::compile_for`]).
pub fn fills(capture: &RegexCapture) -> Vec<String> {
    let Some(Reader::Regex { groups, .. }) = regex_reader(capture) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    for (_, var) in groups {
        if !names.contains(&var) {
            names.push(var);
        }
    }
    names
}

fn regex_reader(capture: &RegexCapture) -> Option<Reader> {
    let [pattern] = capture.lines.as_slice() else {
        return None;
    };
    if pattern.is_empty() {
        return None;
    }
    let line = Regex::new(pattern).ok()?;
    let groups = line
        .capture_names()
        .enumerate()
        .skip(1)
        .filter_map(|(index, name)| {
            let key = name.map_or_else(|| index.to_string(), str::to_string);
            let var = match capture.names.get(&key) {
                Some(var) => var.clone(),
                None => name?.to_string(),
            };
            (!var.is_empty()).then_some((index, var))
        })
        .collect();
    // A stored flag holds only for a pattern that can settle, so an
    // unanchored pattern never settles on a prefix plus more text.
    let settle = capture.settle && can_settle(pattern);
    Some(Reader::Regex {
        line,
        groups,
        settle,
    })
}

fn regex_read(
    line: &Regex,
    groups: &[(usize, String)],
    settle: bool,
    plain: &str,
) -> Option<Recognized> {
    let found = line.captures(plain)?;
    let mut values = BTreeMap::new();
    for (index, var) in groups {
        let value = found.get(*index).map_or("", |m| m.as_str());
        let known = values.get(var).is_some_and(|v: &String| !v.is_empty());
        if !known {
            values.insert(var.clone(), value.to_string());
        }
    }
    let names = groups.iter().map(|(_, var)| var.clone()).collect();
    Some(Recognized {
        values,
        settle,
        afk: false,
        lines: vec![names],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AabahranCapture;
    use crate::testkit::mud::PROMPT;

    /// The capture `#prompt` wrote for James's prompt, and the catalog
    /// still holds.
    const PATTERN: &str = r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";
    const BODY: &str = "mud.set_prompt_var(\"hp\", captures[2])\nmud.set_prompt_var(\"maxhp\", captures[3])\nmud.set_prompt_var(\"mana\", captures[4])\nmud.set_prompt_var(\"maxmana\", captures[5])\nmud.set_prompt_var(\"move\", captures[6])\nmud.set_prompt_var(\"maxmove\", captures[7])";

    #[test]
    fn a_pattern_anchored_at_both_ends_and_ending_in_text_settles() {
        assert!(settle(r"^\[(?<hp>\d+)hp\] $"));
        assert!(settle(r"^<(\d+)hp (\d+)m (\d+)mv> $"));
        assert!(settle(r"\A> \z"));
        assert!(settle(r"(?m)^\[(\d+)hp\]$"));
        // A run of spaces at the end can hold a stored flag, which a line
        // you point at sets, but derives none.
        assert!(!settle(r"^<(\d+)hp> +$"));
        assert!(can_settle(r"^<(\d+)hp> +$"));
        assert!(can_settle(r"^<(\d+)hp> $"));
        assert!(!can_settle(r"^<(\d+)hp> *$"));
        assert!(!can_settle(r"^<(\d+)hp> \d+$"));
        assert!(!can_settle(r"<(\d+)hp> +$"));
    }

    #[test]
    fn anything_else_waits_for_a_line_end() {
        // Unanchored, as the old capture trigger was.
        assert!(!settle(PATTERN));
        // Anchored at one end only.
        assert!(!settle(r"^\[(\d+)hp\]"));
        assert!(!settle(r"\[(\d+)hp\]$"));
        // Ends in a value or a run of spaces, which a longer line extends.
        assert!(!settle(r"^\[(\d+)hp\] (\d+)$"));
        assert!(!settle(r"^\[(\d+)hp\] *$"));
        assert!(!settle(r"^\[(\d+)hp\] +$"));
        // An alternation at the top, a group around it all, and a pattern
        // that does not parse.
        assert!(!settle(r"^a$|^b$"));
        assert!(!settle(r"(^a$)"));
        assert!(!settle(r"^[(\d+$"));
        assert!(!settle(""));
        assert!(!settle("^$"));
    }

    #[test]
    fn the_capture_prompt_wrote_moves_as_it_is() {
        let capture = from_trigger(&[PATTERN], BODY).unwrap();
        assert_eq!(capture.lines, [PATTERN]);
        assert!(
            !capture.settle,
            "an unanchored pattern waits for a line end"
        );
        assert!(capture.names.is_empty(), "{:?}", capture.names);
        assert_eq!(capture.source, Some(CaptureSource::Migrated));
        assert_eq!(capture.seen_at, None);
    }

    #[test]
    fn groups_under_other_names_and_unread_groups_are_listed() {
        let capture = from_trigger(
            &[r"<(?<h>\d+)hp (\d+)m (?<v>\d+)mv (?<gold>\d+)g>"],
            "mud.set_prompt_var('hp', captures[2])\n\n  mud.set_prompt_var(\"mana\", captures[ 3 ]);\n",
        )
        .unwrap();
        assert_eq!(
            capture.names,
            BTreeMap::from([
                ("h".to_string(), "hp".to_string()),
                ("2".to_string(), "mana".to_string()),
                ("v".to_string(), String::new()),
                ("gold".to_string(), String::new()),
            ])
        );
    }

    #[test]
    fn a_script_that_does_more_stays_a_trigger() {
        for body in [
            "mud.set_prompt_var(\"hp\", captures[2])\nmud.send(\"flee\")",
            "if captures[2] then mud.set_prompt_var(\"hp\", captures[2]) end",
            "mud.set_prompt_var(\"hp\", tonumber(captures[2]))",
            "mud.set_prompt_var(\"hp\", captures.hp)",
            // The whole match, a group the pattern lacks, and no group.
            "mud.set_prompt_var(\"hp\", captures[1])",
            "mud.set_prompt_var(\"hp\", captures[9])",
            "mud.set_prompt_var(\"hp\", captures[0])",
            "mud.set_prompt_var(\"\", captures[2])",
            // One group read under two names.
            "mud.set_prompt_var(\"hp\", captures[2])\nmud.set_prompt_var(\"health\", captures[2])",
            "",
            "-- nothing",
        ] {
            assert_eq!(
                from_trigger(&[PATTERN], body),
                Err(NotACapture::ScriptDoesMore),
                "{body}"
            );
        }
    }

    #[test]
    fn a_trigger_with_more_than_one_pattern_or_a_broken_one_stays() {
        assert_eq!(from_trigger(&[], BODY), Err(NotACapture::Patterns));
        assert_eq!(
            from_trigger(&[PATTERN, PATTERN], BODY),
            Err(NotACapture::Patterns)
        );
        assert_eq!(
            from_trigger(&[r"\[(?<hp>\d+"], BODY),
            Err(NotACapture::BadPattern)
        );
    }

    fn regex(lines: &[&str], settle: bool, names: &[(&str, &str)]) -> CaptureConfig {
        CaptureConfig::Regex(RegexCapture {
            lines: lines.iter().map(|l| (*l).to_string()).collect(),
            settle,
            names: names
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            seen_at: None,
            source: None,
        })
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn the_migrated_capture_reads_the_line_it_read_as_a_trigger() {
        let reader = Recognizer::compile(&regex(&[PATTERN], false, &[])).expect("it compiles");
        let full = values(&[
            ("hp", "1020"),
            ("maxhp", "1020"),
            ("mana", "800"),
            ("maxmana", "800"),
            ("move", "930"),
            ("maxmove", "930"),
        ]);
        let read = reader
            .line("[1020/1020hp 800/800mn 930/930mv]")
            .expect("the prompt");
        assert_eq!(read.values, full);
        // Unanchored, as the trigger was, so the immortal prefix still
        // reads.
        let read = reader
            .line("(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv]")
            .expect("the prompt with a prefix");
        assert_eq!(read.values, full);
        assert!(reader.line("You are hungry.").is_none());
        // It never settles, so a partial waits for a line end.
        assert!(reader
            .partial("[1020/1020hp 800/800mn 930/930mv]")
            .is_none());
        assert!(!reader.settles());
    }

    #[test]
    fn names_rename_groups_leave_them_out_and_read_numbered_ones() {
        let reader = Recognizer::compile(&regex(
            &[r"<(?<h>\d+)hp (\d+)m (?<v>\d+)mv (\d+)x (?<gold>\d+)g>"],
            false,
            &[("h", "hp"), ("2", "mana"), ("v", "")],
        ))
        .expect("it compiles");
        let read = reader.line("<10hp 20m 30mv 40x 50g>").expect("a prompt");
        // h reads as hp, group 2 as mana, v is left out, the unnamed
        // group 4 has no name to read into, and gold keeps its own.
        assert_eq!(
            read.values,
            values(&[("hp", "10"), ("mana", "20"), ("gold", "50")])
        );
    }

    #[test]
    fn a_regex_capture_fills_each_name_its_groups_feed_once() {
        let CaptureConfig::Regex(capture) = regex(
            &[r"<(?<h>\d+)hp (\d+)m (?<v>\d+)mv (\d+)x (?<gold>\d+)g (?<hp>\d+)>"],
            false,
            &[("h", "hp"), ("2", "mana"), ("v", "")],
        ) else {
            unreachable!()
        };
        assert_eq!(fills(&capture), ["hp", "mana", "gold"]);
        // A capture that reads nothing fills nothing.
        for lines in [&[][..], &[""], &["a", "b"], &["(?<hp>"]] {
            let CaptureConfig::Regex(capture) = regex(lines, false, &[]) else {
                unreachable!()
            };
            assert!(fills(&capture).is_empty(), "{lines:?}");
        }
    }

    #[test]
    fn a_group_that_printed_nothing_reads_empty() {
        let reader = Recognizer::compile(&regex(
            &[r"^(?:\(Wizi (?<wizi>\d+)\) )?<(?<hp>\d+)hp> $"],
            true,
            &[],
        ))
        .expect("it compiles");
        let read = reader.line("<10hp> ").expect("a prompt");
        assert_eq!(read.values, values(&[("wizi", ""), ("hp", "10")]));
    }

    #[test]
    fn an_anchored_capture_that_settles_reads_a_whole_partial() {
        let reader =
            Recognizer::compile(&regex(&[r"^<(?<hp>\d+)hp> $"], true, &[])).expect("it compiles");
        assert!(reader.settles());
        let read = reader.partial("<10hp> ").expect("the whole prompt");
        assert_eq!(read.values, values(&[("hp", "10")]));
        // A read split before the final space waits.
        assert!(reader.partial("<10hp>").is_none());
        assert!(reader.partial("<10h").is_none());
    }

    #[test]
    fn a_stored_settle_holds_only_for_a_pattern_that_can_settle() {
        // Hand edited to settle, but unanchored, so it never settles on a
        // prefix of a longer line.
        let reader =
            Recognizer::compile(&regex(&[r"<(?<hp>\d+)hp>"], true, &[])).expect("it compiles");
        assert!(!reader.settles());
        assert!(reader.partial("<10hp> and more").is_none());
        // Anchored and ending in text, but stored as waiting.
        let reader =
            Recognizer::compile(&regex(&[r"^<(?<hp>\d+)hp> $"], false, &[])).expect("it compiles");
        assert!(!reader.settles());
        assert!(reader.partial("<10hp> ").is_none());
        assert!(reader.line("<10hp> ").is_some());
    }

    #[test]
    fn a_regex_capture_reads_one_line() {
        assert!(Recognizer::compile(&CaptureConfig::None).is_none());
        assert!(Recognizer::compile(&regex(&[], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&["^a$", "^b$"], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&[r"\[(?<hp>\d+"], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&[""], false, &[])).is_none());
        let reader = Recognizer::compile(&regex(&["^a$"], false, &[])).expect("it compiles");
        assert!(reader.read(&["a", "a"]).is_none());
        assert!(!reader.starts(&["a"]));
        assert!(reader.codes().is_none());
    }

    /// An Aabahran capture of `prompt` and `fprompt` as the game stores
    /// them.
    fn codes(prompt: &str, fprompt: &str) -> CaptureConfig {
        CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.to_string(),
            fprompt: fprompt.to_string(),
            ..AabahranCapture::default()
        })
    }

    #[test]
    fn codes_read_a_one_line_prompt() {
        let reader = Recognizer::compile(&codes(PROMPT, "")).expect("it compiles");
        assert!(reader.codes().is_some());
        let read = reader
            .line("(Wizi 60) [1020/1020hp 800/800mn 930/930mv]")
            .expect("the prompt");
        assert_eq!(read.values["hp"], "1020");
        assert_eq!(read.values["wizi"], "60");
        assert_eq!(read.values["incog"], "");
        assert!(!read.values.contains_key("fight"), "out of a fight");
        assert!(!read.afk);
        assert!(!read.settle, "it ends in %c, so it waits for its line end");
        assert!(reader.line("You are hungry.").is_none());
    }

    #[test]
    fn codes_read_a_prompt_that_spans_lines_as_one_block() {
        let reader = Recognizer::compile(&codes(PROMPT, "")).expect("it compiles");
        let head = "Tester: [===|===|---|---]";
        let last = "[159/1020hp 310/800mn 489/930mv]";
        // The tank line starts a longer shape and is no prompt alone.
        assert!(reader.starts(&[head]));
        assert!(reader.line(head).is_none());
        assert!(!reader.starts(&[last]), "the normal prompt is whole");
        assert!(!reader.starts(&["You are hungry."]));

        let read = reader.read(&[head, last]).expect("the tank block");
        assert_eq!(read.values["tank"], "Tester");
        assert_eq!(read.values["tank_bar"], "===|===|---|---");
        assert_eq!(read.values["hp"], "159");
        assert_eq!(
            read.values["fight"], "1",
            "the tank shape prints in a fight"
        );
        // The groups each line carries, the immortal prefix on the top one.
        assert_eq!(read.lines.len(), 2);
        assert!(read.lines[0].contains(&"tank".to_string()));
        assert!(read.lines[0].contains(&"wizi".to_string()));
        assert!(read.lines[1].contains(&"hp".to_string()));
        assert!(!read.lines[1].contains(&"tank".to_string()));
        assert!(reader.read(&["You are hungry.", last]).is_none());
    }

    #[test]
    fn prompt_all_settles_and_its_tank_line_holds() {
        let reader =
            Recognizer::compile(&codes("%n%P%C<%hhp %mm %vmv> ", "")).expect("it compiles");
        assert!(reader.settles());
        let read = reader
            .partial("<159hp 310m 489mv> ")
            .expect("a whole partial");
        assert!(read.settle);
        assert_eq!(read.values["move"], "489");
        assert!(
            reader.partial("<159hp 310m 48").is_none(),
            "split, it waits"
        );
        let read = reader
            .read_partial(&["Tester: [===|===|===|---]", "<159hp 310m 489mv> "])
            .expect("tanking");
        assert_eq!(read.values["tank"], "Tester");
        assert_eq!(read.values["fight"], "1");
    }

    #[test]
    fn the_away_prompt_reads_first_and_shows_as_sent() {
        let reader = Recognizer::compile(&codes("%s> ", "")).expect("it compiles");
        // `<AFK> ` could read as a language, but while you are away the
        // game prints it in place of your prompt.
        let read = reader.partial("<AFK> ").expect("away");
        assert!(read.afk);
        assert_eq!(read.values["afk"], "1");
        assert!(reader.line("(Incog 55) <AFK>").is_some_and(|r| r.afk));
        let read = reader.partial("common> ").expect("your prompt");
        assert!(!read.afk);
    }

    #[test]
    fn a_fight_prompt_marks_the_fight() {
        let reader =
            Recognizer::compile(&codes("<%hhp> ", "`1%h``hp [%p] > ")).expect("it compiles");
        let read = reader.partial("<50hp> ").expect("out of a fight");
        assert!(!read.values.contains_key("fight"));
        // %p prints its own brackets inside the ones you wrote.
        let read = reader.partial("50hp [[45]] > ").expect("the fight prompt");
        assert_eq!(read.values["fight"], "1");
        assert_eq!(read.values["tank_pct"], "45");
    }

    #[test]
    fn an_empty_setting_reads_the_fallback() {
        let reader = Recognizer::compile(&codes("", "")).expect("it compiles");
        let read = reader.partial("<20hp 100m 110mv> ").expect("the fallback");
        assert_eq!(read.values["hp"], "20");
        assert_eq!(read.values["move"], "110");
    }

    #[test]
    fn who_decides_what_pacify_reads() {
        let capture = codes("<%h %u> ", "");
        let mortal = Recognizer::compile(&capture).expect("it compiles");
        assert!(!mortal
            .partial("<10 pacified> ")
            .expect("a prompt")
            .values
            .contains_key("pacify"));
        let immortal = Recognizer::compile_for(
            &capture,
            Who {
                immortal: true,
                mobile: false,
                keeps_backticks: false,
            },
        )
        .expect("it compiles");
        let read = immortal.partial("<10 not pacified> ").expect("a prompt");
        assert_eq!(read.values["pacify"], "not pacified");
    }

    #[test]
    fn a_partial_is_live_while_a_shape_can_still_follow_it() {
        let mut reader = Recognizer::compile(&codes(PROMPT, "")).expect("it compiles");
        for partial in [
            "",
            "[",
            "[1020/10",
            "(Wizi 60) [1020/1020hp 800/800mn 930/930mv]",
        ] {
            assert!(reader.live(&[], partial), "{partial:?}");
        }
        // The tank line and a partial of the prompt under it. %n prints
        // any name, so any text might yet be a tank line.
        assert!(reader.live(&[], "Tester: [===|==="));
        assert!(reader.live(&[], "You are hungry"));
        assert!(reader.live(&["Tester: [===|===|---|---]"], "[159/10"));
        assert!(!reader.live(&["Tester: [===|===|---|---]"], "You"));
        assert!(!reader.live(&["You are hungry."], "[159/10"));

        let mut reader = Recognizer::compile(&codes("<%hhp %mm %vmv> ", "")).expect("it compiles");
        assert!(reader.live(&[], "<10hp 2"));
        assert!(reader.live(&[], "<AF"));
        for partial in ["You are hungry", "<10hp x", "<AFK> x", "<10hp 20m 30mv> x"] {
            assert!(!reader.live(&[], partial), "{partial:?}");
        }

        // An anchored pattern dies on a mismatch, and an unanchored one
        // can match further on, so it never does.
        let mut anchored =
            Recognizer::compile(&regex(&[r"^<(?<hp>\d+)hp> $"], true, &[])).expect("it compiles");
        assert!(anchored.live(&[], "<10h"));
        assert!(!anchored.live(&[], "x<10h"));
        assert!(!anchored.live(&["<10hp> "], "<10h"), "one line only");
        let mut loose = Recognizer::compile(&regex(&[PATTERN], false, &[])).expect("it compiles");
        assert!(loose.live(&[], "You are hungry"));
    }

    #[test]
    fn codes_the_game_cannot_print_readably_recognize_nothing() {
        assert!(Recognizer::compile(&codes("<`%h> ", "")).is_none());
    }

    #[test]
    fn a_capture_with_no_values_still_recognizes() {
        let reader = Recognizer::compile(&regex(&["^> $"], true, &[])).expect("it compiles");
        let read = reader.partial("> ").expect("the prompt");
        assert!(read.values.is_empty());
    }

    #[test]
    fn an_anchored_trigger_settles() {
        let capture = from_trigger(
            &[r"^<(?<hp>\d+)hp> $"],
            "mud.set_prompt_var(\"hp\", captures[2])",
        )
        .unwrap();
        assert!(capture.settle);
    }
}
