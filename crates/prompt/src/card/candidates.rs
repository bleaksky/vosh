//! The candidates ring as the card reads it: `prompt_candidates` groups the
//! entries by their shape with the digits masked, and
//! `prompt_capture_check` counts how a capture matches them and the lines
//! in your scrollback. Each entry it reads carries a [`Mark`] for what each
//! value printed.
//!
//! The ring holds what came right before each of your sends and each GA
//! or EOR, so its entries are your prompts. A line in the scrollback that
//! a capture reads, and that looks like none of the entries it read, is a
//! line the capture would draw over by mistake.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::aabahran::{Compiled, Shape, WarningKind};
use crate::capture::{shapes_in_order, Reader, Recognizer};
use crate::stage::Candidate;

/// `text` with each run of digits, and a minus sign before one, as `#`,
/// so a prompt reads the same whatever your values are. A minus right
/// after a letter or a digit is a dash, not a sign.
pub fn shape_of(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.trim_end().chars().peekable();
    let mut prev: Option<char> = None;
    while let Some(c) = chars.next() {
        let sign = c == '-'
            && !prev.is_some_and(|p| p.is_ascii_alphanumeric())
            && chars.peek().is_some_and(char::is_ascii_digit);
        prev = Some(c);
        let digits = c.is_ascii_digit() || sign;
        if digits {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
            prev = Some('0');
            out.push('#');
        } else {
            out.push(c);
        }
    }
    out
}

/// One ring entry as the card reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateEntry {
    pub id: u64,
    /// As the game sent it, colors included, lines joined by `\r\n`.
    pub raw: String,
    /// Lines joined by `\n`.
    pub plain: String,
    /// Milliseconds since the epoch.
    pub at_ms: i64,
    /// Vosh read it as your prompt.
    pub recognized: bool,
    /// Drawing was on.
    pub draw: bool,
    /// The profile had a capture.
    pub capture: bool,
}

/// Ring entries that share a shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateGroup {
    /// The plain text with its digits masked as `#`.
    pub shape: String,
    pub count: usize,
    /// Vosh read one of them as your prompt.
    pub recognized: bool,
    /// Newest first.
    pub entries: Vec<CandidateEntry>,
}

/// The ring grouped by shape, the group with the most entries first and,
/// between groups of one size, the one seen last.
pub fn groups<'a>(ring: impl Iterator<Item = &'a Candidate>) -> Vec<CandidateGroup> {
    let mut groups: Vec<CandidateGroup> = Vec::new();
    for candidate in ring {
        let entry = CandidateEntry {
            id: candidate.id,
            raw: String::from_utf8_lossy(&candidate.raw).into_owned(),
            plain: candidate.plain.clone(),
            at_ms: candidate.at_ms,
            recognized: candidate.recognized,
            draw: candidate.draw,
            capture: candidate.capture,
        };
        let shape = shape_of(&candidate.plain);
        match groups.iter_mut().find(|g| g.shape == shape) {
            Some(group) => {
                group.count += 1;
                group.recognized |= entry.recognized;
                group.entries.insert(0, entry);
            }
            None => groups.push(CandidateGroup {
                shape,
                count: 1,
                recognized: entry.recognized,
                entries: vec![entry],
            }),
        }
    }
    let latest = |g: &CandidateGroup| g.entries.first().map_or(0, |e| e.id);
    groups.sort_by(|a, b| b.count.cmp(&a.count).then(latest(b).cmp(&latest(a))));
    groups
}

/// How a capture matches the ring and your scrollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureCheck {
    /// Ring entries the capture reads as your prompt.
    pub matched: usize,
    /// Ring entries.
    pub total: usize,
    /// Of those it reads, the ones from a fight.
    pub fight_matched: usize,
    /// Lines in your scrollback it reads that look like none of the
    /// entries it read.
    pub false_matches: usize,
    /// What the card says about it.
    pub text: String,
    /// Each ring entry it reads, newest first, with its values marked,
    /// for the card's candidate box and stepper.
    pub reads: Vec<CheckRead>,
}

/// A ring entry a capture reads, as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckRead {
    pub id: u64,
    /// As the game sent it, colors included, lines joined by `\r\n`.
    pub raw: String,
    /// Lines joined by `\n`.
    pub plain: String,
    /// Milliseconds since the epoch.
    pub at_ms: i64,
    /// The game printed it in a fight.
    pub fight: bool,
    /// What each value printed, in the order the lines print them.
    pub marks: Vec<Mark>,
}

impl CaptureCheck {
    fn new(matched: usize, total: usize, fight_matched: usize, false_matches: usize) -> Self {
        let mut check = Self {
            matched,
            total,
            fight_matched,
            false_matches,
            text: String::new(),
            reads: Vec::new(),
        };
        check.text = check.sentence();
        check
    }

    /// The match line the card and Settings show.
    pub fn sentence(&self) -> String {
        if self.total == 0 {
            return "Vosh has not seen your prompt since you connected. Send a command and Vosh checks again."
                .to_string();
        }
        if self.matched == 0 {
            return if self.total == 1 {
                "Does not match the line before your last command.".to_string()
            } else {
                format!(
                    "Does not match any of the {} lines before your last commands.",
                    self.total
                )
            };
        }
        let prompts = if self.matched == 1 {
            "Matches your last prompt".to_string()
        } else {
            format!("Matches your last {} prompts", self.matched)
        };
        let others = match self.false_matches {
            0 => "and no other line.".to_string(),
            1 => "and 1 other line.".to_string(),
            n => format!("and {n} other lines."),
        };
        let fight = match (self.matched, self.fight_matched) {
            (_, 0) => String::new(),
            (1, _) => " It is from a fight.".to_string(),
            (_, 1) => " 1 of them is from a fight.".to_string(),
            (_, n) => format!(" {n} of them are from a fight."),
        };
        format!("{prompts} {others}{fight}")
    }
}

/// Check `recognizer` against the ring and the plain lines of your
/// scrollback. With no recognizer nothing matches.
pub fn check<'a, 'b>(
    recognizer: Option<&Recognizer>,
    ring: impl Iterator<Item = &'a Candidate>,
    scrollback: impl Iterator<Item = &'b str>,
) -> CaptureCheck {
    let ring: Vec<&Candidate> = ring.collect();
    let total = ring.len();
    let Some(recognizer) = recognizer else {
        return CaptureCheck::new(0, total, 0, 0);
    };
    let mut matched = 0;
    let mut fight = 0;
    let mut shapes: BTreeSet<String> = BTreeSet::new();
    let mut reads: Vec<CheckRead> = Vec::new();
    for candidate in &ring {
        let lines: Vec<&str> = candidate.plain.split('\n').collect();
        let read = recognizer
            .read(&lines)
            .or_else(|| recognizer.read_partial(&lines));
        let Some(read) = read else {
            continue;
        };
        matched += 1;
        let in_fight = read.values.get("fight").is_some_and(|v| v == "1");
        if in_fight {
            fight += 1;
        }
        reads.insert(
            0,
            CheckRead {
                id: candidate.id,
                raw: String::from_utf8_lossy(&candidate.raw).into_owned(),
                plain: candidate.plain.clone(),
                at_ms: candidate.at_ms,
                fight: in_fight,
                marks: recognizer.marks(&lines).unwrap_or_default(),
            },
        );
        shapes.extend(lines.iter().map(|l| shape_of(l)));
    }
    let false_matches = scrollback
        .filter(|line| !line.trim().is_empty())
        .filter(|line| recognizer.line(line).is_some())
        .filter(|line| !shapes.contains(&shape_of(line)))
        .count();
    CaptureCheck {
        reads,
        ..CaptureCheck::new(matched, total, fight, false_matches)
    }
}

/// What one value printed in a prompt a capture read, for the card to
/// mark: its line, top line first, and its characters in that line, by
/// Unicode scalar value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mark {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    /// The value it fills. None for codes that run together, which Vosh
    /// cannot tell apart, so it reads neither.
    pub field: Option<String>,
    /// What the card names it, such as `Max health`, or `Health and
    /// Mana` for codes that run together.
    pub label: String,
    /// Vosh cannot read it: codes that run together.
    pub warn: bool,
}

// Only the candidates view marks what a capture read, so the marking
// lives here rather than in the capture module.
impl Recognizer {
    /// What each value printed in `lines`, a whole prompt or one whose
    /// last line is a partial, for the card to mark, in the order the
    /// lines print them. Codes that run together make one mark Vosh cannot
    /// read. A code Vosh does not read for any other reason, such as the
    /// second use of one, makes none. None when the capture does not read
    /// `lines`.
    pub(crate) fn marks(&self, lines: &[&str]) -> Option<Vec<Mark>> {
        match &self.reader {
            Reader::Regex { line, groups, .. } => {
                let [plain] = lines else {
                    return None;
                };
                let found = line.captures(plain)?;
                let mut out: Vec<Mark> = groups
                    .iter()
                    .filter_map(|(index, var)| {
                        let m = found.get(*index).filter(|m| !m.is_empty())?;
                        Some(Mark {
                            line: 0,
                            start: chars_before(plain, m.start()),
                            end: chars_before(plain, m.end()),
                            field: Some(var.clone()),
                            label: crate::card::sentences::value_label(var),
                            warn: false,
                        })
                    })
                    .collect();
                out.sort_by_key(|m| (m.start, m.end));
                Some(out)
            }
            Reader::Codes(compiled) => {
                let shape = shapes_in_order(compiled).find(|shape| {
                    shape.read(lines).is_some() || shape.read_partial(lines).is_some()
                })?;
                Some(code_marks(compiled, shape, lines))
            }
        }
    }
}

/// The marks of a prompt `shape` read, codes that run together merged
/// into one mark Vosh cannot read.
fn code_marks(compiled: &Compiled, shape: &Shape, lines: &[&str]) -> Vec<Mark> {
    let runs: Vec<&std::ops::Range<usize>> = compiled
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::RunTogether && w.which == shape.which)
        .map(|w| &w.span)
        .collect();
    let mut out: Vec<Mark> = Vec::new();
    // Each merged mark's place in `out`, its run and line, and the labels
    // of the codes in it.
    let mut merged: Vec<(usize, usize, usize, Vec<String>)> = Vec::new();
    for mark in shape.marks(lines).unwrap_or_default() {
        let plain = lines[mark.line];
        let start = chars_before(plain, mark.bytes.start);
        let end = chars_before(plain, mark.bytes.end);
        let Some(code) = &mark.code else {
            // The immortal prefix and the fallback prompt's values.
            let name = mark.name.clone().unwrap_or_default();
            out.push(Mark {
                line: mark.line,
                start,
                end,
                label: crate::card::sentences::value_label(&name),
                field: Some(name),
                warn: false,
            });
            continue;
        };
        if code.read {
            out.push(Mark {
                line: mark.line,
                start,
                end,
                field: mark.name.clone(),
                label: code.code.label(),
                warn: false,
            });
            continue;
        }
        let Some(run) = runs
            .iter()
            .position(|r| r.start <= code.span.start && code.span.end <= r.end)
        else {
            // Vosh reads it elsewhere, or not at all, and marks nothing.
            continue;
        };
        let label = code.code.label();
        match merged
            .iter_mut()
            .find(|(_, r, line, _)| *r == run && *line == mark.line)
        {
            Some((at, _, _, labels)) => {
                let into = &mut out[*at];
                into.start = into.start.min(start);
                into.end = into.end.max(end);
                if !labels.contains(&label) {
                    labels.push(label);
                }
                into.label = crate::card::sentences::and_list(labels);
            }
            None => {
                merged.push((out.len(), run, mark.line, vec![label.clone()]));
                out.push(Mark {
                    line: mark.line,
                    start,
                    end,
                    field: None,
                    label,
                    warn: true,
                });
            }
        }
    }
    out
}

/// How many characters of `text` come before byte `at`.
fn chars_before(text: &str, at: usize) -> usize {
    text.get(..at).map_or(0, |head| head.chars().count())
}
