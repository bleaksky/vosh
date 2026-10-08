//! The session log in logs.sqlite, with no app state.
//!
//! [`LogStore`] holds the connection, and each module below adds the
//! methods for its job.
//!
//! `sqlite` opens the file in WAL mode and creates or upgrades the tables.
//! `sessions` starts, ends and lists sessions, writes each game line with
//! its plain text and its raw bytes, and exports a session. It also owns
//! the `> ` rows that record what you sent.
//! `search` runs the regex search the log view pages through.
//! `lookup` reads the sessions that belong to a profile's characters.
//! `forget` finds the lines where you sent a password and blanks them for
//! good.

use rusqlite::Connection;
use thiserror::Error;

mod forget;
mod lookup;
mod search;
mod sessions;
mod sqlite;

pub use forget::{Forgotten, PasswordLines};
pub use lookup::{CharacterScope, ScopedLine, ScopedSession};
pub use search::{SearchHit, SearchOptions, SearchPage};
pub use sessions::{sent_entries, sent_rows, snoop_rows, LogEntry, SessionRow, HIDDEN_SENT_TEXT};

#[derive(Debug, Error)]
pub enum LogError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),
}

pub type Result<T> = std::result::Result<T, LogError>;

/// One connection to logs.sqlite. Its methods live in the modules the
/// crate doc lists, and the password wipe's in `forget/wipe.rs`.
pub struct LogStore {
    conn: Connection,
}
