//! The commands you sent from the command line, kept for each session in
//! a file beside its scrollback, so Up brings them back after a restart
//! or a crash. The page decides which lines join history, as it always
//! has. Each one comes here as it goes out and lands at the end of the
//! file at once, so a crash loses none. A line that gives the game a
//! password never lands, and the page drops it from what it holds.
//!
//! One lock covers every read, write and delete of the files, so a trim
//! never swaps out a line added while it read, and a session that closes
//! leaves no file a late line made again.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

use vosh_automation::alias::ExpandStep;

use crate::disk::atomic::swap_in;
use crate::profile::live::Profile;
use crate::session::connection::Connection;

/// How many commands a session keeps, the newest.
pub(crate) const HISTORY_CAP: usize = 1000;

/// Held for each read, write and delete of a history file.
static FILES: Mutex<()> = Mutex::new(());

fn files() -> std::sync::MutexGuard<'static, ()> {
    FILES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether `line`, typed in the session `c` on `profile`, may join
/// history. It never does when any command it sends gives the game a
/// password, like `password <old> <new>`, by the rule the log's password
/// wipe blanks such a line with. The line is read as the pipeline reads
/// it, variables filled in, split at `;` and aliases expanded, so
/// `look;password a b` and an alias that sends `password` stay out. A
/// line whose aliases run too deep sends nothing and stays out too.
///
/// While `connected` to a game that speaks GMCP and has not said you
/// play, every line stays out. That is the login, where a password typed
/// before the game hides your typing goes out as a plain line. The log
/// counts the same lines as login.
pub(crate) fn keeps(profile: &Profile, c: &Connection, line: &str, connected: bool) -> bool {
    if connected && !c.log_kinds.in_play(c.link.playing()) {
        return false;
    }
    if vosh_log::carries_password(line) {
        return false;
    }
    if line.trim_start().starts_with('#') {
        return true;
    }
    let interpolated = c.var_view(profile).interpolate(line.trim());
    match profile
        .aliases
        .expand_line_full(&interpolated, &c.plugin_aliases, c.stop_key)
    {
        Ok(steps) => !steps.iter().any(|step| match step {
            ExpandStep::Command(cmd) => vosh_log::carries_password(cmd),
            ExpandStep::Script(_) => false,
        }),
        Err(_) => false,
    }
}

/// The newest [`HISTORY_CAP`] commands in `path`, oldest first. A file
/// that holds more is cut back to them, so it never grows past one run's
/// worth over the cap. A missing file is an empty history, a byte that is
/// not UTF-8 reads as a replacement mark, and a cut that fails leaves the
/// file long and still hands back the commands.
pub(crate) fn load(path: &Path) -> Vec<String> {
    let _files = files();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!(path = %path.display(), error = %e, "command history unreadable");
            }
            return Vec::new();
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    let start = lines.len().saturating_sub(HISTORY_CAP);
    let kept: Vec<String> = lines[start..]
        .iter()
        .map(|&line| line.to_string())
        .collect();
    if start > 0 {
        let mut trimmed = kept.join("\n");
        trimmed.push('\n');
        if let Err(e) = swap_in(path, &trimmed) {
            tracing::warn!(path = %path.display(), error = %e, "command history not cut back");
        }
    }
    kept
}

/// Add `line` to the end of `path` while `open` still says its session
/// is open, and say whether it did. A line break inside it becomes a
/// space, so each command stays one line of the file.
pub(crate) fn append(path: &Path, line: &str, open: impl FnOnce() -> bool) -> io::Result<bool> {
    let _files = files();
    if !open() {
        return Ok(false);
    }
    let mut row: String = line
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    row.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(row.as_bytes())?;
    Ok(true)
}

/// Delete the history file at `path`, as its session closes.
pub(crate) fn remove(path: &Path) {
    let _files = files();
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use vosh_automation::alias::Alias;

    use super::{append, keeps, load, remove, HISTORY_CAP};
    use crate::profile::live::Profile;
    use crate::session::connection::Connection;

    fn add(path: &std::path::Path, line: &str) {
        assert!(append(path, line, || true).unwrap());
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("history.txt")), Vec::<String>::new());
    }

    #[test]
    fn commands_come_back_in_the_order_they_went_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        add(&path, "look");
        add(&path, "cast 'armor' Orla");
        add(&path, "look");
        assert_eq!(load(&path), ["look", "cast 'armor' Orla", "look"]);
    }

    #[test]
    fn a_line_break_inside_a_command_becomes_a_space() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        add(&path, "say one\r\ntwo");
        assert_eq!(load(&path), ["say one  two"]);
    }

    #[test]
    fn a_long_file_is_cut_back_to_the_newest_commands() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        for n in 0..HISTORY_CAP + 25 {
            add(&path, &format!("say {n}"));
        }
        let loaded = load(&path);
        assert_eq!(loaded.len(), HISTORY_CAP);
        assert_eq!(loaded.first().map(String::as_str), Some("say 25"));
        assert_eq!(loaded.last(), Some(&format!("say {}", HISTORY_CAP + 24)));
        // The file itself now holds only those, so the next load agrees.
        assert_eq!(load(&path), loaded);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            HISTORY_CAP
        );
    }

    #[test]
    fn a_byte_that_is_not_utf8_still_loads_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        std::fs::write(&path, b"look\nsay caf\xe9\nscore\n").unwrap();
        assert_eq!(load(&path), ["look", "say caf\u{fffd}", "score"]);
    }

    #[test]
    fn a_closed_session_adds_nothing_and_its_file_goes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history-2.txt");
        add(&path, "look");
        remove(&path);
        assert!(!append(&path, "score", || false).unwrap());
        assert!(!path.exists());
    }

    #[test]
    fn a_command_that_gives_a_password_never_joins() {
        let mut profile = Profile::default();
        profile.aliases.set(Alias::new("pw", "password $1 $2"));
        let c = Connection::default();
        let keeps = |line: &str| keeps(&profile, &c, line, false);
        assert!(!keeps("password hunter2 swordfish"));
        assert!(!keeps("account password tolliver swordfish"));
        assert!(!keeps("look;password hunter2 swordfish"));
        assert!(!keeps("pw hunter2 swordfish"));
        assert!(keeps("look"));
        assert!(keeps("say my password is safe"));
        assert!(keeps("#alias pw {password %1 %2}"));
    }

    #[test]
    fn nothing_joins_before_a_game_that_speaks_gmcp_says_you_play() {
        let profile = Profile::default();
        let mut c = Connection::default();
        // A game with no GMCP never says, so every line counts.
        assert!(keeps(&profile, &c, "Tolliver", true));
        c.log_kinds.gmcp_on();
        assert!(!keeps(&profile, &c, "Tolliver", true));
        assert!(!keeps(&profile, &c, "swordfish", true));
        // Offline nothing goes to the game, so a # command counts.
        assert!(keeps(&profile, &c, "#tick reset", false));
        c.link.gmcp("Char.Status");
        assert!(keeps(&profile, &c, "look", true));
    }
}
