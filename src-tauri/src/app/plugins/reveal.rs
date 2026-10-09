//! Show a plugin's folder in the system's file manager, through a small
//! command of Vosh's own rather than an opener plugin. The page labels it
//! Show in Finder on macOS, Show in Explorer
//! on Windows and Show the folder on Linux.

use std::path::Path;
use std::process::Command;

/// Open the file manager on `dir`: Finder with the folder selected in
/// its parent on macOS, Explorer the same way on Windows, and the folder
/// itself on Linux. Returns once the opener starts, and a thread of its
/// own waits for the opener to end, so none lingers.
pub(crate) fn reveal(dir: &Path) -> std::io::Result<()> {
    let mut opener = command(dir).spawn()?;
    std::thread::spawn(move || {
        let _ = opener.wait();
    });
    Ok(())
}

#[cfg(target_os = "macos")]
fn command(dir: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-R").arg(dir);
    command
}

#[cfg(windows)]
fn command(dir: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("explorer.exe");
    // Explorer reads `/select,` and the path as one argument and does not
    // select a path when quotes wrap both, so only the path takes them.
    command.raw_arg(format!("/select,\"{}\"", dir.display()));
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command(dir: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(dir);
    command
}

#[cfg(test)]
mod tests {
    /// The command for a folder, read without running it, so no file
    /// manager opens.
    #[cfg(target_os = "macos")]
    #[test]
    fn finder_shows_the_folder_selected() {
        let dir = std::path::Path::new("/Users/orla/Vosh/plugins/wait_full");
        let command = super::command(dir);
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["-R", "/Users/orla/Vosh/plugins/wait_full"]);
    }
}
