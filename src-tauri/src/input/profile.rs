//! `#profile save`, `load` and `reset` on the file of the profile the
//! session plays, and
//! `#import-tintin`, which reads aliases and variables from a .tin file.

use super::{split_first_word, InputResult};
use crate::app::state::AppState;
use crate::disk::paths;
use crate::import::tintin;
use crate::profile::file::ProfileConfig;
use crate::profile::live::Profile;
use crate::profile::switch::hand_to_connection;
use crate::session::connection::Connection;

/// What `#profile save`, `load`, and `reset` answer between the shared
/// catalog wizard and the relaunch that finishes it. Nothing saves in
/// that window, the profile files hold no aliases, triggers, or macros
/// any more, and the shared catalog loads only at launch.
const PROFILE_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts.";

/// What `#profile save` answers while another profile write holds
/// [`crate::disk::save::PERSIST_LOCK`].
pub(super) const PROFILE_SAVE_BUSY: &str = "Vosh is saving this profile. Try again.";

/// `#profile save`, `load` and `reset` on the file of `profile` in the
/// app data folder `state` holds. A load or a reset hands the
/// connection `c` the new tick settings and `[prompt]` table, as a
/// profile switch does, and leaves the rest of it as it was.
pub(super) fn slash_profile(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    args: &str,
    replaced: &mut bool,
) -> InputResult {
    let (cmd, _rest) = split_first_word(args);
    let migration_pending = state
        .relaunch_pending
        .load(std::sync::atomic::Ordering::Acquire);
    if migration_pending && matches!(cmd, "save" | "load" | "reset") {
        return InputResult::error(PROFILE_MIGRATION_PENDING);
    }
    // Loadout mode keeps authored items in the catalog and persists them
    // automatically. The legacy save/load/reset trio would write, load,
    // or blank the wrong files there, so it bows out with a pointer.
    if state
        .loadout_mode
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return match cmd {
            "save" => InputResult::echo_line("loadout mode saves your changes automatically"),
            "load" => InputResult::echo_line("loadout mode loads the catalog at startup"),
            "reset" => InputResult::error(
                "profile reset does not apply in loadout mode. delete items from settings instead",
            ),
            "" => InputResult::error("usage #profile save | load | reset"),
            other => InputResult::error(format!("unknown #profile subcommand `{other}`")),
        };
    }
    let path = state
        .app_data
        .get()
        .zip(profile.name.as_deref())
        .map(|(app_data, name)| paths::profile_path(app_data, name));
    match cmd {
        "save" => match path {
            Some(path) => {
                // Every profile file write holds the persist lock. This
                // runs under the profile lock, which the persist takes
                // after the persist lock, so it only tries.
                let Ok(_persist_guard) = crate::disk::save::PERSIST_LOCK.try_lock() else {
                    return InputResult::error(PROFILE_SAVE_BUSY);
                };
                if crate::disk::atomic::is_unread(&path) {
                    return InputResult::error(
                        "Vosh could not read this profile file at launch, so it will not save \
                         over it. Fix the file or switch to another profile.",
                    );
                }
                let snapshot = ProfileConfig::from_profile(profile);
                match snapshot.save(&path) {
                    Ok(()) => {
                        InputResult::echo_line(format!("profile saved to {}", path.display()))
                    }
                    Err(e) => InputResult::error(format!("save failed: {e}")),
                }
            }
            None => InputResult::error("could not resolve profile path"),
        },
        "load" => match path {
            Some(path) => load_profile_file(profile, c, &path, replaced),
            None => InputResult::error("could not resolve profile path"),
        },
        "reset" => {
            let blank = ProfileConfig::default();
            let tick_before = profile.tick.config.clone();
            let _ = blank.apply_to(profile);
            hand_to_connection(profile, c, &tick_before);
            *replaced = true;
            InputResult::echo_line("profile reset to defaults")
        }
        "" => InputResult::error("usage #profile save | load | reset"),
        other => InputResult::error(format!("unknown #profile subcommand `{other}`")),
    }
}

/// `#profile load` from `path`. Replaces the live profile, hands the
/// connection `c` the file's tick settings and `[prompt]` table, and sets
/// `replaced`, only when the file reads. A file that does not read leaves
/// both as they were.
pub(super) fn load_profile_file(
    profile: &mut Profile,
    c: &mut Connection,
    path: &std::path::Path,
    replaced: &mut bool,
) -> InputResult {
    let snapshot = match ProfileConfig::load(path) {
        Ok(snapshot) => snapshot,
        Err(e) => return InputResult::error(format!("load failed: {e}")),
    };
    // The file reads now and the live profile holds what it says, so the
    // saves may write it again.
    crate::disk::atomic::release_unread(path);
    let tick_before = profile.tick.config.clone();
    let warnings = snapshot.apply_to(profile);
    hand_to_connection(profile, c, &tick_before);
    *replaced = true;
    let mut lines = vec![format!("profile loaded from {}", path.display())];
    for w in warnings {
        lines.push(format!("  {w}"));
    }
    InputResult::echo_lines(lines)
}

pub(super) fn slash_import_tintin(profile: &mut Profile, args: &str) -> InputResult {
    let path = args.trim();
    if path.is_empty() {
        return InputResult::error("usage #import-tintin <path>");
    }
    let expanded = expand_home(path);
    let report = match tintin::import_file(&expanded) {
        Ok(r) => r,
        Err(e) => return InputResult::error(format!("read failed: {e}")),
    };
    for alias in &report.aliases {
        profile.aliases.set(alias.clone());
    }
    for (name, value) in &report.vars {
        profile.vars.set(name.clone(), value.clone());
    }
    let mut lines = vec![
        format!("imported {}", expanded.display()),
        format!(
            "  {} aliases, {} vars",
            report.aliases.len(),
            report.vars.len()
        ),
    ];
    if !report.unsupported.is_empty() {
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for (kind, _) in &report.unsupported {
            *counts.entry(kind.clone()).or_default() += 1;
        }
        let summary: Vec<String> = counts.iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines.push(format!("  skipped (unsupported): {}", summary.join(" ")));
    }
    if !report.unparsed.is_empty() {
        lines.push(format!("  unparsed: {} line(s)", report.unparsed.len()));
    }
    InputResult::echo_lines(lines)
}

fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}
