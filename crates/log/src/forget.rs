//! Find the lines in a session log where you sent a password, and blank
//! them for good.
//!
//! # Which lines
//!
//! A line you sent is a `> ` row with no raw bytes. It holds a password
//! when any of these is true.
//!
//! 1. The nearest earlier row in its session, by id, is game output that
//!    reads as a password prompt ([`is_password_prompt`]).
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

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, TransactionBehavior};

use crate::{LogStore, Result};

/// The text a blanked sent line keeps. The session log writes the same
/// row for a line you send while the server hides your input, so a
/// blanked line reads like one that was never saved.
pub const HIDDEN_SENT_TEXT: &str = "> (hidden)";

/// A table that says an earlier run blanked lines and has not yet cleared
/// the old copies of their text from the file. The blanking transaction
/// creates it, so it outlasts a quit, a full disk, or a busy checkpoint
/// between the update and the rebuild. Only a finished rebuild drops it,
/// and until then every blanking run rebuilds the file again.
const WIPE_PENDING_TABLE: &str = "forget_passwords_wipe_pending";

/// The line the game prints right above the main menu.
const MAIN_MENU: &str = "Abandon hope, all ye who enter here...";
/// The account menu's option line starts and ends with these.
const ACCOUNT_MENU_START: &str = "[#] Play";
const ACCOUNT_MENU_END: &str = "[D]isconnect";
/// Lines the game prints after taking an answer at `Imm-Password?> `.
const AFTER_IMM_PASSWORD: [&str; 4] = [
    "Prepare yourself. For you are about to <Enter> the Forsaken Lands!",
    "Login failed.",
    "Reconnecting. Type replay to see missed tells.",
    ALREADY_PLAYING,
];
const ALREADY_PLAYING: &str = "That character is already playing.";
const RECONNECT_FAILED: &str = "Reconnect attempt failed.";
/// Printed before `Password?> ` for a character with no password.
const NULL_PASSWORD: &str = "NULL password, resetting to player name...";
/// Printed after the pick of an immortal with no immortal password.
const IMM_PASSWORD_NEEDED: &str = "This immortal character requires an immortal password.";
/// After a password, the game asks the same prompt again.
const ASK_AGAIN: [&str; 4] = [
    "Password must be at least five characters.",
    "Password must be at least five characters long.",
    "Immortal password must differ from your account password.",
    "Immortal password must differ from the character password.",
];
/// After a confirmation, the game asks for the new password again.
const MISMATCH: [&str; 2] = ["Passwords don't match.", "Passwords do not match."];
/// After a character password while linking an immortal.
const VERIFY_IMM: &str = "Immortal character detected. Verify immortal password.";
const SET_IMM: &str = "This immortal character requires an immortal password before linking.";

/// Game commands that take a password as an argument, each with the
/// shortest abbreviation the game's command table (interp.c) resolves to
/// it. `delete` takes the deletion password once one is set.
const PASSWORD_COMMANDS: [(&str, usize); 5] = [
    ("password", 2),
    ("acctpassword", 4),
    ("delpassword", 4),
    ("immpass", 4),
    ("delete", 6),
];

/// `account password <account> <new password>` (`act_info.c`
/// `do_account`). `acco` is the shortest form of `account` in interp.c,
/// and `do_account` reads `p` as `password`, since no subcommand it
/// checks first starts with `p`.
const ACCOUNT: (&str, usize) = ("account", 4);
const ACCOUNT_PASSWORD: (&str, usize) = ("password", 1);

/// `set char <name> pwdreset <new password>` and the same with
/// `immpwdreset` (`act_wiz.c` `do_set`, then `do_mset`). `do_set` reads
/// any prefix of `mob` or `char` as the character form. `do_mset` tries
/// its fields in order, so `p` reaches `pkstrip` first and `pw` is the
/// shortest `pwdreset`, while no field before `immpwdreset` starts with
/// `i`.
const SET: (&str, usize) = ("set", 3);
const SET_CHARACTER: [(&str, usize); 2] = [("mob", 1), ("char", 1)];
const SET_PASSWORD: [(&str, usize); 2] = [("pwdreset", 2), ("immpwdreset", 1)];

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
    /// Lines whose text Vosh replaced with [`HIDDEN_SENT_TEXT`].
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

/// Every password prompt in the game's source, the account login in
/// tables.c and the older nanny login in comm.c.
const PASSWORD_PROMPT: &str = r"(?:
        (?:imm-)?password\??
      | (?:please\s+)?retype\s+password
      | (?:new|current|character|immortal|set\s+immortal
          |confirm|confirm\s+new|confirm\s+immortal)\s+password
      | enter\s+password\s+for\s+\S+
      | please\s+enter\s+your\s+imm\s+password
      | please\s+choose\s+a\s+password\s+for\s+\S+
      | please\s+enter\s+the\s+old\s+password\s+in\s+order\s+to\s+use\s+the\s+name\s+\S+
    )\s*[>:]";

/// [`PASSWORD_PROMPT`] at the start of a line, followed by `end`.
fn prompt_pattern(end: &str) -> Regex {
    RegexBuilder::new(&format!("^{PASSWORD_PROMPT}{end}"))
        .case_insensitive(true)
        .ignore_whitespace(true)
        .build()
        .expect("the password prompt pattern compiles")
}

/// True when `line`, one line of game output, is a prompt that asks for
/// a password. It covers every password prompt in the game's source, and
/// only a whole line shaped like one, so a channel line that mentions a
/// password never matches.
pub fn is_password_prompt(line: &str) -> bool {
    static WHOLE: OnceLock<Regex> = OnceLock::new();
    WHOLE
        .get_or_init(|| prompt_pattern("$"))
        .is_match(line.trim())
}

/// `line` without a password prompt at its front. The login sends no GA,
/// so a prompt the game prints between two pieces of a split line stays
/// a partial line until the next output ends it, and it lands in the log
/// as a row of its own or joined to the front of the next line.
fn after_password_prompt(line: &str) -> &str {
    static LEADING: OnceLock<Regex> = OnceLock::new();
    let trimmed = line.trim_start();
    match LEADING.get_or_init(|| prompt_pattern("")).find(trimmed) {
        Some(prompt) => trimmed[prompt.end()..].trim_start(),
        None => line,
    }
}

/// True when `word`, as typed, is a form of `name` the game accepts, at
/// least `shortest` letters of it in any case.
fn reads_as(word: &str, (name, shortest): (&str, usize)) -> bool {
    word.len() >= shortest
        && word.len() <= name.len()
        && name.as_bytes()[..word.len()].eq_ignore_ascii_case(word.as_bytes())
}

/// True when `answer`, a line as the game reads it, runs a command that
/// takes a password and gives it one. The player commands take it as
/// their first argument, the implementor commands as their last.
fn carries_password(answer: &str) -> bool {
    let words: Vec<&str> = answer.split_whitespace().collect();
    match words.as_slice() {
        [command, _, ..] if PASSWORD_COMMANDS.iter().any(|c| reads_as(command, *c)) => true,
        [command, sub, _account, _password, ..] => {
            let account = reads_as(command, ACCOUNT) && reads_as(sub, ACCOUNT_PASSWORD);
            let set = reads_as(command, SET)
                && words.len() >= 5
                && SET_CHARACTER.iter().any(|c| reads_as(sub, *c))
                && SET_PASSWORD.iter().any(|f| reads_as(words[3], *f));
            account || set
        }
        _ => false,
    }
}

fn is_account_menu(line: &str) -> bool {
    let line = line.trim();
    line.starts_with(ACCOUNT_MENU_START) && line.ends_with(ACCOUNT_MENU_END)
}

/// The game reads a lone `q` as quit at a name prompt.
fn is_quit(answer: &str) -> bool {
    answer.eq_ignore_ascii_case("q")
}

/// What the login waits for next, as far as the log shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// Play, or anything the replay does not follow.
    Nothing,
    MainMenu,
    AccountName,
    NewAccountName,
    AccountMenu,
    LinkName,
    CharacterName,
    ConfirmName,
    ConnectAnyway,
    EnterName,
    /// The next `left` lines you send answer password prompts.
    Password {
        left: u8,
        then: After,
        refused: Refused,
    },
    /// You picked a character. An immortal one asks `Imm-Password?> `
    /// with nothing printed first.
    MaybeImmPassword,
}

/// Where the login goes after the last password of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Nothing,
    MaybeImmPassword,
}

impl After {
    fn wait(self) -> Wait {
        match self {
            After::Nothing => Wait::Nothing,
            After::MaybeImmPassword => Wait::MaybeImmPassword,
        }
    }
}

/// Where the login goes when the game turns down the name that would
/// have led to a password prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refused {
    Nothing,
    NewAccountName,
    EnterName,
}

impl Refused {
    fn wait(self) -> Wait {
        match self {
            Refused::Nothing => Wait::Nothing,
            Refused::NewAccountName => Wait::NewAccountName,
            Refused::EnterName => Wait::EnterName,
        }
    }
}

/// `left` password answers due, then play.
fn passwords(left: u8) -> Wait {
    Wait::Password {
        left,
        then: After::Nothing,
        refused: Refused::Nothing,
    }
}

/// The last password prompt your lines answered, which the game's reply
/// can take back by asking again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answered {
    /// The wait that line answered.
    wait: Wait,
    /// Pieces that went out in the same write after it and found no
    /// password due. The game reads each piece as its own answer, so when
    /// it asks again, these answer the prompts it asks from there.
    ahead: u8,
}

/// A line sent right after a pick, blanked once the game shows it was
/// the answer to `Imm-Password?> `.
#[derive(Debug, Clone)]
struct Candidate {
    /// The line, and any pieces of it that went out in the same write.
    ids: Vec<i64>,
    /// The time of that write.
    ts: i64,
    /// No game output came since, so a line with the same time is a piece.
    open: bool,
    /// The game printed something else since.
    heard: bool,
}

/// One session's replay.
#[derive(Debug)]
struct Replay {
    wait: Wait,
    /// The nearest earlier row reads as a password prompt.
    after_prompt: bool,
    /// The password prompt your last lines answered, when they did.
    answered: Option<Answered>,
    candidate: Option<Candidate>,
    /// The time of the write your last line went out in, when that line
    /// was blanked and no game output came since.
    blank_write: Option<i64>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            wait: Wait::Nothing,
            after_prompt: false,
            answered: None,
            candidate: None,
            blank_write: None,
        }
    }
}

impl Replay {
    /// A menu line: the login waits at that menu whatever came before.
    fn enter(&mut self, wait: Wait) {
        self.wait = wait;
        self.answered = None;
        self.candidate = None;
    }

    /// Take one line as the answer to a password prompt, when one is due.
    /// True when it was.
    fn answer_password(&mut self) -> bool {
        let Wait::Password { left, then, .. } = self.wait else {
            return false;
        };
        self.answered = Some(Answered {
            wait: self.wait,
            ahead: 0,
        });
        self.wait = if left > 1 {
            Wait::Password {
                left: left - 1,
                then,
                refused: Refused::Nothing,
            }
        } else {
            then.wait()
        };
        true
    }

    /// The game turned an answer down and waits at `wait` again. The
    /// pieces sent ahead of its reply answer the prompts from there.
    fn ask_again(&mut self, wait: Wait, ahead: u8) {
        self.wait = wait;
        self.answered = None;
        for _ in 0..ahead {
            if !self.answer_password() {
                if let Some(answered) = self.answered.as_mut() {
                    answered.ahead = answered.ahead.saturating_add(1);
                }
            }
        }
    }

    /// One row of game output. Returns the candidate lines it confirms.
    fn output(&mut self, text: &str) -> Vec<i64> {
        let row = text.trim_end();
        self.after_prompt = is_password_prompt(row);
        // A prompt row asks for what the replay already waits for, and a
        // prompt joined to the next line leaves that line as the reply.
        let line = after_password_prompt(row);
        self.blank_write = None;
        if let Some(candidate) = self.candidate.as_mut() {
            candidate.open = false;
        }
        if line == MAIN_MENU {
            self.enter(Wait::MainMenu);
            return Vec::new();
        }
        if is_account_menu(line) {
            self.enter(Wait::AccountMenu);
            return Vec::new();
        }
        // The line break in front of a prompt, or the prompt itself.
        if line.trim_start().is_empty() {
            return Vec::new();
        }

        let mut confirmed = Vec::new();
        if AFTER_IMM_PASSWORD.contains(&line) {
            if let Some(candidate) = self.candidate.take() {
                confirmed = candidate.ids;
            }
        } else if let Some(candidate) = self.candidate.as_mut() {
            candidate.heard = true;
        }

        if line == ALREADY_PLAYING {
            self.wait = Wait::ConnectAnyway;
            self.answered = None;
            return confirmed;
        }
        if line == RECONNECT_FAILED {
            self.wait = Wait::EnterName;
            self.answered = None;
            return confirmed;
        }
        if ASK_AGAIN.contains(&line) {
            // The same prompt again. Answers sent ahead of the game's
            // reply each earn one of these lines, so with no answer left
            // to take back, one more password is due.
            match (self.answered.take(), self.wait) {
                (Some(answered), _) => self.ask_again(answered.wait, answered.ahead),
                (
                    None,
                    Wait::Password {
                        left,
                        then,
                        refused,
                    },
                ) => {
                    self.wait = Wait::Password {
                        left: left.saturating_add(1),
                        then,
                        refused,
                    };
                }
                (None, _) => {}
            }
            return confirmed;
        }
        if let Some(answered) = self.answered.take() {
            let again = if MISMATCH.contains(&line) || line == SET_IMM {
                Some(passwords(2))
            } else if line == VERIFY_IMM {
                Some(passwords(1))
            } else {
                None
            };
            if let Some(wait) = again {
                self.ask_again(wait, answered.ahead);
                return confirmed;
            }
        }
        self.wait = match self.wait {
            Wait::MaybeImmPassword if line == IMM_PASSWORD_NEEDED => passwords(2),
            Wait::MaybeImmPassword => Wait::Nothing,
            Wait::Password { .. } if line == NULL_PASSWORD => self.wait,
            Wait::Password { refused, .. } => refused.wait(),
            Wait::ConfirmName => Wait::CharacterName,
            other => other,
        };
        confirmed
    }

    /// One line you sent. Returns its id when it holds a password.
    fn sent(&mut self, id: i64, ts_ms: i64, text: &str) -> Option<i64> {
        // A line that went out in the same write as a blanked one is a
        // piece of it. The input pipeline used to split a password at `;`.
        let same_write = self.blank_write == Some(ts_ms);
        let hidden = text == HIDDEN_SENT_TEXT;
        // The game skips leading spaces before it reads an answer.
        let answer = text.strip_prefix("> ").unwrap_or(text).trim_start();
        let first = answer.chars().next().map(|c| c.to_ascii_uppercase());
        let after_prompt = std::mem::take(&mut self.after_prompt);
        if let Some(candidate) = self.candidate.as_mut() {
            if candidate.open && candidate.ts == ts_ms {
                if !hidden {
                    candidate.ids.push(id);
                }
                return None;
            }
        }
        // A line after a pick that the game never showed to be the
        // immortal password was an ordinary command.
        self.candidate = None;

        if self.answer_password() {
            self.blank_write = Some(ts_ms);
            return (!hidden).then_some(id);
        }
        let blank = after_prompt || same_write || carries_password(answer);
        // A piece of the last answer's write answers whatever the game
        // asks once it turns that answer down.
        match self.answered.as_mut() {
            Some(answered) if same_write => answered.ahead = answered.ahead.saturating_add(1),
            _ => self.answered = None,
        }
        self.wait = match self.wait {
            // Taken above.
            Wait::Password { .. } => self.wait,
            Wait::MaybeImmPassword => {
                if !hidden {
                    self.candidate = Some(Candidate {
                        ids: vec![id],
                        ts: ts_ms,
                        open: true,
                        heard: false,
                    });
                }
                Wait::Nothing
            }
            Wait::MainMenu => match first {
                Some('E' | 'L') => Wait::AccountName,
                Some('C') => Wait::NewAccountName,
                _ => Wait::Nothing,
            },
            Wait::NewAccountName if is_quit(answer) => Wait::Nothing,
            Wait::NewAccountName => Wait::Password {
                left: 2,
                then: After::Nothing,
                refused: Refused::NewAccountName,
            },
            Wait::AccountMenu => match first {
                Some('P') => passwords(3),
                Some('L') => Wait::LinkName,
                Some('N') => Wait::CharacterName,
                Some('D') => Wait::Nothing,
                Some(c) if c.is_ascii_digit() => Wait::MaybeImmPassword,
                _ => Wait::AccountMenu,
            },
            Wait::LinkName | Wait::CharacterName | Wait::EnterName if is_quit(answer) => {
                Wait::Nothing
            }
            // An account name that is too short or long, or a character
            // that cannot be linked, prints an error and the menu before
            // any prompt, which ends the wait.
            Wait::AccountName | Wait::LinkName => passwords(1),
            Wait::CharacterName => Wait::ConfirmName,
            Wait::ConfirmName if first == Some('Y') => passwords(2),
            Wait::ConfirmName => Wait::CharacterName,
            Wait::ConnectAnyway if first == Some('Y') => Wait::Nothing,
            Wait::ConnectAnyway => Wait::EnterName,
            Wait::EnterName => Wait::Password {
                left: 1,
                then: After::MaybeImmPassword,
                refused: Refused::EnterName,
            },
            Wait::Nothing => Wait::Nothing,
        };
        self.blank_write = blank.then_some(ts_ms);
        (blank && !hidden).then_some(id)
    }
}

/// Replays session logs row by row to find the lines where you sent a
/// password. See the module docs for the rule.
#[derive(Debug, Default)]
pub struct PasswordFinder {
    sessions: HashMap<i64, Replay>,
    found: Vec<(i64, i64)>,
}

impl PasswordFinder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one log row. Rows arrive in id order, and sessions may
    /// interleave. `sent` marks a line you sent (a `> ` row with no raw
    /// bytes). `ts_ms` is the row's time. The lines of one write to the
    /// game share it.
    pub fn row(&mut self, id: i64, session_id: i64, ts_ms: i64, text: &str, sent: bool) {
        let replay = self.sessions.entry(session_id).or_default();
        if sent {
            if let Some(line) = replay.sent(id, ts_ms, text) {
                self.found.push((line, session_id));
            }
        } else {
            for line in replay.output(text) {
                self.found.push((line, session_id));
            }
        }
    }

    /// The lines found, oldest first. A session whose log ends right
    /// after the line that followed a pick counts that line, since the
    /// game said nothing to show it was a command.
    pub fn finish(mut self) -> PasswordLines {
        for (sid, replay) in &self.sessions {
            if let Some(candidate) = &replay.candidate {
                if !candidate.heard {
                    self.found
                        .extend(candidate.ids.iter().map(|id| (*id, *sid)));
                }
            }
        }
        self.found.sort_unstable();
        self.found.dedup();
        PasswordLines {
            lines: self.found,
            wipe_pending: false,
        }
    }
}

impl LogStore {
    /// Find the sent lines that still hold a password. Reads only, one
    /// pass over the log in id order.
    pub fn find_password_lines(&self) -> Result<PasswordLines> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, ts_ms, text, raw IS NULL FROM log_lines ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        let mut finder = PasswordFinder::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let session_id: i64 = row.get(1)?;
            let ts_ms: i64 = row.get(2)?;
            let no_raw: bool = row.get(4)?;
            // Read the bytes as they are, so a line that is not valid
            // UTF-8 never stops the pass.
            let text = match row.get_ref(3)? {
                ValueRef::Text(bytes) | ValueRef::Blob(bytes) => String::from_utf8_lossy(bytes),
                _ => Cow::Borrowed(""),
            };
            let sent = no_raw && text.starts_with("> ");
            finder.row(id, session_id, ts_ms, &text, sent);
        }
        drop(rows);
        let mut found = finder.finish();
        found.wipe_pending = self.wipe_pending()?;
        Ok(found)
    }

    /// Replace the text of each line in `found` with [`HIDDEN_SENT_TEXT`]
    /// and wipe every old copy of it from the database file. Only sent
    /// lines change, and a line already blanked is not counted again.
    ///
    /// `secure_delete` zeroes the old bytes in the pages the update
    /// rewrites. A checkpoint moves those pages into the main file and
    /// truncates the write ahead log, which still holds the old pages.
    /// Then `VACUUM` rebuilds the file, because a page split from before
    /// this run can leave a stale copy of a row in free space no update
    /// reaches (the table's first page keeps its old rows when it
    /// splits), and a last checkpoint truncates the log again. The
    /// rebuild takes time in proportion to the log, so the app runs this
    /// off the input path.
    ///
    /// When the rebuild cannot finish, the lines stay blanked and a
    /// marker table stays behind. The next run then rebuilds the file
    /// even with no new line to blank, and reports it as `resumed`.
    pub fn blank_password_lines(&mut self, found: &PasswordLines) -> Result<Forgotten> {
        let before: i64 = self
            .conn
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
        self.conn.execute_batch("PRAGMA secure_delete = ON;")?;
        let outcome = self.blank_lines(found);
        let restore = match before {
            0 => "OFF",
            2 => "FAST",
            _ => "ON",
        };
        let restored = self
            .conn
            .execute_batch(&format!("PRAGMA secure_delete = {restore};"));
        let outcome = outcome?;
        restored?;
        Ok(outcome)
    }

    fn blank_lines(&mut self, found: &PasswordLines) -> Result<Forgotten> {
        let resumed = self.wipe_pending()?;
        let (lines, sessions) = self.blank_rows(found)?;
        let wiped = (lines == 0 && !resumed) || wipe(&self.conn);
        Ok(Forgotten {
            lines,
            sessions,
            resumed,
            wiped,
        })
    }

    /// The update itself, in one transaction that also leaves the marker
    /// of a wipe due. Returns how many lines it blanked and in how many
    /// sessions.
    fn blank_rows(&mut self, found: &PasswordLines) -> Result<(usize, usize)> {
        let mut lines = 0;
        let mut sessions = BTreeSet::new();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut stmt = tx.prepare(
                "UPDATE log_lines SET text = ?1
                 WHERE id = ?2 AND raw IS NULL AND substr(text, 1, 2) = '> ' AND text <> ?1",
            )?;
            for (id, session_id) in &found.lines {
                if stmt.execute(params![HIDDEN_SENT_TEXT, id])? > 0 {
                    lines += 1;
                    sessions.insert(*session_id);
                }
            }
        }
        if lines > 0 {
            tx.execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {WIPE_PENDING_TABLE} (id INTEGER PRIMARY KEY);"
            ))?;
        }
        tx.commit()?;
        Ok((lines, sessions.len()))
    }

    /// True when an earlier run blanked lines and could not clear the
    /// old copies of their text from the file.
    pub fn wipe_pending(&self) -> Result<bool> {
        let pending = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [WIPE_PENDING_TABLE],
            |r| r.get(0),
        )?;
        Ok(pending)
    }

    /// [`Self::find_password_lines`], then [`Self::blank_password_lines`].
    pub fn forget_passwords(&mut self) -> Result<Forgotten> {
        let found = self.find_password_lines()?;
        self.blank_password_lines(&found)
    }
}

/// Push every page into the main file and truncate the write ahead log.
/// False when another connection still reads an older snapshot.
fn checkpoint(conn: &Connection) -> bool {
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
        r.get::<_, i64>(0)
    })
    .is_ok_and(|busy| busy == 0)
}

/// Clear old copies of changed rows out of the file, then drop the
/// marker of a wipe due. True when no old copy is left.
fn wipe(conn: &Connection) -> bool {
    // The first checkpoint lands the zeroed pages even if the rebuild
    // below cannot run, for lack of disk space say.
    checkpoint(conn);
    let rebuilt = conn.execute_batch("VACUUM;").is_ok();
    // The rebuild writes the new file through the write ahead log, and
    // this checkpoint moves it in and truncates the log to nothing.
    if !(rebuilt && checkpoint(conn)) {
        return false;
    }
    // The file is clean now. A marker that fails to drop only means the
    // next run rebuilds the file once more.
    if conn
        .execute_batch(&format!("DROP TABLE IF EXISTS {WIPE_PENDING_TABLE};"))
        .is_ok()
    {
        checkpoint(conn);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Feed `rows` as session `sid` from id `first`, the way the log
    /// stores them. Each row gets its own time except an [`Also`] row,
    /// which shares the time of the line before it.
    fn feed(finder: &mut PasswordFinder, sid: i64, first: i64, rows: &[Row<'_>]) {
        let mut ts = 0;
        for (i, row) in rows.iter().enumerate() {
            let id = first + i as i64;
            match row {
                Out(text) => finder.row(id, sid, id, text, false),
                Sent(line) => {
                    ts = id;
                    finder.row(id, sid, ts, &format!("> {line}"), true);
                }
                Also(line) => finder.row(id, sid, ts, &format!("> {line}"), true),
            }
        }
    }

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

    /// Indexes of the rows the finder blanks when `rows` are one session.
    fn blanked(rows: &[Row<'_>]) -> Vec<usize> {
        let mut finder = PasswordFinder::new();
        feed(&mut finder, 7, 1, rows);
        finder
            .finish()
            .lines
            .iter()
            .map(|(id, sid)| {
                assert_eq!(*sid, 7, "a found line names another session");
                (*id - 1) as usize
            })
            .collect()
    }

    /// Index of the `n`th sent row (from zero) in `rows`.
    fn sent_at(rows: &[Row<'_>], n: usize) -> usize {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Sent(_) | Also(_)))
            .nth(n)
            .map(|(i, _)| i)
            .expect("no such sent row")
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

    // ---- the prompt test ----

    #[test]
    fn every_password_prompt_the_game_shows_reads_as_one() {
        // tables.c chargen_table, the prompts comm.c char_gen writes.
        for prompt in [
            "Password?> ",
            "Imm-Password?> ",
            "Enter password for Quill>",
            "Retype Password>",
            "Password> ",
            "New password> ",
            "Confirm password> ",
            "Character password> ",
            "Set immortal password> ",
            "Confirm immortal password> ",
            "Current password> ",
            "Confirm new password> ",
            "Immortal password> ",
            // comm.c nanny, the older login.
            "Password: ",
            "Please enter your Imm password: ",
            "Please enter the old password in order to use the name Quill: ",
            "Please choose a password for Quill: ",
            "Please retype password: ",
            "Retype password: ",
        ] {
            assert!(
                is_password_prompt(prompt),
                "{prompt:?} should read as a password prompt"
            );
        }
    }

    #[test]
    fn chatter_about_passwords_is_no_prompt() {
        for line in [
            "Bob gossips 'what is the password?'",
            "You say 'my password is safe'",
            "Wrong password.",
            "Wrong password.  Wait 10 seconds.",
            "Passwords don't match.",
            "New password set.",
            "Syntax: password <old> <new>.",
            "Password must be at least five characters.",
            "Invalid account name or password.",
            "Account password changed successfully.",
            " [#] Play   [N]ew character   [L]ink character   [P]assword   [D]isconnect",
            "Bob tells you 'type password> to see it'",
            "Your choice> ",
            "Account name> ",
            "",
        ] {
            assert!(
                !is_password_prompt(line),
                "{line:?} should not read as a password prompt"
            );
        }
    }

    // ---- the login replay ----

    #[test]
    fn the_account_password_is_blanked() {
        let rows = session(&[&greeting(), &login(SECRET_ACCOUNT), &account_menu()]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
    }

    #[test]
    fn a_wrong_account_password_and_the_retry_are_both_blanked() {
        let rows = session(&[
            &greeting(),
            &login("Wr0ngGuess"),
            &[
                Out(""),
                Out("Invalid account name or password."),
                Out("            open for play. welcome."),
                Out(""),
                Out(TAGLINE),
            ],
            &login(SECRET_ACCOUNT),
            &account_menu(),
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 5)]);
    }

    #[test]
    fn a_short_account_name_goes_back_to_the_menu_and_blanks_nothing() {
        let rows = session(&[
            &greeting(),
            &[
                Sent("e"),
                Out(""),
                Sent("ab"),
                Out("Account name must be 3-20 characters."),
                Out("            open for play. welcome."),
                Out(""),
                Out(TAGLINE),
                Sent("h"),
                Out(""),
                Sent("rules"),
            ],
        ]);
        assert_eq!(blanked(&rows), Vec::<usize>::new());
    }

    #[test]
    fn a_mortal_pick_goes_to_the_game_and_blanks_nothing_more() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("1"),
                Out("Welcome to the Forsaken Lands."),
                Out(""),
                Out(MOTD),
                Out(""),
                Out("The Temple Square"),
                Out(""),
                Sent("look"),
                Out("The Temple Square"),
                Sent("north"),
            ],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
    }

    #[test]
    fn the_immortal_password_after_a_pick_is_blanked() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Sent(SECRET_IMM),
                Out("Immortals, read the board before you build."),
                Out(""),
                Out(MOTD),
                Sent("look"),
            ],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
    }

    #[test]
    fn a_failed_immortal_password_is_blanked() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[Sent("2"), Out(""), Sent("Wr0ngImm"), Out("Login failed.")],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
    }

    #[test]
    fn a_quiet_reconnect_after_a_pick_keeps_the_first_command() {
        // A character left link dead in the game reconnects straight from
        // the pick with nothing but the prompt's line break, and the next
        // line is an ordinary command.
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("1"),
                Out(""),
                Sent("look"),
                Out("The Temple Square"),
                Out(""),
                Sent("north"),
            ],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
    }

    #[test]
    fn an_immortal_without_an_immortal_password_sets_one_twice() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Out("This immortal character requires an immortal password."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Out("Immortal password set and character linked successfully."),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        assert_eq!(
            blanked(&rows),
            vec![sent_at(&rows, 2), sent_at(&rows, 4), sent_at(&rows, 5)]
        );
    }

    #[test]
    fn a_new_account_blanks_the_password_and_its_confirmation() {
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
            ],
            &account_menu(),
            &[Sent("d"), Out("Goodbye.")],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 3)]);
    }

    #[test]
    fn a_taken_account_name_asks_for_another_name_first() {
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("taken"),
                Out("That account name is already taken."),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
            ],
            &account_menu(),
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 3), sent_at(&rows, 4)]);
    }

    #[test]
    fn a_short_or_mismatched_new_password_asks_again_and_every_try_is_blanked() {
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent("abc"),
                Out("Password must be at least five characters."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent("Typo9cinderLark"),
                Out("Passwords don't match."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("score")],
        ]);
        assert_eq!(
            blanked(&rows),
            (2..=6).map(|n| sent_at(&rows, n)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_password_change_from_the_account_menu_blanks_all_three() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("p"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Out("Account password changed successfully."),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        assert_eq!(
            blanked(&rows),
            vec![
                sent_at(&rows, 2),
                sent_at(&rows, 4),
                sent_at(&rows, 5),
                sent_at(&rows, 6)
            ]
        );
    }

    #[test]
    fn a_wrong_old_password_goes_back_to_the_menu() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("p"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Out("Wrong password."),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
    }

    #[test]
    fn linking_a_character_blanks_its_password() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("l"),
                Out(""),
                Sent("Quill"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Out("Character linked successfully."),
            ],
            &account_menu(),
            // An immortal asks for the immortal password too.
            &[
                Sent("l"),
                Out(""),
                Sent("Vellin"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Out("Immortal character detected. Verify immortal password."),
                Out(""),
                Sent(SECRET_IMM),
                Out(""),
                Out("Character linked successfully."),
            ],
            &account_menu(),
            // One with no immortal password sets one, twice.
            &[
                Sent("l"),
                Out(""),
                Sent("Maudrey"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Out("This immortal character requires an immortal password before linking."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Out("Immortal password set and character linked successfully."),
            ],
            &account_menu(),
            // A name that does not exist goes straight back to the menu.
            &[
                Sent("l"),
                Out(""),
                Sent("Nobody"),
                Out("That character doesn't exist."),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        let expected: Vec<usize> = [2, 5, 8, 9, 12, 13, 14]
            .iter()
            .map(|&n| sent_at(&rows, n))
            .collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn a_new_character_blanks_its_password_and_the_retype() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("n"),
                Out(""),
                Sent("Qu1ll"),
                Out("Illegal name, try another."),
                Out(""),
                Sent("Quilla"),
                Sent("y"),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent("Typo9cinderLark"),
                Out("Passwords do not match."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent("n"),
                Out(""),
                Sent("m"),
            ],
        ]);
        let expected: Vec<usize> = [2, 7, 8, 9, 10]
            .iter()
            .map(|&n| sent_at(&rows, n))
            .collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn connecting_past_a_character_already_playing_asks_for_its_password() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("1"),
                Out("That character is already playing."),
                Out(""),
                Sent("n"),
                Out(""),
                Sent("Quill"),
                Out(""),
                Sent(SECRET_OLD),
                Out("Welcome."),
                Out(MOTD),
                Sent("look"),
            ],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 6)]);
    }

    #[test]
    fn the_direct_login_asks_an_immortal_twice() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out("That character is already playing."),
                Out(""),
                Sent("y"),
                Out("Reconnect attempt failed."),
                Out(""),
                Sent("Vellin"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Sent(SECRET_IMM),
                Out("Immortals, read the board."),
                Out(MOTD),
                Sent("look"),
            ],
        ]);
        assert_eq!(
            blanked(&rows),
            vec![sent_at(&rows, 2), sent_at(&rows, 6), sent_at(&rows, 7)]
        );
    }

    #[test]
    fn a_password_command_in_the_game_is_blanked() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("1"),
                Out("Welcome."),
                Out(MOTD),
                Sent("password Mn2quartzWren Pw9cinderLark"),
                Out("New password set."),
                Sent("pa Mn2quartzWren Pw9cinderLark"),
                Sent("PASSW Mn2quartzWren"),
                Sent("acctpassword Mn2quartzWren Pw9cinderLark"),
                Sent("acct Mn2quartzWren Pw9cinderLark"),
                Sent("delpassword Mn2quartzWren Pw9cinderLark"),
                Sent("immpass Pw9cinderLark Pw9cinderLark"),
                Sent("delete Pw9cinderLark"),
                // No argument, or not a password command.
                Sent("password"),
                Sent("pat dog"),
                Sent("p Mn2quartzWren"),
                Sent("acc sword"),
                Sent("delet"),
                Sent("delete"),
                Sent("dele Mn2quartzWren"),
                Sent("say my password is safe"),
                Sent("practice"),
            ],
        ]);
        // The account password, then every password command from
        // `password` through `delete`. The pick in between keeps its text.
        let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11]
            .iter()
            .map(|&n| sent_at(&rows, n))
            .collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn an_implementor_command_that_sets_a_password_is_blanked() {
        // act_info.c do_account and act_wiz.c do_mset, reached through
        // do_set, take a new password as their last argument. The game
        // reads the command, the subcommand, and the field by prefix.
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Sent(SECRET_IMM),
                Out(MOTD),
                Sent("account password Tester Pw9cinderLark"),
                Out("Password for account 'Tester' has been changed."),
                Sent("acco p Tester Pw9cinderLark"),
                Sent("ACCOUNT Pass Tester Pw9cinderLark"),
                Sent("set char Quill pwdreset Pw9cinderLark"),
                Out("New password set."),
                Sent("set c Quill pw Pw9cinderLark"),
                Sent("set mob Vellin immpwdreset Pw9cinderLark"),
                Sent("SET M Vellin i Pw9cinderLark"),
                Sent("set cha Vellin immpw two words"),
                // Another subcommand or field, another command, or no
                // password given.
                Sent("account list"),
                Sent("account password Tester"),
                Sent("account link Quill Tester"),
                Sent("acc password Tester Pw9cinderLark"),
                Sent("set char Quill pwdreset"),
                Sent("set char Quill p 5"),
                Sent("set char Quill int 18"),
                Sent("set skill Quill pw 75"),
                Sent("set obj sword i 5"),
                Sent("se char Quill pwdreset Pw9cinderLark"),
                Sent("say set char Quill pwdreset is the syntax"),
            ],
        ]);
        // The account password, the immortal password, then the eight
        // commands from `account password` through `set cha`.
        let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11, 12]
            .iter()
            .map(|&n| sent_at(&rows, n))
            .collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn chatter_before_a_line_blanks_nothing() {
        let rows = vec![
            Out("Bob gossips 'what is the password?'"),
            Sent("laugh"),
            Out("Wrong password."),
            Sent("look"),
            Out("Password must be at least five characters."),
            Sent("north"),
            Out("   Abandon hope, all ye who enter here..."),
            Sent("east"),
            Out(""),
            Sent("look"),
            Out(""),
            Sent("score"),
        ];
        assert_eq!(blanked(&rows), Vec::<usize>::new());
    }

    #[test]
    fn a_line_right_after_a_logged_prompt_is_blanked() {
        // The prompt reaches the log when the game ends its line before
        // you answer. Any world, any prompt the game uses.
        let rows = vec![
            Out("By what name do you wish to be known?"),
            Sent("Quill"),
            Out("Password: "),
            Sent(SECRET_OLD),
            Out(""),
            Out("Welcome."),
            Sent("look"),
            Out("Enter password for Quill>"),
            Out("Something else first."),
            Sent("north"),
        ];
        assert_eq!(blanked(&rows), vec![3]);
    }

    #[test]
    fn a_line_already_hidden_is_not_counted_again() {
        let rows = session(&[
            &greeting(),
            &login("(hidden)"),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Sent("(hidden)"),
                Out(MOTD),
                Sent("look"),
            ],
        ]);
        assert_eq!(blanked(&rows), Vec::<usize>::new());
    }

    #[test]
    fn every_piece_of_a_password_split_at_a_semicolon_is_blanked() {
        // Before masked input skipped the pipeline, a `;` in a password
        // split it into lines the session logged together, one write,
        // one time. The game took the first piece as the answer.
        let rows = session(&[
            &greeting(),
            &[
                Sent("e"),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent("Zq7vellum"),
                Also("Sparrow"),
                Also("Branwick"),
                Out(""),
                Out("Invalid account name or password."),
                Out(TAGLINE),
                Sent("e"),
                // Its own write, so its own time.
                Sent("look"),
            ],
        ]);
        assert_eq!(
            blanked(&rows),
            vec![sent_at(&rows, 2), sent_at(&rows, 3), sent_at(&rows, 4)]
        );
    }

    #[test]
    fn a_short_new_password_split_in_two_asks_twice_and_every_try_is_blanked() {
        // The game reads each piece as its own answer and turns down both,
        // so it asks for the new password and its confirmation after.
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent("ab"),
                Also("cd"),
                Out("Password must be at least five characters."),
                Out(""),
                Out("Password must be at least five characters."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        let expected: Vec<usize> = (2..=5).map(|n| sent_at(&rows, n)).collect();
        assert_eq!(blanked(&rows), expected);
    }

    // The game reads one piece of a split line each pulse. The login sends
    // no GA, so a prompt it prints between two pieces stays a partial
    // line until the next pulse's output ends it. It then lands in the
    // log as a row of its own, or joined to the front of the next line.

    #[test]
    fn a_split_password_refused_at_the_confirmation_keeps_every_later_try_blanked() {
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("tester"),
                Out(""),
                // `ab` is too short, and `cdefgh` becomes the new password.
                Sent("ab"),
                Also("cdefgh"),
                Out("Password must be at least five characters."),
                Out(""),
                Out("New password> "),
                // `ab` fails the confirmation, and `cdefgh` becomes the
                // new password again.
                Sent("ab"),
                Also("cdefgh"),
                Out("Passwords don't match."),
                Out(""),
                Out("New password> "),
                // A confirmation that fails, then the password you keep,
                // twice.
                Sent("Fn7emberKite"),
                Out("Passwords don't match."),
                Out(""),
                Sent("Fn7emberKite"),
                Out(""),
                Sent("Fn7emberKite"),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        let expected: Vec<usize> = (2..=8).map(|n| sent_at(&rows, n)).collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn a_split_password_refused_in_a_password_change_keeps_every_later_try_blanked() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("p"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Sent("ab"),
                Also("cdefgh"),
                Out("Password must be at least five characters."),
                Out(""),
                Out("New password> "),
                Sent("ab"),
                Also("cdefgh"),
                Out("Passwords don't match."),
                Out(""),
                Out("New password> "),
                Sent("Fn7emberKite"),
                Out("Passwords don't match."),
                Out(""),
                Sent("Fn7emberKite"),
                Out(""),
                Sent("Fn7emberKite"),
                Out(""),
                Out("Account password changed successfully."),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11]
            .iter()
            .map(|&n| sent_at(&rows, n))
            .collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn a_prompt_joined_to_the_next_line_still_reads_as_the_reply() {
        // Both pieces are too short. The prompt after the first one joins
        // the reply to the second.
        let rows = session(&[
            &greeting(),
            &[
                Sent("c"),
                Out(""),
                Sent("tester"),
                Out(""),
                Sent("ab"),
                Also("cd"),
                Out("Password must be at least five characters."),
                Out(""),
                Out("New password> Password must be at least five characters."),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
            ],
            &account_menu(),
            &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
        ]);
        let expected: Vec<usize> = (2..=5).map(|n| sent_at(&rows, n)).collect();
        assert_eq!(blanked(&rows), expected);
    }

    #[test]
    fn an_immortal_password_split_in_two_is_blanked_whole() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Sent("Kx4tallow"),
                Also("Heron"),
                Out("Login failed."),
            ],
        ]);
        assert_eq!(
            blanked(&rows),
            vec![sent_at(&rows, 2), sent_at(&rows, 4), sent_at(&rows, 5)]
        );
    }

    #[test]
    fn a_session_that_ends_after_an_immortal_password_blanks_it() {
        let rows = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[Sent("2"), Out(""), Sent(SECRET_IMM)],
        ]);
        assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
    }

    #[test]
    fn sessions_replay_apart_even_when_their_rows_interleave() {
        let mut finder = PasswordFinder::new();
        let a = session(&[&greeting(), &login(SECRET_ACCOUNT)]);
        let b = session(&[&greeting(), &[Sent("e"), Out(""), Sent("look")]]);
        let mut id = 0;
        let mut expected = Vec::new();
        for i in 0..a.len().max(b.len()) {
            for (sid, rows) in [(1_i64, &a), (2_i64, &b)] {
                if let Some(row) = rows.get(i) {
                    id += 1;
                    feed(&mut finder, sid, id, &[*row]);
                    if sid == 1 && i == a.len() - 1 {
                        expected.push((id, sid));
                    }
                }
            }
        }
        let found = finder.finish();
        assert_eq!(found.lines, expected);
        assert_eq!(found.sessions(), 1);
    }

    // ---- the store ----

    /// A log file in a fresh temp folder, removed on drop.
    struct TempLog {
        dir: std::path::PathBuf,
    }

    impl TempLog {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("vosh-forget-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn path(&self) -> std::path::PathBuf {
            self.dir.join("logs.sqlite")
        }

        /// Every byte of the database file and its sidecars.
        fn bytes(&self) -> Vec<u8> {
            let mut all = Vec::new();
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let mut name = self.path().into_os_string();
                name.push(suffix);
                if let Ok(b) = std::fs::read(std::path::PathBuf::from(name)) {
                    all.extend_from_slice(&b);
                }
            }
            all
        }

        fn holds(&self, secret: &str) -> bool {
            let hay = self.bytes();
            hay.windows(secret.len()).any(|w| w == secret.as_bytes())
        }
    }

    impl Drop for TempLog {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Log `rows` as one session the way the app does: game lines with
    /// their raw bytes, sent lines as `> ` rows with none.
    fn log_session(store: &mut LogStore, rows: &[Row<'_>], filler: usize) -> i64 {
        let sid = store
            .start_session("play.theforsakenlands.com", 1848, 0)
            .unwrap();
        let mut sent_ts = 0;
        for (n, row) in rows.iter().enumerate() {
            let ts = n as i64;
            match row {
                Out(text) => {
                    store
                        .append_raw(sid, ts, format!("\x1b[0m{text}").as_bytes())
                        .unwrap();
                }
                Sent(line) => {
                    sent_ts = ts;
                    store
                        .append(sid, sent_ts, &format!("> {line}"), None)
                        .unwrap();
                }
                Also(line) => {
                    store
                        .append(sid, sent_ts, &format!("> {line}"), None)
                        .unwrap();
                }
            }
        }
        // Enough play after the login that the table spans many pages.
        let entries: Vec<crate::LogEntry> = (0..filler)
            .map(|n| crate::LogEntry {
                session_id: sid,
                ts_ms: 10_000 + n as i64,
                text: format!("The Temple Square hums with voices, line {n}."),
                raw: Some(
                    format!("\x1b[1;37mThe Temple Square hums with voices, line {n}.\x1b[0m")
                        .into_bytes(),
                ),
            })
            .collect();
        store.append_batch(&entries).unwrap();
        sid
    }

    /// One row: id, session id, time, text, raw bytes.
    type RowSnapshot = (i64, i64, i64, String, Option<Vec<u8>>);

    /// Every row, for comparing a log before and after.
    fn snapshot(store: &LogStore) -> Vec<RowSnapshot> {
        let mut stmt = store
            .conn
            .prepare("SELECT id, session_id, ts_ms, text, raw FROM log_lines ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
    }

    /// Three sessions. The first logs in with the account password and
    /// an immortal password, the second changes the account password,
    /// the third only plays.
    fn populated(log: &TempLog) -> (LogStore, Vec<i64>) {
        let mut store = LogStore::open(&log.path()).unwrap();
        let first = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("2"),
                Out(""),
                Sent(SECRET_IMM),
                Out(MOTD),
                Sent("look"),
            ],
        ]);
        let second = session(&[
            &greeting(),
            &login(SECRET_ACCOUNT),
            &account_menu(),
            &[
                Sent("p"),
                Out(""),
                Sent(SECRET_OLD),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Sent(SECRET_NEW),
                Out(""),
                Out("Account password changed successfully."),
            ],
        ]);
        let third = vec![Out("The Temple Square"), Sent("look"), Sent("north")];
        let a = log_session(&mut store, &first, 1500);
        let b = log_session(&mut store, &second, 1500);
        let c = log_session(&mut store, &third, 200);
        (store, vec![a, b, c])
    }

    #[test]
    fn the_preview_counts_without_changing_anything() {
        let log = TempLog::new("preview");
        let (store, _) = populated(&log);
        let before = snapshot(&store);
        let found = store.find_password_lines().unwrap();
        assert_eq!(found.count(), 6);
        assert_eq!(found.sessions(), 2);
        assert!(snapshot(&store) == before, "the preview changed the log");
    }

    #[test]
    fn forgetting_blanks_exactly_those_lines_and_a_second_run_finds_none() {
        let log = TempLog::new("blank");
        let (mut store, _) = populated(&log);
        let before = snapshot(&store);
        let found = store.find_password_lines().unwrap();
        let ids: Vec<i64> = found.lines.iter().map(|(id, _)| *id).collect();

        let done = store.blank_password_lines(&found).unwrap();
        assert_eq!(done.lines, 6);
        assert_eq!(done.sessions, 2);
        assert!(done.wiped, "the file was not wiped");

        let after = snapshot(&store);
        assert_eq!(before.len(), after.len(), "rows came or went");
        for (old, new) in before.iter().zip(&after) {
            assert_eq!((old.0, old.1, old.2), (new.0, new.1, new.2), "a row moved");
            assert!(old.4 == new.4, "raw bytes changed on row {}", old.0);
            if ids.contains(&old.0) {
                assert!(new.3 == HIDDEN_SENT_TEXT, "row {} was not blanked", old.0);
            } else {
                assert!(
                    old.3 == new.3,
                    "row {} changed though it holds no password",
                    old.0
                );
            }
        }

        assert_eq!(store.find_password_lines().unwrap().count(), 0);
        let again = store.forget_passwords().unwrap();
        assert_eq!(again.lines, 0);
        assert!(snapshot(&store) == after, "a second run changed the log");
    }

    #[test]
    fn no_old_copy_of_a_secret_stays_in_the_file() {
        let log = TempLog::new("wipe");
        let (mut store, _) = populated(&log);
        let secrets = [SECRET_ACCOUNT, SECRET_IMM, SECRET_OLD, SECRET_NEW];
        // The test means something only if the secrets start on disk.
        for (n, secret) in secrets.iter().enumerate() {
            assert!(
                log.holds(secret),
                "made up secret {n} never reached the file"
            );
        }
        let done = store.forget_passwords().unwrap();
        assert!(done.wiped, "the file was not wiped");
        for (n, secret) in secrets.iter().enumerate() {
            assert!(
                !log.holds(secret),
                "made up secret {n} is still in the file"
            );
        }
        // The rest of the log is still on disk.
        assert!(log.holds("line 1499."), "ordinary lines went missing");
    }

    const SECRETS: [&str; 4] = [SECRET_ACCOUNT, SECRET_IMM, SECRET_OLD, SECRET_NEW];

    /// Indexes of the made up secrets still somewhere in the file.
    fn secrets_left(log: &TempLog) -> Vec<usize> {
        (0..SECRETS.len())
            .filter(|&n| log.holds(SECRETS[n]))
            .collect()
    }

    #[test]
    fn a_wipe_that_could_not_finish_is_finished_by_the_next_run() {
        let log = TempLog::new("unfinished");
        let (mut store, _) = populated(&log);
        // Another reader holds an old snapshot open, so the last
        // checkpoint cannot empty the write ahead log. A full disk under
        // the rebuild ends the same way.
        let reader = Connection::open(log.path()).unwrap();
        reader.execute_batch("BEGIN;").unwrap();
        let _: i64 = reader
            .query_row("SELECT count(*) FROM log_lines", [], |r| r.get(0))
            .unwrap();
        let done = store.forget_passwords().unwrap();
        assert_eq!((done.lines, done.resumed, done.wiped), (6, false, false));

        // The preview finds no line left to blank and says the wipe is due.
        let found = store.find_password_lines().unwrap();
        assert_eq!(found.count(), 0);
        assert!(found.wipe_pending, "the preview forgot the unfinished wipe");

        reader.execute_batch("COMMIT;").unwrap();
        drop(reader);
        let again = store.forget_passwords().unwrap();
        assert_eq!((again.lines, again.resumed, again.wiped), (0, true, true));
        assert_eq!(
            secrets_left(&log),
            Vec::<usize>::new(),
            "made up secrets are still in the file"
        );

        // Nothing is due after that.
        assert!(!store.find_password_lines().unwrap().wipe_pending);
        let before = snapshot(&store);
        let third = store.forget_passwords().unwrap();
        assert_eq!((third.lines, third.resumed, third.wiped), (0, false, true));
        assert!(
            snapshot(&store) == before,
            "a run with nothing due changed the log"
        );
    }

    #[test]
    fn a_wipe_cut_short_by_a_quit_is_finished_after_a_restart() {
        let log = TempLog::new("cut-short");
        let (mut store, _) = populated(&log);
        let found = store.find_password_lines().unwrap();
        // Vosh quits once the lines are blanked, before the rebuild ends.
        assert_eq!(store.blank_rows(&found).unwrap(), (6, 2));
        drop(store);
        assert!(
            !secrets_left(&log).is_empty(),
            "the test means something only if old copies stay behind"
        );

        let mut store = LogStore::open(&log.path()).unwrap();
        let found = store.find_password_lines().unwrap();
        assert_eq!(found.count(), 0);
        assert!(found.wipe_pending, "the preview forgot the unfinished wipe");
        let done = store.blank_password_lines(&found).unwrap();
        assert_eq!((done.lines, done.resumed, done.wiped), (0, true, true));
        assert_eq!(
            secrets_left(&log),
            Vec::<usize>::new(),
            "made up secrets are still in the file"
        );
        assert!(!store.wipe_pending().unwrap());
    }

    #[test]
    fn the_live_log_keeps_working_after_forgetting() {
        let log = TempLog::new("live");
        let (mut store, sessions) = populated(&log);
        let reader = LogStore::open(&log.path()).unwrap();
        store.forget_passwords().unwrap();

        // The session that was open keeps appending, one line and a batch.
        let sid = sessions[2];
        store.append(sid, 90_000, "> kill rat", None).unwrap();
        store
            .append_batch(&[crate::LogEntry {
                session_id: sid,
                ts_ms: 90_001,
                text: "You slay the rat.".into(),
                raw: Some(b"\x1b[31mYou slay the rat.\x1b[0m".to_vec()),
            }])
            .unwrap();
        store.end_session(sid, 90_002).unwrap();
        // A new session opens and logs too.
        let next = store
            .start_session("play.theforsakenlands.com", 1848, 95_000)
            .unwrap();
        store.append(next, 95_001, "> look", None).unwrap();

        // Both connections read the new lines.
        for s in [&store, &reader] {
            let page = s
                .search_page(
                    "slay the rat|^> kill rat$",
                    &crate::SearchOptions::default(),
                    true,
                )
                .unwrap();
            assert_eq!(page.total, Some(2));
            assert_eq!(s.get_session(next).unwrap().unwrap().line_count, 1);
            let hidden = s
                .search(r"^> \(hidden\)$", &crate::SearchOptions::default())
                .unwrap();
            assert_eq!(hidden.len(), 6);
        }
    }
}
