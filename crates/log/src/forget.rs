//! Find the lines in a session log where you sent a password, and blank
//! them for good.
//!
//! # Which lines
//!
//! A line you sent is a `> ` row with no raw bytes. It holds a password
//! when any of these is true.
//!
//! 1. The nearest earlier row in its session, by id, is game output that
//!    reads as a password prompt ([`is_password_prompt`](aabahran::is_password_prompt)).
//! 2. The game's login was waiting for a password when you sent it.
//! 3. It is a game command that takes a password as its argument, like
//!    `password <old> <new>`, or an implementor command that sets one,
//!    `account password <account> <new>` or `set char <name> pwdreset
//!    <new>`. The commands and their shortest forms come from the game's
//!    command table in interp.c, and the subcommands and fields from the
//!    order `do_account` and `do_mset` check them in.
//! 4. It went out in the same write as a line blanked by rules 1 to 3,
//!    with no game output between. The input pipeline used to split a
//!    typed line at `;`, and each piece of a password landed as its own
//!    row with the same time.
//!
//! Rule 1 rarely fires on its own, because Vosh never logs a prompt. The
//! game leaves `Password> ` on the input line without a line end, the
//! session drops that partial line when you send, and all the log keeps
//! is the empty row the prompt's leading line break makes. So rule 2
//! replays the login the game runs (comm.c `char_gen`, the prompts in
//! tables.c `chargen_table`) from the rows the log does keep.
//!
//! * The line `Abandon hope, all ye who enter here...` means the main
//!   menu waits. `e` or `l` asks for an account name and then, with echo
//!   off, `Password> `. `c` asks for a new account name, then `New
//!   password> ` and `Confirm password> `.
//! * The account menu, whose last line starts `[#] Play` and ends
//!   `[D]isconnect`, waits for a choice. `p` asks `Current password> `,
//!   `New password> `, and `Confirm new password> `. `l` asks for a
//!   character name, then `Character password> `, and for an immortal
//!   `Immortal password> ` or `Set immortal password> ` and `Confirm
//!   immortal password> `. `n` asks for a name, then `Did I get that
//!   right`, and `y` asks `Enter password for <name>>` and `Retype
//!   Password>`. A number picks a character, and an immortal one asks
//!   `Imm-Password?> ` with nothing else printed.
//! * `That character is already playing.` asks `Connect anyway?`. `n`, or
//!   `Reconnect attempt failed.`, asks for a name the old way, then
//!   `Password?> `, and an immortal is asked `Imm-Password?> ` next.
//!
//! A password prompt comes with nothing printed since your last line but
//! empty rows and a few known lead ins, like `NULL password, resetting to
//! player name...`. Any other line means the game turned the name down
//! or moved on, and the replay stops expecting a password.
//!
//! The game asks twice in a row for a new password and its confirmation,
//! so the line after a blanked one is blanked too while a confirmation is
//! due. `Password must be at least five characters.` and the `must
//! differ` lines ask the same prompt again. `Passwords don't match.` goes
//! back to the new password and its confirmation. Each try is blanked.
//!
//! The pieces of a split line reach the game one per pulse, and it
//! answers each on its own. A piece that finds no password due still
//! answers the prompt the game asks once it turns down the piece before,
//! so the replay counts those pieces and plays them again from there. The
//! login sends no GA, so a prompt the game prints between two pieces
//! stays a partial line until the next output ends it. It lands in the
//! log as a row of its own, which keeps the wait, or joined to the front
//! of the next line, which the replay reads without it.
//!
//! The pick of an immortal is the one prompt that looks like nothing at
//! all, and a character left link dead in the game reconnects from the
//! pick just as quietly. So the line after a pick is blanked only once the
//! game goes on the way it does after `Imm-Password?> `. That is the
//! message of the day line `Prepare yourself...`, `Login failed.`,
//! `Reconnecting...`, or `That character is already playing.`, or the
//! session ending with nothing printed after it. A game reply to an
//! ordinary command keeps the line.

use std::collections::BTreeSet;

mod aabahran;
mod replay;
mod wipe;

/// Sent lines that still hold a password. Row ids only, never text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PasswordLines {
    /// `(line id, session id)` for each line, oldest first.
    pub lines: Vec<(i64, i64)>,
    /// An earlier run blanked lines but could not clear the old copies
    /// of their text from the file. The next blanking run finishes it.
    pub wipe_pending: bool,
}

impl PasswordLines {
    /// How many lines.
    pub fn count(&self) -> usize {
        self.lines.len()
    }

    /// How many sessions hold at least one of the lines.
    pub fn sessions(&self) -> usize {
        self.lines
            .iter()
            .map(|(_, sid)| *sid)
            .collect::<BTreeSet<_>>()
            .len()
    }
}

/// What blanking did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Forgotten {
    /// Lines whose text Vosh replaced with [`HIDDEN_SENT_TEXT`](crate::HIDDEN_SENT_TEXT).
    pub lines: usize,
    /// Sessions those lines belong to.
    pub sessions: usize,
    /// An earlier run left old copies of the lines it blanked in the
    /// file, so this run rewrote the file for them too.
    pub resumed: bool,
    /// No old copy of a blanked line is left on disk. The file was
    /// rewritten and its write ahead log emptied, or nothing was due.
    pub wiped: bool,
}

#[cfg(test)]
mod tests;
