//! SQLite-backed log store for Vosh sessions.
//!
//! Each connection opens a `sessions` row, every server line lands in
//! `log_lines` with both its plain-text form (ANSI stripped) and the
//! original ANSI-bearing bytes, and search runs as a regex scan over
//! the `text` column. Export reproduces a session as plain text or with
//! ANSI codes restored.

use rusqlite::Connection;
use thiserror::Error;

mod forget;
mod lookup;
mod search;
mod sessions;
mod sqlite;

pub use forget::{Forgotten, PasswordLines, HIDDEN_SENT_TEXT};
pub use lookup::{CharacterScope, ScopedLine, ScopedSession};
pub use search::{SearchHit, SearchOptions, SearchPage};
pub use sessions::{LogEntry, SessionRow};

#[derive(Debug, Error)]
pub enum LogError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("regex: {0}")]
    Regex(#[from] regex::Error),
}

pub type Result<T> = std::result::Result<T, LogError>;

pub struct LogStore {
    conn: Connection,
}
