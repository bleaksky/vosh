//! The kinds of text the writing card takes, one row each (Note Editor
//! Q13). Every kind goes through the game's one line editor
//! (`string_append`, `olc.c:3383`), and a row says how the card opens
//! it, which line the game prints before its banner, the fields it sets
//! at the game's prompt first, how it reads the text back and how it
//! ends.

use serde::{Deserialize, Serialize};

/// A text the game's editor holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    /// Your description, `description edit` (`act_info.c:7550`).
    Description,
    /// A werebeast's beast description, `beastdesc edit`
    /// (`act_info.c:7656`).
    Beast,
    /// Your history, `history edit` (`act_comm.c:5109`).
    History,
    /// Your personality, `history personality`.
    Personality,
    /// Your purpose, `history purpose`.
    Purpose,
    /// A note on one of the boards `parse_note` runs (`recycle.c:3421`).
    Note,
    Journal,
    Application,
    Idea,
    Bug,
    Typo,
    /// The staff boards, for an immortal at the level each needs.
    News,
    Changes,
    Penalty,
}

/// Every board in the order the card asks them for a note in progress.
pub(crate) const BOARDS: [Kind; 9] = [
    Kind::Note,
    Kind::Journal,
    Kind::Application,
    Kind::Idea,
    Kind::Bug,
    Kind::Typo,
    Kind::News,
    Kind::Changes,
    Kind::Penalty,
];

impl Kind {
    /// The board's command, for a kind `parse_note` holds.
    pub(crate) fn board(self) -> Option<&'static str> {
        match self {
            Kind::Note => Some("note"),
            Kind::Journal => Some("journal"),
            Kind::Application => Some("application"),
            Kind::Idea => Some("idea"),
            Kind::Bug => Some("bug"),
            Kind::Typo => Some("typo"),
            Kind::News => Some("news"),
            Kind::Changes => Some("changes"),
            Kind::Penalty => Some("penalty"),
            _ => None,
        }
    }

    /// The command that opens the game's editor on the text, every word
    /// in full, as the game wants `edit` and the history words.
    pub(crate) fn opener(self) -> String {
        match self {
            Kind::Description => "description edit".to_string(),
            Kind::Beast => "beastdesc edit".to_string(),
            Kind::History => "history edit".to_string(),
            Kind::Personality => "history personality".to_string(),
            Kind::Purpose => "history purpose".to_string(),
            board => format!("{} edit", board.board().unwrap_or("note")),
        }
    }

    /// The line the game prints before the banner, which names the text
    /// (`act_comm.c:5111` to `5123`), or the start of it for a beast,
    /// whose line names your beast (`act_info.c:7659`).
    pub(crate) fn names_itself(self) -> Option<&'static str> {
        match self {
            Kind::Beast => Some("Remember, your beast is "),
            Kind::History => Some("Editing your HISTORY.."),
            Kind::Personality => Some("Editing your PERSONALITY.."),
            Kind::Purpose => Some("Editing your PURPOSE.."),
            _ => None,
        }
    }

    /// The game turns any To into Immortal for these and takes nothing
    /// but `immortal` there (`recycle.c:4450` to `4504`).
    pub(crate) fn to_immortal(self) -> bool {
        matches!(self, Kind::Journal | Kind::Idea | Kind::Bug | Kind::Typo)
    }

    /// Only a note on the note board can be written in a language
    /// (`recycle.c:4420`).
    pub(crate) fn takes_language(self) -> bool {
        self == Kind::Note
    }

    /// The command that reads the text back once the editor closes, for
    /// a kind the game saves in place. A history reads back with `.s`
    /// before `@`, since `history show` prints all three texts and cuts
    /// a long one short (`comm.c:7476`).
    pub(crate) fn read_back(self) -> Option<&'static str> {
        match self {
            Kind::Description => Some("description"),
            Kind::Beast => Some("beastdesc"),
            _ => None,
        }
    }

    /// The command that sends the text for a review, once.
    pub(crate) fn check(self) -> Option<&'static str> {
        match self {
            Kind::Description => Some("dcheck"),
            Kind::History => Some("history check"),
            _ => None,
        }
    }
}
