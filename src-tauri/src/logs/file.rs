//! Save as file in the log view: every line a scope holds, oldest
//! first, as plain text, with the game's colors, or as one HTML page drawn
//! by the scene's renderer. With times on, each line starts with the time
//! it came on the local 24 hour clock, as the log view shows it, and a
//! file that runs over more than one day names each day above its lines.
//! A line forget passwords would blank always goes out blanked.

use std::collections::BTreeSet;
use std::io::Write;

use chrono::NaiveDate;
use serde::Deserialize;
use vosh_log::{LogStore, Scope, ScopedLogSpan};

use super::scene::{self, html, SceneFormat, ScenePalette};

/// How Save as file writes the lines, as the page sends it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileOptions {
    pub(crate) format: SceneFormat,
    /// Start each line with its time.
    #[serde(default)]
    pub(crate) times: bool,
    /// The theme showing as you save, for the HTML page.
    #[serde(default)]
    pub(crate) palette: Option<ScenePalette>,
}

/// The times a file starts its lines with, and the day above each day's
/// lines when the file runs over more than one.
struct Clock {
    /// The lines run over more than one day.
    days: bool,
    /// The day of the line before.
    day: Option<NaiveDate>,
}

impl Clock {
    fn new(spans: &[ScopedLogSpan]) -> Self {
        let first = spans.iter().map(|s| s.first_ms).min();
        let last = spans.iter().map(|s| s.last_ms).max();
        let days = match (first, last) {
            (Some(first), Some(last)) => {
                scene::local(first).date_naive() != scene::local(last).date_naive()
            }
            _ => false,
        };
        Self { days, day: None }
    }

    /// The day to name above a line at `ms`, like `October 3, 2026`, when
    /// it starts a new day of a file that runs over more than one. A day
    /// that turns while the file is written is named too, even in a file
    /// the header said held one day.
    fn day_above(&mut self, ms: i64) -> Option<String> {
        let at = scene::local(ms);
        let day = at.date_naive();
        let before = self.day.replace(day);
        let named = match before {
            None => self.days,
            Some(before) => before != day,
        };
        named.then(|| at.format("%B %-d, %Y").to_string())
    }

    /// The time a line at `ms` starts with, as the log view shows it, an
    /// hour of one digit padded so the lines stay in a column: ` 9:05`.
    fn time(ms: i64) -> String {
        format!("{:>5}", scene::local(ms).format("%-H:%M").to_string())
    }
}

/// Who played, where, as the title of the HTML page: `Orla in The
/// Forsaken Lands`, `Orla and Maren in The Forsaken Lands`, or the world
/// alone when the game named nobody.
fn place(spans: &[ScopedLogSpan]) -> Option<String> {
    let mut worlds: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for span in spans {
        let world = crate::profile::worlds::world_label(&span.log.host, span.log.port);
        if !worlds.contains(&world) {
            worlds.push(world);
        }
        if let Some(name) = span.log.character.as_deref().map(str::trim) {
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
    if worlds.is_empty() {
        return None;
    }
    let worlds = and_list(&worlds);
    Some(if names.is_empty() {
        worlds
    } else {
        format!("{} in {worlds}", and_list(&names))
    })
}

/// `a`, `a and b`, `a, b, and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Write every line in `scope` to `out` as `options` asks, a file named
/// `name` before its extension. A line forget passwords would blank goes
/// out blanked. Returns how many lines it wrote.
pub(crate) fn write(
    store: &LogStore,
    scope: &Scope,
    options: &FileOptions,
    name: &str,
    out: &mut dyn Write,
) -> vosh_log::Result<u64> {
    let spans = if options.times || options.format == SceneFormat::Html {
        store.scope_spans(scope)?
    } else {
        Vec::new()
    };
    let mut clock = Clock::new(&spans);
    let with_ansi = options.format != SceneFormat::Text;
    if options.format != SceneFormat::Html {
        let times = options.times;
        let mut first = true;
        return store.export_lines(scope, with_ansi, true, &mut |ms, line| {
            if times {
                if let Some(day) = clock.day_above(ms) {
                    if !first {
                        out.write_all(b"\n")?;
                    }
                    writeln!(out, "{day}")?;
                }
                write!(out, "{} ", Clock::time(ms))?;
            }
            first = false;
            out.write_all(line)?;
            out.write_all(b"\n")
        });
    }

    let title = place(&spans).unwrap_or_else(|| name.to_string());
    let first_ms = spans.iter().map(|s| s.first_ms).min();
    let last_ms = spans.iter().map(|s| s.last_ms).max();
    let meta = match first_ms.zip(last_ms) {
        Some((first, last)) => scene::when(first, last),
        None => "Nothing saved here yet.".to_string(),
    };
    let header = html::Header {
        name,
        title: &title,
        meta: &meta,
        footer: "Saved from Vosh.",
    };
    let palette = options.palette.clone().unwrap_or_default();
    // The style block goes out before the lines are read, so it holds
    // every rule a line could need.
    out.write_all(html::head(&header, &palette, &html::every_class()).as_bytes())?;
    let mut used = BTreeSet::new();
    let mut body = String::new();
    let mut first = true;
    let written = store.export_lines(scope, true, true, &mut |ms, line| {
        body.clear();
        if !first {
            body.push('\n');
        }
        if options.times {
            if let Some(day) = clock.day_above(ms) {
                if !first {
                    body.push('\n');
                }
                html::push_quiet(&mut body, &day, &mut used);
                body.push('\n');
            }
            html::push_quiet(&mut body, &Clock::time(ms), &mut used);
            body.push(' ');
        }
        first = false;
        html::push_line(&mut body, line, &mut used);
        out.write_all(body.as_bytes())
    })?;
    out.write_all(html::tail(&header).as_bytes())?;
    Ok(written)
}

#[cfg(test)]
mod tests;
