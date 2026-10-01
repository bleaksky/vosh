//! What a capture compiles to, for `prompt_compile` (section 6).
//!
//! The report is pure. It says whether Vosh can read the prompt, the ways
//! the game prints it, the values it reads, each code with its label, the
//! warnings with the span of the setting each is about, and the designs
//! the card offers to start from. [`line_report`] reports a capture built
//! from a line another game prints, with each number in it, for
//! `prompt_capture_from_line`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::aabahran::lex::{self, Token as GameToken};
use crate::aabahran::{self, Origin, ShapeKind, Which, Who};
use crate::capture;
use crate::config::{CaptureConfig, RegexCapture};
use crate::generic;
use crate::presets::{self, Preset};

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
    /// The values Vosh reads, each once, the way the game prints the
    /// prompt out of a fight first.
    pub vars: Vec<String>,
    /// Every value code in the settings, in order.
    pub codes: Vec<ReportCode>,
    pub warnings: Vec<ReportWarning>,
    pub presets: Vec<Preset>,
    /// The value each group of a pattern feeds where it differs from the
    /// group's own name, as `[prompt.capture] names` keeps it. Empty for
    /// codes.
    pub names: BTreeMap<String, String>,
    /// Each number of a line you pointed at, with the name it reads into,
    /// for the card to mark. Empty otherwise.
    pub numbers: Vec<generic::Number>,
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
    pub label: String,
    /// `normal`, `tank`, `either`, `afk`, `fallback`, or `line` for a
    /// pattern.
    pub kind: String,
    pub which: Option<Which>,
    /// The pattern for each line, top line first.
    pub lines: Vec<String>,
    /// A partial the last line reads is the prompt at once.
    pub settle: bool,
}

/// One value code in a setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportCode {
    /// As you write it, `%h`.
    pub code: String,
    pub label: String,
    /// The value it fills, None for a code that fills none.
    pub field: Option<String>,
    pub which: Which,
    pub span: [usize; 2],
    /// Vosh reads its value.
    pub read: bool,
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
                    label: shape_label(shape.kind, shape.which).to_string(),
                    kind: shape_kind(shape.kind).to_string(),
                    which: Some(shape.which),
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
            let field = code.name().map(str::to_string);
            let read = field
                .as_deref()
                .is_some_and(|f| vars.iter().any(|v| v == f));
            codes.push(ReportCode {
                code: code.written(),
                label: code.label(),
                field,
                which,
                span: span(&lexed.span),
                read,
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
    CompileReport {
        ok: error.is_none(),
        error,
        prompt: stored_prompt,
        fprompt: stored_fprompt,
        shapes,
        vars,
        codes,
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
    }
}

fn shape_kind(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Normal => "normal",
        ShapeKind::Tank => "tank",
        ShapeKind::Either => "either",
        ShapeKind::Afk => "afk",
        ShapeKind::Fallback => "fallback",
    }
}

fn shape_label(kind: ShapeKind, which: Which) -> &'static str {
    match (kind, which) {
        (ShapeKind::Normal | ShapeKind::Either, Which::Prompt) => "Your prompt",
        (ShapeKind::Tank, Which::Prompt) => "Your prompt while someone in your group tanks",
        (ShapeKind::Normal | ShapeKind::Either, Which::Fight) => "Your fight prompt",
        (ShapeKind::Tank, Which::Fight) => "Your fight prompt while someone in your group tanks",
        (ShapeKind::Afk, _) => "Your prompt while you are away",
        (ShapeKind::Fallback, _) => "The prompt the game draws for an empty setting",
    }
}

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
        vars: Vec::new(),
        codes: Vec::new(),
        warnings: Vec::new(),
        presets: presets::other(supplied),
        names: names.clone(),
        numbers: Vec::new(),
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
            label: "Your prompt".to_string(),
            kind: "line".to_string(),
            which: None,
            lines: vec![pattern.clone()],
            settle: capture.settle,
        }],
        vars: vars.clone(),
        codes: Vec::new(),
        warnings: Vec::new(),
        presets: presets::other(&reads),
        names: names.clone(),
        numbers: Vec::new(),
    }
}

/// The report for a capture built from `line`, the plain text of a line
/// another game prints, with `names` for its numbers in order (see
/// [`generic::from_line`]). Its shape holds the pattern, and its numbers
/// say where each number sits in the line and what it reads into.
pub fn line_report(line: &str, names: &[String], supplied: &dyn Fn(&str) -> bool) -> CompileReport {
    let built = generic::from_line(line, names);
    let mut report = regex_report(&built.capture.lines, &built.capture.names, supplied);
    report.numbers = built.numbers;
    report
}
