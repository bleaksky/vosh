//! The Lua commands. `#script load` and `#script reload` run script
//! files from the scripts folder, `#lua` runs a line of Lua, and
//! `#scripts` lists what is loaded.

use vosh_script::Owner;

use super::{split_first_word, InputResult};
use crate::app::state::AppState;
use crate::disk::paths;
use crate::profile::live::Profile;
use crate::script;

pub(super) fn slash_script(
    state: &AppState,
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "load" => slash_script_load(state, profile, rest, lua),
        "reload" => slash_script_reload(profile, lua),
        "" => InputResult::error("usage #script load <name> | #script reload"),
        other => InputResult::error(format!("unknown #script subcommand `{other}`")),
    }
}

/// `#script load <name>`, from the scripts folder in the app data folder
/// `state` holds.
fn slash_script_load(
    state: &AppState,
    profile: &mut Profile,
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
    let path = script_path_for(app_data, name);
    let code = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return InputResult::error(format!("read failed: {e} ({})", path.display())),
    };
    // A loose file's owner is its path inside the scripts folder with
    // `.lua` on the end, so `combat` and `combat.lua` load as one.
    let file = script_file_name(name);
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome =
        profile
            .script
            .load_script(Owner::Script(file.clone()), &format!("@{file}"), code);
    let failed = outcome.failed;
    lua.append(script::apply_actions(profile, outcome));
    // A script that failed says why in its own lines.
    if failed {
        return InputResult::empty();
    }
    InputResult::echo_line(format!("loaded {}", path.display()))
}

fn slash_script_reload(profile: &mut Profile, lua: &mut script::ApplyResult) -> InputResult {
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = profile.script.reload_scripts();
    lua.append(script::apply_actions(profile, outcome));
    InputResult::echo_line("scripts reloaded")
}

pub(super) fn slash_scripts_list(profile: &Profile) -> InputResult {
    let names = profile.script.loaded_script_names();
    let triggers = profile.script.lua_triggers();
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
        // the #triggers listing.
        for t in triggers {
            lines.push(format!("    [  0] {} /{}/", t.name, t.pattern));
        }
    }
    InputResult::echo_lines(lines)
}

pub(super) fn slash_lua(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let code = args.trim_start();
    if code.is_empty() {
        return InputResult::error("usage #lua <code>");
    }
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = profile.script.eval(code, "=#lua");
    lua.append(script::apply_actions(profile, outcome));
    InputResult::empty()
}

/// The file `#script load <name>` reads, `<app_data>/scripts/<name>.lua`
/// under the app data folder `app_data`.
fn script_path_for(app_data: &std::path::Path, name: &str) -> std::path::PathBuf {
    paths::scripts_dir(app_data).join(script_file_name(name))
}

/// `name` with `.lua` on the end, unless it ends so already.
fn script_file_name(name: &str) -> String {
    if std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        name.to_string()
    } else {
        format!("{name}.lua")
    }
}
