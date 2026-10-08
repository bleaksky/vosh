//! The players a session snoops (Snoop SN3 and SN5). Aabahran sends
//! Snoop.Start and Snoop.Stop with the name of each player you start or
//! stop snooping, and Snoop.Output with what that player's screen got,
//! ANSI and all (gmcp.c `gmcp_send_snoop` and `gmcp_send_snoop_state`).
//! [`Snoops`] keeps one tab per player in the order they started, with
//! the newest [`MAX_LINES`] lines of its text as the game sent them. It
//! sits on the session's [`Connection`](super::connection::Connection),
//! so it outlives each link, and goes when the session closes.
//!
//! The text never touches the line pipeline, so no trigger, highlight,
//! gag or preset sound sees it. Lua still hears the packets through its
//! GMCP handlers. Each whole line goes in the session log as its own row
//! marked with the player's name (SN4), through [`Snoops::take_log`]. A
//! partial waits in the tab for its newline, or for the end of the snoop
//! or the link. The session sends the tab list on `session://snoop`
//! and new text on `session://snoop-output` once per read, and the page
//! reads every tab with its text through `snoop_get`.
//!
//! Open in a window moves every tab of a session into a window of its
//! own (SN1). [`Snoops`] keeps whether it is out, and the tab list
//! carries it, so the split closes while the window is open and comes
//! back with the same tabs once it closes.

use std::collections::VecDeque;

use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use crate::app::events;
use crate::sessions::Session;

/// The lines a tab keeps, as the snoop terminal's scrollback does.
pub(crate) const MAX_LINES: usize = 5_000;

/// Where a snoop stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// The game sends this player's screen.
    Live,
    /// You pressed Stop, and the game has not said the snoop ended yet.
    Stopping,
    /// The snoop ended at this time, in ms since the epoch, by the game
    /// or a link that ended. The tab stays until you close it.
    Ended { at_ms: i64 },
}

/// One snooped player.
#[derive(Debug)]
struct Tab {
    name: String,
    state: State,
    /// When the game last sent this player's screen, in ms since the
    /// epoch.
    last_output_ms: Option<i64>,
    /// Each whole line, its `\n` kept, oldest first.
    lines: VecDeque<String>,
    /// The text after the last `\n`.
    partial: String,
    /// How many bytes of the partial are in the log already, which the
    /// end of a snoop or a link puts there.
    logged: usize,
}

impl Tab {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: State::Live,
            last_output_ms: None,
            lines: VecDeque::new(),
            partial: String::new(),
            logged: 0,
        }
    }

    /// Add `text` as the game sent it, keeping the newest [`MAX_LINES`]
    /// lines. Returns the text of the lines it ended that the log does
    /// not have yet.
    fn push(&mut self, text: &str) -> String {
        let mut ended = String::new();
        let mut rest = text;
        while let Some(end) = rest.find('\n') {
            let mut line = std::mem::take(&mut self.partial);
            line.push_str(&rest[..=end]);
            ended.push_str(&line[std::mem::take(&mut self.logged)..]);
            self.lines.push_back(line);
            rest = &rest[end + 1..];
        }
        self.partial.push_str(rest);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
        ended
    }

    /// The snoop ended, so the part of the partial the log does not have
    /// yet goes in it. The partial stays in the text.
    fn end_log(&mut self) -> String {
        let rest = self.partial[self.logged..].to_string();
        self.logged = self.partial.len();
        rest
    }

    fn text(&self) -> String {
        let mut text: String = self.lines.iter().map(String::as_str).collect();
        text.push_str(&self.partial);
        text
    }

    fn row(&self) -> SnoopTab {
        let (live, ended_at) = match self.state {
            State::Live | State::Stopping => (true, None),
            State::Ended { at_ms } => (false, Some(at_ms)),
        };
        SnoopTab {
            name: self.name.clone(),
            live,
            ended_at,
            last_output_at: self.last_output_ms,
        }
    }
}

/// A tab as the page reads it. A snoop you pressed Stop on reads live
/// until the game says it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnoopTab {
    pub(crate) name: String,
    pub(crate) live: bool,
    /// When the snoop ended, in ms since the epoch.
    pub(crate) ended_at: Option<i64>,
    /// When the game last sent the player's screen, in ms since the
    /// epoch.
    pub(crate) last_output_at: Option<i64>,
}

/// What `session://snoop` carries beside the session: every tab, in the
/// order they started, and whether they show in the snoop window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnoopPayload {
    pub(crate) tabs: Vec<SnoopTab>,
    /// The tabs show in the snoop window, and the split stays closed.
    pub(crate) windowed: bool,
}

/// What `session://snoop-output` carries beside the session: the text
/// one player's screen got in one read, raw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnoopOutputPayload {
    pub(crate) name: String,
    pub(crate) text: String,
}

/// A tab with its text, as `snoop_get` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnoopTabText {
    #[serde(flatten)]
    pub(crate) tab: SnoopTab,
    pub(crate) text: String,
}

/// What `snoop_get` returns: every tab with its text, in the order they
/// started, and whether they show in the snoop window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SnoopSnapshot {
    pub(crate) tabs: Vec<SnoopTabText>,
    pub(crate) windowed: bool,
}

/// What changed since the last send.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct SnoopChanges {
    /// Every tab, when the list changed.
    pub(crate) list: Option<SnoopPayload>,
    /// The new text of each player, one entry a name.
    pub(crate) output: Vec<SnoopOutputPayload>,
}

/// The snoops of one session, and what changed since the last send.
#[derive(Debug, Default)]
pub(crate) struct Snoops {
    tabs: Vec<Tab>,
    /// The tabs show in the snoop window.
    windowed: bool,
    list_changed: bool,
    output: Vec<SnoopOutputPayload>,
    /// Text for the log, each with the name of its player, in the order
    /// it ended.
    log: Vec<(String, String)>,
}

impl Snoops {
    /// Take a Snoop.Start, Snoop.Stop or Snoop.Output packet at `now_ms`.
    /// Returns false for any other package, or one with no name.
    pub(crate) fn gmcp(&mut self, package: &str, data: &Value, now_ms: i64) -> bool {
        if !package.starts_with("Snoop.") {
            return false;
        }
        let Some(name) = data.get("name").and_then(Value::as_str) else {
            return false;
        };
        match package {
            "Snoop.Start" => self.start(name),
            "Snoop.Stop" => self.stop(name, now_ms),
            "Snoop.Output" => {
                let text = data.get("text").and_then(Value::as_str).unwrap_or("");
                self.output(name, text, now_ms);
            }
            _ => return false,
        }
        true
    }

    /// Keep the text a tab ended for the log.
    fn keep_for_log(&mut self, at: usize, text: String) {
        if !text.is_empty() {
            self.log.push((self.tabs[at].name.clone(), text));
        }
    }

    /// End the log of the tab at `at`, see [`Tab::end_log`].
    fn end_log(&mut self, at: usize) {
        let rest = self.tabs[at].end_log();
        self.keep_for_log(at, rest);
    }

    fn find(&mut self, name: &str) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.name == name)
    }

    /// The game started a snoop. A player you snooped before gets the
    /// tab back, live, with its old text.
    fn start(&mut self, name: &str) {
        match self.find(name) {
            Some(tab) => tab.state = State::Live,
            None => self.tabs.push(Tab::new(name)),
        }
        self.list_changed = true;
    }

    /// The game ended a snoop. The one you pressed Stop on goes, and any
    /// other stays as ended.
    fn stop(&mut self, name: &str, now_ms: i64) {
        let Some(at) = self.tabs.iter().position(|tab| tab.name == name) else {
            return;
        };
        self.end_log(at);
        match self.tabs[at].state {
            State::Stopping => self.remove(at),
            State::Live => self.tabs[at].state = State::Ended { at_ms: now_ms },
            State::Ended { .. } => return,
        }
        self.list_changed = true;
    }

    /// A player's screen got `text`. Text for a player with no tab opens
    /// one, since the game only sends it for a snoop.
    fn output(&mut self, name: &str, text: &str, now_ms: i64) {
        let at = match self.tabs.iter().position(|tab| tab.name == name) {
            Some(at) => at,
            None => {
                self.tabs.push(Tab::new(name));
                self.tabs.len() - 1
            }
        };
        let ended = self.tabs[at].push(text);
        self.tabs[at].last_output_ms = Some(now_ms);
        self.keep_for_log(at, ended);
        self.list_changed = true;
        match self.output.iter_mut().find(|o| o.name == name) {
            Some(sent) => sent.text.push_str(text),
            None => self.output.push(SnoopOutputPayload {
                name: name.to_string(),
                text: text.to_string(),
            }),
        }
    }

    fn remove(&mut self, at: usize) {
        let tab = self.tabs.remove(at);
        self.output.retain(|o| o.name != tab.name);
    }

    /// The link ended at `now_ms`, and every snoop with it. A live one
    /// stays as ended, and one you pressed Stop on goes. The partial of
    /// each goes in the log.
    pub(crate) fn link_ended(&mut self, now_ms: i64) {
        for at in 0..self.tabs.len() {
            self.end_log(at);
        }
        let before = self.tabs.len();
        self.tabs.retain(|tab| tab.state != State::Stopping);
        let mut changed = self.tabs.len() != before;
        for tab in &mut self.tabs {
            if tab.state == State::Live {
                tab.state = State::Ended { at_ms: now_ms };
                changed = true;
            }
        }
        self.list_changed |= changed;
    }

    /// You pressed Stop on the snoop of `name`, or Stop every snoop with
    /// no name. Each live one it names waits for the game to end it.
    pub(crate) fn stopping(&mut self, name: Option<&str>) {
        for tab in &mut self.tabs {
            if tab.state == State::Live && name.map_or(true, |name| tab.name == name) {
                tab.state = State::Stopping;
                self.list_changed = true;
            }
        }
    }

    /// Close the ended tab of `name`, or every ended tab with no name,
    /// and its text.
    pub(crate) fn close(&mut self, name: Option<&str>) {
        while let Some(at) = self.tabs.iter().position(|tab| {
            matches!(tab.state, State::Ended { .. }) && name.map_or(true, |name| tab.name == name)
        }) {
            self.remove(at);
            self.list_changed = true;
        }
    }

    /// The session log rows for the snoop text that ended since the
    /// last call, at `ts_ms`, for the log's row `session_id`. With no
    /// row the session writes no log, and the text goes.
    pub(crate) fn take_log(
        &mut self,
        session_id: Option<i64>,
        ts_ms: i64,
    ) -> Vec<vosh_log::LogEntry> {
        let text = std::mem::take(&mut self.log);
        let Some(session_id) = session_id else {
            return Vec::new();
        };
        text.iter()
            .flat_map(|(name, text)| vosh_log::snoop_rows(name, text))
            .map(|(text, raw)| vosh_log::LogEntry {
                session_id,
                ts_ms,
                text,
                raw: Some(raw),
                kind: vosh_log::LineKind::Text,
            })
            .collect()
    }

    /// The snoop window opened, `on`, or closed. The tab list goes again
    /// when that changes where the tabs show.
    pub(crate) fn set_windowed(&mut self, on: bool) {
        if self.windowed != on {
            self.windowed = on;
            self.list_changed = true;
        }
    }

    /// Every tab with its text, and whether they show in the window.
    pub(crate) fn snapshot(&self) -> SnoopSnapshot {
        SnoopSnapshot {
            tabs: self.all(),
            windowed: self.windowed,
        }
    }

    /// Every tab with its text, in the order they started.
    fn all(&self) -> Vec<SnoopTabText> {
        self.tabs
            .iter()
            .map(|tab| SnoopTabText {
                tab: tab.row(),
                text: tab.text(),
            })
            .collect()
    }

    /// What changed since the last call.
    pub(crate) fn take_changes(&mut self) -> SnoopChanges {
        let list = std::mem::take(&mut self.list_changed).then(|| SnoopPayload {
            tabs: self.tabs.iter().map(Tab::row).collect(),
            windowed: self.windowed,
        });
        SnoopChanges {
            list,
            output: std::mem::take(&mut self.output),
        }
    }
}

/// Send `changes` for `session`: the tab list first, so a tab that
/// started shows before its text, then the text of each player.
pub(crate) fn emit<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    changes: SnoopChanges,
) {
    if let Some(list) = changes.list {
        session.emit(app, events::SNOOP, &list);
    }
    for output in changes.output {
        session.emit(app, events::SNOOP_OUTPUT, &output);
    }
}

/// Send what changed in the snoops of `session` since the last send.
pub(crate) fn emit_changes<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session) {
    let changes = session.connection.lock().snoops.take_changes();
    emit(app, session, changes);
}

/// Mark the snoops of `session` as shown in the snoop window, `on`, or
/// back in the split, and send the tab list when that changed.
pub(crate) fn set_windowed<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, on: bool) {
    session.connection.lock().snoops.set_windowed(on);
    emit_changes(app, session);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SnoopTab, Snoops, MAX_LINES};

    fn start(s: &mut Snoops, name: &str) {
        assert!(s.gmcp("Snoop.Start", &json!({ "name": name }), 0));
    }

    fn stop(s: &mut Snoops, name: &str, at: i64) {
        assert!(s.gmcp("Snoop.Stop", &json!({ "name": name }), at));
    }

    fn output(s: &mut Snoops, name: &str, text: &str, at: i64) {
        assert!(s.gmcp("Snoop.Output", &json!({ "name": name, "text": text }), at));
    }

    fn tabs(s: &Snoops) -> Vec<(String, bool, Option<i64>)> {
        s.all()
            .into_iter()
            .map(|t| (t.tab.name, t.tab.live, t.tab.ended_at))
            .collect()
    }

    fn tab(name: &str, live: bool, ended: Option<i64>) -> (String, bool, Option<i64>) {
        (name.to_string(), live, ended)
    }

    #[test]
    fn tabs_keep_the_order_they_started_in_and_the_text_as_sent() {
        let mut s = Snoops::default();
        start(&mut s, "Tolliver");
        start(&mut s, "Maren");
        output(
            &mut s,
            "Maren",
            "\u{1b}[0;33mA Trail\u{1b}[0;0m\n\r  This",
            5,
        );
        output(&mut s, "Maren", " is a path.\n\r", 7);
        let all = s.all();
        assert_eq!(
            all.iter().map(|t| t.tab.name.as_str()).collect::<Vec<_>>(),
            ["Tolliver", "Maren"]
        );
        assert_eq!(all[0].text, "");
        assert_eq!(
            all[1].text,
            "\u{1b}[0;33mA Trail\u{1b}[0;0m\n\r  This is a path.\n\r"
        );
        assert_eq!(all[1].tab.last_output_at, Some(7));
        assert_eq!(all[0].tab.last_output_at, None);
    }

    #[test]
    fn a_read_sends_the_list_once_and_one_text_a_name() {
        let mut s = Snoops::default();
        start(&mut s, "Orla");
        output(&mut s, "Orla", "one\n\r", 1);
        output(&mut s, "Orla", "two\n\r", 2);
        let changes = s.take_changes();
        assert_eq!(
            changes.list.expect("the list").tabs,
            [SnoopTab {
                name: "Orla".into(),
                live: true,
                ended_at: None,
                last_output_at: Some(2),
            }]
        );
        assert_eq!(changes.output.len(), 1);
        assert_eq!(changes.output[0].text, "one\n\rtwo\n\r");
        assert_eq!(s.take_changes(), super::SnoopChanges::default());
    }

    #[test]
    fn a_stop_you_asked_for_removes_the_tab_and_any_other_ends_it() {
        let mut s = Snoops::default();
        start(&mut s, "Tolliver");
        start(&mut s, "Maren");
        s.stopping(Some("Tolliver"));
        assert_eq!(tabs(&s)[0], tab("Tolliver", true, None));
        stop(&mut s, "Tolliver", 10);
        stop(&mut s, "Maren", 11);
        assert_eq!(tabs(&s), [tab("Maren", false, Some(11))]);
        // A repeat start brings it back live with its text.
        output(&mut s, "Maren", "kept\n\r", 12);
        stop(&mut s, "Maren", 13);
        start(&mut s, "Maren");
        assert_eq!(tabs(&s), [tab("Maren", true, None)]);
        assert_eq!(s.all()[0].text, "kept\n\r");
    }

    #[test]
    fn stop_every_snoop_and_close_every_ended_tab() {
        let mut s = Snoops::default();
        for name in ["Tolliver", "Maren", "Orla"] {
            start(&mut s, name);
        }
        stop(&mut s, "Orla", 3);
        s.stopping(None);
        stop(&mut s, "Tolliver", 4);
        stop(&mut s, "Maren", 4);
        assert_eq!(tabs(&s), [tab("Orla", false, Some(3))]);
        s.close(Some("Tolliver"));
        assert_eq!(tabs(&s).len(), 1);
        s.close(None);
        assert_eq!(s.all(), Vec::new());
    }

    #[test]
    fn a_closed_tab_takes_its_unsent_text_and_a_live_one_stays_open() {
        let mut s = Snoops::default();
        start(&mut s, "Maren");
        output(&mut s, "Maren", "x", 1);
        s.close(Some("Maren"));
        assert_eq!(tabs(&s), [tab("Maren", true, None)]);
        stop(&mut s, "Maren", 2);
        s.close(Some("Maren"));
        assert_eq!(s.take_changes().output, Vec::new());
    }

    #[test]
    fn the_window_flag_sends_the_list_only_when_it_changes() {
        let mut s = Snoops::default();
        start(&mut s, "Tolliver");
        s.take_changes();
        s.set_windowed(true);
        let list = s.take_changes().list.expect("the list");
        assert!(list.windowed);
        assert_eq!(list.tabs.len(), 1);
        s.set_windowed(true);
        assert_eq!(s.take_changes().list, None);
        assert!(s.snapshot().windowed);
        s.set_windowed(false);
        assert!(!s.take_changes().list.expect("the list").windowed);
    }

    #[test]
    fn the_end_of_the_link_ends_each_live_snoop() {
        let mut s = Snoops::default();
        start(&mut s, "Tolliver");
        start(&mut s, "Maren");
        s.stopping(Some("Maren"));
        s.take_changes();
        s.link_ended(20);
        assert_eq!(tabs(&s), [tab("Tolliver", false, Some(20))]);
        assert!(s.take_changes().list.is_some());
        s.link_ended(30);
        assert_eq!(s.take_changes().list, None);
    }

    #[test]
    fn the_ring_keeps_the_newest_lines_and_the_partial() {
        let mut s = Snoops::default();
        let text: String = (0..MAX_LINES + 3).map(|i| i.to_string() + "\n\r").collect();
        output(&mut s, "Orla", &text, 1);
        output(&mut s, "Orla", "<612hp 480m 702mv> ", 2);
        let kept = &s.all()[0].text;
        assert!(kept.starts_with("\r3\n\r4\n"), "{:?}", &kept[..12]);
        assert!(kept.ends_with("\r<612hp 480m 702mv> "));
        assert_eq!(kept.matches('\n').count(), MAX_LINES);
    }

    /// The text and raw rows `take_log` gives for session 1.
    fn logged(s: &mut Snoops) -> Vec<(String, String)> {
        s.take_log(Some(1), 9)
            .into_iter()
            .map(|row| {
                let raw = String::from_utf8(row.raw.expect("raw")).expect("utf8");
                (row.text, raw)
            })
            .collect()
    }

    fn texts(s: &mut Snoops) -> Vec<String> {
        logged(s).into_iter().map(|(text, _)| text).collect()
    }

    #[test]
    fn whole_lines_go_to_the_log_and_the_partial_waits_for_its_newline() {
        let mut s = Snoops::default();
        start(&mut s, "Maren");
        output(
            &mut s,
            "Maren",
            "\u{1b}[0;33mA Trail\u{1b}[0;0m\n\r  This",
            1,
        );
        assert_eq!(
            logged(&mut s),
            [(
                "Maren| A Trail".to_string(),
                "Maren| \u{1b}[0;33mA Trail\u{1b}[0;0m".to_string()
            )]
        );
        output(&mut s, "Maren", " is a path.\n\r", 2);
        assert_eq!(texts(&mut s), ["Maren|   This is a path."]);
        // The ring keeps the text as sent, with no mark.
        assert!(!s.all()[0].text.contains("Maren|"));
        // With no log row the text goes.
        output(&mut s, "Maren", "gone\n\r", 3);
        assert!(s.take_log(None, 9).is_empty());
        assert_eq!(texts(&mut s), Vec::<String>::new());
    }

    #[test]
    fn the_end_of_a_snoop_or_the_link_logs_the_partial_once() {
        let mut s = Snoops::default();
        start(&mut s, "Tolliver");
        start(&mut s, "Orla");
        output(&mut s, "Tolliver", "<612hp 480m 702mv> ", 1);
        output(&mut s, "Orla", "Orla is here.\n\r<20hp ", 1);
        assert_eq!(texts(&mut s), ["Orla| Orla is here."]);
        stop(&mut s, "Tolliver", 2);
        assert_eq!(texts(&mut s), ["Tolliver| <612hp 480m 702mv> "]);
        s.link_ended(3);
        assert_eq!(texts(&mut s), ["Orla| <20hp "]);
        s.link_ended(4);
        assert_eq!(texts(&mut s), Vec::<String>::new());
        // A repeat snoop logs only what follows the partial it ended on.
        start(&mut s, "Orla");
        output(&mut s, "Orla", "30m>\n\r", 5);
        assert_eq!(texts(&mut s), ["Orla| 30m>"]);
        assert!(s.all()[1].text.ends_with("<20hp 30m>\n\r"));
    }

    #[test]
    fn other_packages_and_nameless_packets_change_nothing() {
        let mut s = Snoops::default();
        assert!(!s.gmcp("Char.Vitals", &json!({ "name": "Orla" }), 0));
        assert!(!s.gmcp("Snoop.Start", &json!({}), 0));
        assert_eq!(s.take_changes(), super::SnoopChanges::default());
    }
}
