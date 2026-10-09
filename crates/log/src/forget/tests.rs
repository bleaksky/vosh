//! Tests for the password wipe. This file holds the made up secrets and
//! the session rows the replay tests and the store tests share.

mod export;
mod replay;
mod wipe;

// Made up secrets. None of them is anyone's password, and no test
// prints one: failures name rows by index or say what changed.
const SECRET_ACCOUNT: &str = "Zq7vellumSparrow";
const SECRET_IMM: &str = "Kx4tallowHeron";
const SECRET_NEW: &str = "Pw9cinderLark";
const SECRET_OLD: &str = "Mn2quartzWren";

/// One row as Vosh logs it. The prompt the game leaves on the input
/// line never reaches the log, only the line break that starts it,
/// which lands as an empty output row.
#[derive(Clone, Copy)]
enum Row<'a> {
    Out(&'a str),
    Sent(&'a str),
    /// A line that went out in the same write as the sent line
    /// before it, as the halves of a line split at `;` do.
    Also(&'a str),
}
use Row::{Also, Out, Sent};

const TAGLINE: &str = "Abandon hope, all ye who enter here...";
const MENU_OPTIONS: &str =
    " [#] Play   [N]ew character   [L]ink character   [P]assword   [D]isconnect";
const MOTD: &str = "Prepare yourself. For you are about to <Enter> the Forsaken Lands!";

/// The greeting the game shows on connect, ending on the main menu.
fn greeting() -> Vec<Row<'static>> {
    vec![
        Out(""),
        Out(":: connected. play.theforsakenlands.com | 1848"),
        Out(""),
        Out("           T H E   F O R S A K E N   L A N D S"),
        Out(""),
        Out("            open for play. welcome."),
        Out(""),
        Out(TAGLINE),
    ]
}

/// The account menu the game shows once your account password is
/// right, ending where it waits at `Your choice>`.
fn account_menu() -> Vec<Row<'static>> {
    vec![
        Out(""),
        Out(" Account: Tester"),
        Out(" ──────────────────────────────"),
        Out("  1. Quill      level 12 human warrior"),
        Out("  2. Vellin     level 51 elf immortal"),
        Out(""),
        Out(MENU_OPTIONS),
        Out(""),
    ]
}

/// Rows for a session, in order, from pieces.
fn session<'a>(pieces: &[&[Row<'a>]]) -> Vec<Row<'a>> {
    pieces.iter().flat_map(|p| p.iter().copied()).collect()
}

/// Enter from the main menu, then an account name and its password.
fn login(account_password: &str) -> Vec<Row<'_>> {
    vec![
        Sent("e"),
        Out(""),
        Sent("tester"),
        Out(""),
        Sent(account_password),
    ]
}
