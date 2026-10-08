//! The session log in logs.sqlite, with no app state.
//!
//! [`LogStore`] holds the connection, and each module below adds the
//! methods for its job.
//!
//! `sqlite` opens the file in WAL mode and creates or upgrades the tables.
//! `sessions` starts, ends and lists sessions, writes each game line with
//! its plain text and its raw bytes, and exports the lines of a scope.
//! It also owns the `> ` rows that record what you sent.
//! `kind` names what each row is, your prompt, a line you sent, a line
//! outside play or a channel's line, as the session tags it.
//! `scene` reads the rows of one log over a span of time for Save a
//! scene, each with its kind.
//! `search` runs the regex search the log view pages through, over the
//! logs a `Scope` names.
//! `lookup` reads the sessions that belong to a profile's characters.
//! `retention` deletes whole logs past Keep logs for and gives the space
//! back a little at a time.
//! `forget` finds the lines where you sent a password and blanks them for
//! good.

use rusqlite::Connection;
use thiserror::Error;

mod forget;
mod kind;
mod lookup;
mod retention;
mod scene;
mod search;
mod sessions;
mod sqlite;

pub use forget::{Forgotten, PasswordLines};
pub use kind::LineKind;
pub use lookup::{CharacterScope, ScopedLine, ScopedSession};
pub use scene::{SceneLine, SceneLog, ScopedLogSpan};
pub use search::{Scope, SearchHit, SearchOptions, SearchPage};
pub use sessions::{
    is_local_host, sent_entries, sent_rows, snoop_rows, LogEntry, SessionRow, HIDDEN_SENT_TEXT,
};

#[derive(Debug, Error)]
pub enum LogError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("write: {0}")]
    Write(#[from] std::io::Error),
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),
    /// A search stopped because a newer one replaced it.
    #[error("stopped: a newer search replaced this one")]
    Stopped,
}

pub type Result<T> = std::result::Result<T, LogError>;

/// One connection to logs.sqlite. Its methods live in the modules the
/// crate doc lists, and the password wipe's in `forget/wipe.rs`.
pub struct LogStore {
    conn: Connection,
}
