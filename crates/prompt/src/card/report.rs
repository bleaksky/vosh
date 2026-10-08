//! What a capture compiles to, for `prompt_compile`.
//!
//! The report is pure. It says whether Vosh can read the prompt, the ways
//! the game prints it, the card's code legend, the warnings with the span
//! of the setting each is about, and the designs the card offers to start
//! from. [`line_report`] reports a capture built from a line another game
//! prints, with each number in it, for `prompt_capture_from_line`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::aabahran::lex::{self, Token as GameToken};
use crate::aabahran::{self, Origin, WarningKind, Which, Who};
use crate::capture::{self, generic};
use crate::card::presets::{self, Preset};
use crate::card::sentences;
use crate::config::{CaptureConfig, RegexCapture};

/// What `prompt_compile` reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CompileRequest {
    /// Aabahran's PROMPT and fight prompt settings. `typed` says you
    /// typed or pasted them as you type them in the game, so Vosh stores
    /// them as the game would first. Without it they are as the game
    /// stores them, as Char.Prompt sends them.
    Aabahran {
        prompt: String,
        #[serde(default)]
        fprompt: String,
        #[serde(default)]
        typed: bool,
    },
    /// Patterns you pointed at, one line.
    Regex {
        lines: Vec<String>,
        #[serde(default)]
        names: BTreeMap<String, String>,
    },
}

/// `prompt_compile`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompileReport {
    /// Vosh can read the prompt.
    pub ok: bool,
    /// Why it cannot, when it cannot.
    pub error: Option<ReportError>,
    /// The PROMPT setting as the game stores it, what Use these codes
    /// saves. Empty for a pattern.
    pub prompt: String,
    /// The fight prompt setting, the same way.
    pub fprompt: String,
    /// The ways the game prints the prompt, each with its patterns.
    pub shapes: Vec<ReportShape>,
    pub warnings: Vec<ReportWarning>,
    pub presets: Vec<Preset>,
    /// The value each group of a pattern feeds where it differs from the
    /// group's own name, as `[prompt.capture] names` keeps it. Empty for
    /// codes.
    pub names: BTreeMap<String, String>,
    /// Each number of a line you pointed at, with the name it reads into,
    /// for the card to mark. Empty otherwise.
    pub numbers: Vec<generic::Number>,
    /// The card's code legend: every code and line end in the order the
    /// settings print them, codes that run together as one row, and a
    /// row for each other warning. Empty for a pattern.
    pub legend: Vec<LegendRow>,
    /// What the card says the prompt shows, `It shows Health, Mana, and
    /// Moves.`, or None when codes run together or it reads no value.
    pub shows: Option<String>,
    /// While codes run together, what Vosh still reads and which values
    /// the game supplies until you fix the prompt.
    pub fix_note: Option<String>,
    /// While codes run together, the command that sets each setting with a
    /// space between them, for the card to show with Copy. Vosh never
    /// sends it.
    pub fixes: Vec<String>,
    /// For a line another game prints, the values its GMCP sends that
    /// Vosh has no name for, which the card offers as names.
    pub gmcp_names: Vec<GmcpName>,
}

/// A value a game sends over GMCP that Vosh has no name for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GmcpName {
    pub name: String,
    /// The package it comes in, such as `Char.Vitals`.
    pub package: String,
}

/// One row of the card's code legend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegendRow {
    /// As you write it, `%h`, or the run of codes that run together,
    /// `%h%m`. Empty for a warning about the whole setting, such as the
    /// cut at 255 characters.
    pub code: String,
    pub label: String,
    pub which: Which,
    /// Bytes of the setting as the game stores it.
    pub span: [usize; 2],
    /// It prints only while someone in your group tanks your opponent.
    pub fight: bool,
    /// A few words after the label, such as `run together`.
    pub tag: Option<String>,
    /// Vosh cannot read it as written, so the code carries the warn ring.
    pub warn: bool,
    /// The warning's sentence, which the card shows under the row. Codes
    /// that run together leave it out, since the card says it where it
    /// says how the prompt matches.
    pub warning: Option<String>,
}

/// Why a capture does not compile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportError {
    pub message: String,
    /// `prompt` or `fprompt` for Aabahran's settings, None for a pattern.
    pub which: Option<Which>,
    /// Byte range in that setting, as the game stores it.
    pub span: Option<[usize; 2]>,
}

/// One way the game prints the prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportShape {
    /// The pattern for each line, top line first.
    pub lines: Vec<String>,
    /// A partial the last line reads is the prompt at once.
    pub settle: bool,
}

/// One value code in a setting, for the legend to say which codes that
/// run together Vosh still reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReportCode {
    label: String,
    /// The value it fills, None for a code that fills none.
    field: Option<String>,
    which: Which,
    span: [usize; 2],
}

/// A warning with its sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportWarning {
    pub kind: aabahran::WarningKind,
    pub which: Which,
    pub span: [usize; 2],
    pub message: String,
}

/// The report for a capture. `who` decides what `%u` and `%s` print.
/// `supplied` says which values GMCP gives, for another game's presets.
pub fn report(
    request: &CompileRequest,
    who: Who,
    supplied: &dyn Fn(&str) -> bool,
) -> CompileReport {
    match request {
        CompileRequest::Aabahran {
            prompt,
            fprompt,
            typed,
        } => codes_report(prompt, fprompt, *typed, who),
        CompileRequest::Regex { lines, names } => regex_report(lines, names, supplied),
    }
}

fn span(range: &std::ops::Range<usize>) -> [usize; 2] {
    [range.start, range.end]
}

fn codes_report(prompt: &str, fprompt: &str, typed: bool, who: Who) -> CompileReport {
    let (stored_prompt, stored_fprompt, mut warnings) = if typed {
        let p = lex::normalize(prompt, Which::Prompt, who);
        let f = lex::normalize(fprompt, Which::Fight, who);
        let mut warnings = p.warnings;
        warnings.extend(f.warnings);
        (p.text, f.text, warnings)
    } else {
        (prompt.to_string(), fprompt.to_string(), Vec::new())
    };
    let compiled = aabahran::compile(&stored_prompt, &stored_fprompt, Origin::Stored, who);
    let (error, shapes, vars) = match &compiled {
        Ok(compiled) => {
            warnings.extend(compiled.warnings.iter().cloned());
            let mut vars: Vec<String> = Vec::new();
            for which in [Which::Prompt, Which::Fight] {
                for name in compiled.reads(which) {
                    if !vars.iter().any(|v| v == name) {
                        vars.push(name.to_string());
                    }
                }
            }
            let shapes = compiled
                .shapes
                .iter()
                .map(|shape| ReportShape {
                    lines: shape
                        .lines
                        .iter()
                        .map(|l| l.line.as_str().to_string())
                        .collect(),
                    settle: shape.settle,
                })
                .collect();
            (None, shapes, vars)
        }
        Err(e) => (
            Some(ReportError {
                message: e.text.clone(),
                which: Some(e.which),
                span: Some(span(&e.span)),
            }),
            Vec::new(),
            Vec::new(),
        ),
    };
    let mut codes = Vec::new();
    for (which, setting) in [
        (Which::Prompt, &stored_prompt),
        (Which::Fight, &stored_fprompt),
    ] {
        for lexed in lex::pass_one(setting, which).tokens {
            let GameToken::Code(code) = lexed.token else {
                continue;
            };
            codes.push(ReportCode {
                label: code.label(),
                field: code.name().map(str::to_string),
                which,
                span: span(&lexed.span),
            });
        }
    }
    warnings.sort_by(|a, b| {
        (a.which, a.span.start, a.span.end).cmp(&(b.which, b.span.start, b.span.end))
    });
    warnings.dedup();
    let game = compiled
        .is_ok()
        .then(|| presets::same_as_the_game(&stored_prompt, &stored_fprompt, who))
        .flatten();
    let notes = legend_notes(&stored_prompt, &stored_fprompt, &warnings, &codes, &vars);
    CompileReport {
        ok: error.is_none(),
        error,
        prompt: stored_prompt,
        fprompt: stored_fprompt,
        shapes,
        warnings: warnings
            .into_iter()
            .map(|w| ReportWarning {
                kind: w.kind,
                which: w.which,
                span: span(&w.span),
                message: w.text,
            })
            .collect(),
        presets: presets::aabahran(game),
        names: BTreeMap::new(),
        numbers: Vec::new(),
        legend: notes.legend,
        shows: notes.shows,
        fix_note: notes.fix_note,
        fixes: notes.fixes,
        gmcp_names: Vec::new(),
    }
}

/// The legend and the sentences the card shows for two settings.
struct Notes {
    legend: Vec<LegendRow>,
    shows: Option<String>,
    fix_note: Option<String>,
    fixes: Vec<String>,
}

fn legend_notes(
    prompt: &str,
    fprompt: &str,
    warnings: &[aabahran::Warning],
    codes: &[ReportCode],
    vars: &[String],
) -> Notes {
    let mut legend: Vec<LegendRow> = Vec::new();
    let mut fixes = Vec::new();
    // The labels of codes that run together, each once.
    let mut unread: Vec<String> = Vec::new();
    for (which, setting) in [(Which::Prompt, prompt), (Which::Fight, fprompt)] {
        if setting.is_empty() {
            continue;
        }
        let own: Vec<&aabahran::Warning> = warnings.iter().filter(|w| w.which == which).collect();
        let (rows, boundaries) = setting_legend(setting, which, &own);
        for row in rows
            .iter()
            .filter(|r| r.tag.as_deref() == Some(RUN_TOGETHER))
        {
            for code in codes
                .iter()
                .filter(|c| c.which == which && within(c.span, row.span))
            {
                let read = code
                    .field
                    .as_deref()
                    .is_some_and(|f| vars.iter().any(|v| v == f));
                if !read && !unread.contains(&code.label) {
                    unread.push(code.label.clone());
                }
            }
        }
        if !boundaries.is_empty() {
            let mut fixed = setting.to_string();
            for at in boundaries.iter().rev() {
                fixed.insert(*at, ' ');
            }
            let command = match which {
                Which::Prompt => "prompt",
                Which::Fight => "fprompt",
            };
            fixes.push(format!("{command} {}", fixed.trim_end_matches(' ')));
        }
        for row in rows {
            // A fight prompt's row the prompt already lists adds nothing.
            let listed = row.which == Which::Fight
                && row.warning.is_none()
                && legend
                    .iter()
                    .any(|r| r.code == row.code && r.label == row.label);
            if !listed {
                legend.push(row);
            }
        }
    }
    let names: Vec<&str> = vars.iter().map(String::as_str).collect();
    let run_together = !fixes.is_empty();
    Notes {
        shows: (!run_together)
            .then(|| sentences::shows_sentence(&names))
            .flatten(),
        fix_note: run_together.then(|| sentences::fix_sentence(&names, &unread)),
        legend,
        fixes,
    }
}

const RUN_TOGETHER: &str = "run together";

fn within(inner: [usize; 2], outer: [usize; 2]) -> bool {
    outer[0] <= inner[0] && inner[1] <= outer[1]
}

/// The legend rows of one setting, and where a space would part each run
/// of codes that run together, in bytes of the setting.
fn setting_legend(
    setting: &str,
    which: Which,
    warnings: &[&aabahran::Warning],
) -> (Vec<LegendRow>, Vec<usize>) {
    let row = |code: String, label: String, span: [usize; 2], fight: bool| LegendRow {
        code,
        label,
        which,
        span,
        fight,
        tag: None,
        warn: false,
        warning: None,
    };
    let mut rows: Vec<LegendRow> = Vec::new();
    for lexed in lex::pass_one(setting, which).tokens {
        let at = span(&lexed.span);
        match lexed.token {
            GameToken::Code(code) => {
                // The legend names the bar itself, as the game prints it.
                let label = match code {
                    aabahran::codes::Code::TankBar => "Tank health bar".to_string(),
                    _ => code.label(),
                };
                rows.push(row(code.written(), label, at, code.is_tank()));
            }
            GameToken::Break => rows.push(row("%c".into(), "New line".into(), at, false)),
            GameToken::TankBreak => rows.push(row("%C".into(), "New line".into(), at, true)),
            GameToken::Lit(_) => {}
        }
    }
    let mut boundaries = Vec::new();
    let mut extra: Vec<LegendRow> = Vec::new();
    for warning in warnings {
        let at = span(&warning.span);
        match warning.kind {
            WarningKind::RunTogether => {
                let inside: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| within(r.span, at))
                    .map(|(i, _)| i)
                    .collect();
                let Some(&first) = inside.first() else {
                    continue;
                };
                // The same pair can come from two shapes.
                if rows[first].tag.as_deref() == Some(RUN_TOGETHER) {
                    continue;
                }
                boundaries.extend(inside.iter().skip(1).map(|&i| rows[i].span[0]));
                let labels: Vec<String> = inside.iter().map(|&i| rows[i].label.clone()).collect();
                let fight = inside.iter().any(|&i| rows[i].fight);
                let merged = LegendRow {
                    code: setting.get(at[0]..at[1]).unwrap_or_default().to_string(),
                    label: sentences::and_list(&labels),
                    span: at,
                    fight,
                    tag: Some(RUN_TOGETHER.into()),
                    warn: true,
                    ..row(String::new(), String::new(), at, false)
                };
                for &i in inside.iter().rev() {
                    rows.remove(i);
                }
                rows.insert(first, merged);
            }
            WarningKind::Twice | WarningKind::PacifyMortal | WarningKind::LangMobile => {
                let tag = match warning.kind {
                    WarningKind::Twice => "second use",
                    WarningKind::PacifyMortal => "immortal",
                    _ => "while you control a mobile",
                };
                if let Some(found) = rows.iter_mut().find(|r| r.span == at) {
                    found.tag = Some(tag.into());
                    found.warn = true;
                    found.warning = Some(warning.text.clone());
                }
            }
            WarningKind::LonePercent => extra.push(LegendRow {
                tag: Some("at the end".into()),
                warn: true,
                warning: Some(warning.text.clone()),
                ..row("%".into(), "A lone %".into(), at, false)
            }),
            WarningKind::Short => {
                if !extra.iter().any(|r| r.label == SHORT) {
                    extra.push(LegendRow {
                        warn: true,
                        warning: Some(warning.text.clone()),
                        ..row(setting.to_string(), SHORT.into(), at, false)
                    });
                }
            }
            WarningKind::Cut => extra.push(LegendRow {
                warn: true,
                warning: Some(warning.text.clone()),
                ..row(String::new(), "The first 255 characters".into(), at, false)
            }),
        }
    }
    rows.extend(extra);
    rows.sort_by_key(|r| r.span[0]);
    (rows, boundaries)
}

const SHORT: &str = "A prompt with no values";

/// The one pattern a regex capture reads your prompt with, or why Vosh
/// cannot read with it.
fn one_pattern(lines: &[String]) -> Result<&String, String> {
    let [pattern] = lines else {
        return Err("Vosh reads your prompt from one line.".to_string());
    };
    if pattern.is_empty() || regex::Regex::new(pattern).is_err() {
        return Err("Vosh cannot read that pattern.".to_string());
    }
    Ok(pattern)
}

/// Check a capture a `[prompt]` table holds before it is saved: the
/// codes compile for `who`, or the pattern reads. The error is the
/// sentence the card shows.
pub fn check_capture(capture: &CaptureConfig, who: Who) -> Result<(), String> {
    match capture {
        CaptureConfig::None => Ok(()),
        CaptureConfig::Aabahran(codes) => {
            aabahran::compile(&codes.prompt, &codes.fprompt, Origin::Stored, who)
                .map(|_| ())
                .map_err(|e| e.text)
        }
        CaptureConfig::Regex(capture) => one_pattern(&capture.lines).map(|_| ()),
    }
}

fn regex_report(
    lines: &[String],
    names: &BTreeMap<String, String>,
    supplied: &dyn Fn(&str) -> bool,
) -> CompileReport {
    let fail = |message: &str| CompileReport {
        ok: false,
        error: Some(ReportError {
            message: message.to_string(),
            which: None,
            span: None,
        }),
        prompt: String::new(),
        fprompt: String::new(),
        shapes: Vec::new(),
        warnings: Vec::new(),
        presets: presets::other(supplied),
        names: names.clone(),
        numbers: Vec::new(),
        legend: Vec::new(),
        shows: None,
        fix_note: None,
        fixes: Vec::new(),
        gmcp_names: Vec::new(),
    };
    let pattern = match one_pattern(lines) {
        Ok(pattern) => pattern,
        Err(message) => return fail(&message),
    };
    let capture = RegexCapture {
        lines: vec![pattern.clone()],
        settle: capture::settle(pattern),
        names: names.clone(),
        ..RegexCapture::default()
    };
    let vars = capture::fills(&capture);
    let reads = |name: &str| vars.iter().any(|v| v == name) || supplied(name);
    CompileReport {
        ok: true,
        error: None,
        prompt: String::new(),
        fprompt: String::new(),
        shapes: vec![ReportShape {
            lines: vec![pattern.clone()],
            settle: capture.settle,
        }],
        warnings: Vec::new(),
        presets: presets::other(&reads),
        names: names.clone(),
        numbers: Vec::new(),
        legend: Vec::new(),
        shows: None,
        fix_note: None,
        fixes: Vec::new(),
        gmcp_names: Vec::new(),
    }
}

/// The report for a capture built from `line`, the plain text of a line
/// another game prints, with `names` for its numbers in order (see
/// [`generic::from_line`]). Its shape holds the pattern and whether it
/// settles, as the line says, and its numbers say where each number sits
/// in the line and what it reads into.
pub fn line_report(line: &str, names: &[String], supplied: &dyn Fn(&str) -> bool) -> CompileReport {
    let built = generic::from_line(line, names);
    let mut report = regex_report(&built.capture.lines, &built.capture.names, supplied);
    for shape in &mut report.shapes {
        shape.settle = built.capture.settle;
    }
    report.numbers = built.numbers;
    report
}
