//! The game's login strings, and the matchers that read password prompts
//! and the commands that carry a password.

use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

/// The line the game prints right above the main menu.
pub(super) const MAIN_MENU: &str = "Abandon hope, all ye who enter here...";
/// The account menu's option line starts and ends with these.
const ACCOUNT_MENU_START: &str = "[#] Play";
const ACCOUNT_MENU_END: &str = "[D]isconnect";
/// Lines the game prints after taking an answer at `Imm-Password?> `.
pub(super) const AFTER_IMM_PASSWORD: [&str; 4] = [
    "Prepare yourself. For you are about to <Enter> the Forsaken Lands!",
    "Login failed.",
    "Reconnecting. Type replay to see missed tells.",
    ALREADY_PLAYING,
];
pub(super) const ALREADY_PLAYING: &str = "That character is already playing.";
pub(super) const RECONNECT_FAILED: &str = "Reconnect attempt failed.";
/// Printed before `Password?> ` for a character with no password.
pub(super) const NULL_PASSWORD: &str = "NULL password, resetting to player name...";
/// Printed after the pick of an immortal with no immortal password.
pub(super) const IMM_PASSWORD_NEEDED: &str =
    "This immortal character requires an immortal password.";
/// After a password, the game asks the same prompt again.
pub(super) const ASK_AGAIN: [&str; 4] = [
    "Password must be at least five characters.",
    "Password must be at least five characters long.",
    "Immortal password must differ from your account password.",
    "Immortal password must differ from the character password.",
];
/// After a confirmation, the game asks for the new password again.
pub(super) const MISMATCH: [&str; 2] = ["Passwords don't match.", "Passwords do not match."];
/// After a character password while linking an immortal.
pub(super) const VERIFY_IMM: &str = "Immortal character detected. Verify immortal password.";
pub(super) const SET_IMM: &str =
    "This immortal character requires an immortal password before linking.";

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
pub(crate) fn is_password_prompt(line: &str) -> bool {
    static WHOLE: OnceLock<Regex> = OnceLock::new();
    WHOLE
        .get_or_init(|| prompt_pattern("$"))
        .is_match(line.trim())
}

/// `line` without a password prompt at its front. The login sends no GA,
/// so a prompt the game prints between two pieces of a split line stays
/// a partial line until the next output ends it, and it lands in the log
/// as a row of its own or joined to the front of the next line.
pub(super) fn after_password_prompt(line: &str) -> &str {
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
pub(super) fn carries_password(answer: &str) -> bool {
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

pub(super) fn is_account_menu(line: &str) -> bool {
    let line = line.trim();
    line.starts_with(ACCOUNT_MENU_START) && line.ends_with(ACCOUNT_MENU_END)
}

/// The game reads a lone `q` as quit at a name prompt.
pub(super) fn is_quit(answer: &str) -> bool {
    answer.eq_ignore_ascii_case("q")
}
