//! What each row the session logs is, so Save a scene can leave out your
//! prompt, your commands, the lines outside play and the channels you
//! pick (Q9 of the Alerts and Scenes review).
//!
//! - A line outside play is login: before the game says you play, from
//!   Char.Status or the vitals, and after you step away to the account
//!   menu, as [`LinkWatch`](super::reconnect::LinkWatch) follows it. The
//!   lines you send there are login too, so the name you typed and the
//!   characters the menu lists stay out of every scene.
//! - Your prompt is a prompt as the prompt stage reads it, and a line you
//!   send is sent.
//! - A Comm.Channel packet names the channel of one line, the first
//!   since the last prompt whose plain text holds the packet's speaker
//!   and text, colors stripped. Your own line reads `You say` where the
//!   packet names you, so a line that starts with `You ` holds any
//!   speaker. The game used to write a packet to the socket at once and
//!   its line at the end of the pulse, so a packet came ahead of its line
//!   and of any line already waiting. Since d50e4a24 it queues both in
//!   order, so a packet follows its line. Either way the pair is the same
//!   one: a packet first waits for the line, and a packet after its line
//!   finds it among the rows of the read since the last prompt, the
//!   newest that holds it. A packet
//!   that finds none stays unpaired, and a prompt forgets it.
//! - The game sends no packet to the one who sends a tell
//!   (`languages.c`), so a line shaped like `You tell Tolliver '…'` or
//!   `You project to Tolliver '…'` is a tell. A group tell you send has
//!   the same shape to `your group`, and its gtell packet names it.
//! - `replay` pages the tells you got while away (`act_comm.c`
//!   `do_replay`), so its reply is a tell up to the next prompt.
//!   `replay tells`, `replay says` and the others page the last lines of
//!   a channel (`db_sqlite.c` `playerdb_read_comm`), and their reply
//!   takes that channel the same way.

use vosh_log::{LineKind, LogEntry};

/// The most Comm.Channel packets that wait for their line. A game that
/// sends no prompt Vosh reads would otherwise keep them all.
const WAITING_CAP: usize = 32;

/// A Comm.Channel packet waiting for its line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Heard {
    channel: String,
    /// The speaker, lowercased, since the line can capitalize it, as
    /// `Someone says` reads for a packet that names `someone`.
    speaker: String,
    text: String,
}

impl Heard {
    /// The channel, speaker and text a Comm.Channel packet names, or None
    /// for one with no channel or no text.
    fn of_packet(data: &serde_json::Value) -> Option<Self> {
        let field = |key: &str| data.get(key).and_then(|v| v.as_str()).unwrap_or_default();
        let (channel, text) = (field("channel").trim(), field("text").trim());
        if channel.is_empty() || text.is_empty() {
            return None;
        }
        Some(Self {
            channel: channel.to_string(),
            speaker: field("speaker").trim().to_lowercase(),
            text: text.to_string(),
        })
    }

    /// Whether `plain`, a line's text with its colors stripped, is the
    /// line this packet goes with.
    fn holds(&self, plain: &str) -> bool {
        plain.contains(&self.text)
            && (plain.starts_with("You ") || plain.to_lowercase().contains(&self.speaker))
    }
}

/// What the session follows to tell its rows apart, for one connection.
#[derive(Debug, Default)]
pub(crate) struct LogKinds {
    /// Comm.Channel packets since the last prompt that found no line yet,
    /// oldest first.
    waiting: Vec<Heard>,
    /// You sent `replay`, so its reply takes this channel until the next
    /// prompt.
    replay: Option<&'static str>,
}

impl LogKinds {
    /// The kind of a game line that is not your prompt. `plain` is its
    /// text with colors stripped, and `playing` whether you play.
    pub(crate) fn line(&mut self, plain: &str, playing: bool) -> LineKind {
        if !playing {
            return LineKind::Login;
        }
        if let Some(channel) = self.replay {
            return LineKind::Channel(channel.to_string());
        }
        if sent_tell(plain) {
            return LineKind::Channel("tell".to_string());
        }
        match self.waiting.iter().position(|heard| heard.holds(plain)) {
            Some(at) => LineKind::Channel(self.waiting.remove(at).channel),
            None => LineKind::Text,
        }
    }

    /// The kind of a line of your prompt.
    pub(crate) fn prompt_line(playing: bool) -> LineKind {
        if playing {
            LineKind::Prompt
        } else {
            LineKind::Login
        }
    }

    /// A Comm.Channel packet came, with `data` its body. `since_prompt`
    /// holds the rows of this read since the last prompt, oldest first.
    /// The first plain game line among them that holds the packet takes
    /// its channel, and otherwise the packet waits for its line.
    pub(crate) fn packet(&mut self, data: &serde_json::Value, since_prompt: &mut [LogEntry]) {
        let Some(heard) = Heard::of_packet(data) else {
            return;
        };
        // The newest such row, since the game queues a packet right after
        // its own line, and an earlier line of the pulse can hold a short
        // text and the speaker's name too, as `Tolliver nods yes.` holds
        // the say `yes`.
        let found = since_prompt
            .iter_mut()
            .rev()
            .find(|row| row.kind == LineKind::Text && heard.holds(&row.text));
        match found {
            Some(row) => row.kind = LineKind::Channel(heard.channel),
            None => {
                if self.waiting.len() == WAITING_CAP {
                    self.waiting.remove(0);
                }
                self.waiting.push(heard);
            }
        }
    }

    /// Your prompt came, or a GA or EOR, which ends the pulse's text, so
    /// a packet still waiting found no line, and a `replay` reply ends.
    pub(crate) fn prompt(&mut self) {
        self.waiting.clear();
        self.replay = None;
    }

    /// You sent `bytes`, a line or more. Returns the kind of their rows,
    /// sent in play and login outside it. A `replay` in play makes its
    /// reply the channel it pages.
    pub(crate) fn sent(&mut self, bytes: &[u8], playing: bool) -> LineKind {
        if !playing {
            return LineKind::Login;
        }
        let text = String::from_utf8_lossy(bytes);
        if let Some(channel) = text.split(['\r', '\n']).filter_map(replayed).next_back() {
            self.replay = Some(channel);
        }
        LineKind::Sent
    }
}

/// The channel whose lines `line`, a command you sent, pages, when it is
/// a `replay` that shows lines (`act_comm.c` `do_replay`). A bare
/// `replay` pages the tells you got while away, and `replay tells` and
/// the others the last lines of their channel, with a name after them
/// for an immortal. The game reads `rep` and `repl` as `reply`, so
/// `repla` is the shortest, and it reads the channel word whole.
fn replayed(line: &str) -> Option<&'static str> {
    let mut words = line.split_whitespace();
    let word = words.next()?.to_ascii_lowercase();
    if word.len() < 5 || !"replay".starts_with(&word) {
        return None;
    }
    let what = words.next().map(str::to_ascii_lowercase);
    Some(match what.as_deref() {
        None | Some("tells") => "tell",
        Some("group") => "gtell",
        Some("says") => "say",
        Some("cabal") => "cabal",
        Some("clan") => "clan",
        Some("faction") => "faction",
        Some("new" | "newbie") => "newbie",
        Some("imm") => "immortal",
        Some("imp") => "imp",
        // `replay clear` and a word the game does not know print one
        // line of their own.
        Some(_) => return None,
    })
}

/// True when `plain` is the line the game prints for a tell you send,
/// `You tell Tolliver '…'`, `You project to Tolliver in elvish '…'`, and
/// not one to `your group`, which is a gtell.
fn sent_tell(plain: &str) -> bool {
    let Some(rest) = plain
        .strip_prefix("You tell ")
        .or_else(|| plain.strip_prefix("You project to "))
    else {
        return false;
    };
    !rest.starts_with("your group ") && rest.contains(" '") && plain.trim_end().ends_with('\'')
}
