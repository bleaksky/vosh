//! The Output ring of a session: the newest `[lua]` lines it printed,
//! with whose Lua each one is about, and the lines you typed in the
//! Scripts console. The Scripts page reads it as it opens and hears each
//! new line on `session://lua-output`. The connection keeps it apart from
//! what a disconnect clears, so the lines plugins print at launch and
//! before a connect stay. It also keeps when each plugin last loaded, so
//! the page tells an error of the code that runs now from an older one.

use std::collections::{HashMap, VecDeque};

use serde::Serialize;
use vosh_script::{Owner, Place};

/// How many lines the ring of one session keeps. The oldest goes first.
pub(crate) const OUTPUT_LINES: usize = 500;

/// What a line in the ring is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LuaKind {
    /// A line from `print` or `mud.log`.
    Print,
    /// A Lua error, or a stop.
    Error,
    /// A sentence Vosh writes about a script.
    Note,
    /// A line you typed in the console.
    Input,
}

/// Where in your Lua an error happened, which the page marks in its
/// editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LuaAt {
    /// The chunk as Lua names it, like `vitals_alert/main.lua`.
    pub(crate) source: String,
    pub(crate) line: u32,
}

/// One line of the ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LuaLine {
    /// When Vosh printed it, in milliseconds since the Unix epoch.
    pub(crate) ts_ms: i64,
    /// Whose Lua it is about, as [`Owner::tag`] names it, like
    /// `plugin:vitals_alert` or `#lua`.
    pub(crate) owner: String,
    pub(crate) kind: LuaKind,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) at: Option<LuaAt>,
}

impl LuaLine {
    /// The lines of `kind` that `text` makes about the Lua of `owner`,
    /// one for each line of the text, as the terminal shows them.
    pub(crate) fn lines(
        owner: &Owner,
        kind: LuaKind,
        text: &str,
        at: Option<&Place>,
        ts_ms: i64,
    ) -> Vec<LuaLine> {
        let owner = owner.tag();
        let at = at.map(|place| LuaAt {
            source: place.source.clone(),
            line: place.line,
        });
        text.split('\n')
            .map(|line| LuaLine {
                ts_ms,
                owner: owner.clone(),
                kind,
                text: line.trim_end_matches('\r').to_string(),
                at: at.clone(),
            })
            .collect()
    }
}

/// The newest [`OUTPUT_LINES`] lines of a session, and when each plugin
/// last loaded in it.
#[derive(Debug, Default)]
pub(crate) struct LuaOutput {
    lines: VecDeque<LuaLine>,
    /// When each owner last loaded, in milliseconds since the Unix
    /// epoch, by [`Owner::tag`].
    loads: HashMap<String, i64>,
}

impl LuaOutput {
    /// Keep `line` as the newest, and let the oldest go past
    /// [`OUTPUT_LINES`].
    pub(crate) fn push(&mut self, line: LuaLine) {
        if self.lines.len() == OUTPUT_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Every line, oldest first.
    pub(crate) fn lines(&self) -> Vec<LuaLine> {
        self.lines.iter().cloned().collect()
    }

    /// Note that `owner` loads at `ts_ms`, ahead of any line its load
    /// prints, which therefore carries that time or a later one.
    pub(crate) fn note_load(&mut self, owner: &Owner, ts_ms: i64) {
        self.loads.insert(owner.tag(), ts_ms);
    }

    /// When `owner` last loaded, if it has in this session.
    pub(crate) fn loaded_at(&self, owner: &Owner) -> Option<i64> {
        self.loads.get(&owner.tag()).copied()
    }

    /// Let go of the lines of `owner`, a tag like `plugin:vitals_alert`,
    /// or of every line with None.
    pub(crate) fn clear(&mut self, owner: Option<&str>) {
        match owner {
            Some(owner) => self.lines.retain(|line| line.owner != owner),
            None => self.lines.clear(),
        }
    }
}

/// What `session://lua-output` carries beside the session: the lines
/// one step added to the ring, oldest first.
#[derive(Debug, Serialize)]
pub(crate) struct LuaOutputPayload {
    pub(crate) lines: Vec<LuaLine>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_keeps_the_ring_size_and_reads_the_owner_tag() {
        // The Scripts page keeps as many lines as a session's ring.
        let page = include_str!("../../../src/settings/scripts/ScriptsPage.tsx");
        assert!(page.contains(&format!("const CONSOLE_LINES = {OUTPUT_LINES};")));
        // It finds a plugin's lines by the tag Owner::tag gives them.
        let ipc = include_str!("../../../src/ipc/scripts.ts");
        assert!(ipc.contains("return `plugin:${name}`;"));
        assert_eq!(
            Owner::Plugin("vitals_alert".into()).tag(),
            "plugin:vitals_alert"
        );
    }

    fn print(owner: &str, text: &str) -> LuaLine {
        LuaLine {
            ts_ms: 0,
            owner: owner.into(),
            kind: LuaKind::Print,
            text: text.into(),
            at: None,
        }
    }

    #[test]
    fn the_ring_keeps_the_newest_500_lines() {
        let mut ring = LuaOutput::default();
        for i in 0..OUTPUT_LINES + 20 {
            ring.push(print("plugin:vitals_alert", &i.to_string()));
        }
        let lines = ring.lines();
        assert_eq!(lines.len(), 500);
        assert_eq!(lines[0].text, "20");
        assert_eq!(lines[499].text, "519");
    }

    #[test]
    fn clearing_one_owner_keeps_the_others() {
        let mut ring = LuaOutput::default();
        ring.push(print("plugin:vitals_alert", "one"));
        ring.push(print("plugin:wait_full", "two"));
        ring.push(print("#lua", "three"));
        ring.push(print("plugin:vitals_alert", "four"));
        ring.clear(Some("plugin:vitals_alert"));
        let left: Vec<String> = ring.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(left, ["two", "three"]);
        ring.clear(None);
        let leftover = &ring.lines();
        assert!(leftover.is_empty(), "{leftover:?}");
    }

    #[test]
    fn the_ring_keeps_when_each_plugin_last_loaded() {
        let mut ring = LuaOutput::default();
        let wait_full = Owner::Plugin("wait_full".into());
        assert_eq!(ring.loaded_at(&wait_full), None);
        ring.note_load(&wait_full, 1);
        ring.note_load(&wait_full, 5);
        ring.note_load(&Owner::Plugin("vitals_alert".into()), 3);
        assert_eq!(ring.loaded_at(&wait_full), Some(5));
        // Clear lets go of lines, not of loads.
        ring.clear(None);
        assert_eq!(ring.loaded_at(&wait_full), Some(5));
    }

    #[test]
    fn a_line_reads_as_the_page_hears_it() {
        let wait_full = Owner::Plugin("wait_full".into());
        let at = Place {
            source: "wait_full/main.lua".into(),
            line: 5,
        };
        let stop = LuaLine::lines(
            &wait_full,
            LuaKind::Error,
            "Vosh stopped wait_full at main.lua line 5 after 100 ms.",
            Some(&at),
            1_759_785_611_000,
        );
        assert_eq!(
            serde_json::to_value(&stop).unwrap(),
            serde_json::json!([{
                "ts_ms": 1_759_785_611_000_i64,
                "owner": "plugin:wait_full",
                "kind": "error",
                "text": "Vosh stopped wait_full at main.lua line 5 after 100 ms.",
                "at": {"source": "wait_full/main.lua", "line": 5},
            }])
        );
        // A print of two lines is two lines, as in the terminal, and a
        // line with no place leaves it out.
        let printed = LuaLine::lines(
            &Owner::Typed,
            LuaKind::Print,
            "a bank representative\r\nMaren",
            None,
            0,
        );
        assert_eq!(
            serde_json::to_value(&printed).unwrap(),
            serde_json::json!([
                {"ts_ms": 0, "owner": "#lua", "kind": "print", "text": "a bank representative"},
                {"ts_ms": 0, "owner": "#lua", "kind": "print", "text": "Maren"},
            ])
        );
    }
}
