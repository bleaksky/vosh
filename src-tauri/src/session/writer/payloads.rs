//! What the page and the writer say to each other about a job: what the
//! page asks for on `writing_start`, and how the job stands and ends on
//! `session://writing`.

use serde::{Deserialize, Serialize};

use super::game_text::ShownNote;
use super::kinds::Kind;

/// What the card asks the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    /// Read the text the game holds.
    Read,
    /// Send the text through the game's editor, for a text the game
    /// saves in place.
    Send,
    /// Set a note's fields, send its text and post it.
    Post,
    /// Send the text for its review, `dcheck` or `history check`.
    Check,
    /// Clear the note the game holds.
    Clear,
    /// Send lines into the editor you opened yourself, each on the
    /// game's `> `.
    Paste,
}

/// What the page asks for, on `writing_start`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WriteJob {
    /// The page's number for the job, which its result carries.
    pub(crate) id: u64,
    pub(crate) kind: Kind,
    pub(crate) action: Action,
    /// The text, one line each, codes as typed.
    #[serde(default)]
    pub(crate) lines: Vec<String>,
    #[serde(default)]
    pub(crate) to: String,
    #[serde(default)]
    pub(crate) subject: String,
    #[serde(default)]
    pub(crate) language: Option<String>,
    /// The game's copy the draft began from. A send that finds another
    /// text in the game stops to ask first.
    #[serde(default)]
    pub(crate) base: Option<Vec<String>>,
    /// The note the game holds is this one, opened from the game, so a
    /// post goes on without asking.
    #[serde(default)]
    pub(crate) adopt: bool,
    /// Clear the note the game holds first, which you agreed to.
    #[serde(default)]
    pub(crate) clear_first: bool,
    /// Your character's name, which `show` puts before the subject.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// Your trust is 55 or more, so the game keeps a code anywhere in a
    /// line (`comm.c:1499`).
    #[serde(default)]
    pub(crate) immortal: bool,
}

/// A field of a note, or the editor, where the game refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Field {
    To,
    Subject,
    Language,
    Editor,
    Post,
}

/// Why a job failed, which the page says in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Why {
    /// A line still differs after the card mended it.
    Mend,
    /// No mark `.rl` takes fits around the line.
    NoMark,
    /// The editor said a dot command was wrong.
    BadDot,
    /// The editor closed before the card was done.
    Closed,
    /// The game stopped answering.
    Silent,
    /// The game's answer was not the text the card asked for.
    Unread,
    /// The note the game shows differs from yours, so the card did not
    /// post it.
    Differs,
}

/// How a job ended, for the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum JobResult {
    /// The text the game holds, and for a beast the beast `beastdesc
    /// edit` names, and for a board the note it holds.
    Read {
        lines: Vec<String>,
        beast: Option<String>,
        note: Option<ShownNote>,
    },
    /// The game holds another text than the one your draft began from,
    /// so the card left the editor at once.
    Changed {
        lines: Vec<String>,
    },
    /// The game holds your text, as it reads back, and `restore` is what
    /// it held before.
    Sent {
        lines: Vec<String>,
        restore: Option<Vec<String>>,
    },
    /// The note posted. `forum` is false when the game said the forum
    /// missed a report, and `vote` true when an application went to a
    /// cabal's vote.
    Posted {
        forum: bool,
        vote: bool,
    },
    /// The game's answer to `dcheck` or `history check`.
    Checked {
        lines: Vec<String>,
    },
    Cleared,
    Pasted,
    /// The game said no to a field, the editor or the post, in `line`.
    Refused {
        field: Field,
        line: String,
    },
    /// The board holds another note of yours.
    SameNote {
        note: ShownNote,
    },
    /// Another board holds a note you started, shown here when the card
    /// found it.
    OtherNote {
        board: Option<Kind>,
        note: Option<ShownNote>,
    },
    /// The game waits in a line editor, so the card sent nothing.
    Busy,
    /// The text ran past what the editor holds, after `sent` lines.
    TooLong {
        sent: usize,
    },
    Failed {
        why: Why,
        line: Option<usize>,
    },
    /// You stopped it after `sent` lines.
    Stopped {
        sent: usize,
    },
    /// The link dropped after `sent` lines. `posted` says the post went
    /// out first, so only the board can say whether it took.
    Dropped {
        sent: usize,
        posted: bool,
    },
    /// The offer went before you took it.
    OfferGone,
}

/// Where a job stands, for the page's progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JobProgress {
    pub(crate) id: u64,
    pub(crate) kind: Kind,
    pub(crate) action: Action,
    pub(crate) stage: Shown,
    /// Lines of your text the game took.
    pub(crate) sent: usize,
    /// Lines of your text in all.
    pub(crate) total: usize,
}

/// The stage a job is in, as the page names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Shown {
    Waiting,
    Reading,
    Fields,
    Opening,
    Sending,
    Checking,
    Closing,
    Posting,
}
