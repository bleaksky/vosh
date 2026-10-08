//! What one read hands a screen reader while Read new game lines is on
//! (R21 and R25 review, Q19 to Q21): the plain text of each line that
//! shows, after gags, routes and replaces, and the text of your prompt,
//! which the page reads only when you ask. The steps fill a
//! [`ReaderFeed`] in the read's batch under the profile lock, and the end
//! of the read sends it once on `session://screen-reader`, after the
//! locks drop.

use std::collections::VecDeque;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app::events;
use crate::app::state::SharedState;
use crate::sessions::Session;

/// The most lines one read hands on. The page keeps no more than this
/// either, so a flood never grows a send past it.
const READ_LINES: usize = 500;

/// The text one read hands a screen reader.
#[derive(Debug, Default)]
pub(super) struct ReaderFeed {
    /// The plain text of each line that shows, oldest first, at most
    /// [`READ_LINES`] of them.
    pub(super) lines: VecDeque<String>,
    /// How many lines the read showed, the ones dropped included.
    pub(super) count: usize,
    /// Your prompt's text, its lines joined, when the read brought one.
    pub(super) prompt: Option<String>,
    /// The start of a line an earlier read painted raw and the reader
    /// already read, so the line that completes it reads only the rest.
    pub(super) heard: Option<String>,
}

impl ReaderFeed {
    /// One line that shows, as plain text. A blank line reads nothing.
    pub(super) fn line(&mut self, text: &str) {
        let text = match self.heard.as_deref().and_then(|h| text.strip_prefix(h)) {
            Some(rest) => {
                self.heard = None;
                rest
            }
            None => text,
        };
        let text = text.trim_end();
        if text.trim_start().is_empty() {
            return;
        }
        self.count += 1;
        if self.lines.len() == READ_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(text.to_string());
    }

    /// Each line of `shown`, text with its ANSI, as it shows.
    pub(super) fn lines_of(&mut self, shown: &[u8]) {
        if shown.is_empty() {
            return;
        }
        let plain = vosh_protocol::ansi::plain_text(shown);
        for line in plain.split('\n') {
            self.line(line.trim_matches('\r'));
        }
    }

    /// Your prompt, its lines of plain text joined.
    pub(super) fn prompt<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) {
        let lines: Vec<&str> = lines
            .into_iter()
            .map(str::trim_end)
            .filter(|line| !line.trim_start().is_empty())
            .collect();
        self.prompt = Some(lines.join("\n"));
    }
}

/// A partial the end of a read paints raw, as plain text: the reader
/// reads what it adds to the start it read before, and keeps it all as
/// `heard`, the start the line that completes it does not read again.
pub(super) fn painted_partial(feed: &mut ReaderFeed, heard: &mut Option<String>, plain: String) {
    let before = heard.take();
    let rest = before
        .as_deref()
        .and_then(|b| plain.strip_prefix(b))
        .unwrap_or(&plain);
    feed.line(rest);
    *heard = Some(plain);
}

/// What `session://screen-reader` carries beside the session's id.
#[derive(Debug, Serialize)]
struct ScreenReaderPayload {
    lines: VecDeque<String>,
    count: usize,
    prompt: Option<String>,
    /// No window of Vosh has focus.
    away: bool,
}

/// Send what `feed` holds, once, when it holds a line or a prompt. Reads
/// the focus, a leaf lock, so call it with no profile or connection held.
pub(super) fn emit<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, feed: ReaderFeed) {
    let ReaderFeed {
        lines,
        count,
        prompt,
        heard: _,
    } = feed;
    if lines.is_empty() && prompt.is_none() {
        return;
    }
    let away = app
        .try_state::<SharedState>()
        .is_some_and(|state| !state.focus.front());
    session.emit(
        app,
        events::SCREEN_READER,
        &ScreenReaderPayload {
            lines,
            count,
            prompt,
            away,
        },
    );
}
