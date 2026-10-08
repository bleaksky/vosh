//! What each log row is, as the session tells it apart when it writes
//! the row, so Save a scene can leave out your prompt, your commands,
//! the lines outside play and the channels you pick (Q9 of the Alerts
//! and Scenes review). Two columns of `log_lines` hold it, `kind` and
//! `channel`. A row an older build wrote has neither, and a scene reads
//! it by its text instead.

use serde::Serialize;

/// What a log row is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "channel", rename_all = "snake_case")]
pub enum LineKind {
    /// A line of the game that is none of the others.
    #[default]
    Text,
    /// A line of your prompt, as the prompt stage reads it.
    Prompt,
    /// A line you sent, the `> ` row.
    Sent,
    /// A line outside play, before the game says you play and after you
    /// step away to the account menu, and the lines you sent there.
    Login,
    /// A line said on a channel, named as the game's Comm.Channel names
    /// it, such as `say` or `tell`.
    Channel(String),
}

/// The `kind` column for each kind. The channel goes in a column of its
/// own, so a long log spends one byte on the kind.
const TEXT: i64 = 0;
const PROMPT: i64 = 1;
const SENT: i64 = 2;
const LOGIN: i64 = 3;
const CHANNEL: i64 = 4;

impl LineKind {
    /// The two columns that store this kind.
    pub(crate) fn columns(&self) -> (i64, Option<&str>) {
        match self {
            Self::Text => (TEXT, None),
            Self::Prompt => (PROMPT, None),
            Self::Sent => (SENT, None),
            Self::Login => (LOGIN, None),
            Self::Channel(name) => (CHANNEL, Some(name)),
        }
    }

    /// The kind two stored columns hold, or None for a row an older build
    /// wrote, which stored no kind. A number this build does not know
    /// reads as plain text.
    pub(crate) fn from_columns(kind: Option<i64>, channel: Option<String>) -> Option<Self> {
        Some(match kind? {
            PROMPT => Self::Prompt,
            SENT => Self::Sent,
            LOGIN => Self::Login,
            CHANNEL => Self::Channel(channel.unwrap_or_default()),
            _ => Self::Text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_reads_back_from_its_columns() {
        for kind in [
            LineKind::Text,
            LineKind::Prompt,
            LineKind::Sent,
            LineKind::Login,
            LineKind::Channel("tell".into()),
        ] {
            let (number, channel) = kind.columns();
            let back = LineKind::from_columns(Some(number), channel.map(str::to_string));
            assert_eq!(back, Some(kind));
        }
        assert_eq!(LineKind::from_columns(None, None), None);
    }
}
