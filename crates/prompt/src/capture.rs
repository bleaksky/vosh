//! Patterns you point at or write, and the capture triggers older builds
//! used, as a `kind = "regex"` capture.
//!
//! [`settle`] decides whether a partial line the last pattern matches is
//! the prompt at once. [`from_trigger`] reads a capture trigger, the kind
//! `#prompt {regex}` wrote, into a capture the profile holds.
//! [`Recognizer`] is a capture compiled for the stage, which reads a line
//! or a partial as your prompt.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use regex_syntax::hir::{HirKind, Look};

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

/// A capture compiled for the stage. It reads a line, or a partial the
/// game has not ended yet, as your prompt, and hands each group to the
/// variable it feeds.
///
/// This build reads one line prompts from a `kind = "regex"` capture.
/// Prompts that span lines and Aabahran's codes compile in a later build,
/// and until then those captures recognize nothing, so the game's prompt
/// shows as sent.
#[derive(Debug, Clone)]
pub struct Recognizer {
    line: Regex,
    /// Group number and the variable it feeds, in group order.
    groups: Vec<(usize, String)>,
    /// A partial the pattern matches is the prompt at once.
    settle: bool,
}

/// A prompt the recognizer read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recognized {
    /// Each group the shape has, by variable. A group that printed
    /// nothing reads as an empty string.
    pub values: BTreeMap<String, String>,
}

impl Recognizer {
    /// Compile a capture. None when it recognizes nothing in this build:
    /// no capture, Aabahran's codes, a regex capture with no line or more
    /// than one, or a pattern that does not compile.
    pub fn compile(capture: &CaptureConfig) -> Option<Self> {
        let CaptureConfig::Regex(capture) = capture else {
            return None;
        };
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
        let settle = capture.settle && settle(pattern);
        Some(Self {
            line,
            groups,
            settle,
        })
    }

    /// True when a partial this recognizer reads is the prompt at once.
    pub fn settles(&self) -> bool {
        self.settle
    }

    /// Read a complete line, or a partial a GA or EOR ended, as your
    /// prompt.
    pub fn line(&self, plain: &str) -> Option<Recognized> {
        let found = self.line.captures(plain)?;
        let mut values = BTreeMap::new();
        for (index, var) in &self.groups {
            let value = found.get(*index).map_or("", |m| m.as_str());
            let known = values.get(var).is_some_and(|v: &String| !v.is_empty());
            if !known {
                values.insert(var.clone(), value.to_string());
            }
        }
        Some(Recognized { values })
    }

    /// Read a partial the game has not ended as your prompt, which only a
    /// capture that settles does. A partial split before its last
    /// character fails the anchored match and waits for the next read.
    pub fn partial(&self, plain: &str) -> Option<Recognized> {
        if !self.settle {
            return None;
        }
        self.line(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AabahranCapture;

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
    fn only_a_one_line_regex_capture_recognizes_in_this_build() {
        assert!(Recognizer::compile(&CaptureConfig::None).is_none());
        assert!(
            Recognizer::compile(&CaptureConfig::Aabahran(AabahranCapture::default())).is_none(),
            "the codes compile in a later build"
        );
        assert!(Recognizer::compile(&regex(&[], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&["^a$", "^b$"], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&[r"\[(?<hp>\d+"], false, &[])).is_none());
        assert!(Recognizer::compile(&regex(&[""], false, &[])).is_none());
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
