//! The commands you sent from the command line, kept for each session in
//! a file beside its scrollback, so Up brings them back after a restart
//! or a crash. The page decides which lines join history, as it always
//! has. Each one comes here as it goes out and lands at the end of the
//! file at once, so a crash loses none. A line that gives the game a
//! password never lands, and the page drops it from what it holds.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use crate::disk::atomic::swap_in;

/// How many commands a session keeps, the newest.
pub(crate) const HISTORY_CAP: usize = 1000;

/// Whether `line` may join history. A command that carries a password,
/// like `password <old> <new>`, never does, by the same rule the log's
/// password wipe blanks it with.
pub(crate) fn keeps(line: &str) -> bool {
    !vosh_log::carries_password(line)
}

/// The newest [`HISTORY_CAP`] commands in `path`, oldest first. A file
/// that holds more is cut back to them, so it never grows past one run's
/// worth over the cap. A missing file is an empty history.
pub(crate) fn load(path: &Path) -> io::Result<Vec<String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    let start = lines.len().saturating_sub(HISTORY_CAP);
    let kept: Vec<String> = lines[start..]
        .iter()
        .map(|&line| line.to_string())
        .collect();
    if start > 0 {
        let mut trimmed = kept.join("\n");
        trimmed.push('\n');
        swap_in(path, &trimmed)?;
    }
    Ok(kept)
}

/// Add `line` to the end of `path`. A line break inside it becomes a
/// space, so each command stays one line of the file.
pub(crate) fn append(path: &Path, line: &str) -> io::Result<()> {
    let mut row: String = line
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    row.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(row.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{append, keeps, load, HISTORY_CAP};

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(&dir.path().join("history.txt")).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn commands_come_back_in_the_order_they_went_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        append(&path, "look").unwrap();
        append(&path, "cast 'armor' Orla").unwrap();
        append(&path, "look").unwrap();
        assert_eq!(load(&path).unwrap(), ["look", "cast 'armor' Orla", "look"]);
    }

    #[test]
    fn a_line_break_inside_a_command_becomes_a_space() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        append(&path, "say one\r\ntwo").unwrap();
        assert_eq!(load(&path).unwrap(), ["say one  two"]);
    }

    #[test]
    fn a_long_file_is_cut_back_to_the_newest_commands() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.txt");
        for n in 0..HISTORY_CAP + 25 {
            append(&path, &format!("say {n}")).unwrap();
        }
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), HISTORY_CAP);
        assert_eq!(loaded.first().map(String::as_str), Some("say 25"));
        assert_eq!(loaded.last(), Some(&format!("say {}", HISTORY_CAP + 24)));
        // The file itself now holds only those, so the next load agrees.
        assert_eq!(load(&path).unwrap(), loaded);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            HISTORY_CAP
        );
    }

    #[test]
    fn a_command_that_gives_a_password_never_joins() {
        assert!(!keeps("password hunter2 swordfish"));
        assert!(!keeps("account password tolliver swordfish"));
        assert!(keeps("look"));
        assert!(keeps("say my password is safe"));
    }
}
