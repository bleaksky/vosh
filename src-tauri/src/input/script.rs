//! The Lua commands. `#script load` and `#script reload` run script
//! files from the scripts folder, `#lua` runs a line of Lua, and
//! `#scripts` lists what is loaded.

use tracing::warn;
use vosh_script::{Action, Owner, ScriptOutcome};

use super::{split_first_word, InputResult};
use crate::app::state::AppState;
use crate::disk::paths;
use crate::profile::live::Profile;
use crate::script;
use crate::session::connection::Connection;

pub(super) fn slash_script(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "load" => slash_script_load(state, profile, c, rest, lua),
        "reload" => slash_script_reload(state, profile, c, lua),
        "" => InputResult::error("usage #script load <name> | #script reload"),
        other => InputResult::error(format!("unknown #script subcommand `{other}`")),
    }
}

/// `#script load <name>`, from the scripts folder in the app data folder
/// `state` holds.
fn slash_script_load(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return InputResult::error("usage #script load <name>");
    }
    let Some(app_data) = state.app_data.get() else {
        return InputResult::error("could not resolve scripts directory");
    };
    // A loose file's owner is its path inside the scripts folder with
    // `.lua` on the end, so `combat` and `combat.lua` load as one.
    let Some(file) = script_file_name(name) else {
        return InputResult::error(OUTSIDE_SCRIPTS);
    };
    let scripts = paths::scripts_dir(app_data);
    let path = scripts.join(&file);
    let code = match std::fs::read_to_string(&path) {
        Ok(code) => code,
        Err(e) => return InputResult::error(format!("read failed: {e} ({})", path.display())),
    };
    // A disk that ignores case opens `Combat.lua` for `combat`, so the
    // owner takes the name the folder gives the file.
    let file = spelled_on_disk(&scripts, &file);
    let path = scripts.join(&file);
    script::refresh_vars(profile, c);
    let outcome = c
        .script
        .load_script(Owner::Script(file.clone()), &format!("@{file}"), &code);
    let failed = outcome.failed;
    lua.append(script::apply_actions(profile, c, outcome));
    // A script that failed says why in its own lines.
    if failed {
        return InputResult::empty();
    }
    InputResult::echo_line(format!("loaded {}", path.display()))
}

/// `#script reload`. Reads each loaded plugin and loose script from disk
/// again and loads it, in the order they first loaded, and goes on past
/// one that fails. A file Vosh cannot read leaves its script as it was.
fn slash_script_reload(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let Some(app_data) = state.app_data.get() else {
        return InputResult::error("could not resolve scripts directory");
    };
    script::refresh_vars(profile, c);
    let mut outcome = ScriptOutcome::default();
    for owner in c.script.reload_order() {
        match read_again(app_data, &owner) {
            Some(Ok((chunk, code))) => {
                outcome.append(c.script.load_script(owner, &chunk, &code));
            }
            Some(Err(text)) => outcome.actions.push(Action::Error {
                owner,
                text,
                at: None,
            }),
            None => {}
        }
    }
    lua.append(script::apply_actions(profile, c, outcome));
    InputResult::echo_line("scripts reloaded")
}

/// The chunk name and the code of the plugin or loose script `owner` as
/// its file reads now, or the line that says Vosh could not read it.
/// None for Lua that has no file.
fn read_again(
    app_data: &std::path::Path,
    owner: &Owner,
) -> Option<Result<(String, String), String>> {
    Some(match owner {
        Owner::Script(file) => {
            let path = paths::scripts_dir(app_data).join(file);
            std::fs::read_to_string(&path)
                .map(|code| (format!("@{file}"), code))
                .map_err(|e| {
                    warn!(path = %path.display(), error = %e, "script reload could not read");
                    format!("Vosh could not read {file} and left it as it was.")
                })
        }
        Owner::Plugin(name) => {
            crate::app::plugins::read_plugin(&paths::plugins_dir(app_data), name)
                .map(|plugin| (plugin.chunk(name), plugin.code))
                .map_err(|e| {
                    warn!(plugin = %name, error = %e, "plugin reload could not read");
                    format!("Vosh could not read plugin {name} and left it as it was.")
                })
        }
        Owner::Typed | Owner::Trigger(_) | Owner::Alias { .. } => return None,
    })
}

pub(super) fn slash_scripts_list(c: &Connection) -> InputResult {
    let names = c.script.loaded_script_names();
    let triggers = c.script.lua_triggers();
    let mut lines = Vec::new();
    if names.is_empty() {
        lines.push("no scripts loaded".to_string());
    } else {
        lines.push(format!("{} script(s) loaded:", names.len()));
        for n in names {
            lines.push(format!("  {n}"));
        }
    }
    if !triggers.is_empty() {
        lines.push(format!("{} lua trigger(s):", triggers.len()));
        // Every Lua trigger runs at priority 0. The column lines up with
        // the #triggers listing. Two scripts may each have a trigger of
        // one name, so each line names who registered it.
        for t in triggers {
            lines.push(format!(
                "    [  0] {} /{}/ from {}",
                t.name, t.pattern, t.owner
            ));
        }
    }
    InputResult::echo_lines(lines)
}

pub(super) fn slash_lua(
    profile: &mut Profile,
    c: &mut Connection,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let code = args.trim_start();
    if code.is_empty() {
        return InputResult::error("usage #lua <code>");
    }
    script::refresh_vars(profile, c);
    let outcome = c.script.eval(code, "=#lua");
    lua.append(script::apply_actions(profile, c, outcome));
    InputResult::empty()
}

/// `file`, a path inside `scripts` that opened, as the folders on disk
/// spell it. A disk that ignores case, as macOS and Windows have by
/// default, opens `Combat.lua` for `combat.lua`, and one file must be one
/// script whatever case you type. Each part keeps the case you typed when
/// an entry has it exactly, as on a disk that heeds case, and otherwise
/// takes the one entry that matches it in any case.
fn spelled_on_disk(scripts: &std::path::Path, file: &str) -> String {
    let mut dir = scripts.to_path_buf();
    let mut spelled = Vec::new();
    for part in file.split('/') {
        let names: Vec<String> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        let name = if names.iter().any(|name| name == part) {
            part.to_string()
        } else {
            let lower = part.to_lowercase();
            let mut matches = names
                .into_iter()
                .filter(|name| name.to_lowercase() == lower);
            match (matches.next(), matches.next()) {
                (Some(name), None) => name,
                _ => part.to_string(),
            }
        };
        dir.push(&name);
        spelled.push(name);
    }
    spelled.join("/")
}

/// What `#script load` says to a name that leads out of the scripts
/// folder.
const OUTSIDE_SCRIPTS: &str = "Vosh loads scripts from your scripts folder only.";

/// The path inside the scripts folder that `#script load <name>` reads,
/// with `.lua` on the end unless it ends so already, or None for an
/// absolute path or one that climbs out with `..`.
fn script_file_name(name: &str) -> Option<String> {
    use std::path::Component;
    let mut parts = Vec::new();
    for part in std::path::Path::new(name).components() {
        match part {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let path = parts.join("/");
    if path.is_empty() {
        return None;
    }
    let has_lua = std::path::Path::new(&path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    Some(if has_lua { path } else { format!("{path}.lua") })
}

#[cfg(test)]
mod tests {
    use super::spelled_on_disk;

    #[test]
    fn a_script_takes_the_name_its_folder_gives_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Combat")).unwrap();
        std::fs::write(dir.path().join("Combat").join("Bash.lua"), "").unwrap();
        std::fs::write(dir.path().join("heal.lua"), "").unwrap();
        assert_eq!(
            spelled_on_disk(dir.path(), "combat/bash.lua"),
            "Combat/Bash.lua"
        );
        assert_eq!(spelled_on_disk(dir.path(), "HEAL.lua"), "heal.lua");
        assert_eq!(spelled_on_disk(dir.path(), "heal.lua"), "heal.lua");
        // A name no entry matches stays as you typed it.
        assert_eq!(spelled_on_disk(dir.path(), "flee.lua"), "flee.lua");
    }
}
