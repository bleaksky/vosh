//! Save a scene: a stretch of one log as a file to share, with your
//! prompt, your commands, the lines outside play and the channels you pick
//! left out (boards 5 and 6 of the Alerts and Scenes review, Q9 to Q12).
//!
//! The rows come from the log with the kind the session gave each, and
//! [`older`] reads the kind of a row an older build wrote from its text.
//! [`choose`] says which rows stay and why each other one goes, the same
//! for the preview and the file. The file is the plain text, the bytes the
//! game sent with their colors, or one HTML page ([`html`]), named after
//! the first room in the range and the day, and it goes to Downloads, as
//! every file Vosh writes for you does (Q12).

mod html;
mod older;

use std::path::Path;

use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use vosh_log::{LineKind, LogStore, SceneLine, SceneLog};
use vosh_prompt::capture::Recognizer;

pub(crate) use html::ScenePalette;

/// The most rows the preview draws. The file keeps every row.
pub(crate) const PREVIEW_CAP: usize = 5_000;

/// The stretch of play a scene takes: one log, from one time to another,
/// both ends kept, and from and to a line when you clicked one.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SceneRange {
    pub(crate) log: i64,
    pub(crate) from_ms: i64,
    pub(crate) to_ms: i64,
    #[serde(default)]
    pub(crate) from_id: Option<i64>,
    #[serde(default)]
    pub(crate) to_id: Option<i64>,
}

/// What a scene leaves out. Lines outside play always stay out.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SceneFilter {
    /// Keep your prompt.
    pub(crate) prompts: bool,
    /// Keep the lines you sent.
    pub(crate) commands: bool,
    /// The channels left out, by the name Comm.Channel gives them.
    pub(crate) left_out: Vec<String>,
}

/// The kinds of file a scene saves as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SceneFormat {
    Text,
    Ansi,
    Html,
}

impl SceneFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Ansi => "log",
            Self::Html => "html",
        }
    }
}

/// One row of the preview, and why the scene leaves it out, if it does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PreviewLine {
    pub(crate) id: i64,
    pub(crate) ts_ms: i64,
    pub(crate) text: String,
    pub(crate) raw: Option<Vec<u8>>,
    /// None for a row the scene keeps. Otherwise the word the preview
    /// shows beside it, like `prompt` or `tell`, or empty for a blank
    /// line that folds into the one before it.
    pub(crate) out: Option<String>,
}

/// What the page shows of a scene before you save it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ScenePreview {
    /// The rows in the range, at most [`PREVIEW_CAP`].
    pub(crate) lines: Vec<PreviewLine>,
    /// How many rows the range holds, and how many the scene keeps.
    pub(crate) total: usize,
    pub(crate) kept: usize,
    /// The range holds more rows than the preview draws.
    pub(crate) capped: bool,
    /// Some rows came from a build that did not tag them, so their kind
    /// was read from their text.
    pub(crate) older: bool,
    /// The file's name before Downloads adds a number to a name it holds.
    pub(crate) file_name: String,
}

/// The rows of a range, oldest first, with the log they belong to and
/// whether the range starts at the log's first row.
pub(crate) struct Span {
    log: SceneLog,
    lines: Vec<SceneLine>,
    starts_log: bool,
    /// The range held more rows than were read.
    capped: bool,
}

/// Read the rows of `range`, at most `cap` of them.
pub(crate) fn read(store: &LogStore, range: &SceneRange, cap: usize) -> Result<Span, String> {
    let log = store
        .scene_log(range.log)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "That log is gone. Pick another one.".to_string())?;
    let first = store
        .scene_lines(range.log, i64::MIN, i64::MAX, 1)
        .map_err(|e| e.to_string())?
        .first()
        .map(|line| line.id);
    let mut lines = store
        .scene_lines(range.log, range.from_ms, range.to_ms, cap.saturating_add(1))
        .map_err(|e| e.to_string())?;
    lines.retain(|line| {
        range.from_id.map_or(true, |from| line.id >= from)
            && range.to_id.map_or(true, |to| line.id <= to)
    });
    let capped = lines.len() > cap;
    lines.truncate(cap);
    let starts_log = first.is_some() && lines.first().map(|l| l.id) == first;
    Ok(Span {
        log,
        lines,
        starts_log,
        capped,
    })
}

/// Why `kind` leaves its row out under `filter`, or None when it stays.
fn reason(kind: &LineKind, filter: &SceneFilter) -> Option<String> {
    match kind {
        LineKind::Text => None,
        LineKind::Login => Some("outside play".to_string()),
        LineKind::Prompt => (!filter.prompts).then(|| "prompt".to_string()),
        LineKind::Sent => (!filter.commands).then(|| "your command".to_string()),
        LineKind::Channel(name) => filter.left_out.contains(name).then(|| name.clone()),
    }
}

/// Why each row leaves the scene, or None for each row it keeps. A blank
/// line right after another the scene keeps, with only rows it leaves out
/// between them, folds into the one before it, so a prompt left out never
/// leaves two blank lines behind. A blank line that would open the scene
/// folds too.
pub(crate) fn choose(
    lines: &[SceneLine],
    kinds: &[LineKind],
    filter: &SceneFilter,
) -> Vec<Option<String>> {
    let mut last_blank = true;
    lines
        .iter()
        .zip(kinds)
        .map(|(line, kind)| {
            if let Some(why) = reason(kind, filter) {
                return Some(why);
            }
            let blank = line.text.trim().is_empty();
            if blank && last_blank {
                return Some(String::new());
            }
            last_blank = blank;
            None
        })
        .collect()
}

/// The kind of each row of `span`, read from the text for older rows.
fn kinds(span: &Span, prompt: Option<&Recognizer>) -> Vec<LineKind> {
    older::kinds(&span.lines, span.starts_log, prompt)
}

/// True when the room's tint opens `raw`, a 256 color before the room's
/// own color code, which `do_look` writes before a room's name and no
/// other line.
fn is_room_name(raw: &[u8]) -> bool {
    let Some(rest) = raw.strip_prefix(b"\x1b[38;5;") else {
        return false;
    };
    let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    digits > 0 && rest[digits..].starts_with(b"m\x1b[")
}

/// The scene's title, the first room name in the range, or a plain one
/// when the range shows no room.
fn title(lines: &[SceneLine]) -> String {
    lines
        .iter()
        .find(|line| line.raw.as_deref().is_some_and(is_room_name) && !line.text.trim().is_empty())
        .map_or_else(
            || "Vosh scene".to_string(),
            |line| line.text.trim().to_string(),
        )
}

/// A time on the local clock.
fn local(ms: i64) -> chrono::DateTime<Local> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
}

/// The day a scene starts, `October 3`.
fn day(ms: i64) -> String {
    local(ms).format("%B %-d").to_string()
}

/// The file's name before its extension, the title and the day, like
/// `Thickening Woods, October 3`. A slash or a colon in a room name, which
/// a file name cannot hold, reads as a dash.
fn file_stem(lines: &[SceneLine]) -> String {
    let at = lines
        .first()
        .map_or_else(|| Local::now().timestamp_millis(), |l| l.ts_ms);
    let name: String = title(lines)
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    format!("{name}, {}", day(at))
}

/// The line under the title: who, where and when, like `Orla in The
/// Forsaken Lands, October 3, 2026, from 21:14 to 21:15`.
fn meta(log: &SceneLog, lines: &[SceneLine]) -> String {
    let world = crate::profile::worlds::world_label(&log.host, log.port);
    let place = match &log.character {
        Some(name) => format!("{name} in {world}"),
        None => world,
    };
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return place;
    };
    let (from, to) = (local(first.ts_ms), local(last.ts_ms));
    let date = |at: &chrono::DateTime<Local>| at.format("%B %-d, %Y").to_string();
    let time = |at: &chrono::DateTime<Local>| at.format("%-H:%M").to_string();
    if from.date_naive() == to.date_naive() {
        format!(
            "{place}, {}, from {} to {}",
            date(&from),
            time(&from),
            time(&to)
        )
    } else {
        format!(
            "{place}, from {}, {} to {}, {}",
            date(&from),
            time(&from),
            date(&to),
            time(&to)
        )
    }
}

/// The line under the scene, what it leaves out, so whoever reads it
/// knows the scene is cut, like `Saved from Vosh. Prompts, your commands
/// and five channels were left out.`
fn footer(filter: &SceneFilter) -> String {
    const COUNTS: [&str; 12] = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven",
    ];
    let mut parts = Vec::new();
    if !filter.prompts {
        parts.push("prompts".to_string());
    }
    if !filter.commands {
        parts.push("your commands".to_string());
    }
    match filter.left_out.as_slice() {
        [] => {}
        [one] => parts.push(format!("the {one} channel")),
        many => parts.push(format!(
            "{} channels",
            COUNTS
                .get(many.len())
                .map_or_else(|| many.len().to_string(), |w| (*w).to_string())
        )),
    }
    let Some((last, rest)) = parts.split_last() else {
        return "Saved from Vosh.".to_string();
    };
    let mut list = if rest.is_empty() {
        last.clone()
    } else {
        format!("{} and {last}", rest.join(", "))
    };
    list[..1].make_ascii_uppercase();
    let verb = if parts.len() == 1 && filter.left_out.len() == 1 {
        "was"
    } else {
        "were"
    };
    format!("Saved from Vosh. {list} {verb} left out.")
}

/// The preview of `span` under `filter`.
pub(crate) fn preview(
    span: Span,
    filter: &SceneFilter,
    format: SceneFormat,
    prompt: Option<&Recognizer>,
) -> ScenePreview {
    let kinds = kinds(&span, prompt);
    let out = choose(&span.lines, &kinds, filter);
    let older = span.lines.iter().any(|line| line.kind.is_none());
    let kept = out.iter().filter(|why| why.is_none()).count();
    let file_name = format!("{}.{}", file_stem(&span.lines), format.extension());
    let total = span.lines.len();
    let lines = span
        .lines
        .into_iter()
        .zip(out)
        .map(|(line, out)| PreviewLine {
            id: line.id,
            ts_ms: line.ts_ms,
            text: line.text,
            raw: line.raw,
            out,
        })
        .collect();
    ScenePreview {
        lines,
        total,
        kept,
        capped: span.capped,
        older,
        file_name,
    }
}

/// The bytes of the file for `span` under `filter`.
fn render(
    span: &Span,
    filter: &SceneFilter,
    format: SceneFormat,
    palette: Option<&ScenePalette>,
    prompt: Option<&Recognizer>,
) -> Vec<u8> {
    let kinds = kinds(span, prompt);
    let out = choose(&span.lines, &kinds, filter);
    let kept: Vec<&SceneLine> = span
        .lines
        .iter()
        .zip(&out)
        .filter(|(_, why)| why.is_none())
        .map(|(line, _)| line)
        .collect();
    let bytes = |line: &SceneLine| match &line.raw {
        Some(raw) => raw.clone(),
        None => line.text.as_bytes().to_vec(),
    };
    match format {
        SceneFormat::Text => kept
            .iter()
            .flat_map(|line| [line.text.as_bytes(), b"\n"].concat())
            .collect(),
        SceneFormat::Ansi => kept
            .iter()
            .flat_map(|line| [bytes(line), b"\n".to_vec()].concat())
            .collect(),
        SceneFormat::Html => {
            let lines: Vec<Vec<u8>> = kept.iter().map(|line| bytes(line)).collect();
            let title = title(&span.lines);
            let meta = meta(&span.log, &span.lines);
            let footer = footer(filter);
            let header = html::Header {
                title: &title,
                meta: &meta,
                footer: &footer,
            };
            html::render(&lines, &header, &palette.cloned().unwrap_or_default()).into_bytes()
        }
    }
}

/// Write the scene of `span` to a new file in `dir`, named after its first
/// room and its day, with ` (2)` and on when that name is taken. Returns
/// the file's name.
pub(crate) fn save(
    span: &Span,
    filter: &SceneFilter,
    format: SceneFormat,
    palette: Option<&ScenePalette>,
    prompt: Option<&Recognizer>,
    dir: &Path,
) -> Result<String, String> {
    let path = crate::disk::paths::export_path(dir, &file_stem(&span.lines), format.extension());
    let bytes = render(span, filter, format, palette, prompt);
    let could_not = |e: &std::io::Error| {
        tracing::warn!(path = %path.display(), error = %e, "could not save a scene");
        "Vosh could not save the scene in your Downloads folder.".to_string()
    };
    // Made new, so a file that came since the name was picked stays.
    let mut file = std::fs::File::create_new(&path).map_err(|e| could_not(&e))?;
    if let Err(e) = std::io::Write::write_all(&mut file, &bytes) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(could_not(&e));
    }
    Ok(path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests;
