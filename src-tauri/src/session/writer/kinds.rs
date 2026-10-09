//! The kinds of text the game's editor holds that Vosh names, one row
//! each. Every kind goes through the game's one line editor
//! (`string_append`, `olc.c:3383`), and a row says how the card opens
//! it, which line the game prints before its banner, the fields it sets
//! at the game's prompt first, how it reads the text back and how it
//! ends. The card takes every kind but a tome, a cabal vote, paper and a
//! pet's description, which get only the count on the command line
//! until the card takes them too (Note Editor Q4).

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
    /// The text of the tome you scribe, `scribe text` (`tome.c:931`).
    Tome,
    /// The text of the cabal vote you draft, `vote edit` (`vote.c:1427`).
    Vote,
    /// What you write on the notepaper you hold, `write edit` and a
    /// language (`languages.c:2427`).
    Paper,
    /// Your pet's description, `petedit desc` (`magic5.c:857`).
    Pet,
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
    /// The card takes the text. The rest get only the command line's
    /// count.
    pub(crate) fn card(self) -> bool {
        !matches!(self, Kind::Tome | Kind::Vote | Kind::Paper | Kind::Pet)
    }

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
            Kind::Tome => "scribe text".to_string(),
            Kind::Vote => "vote edit".to_string(),
            Kind::Paper => "write edit".to_string(),
            Kind::Pet => "petedit desc".to_string(),
            board => format!("{} edit", board.board().unwrap_or("note")),
        }
    }

    /// The lines the game prints before the banner, one of which names
    /// the text (`act_comm.c:5111` to `5123`, `tome.c:937`), or the start
    /// of them for a beast, whose line names your beast
    /// (`act_info.c:7659`), and for paper, whose line names the language
    /// and the paper (`languages.c:2472`, `2482`, `2485`). Empty for a
    /// kind the game opens on its banner alone.
    pub(crate) fn names_itself(self) -> &'static [&'static str] {
        match self {
            Kind::Beast => &["Remember, your beast is "],
            Kind::History => &["Editing your HISTORY.."],
            Kind::Personality => &["Editing your PERSONALITY.."],
            Kind::Purpose => &["Editing your PURPOSE.."],
            Kind::Tome => &["Enter the contents of the tome."],
            Kind::Paper => &[
                "You begin writing in ",
                "You continue writing in ",
                "You decide to write in ",
            ],
            _ => &[],
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
