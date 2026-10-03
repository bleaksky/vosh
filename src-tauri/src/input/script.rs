//! The Lua commands. `#script load` and `#script reload` run script
//! files from the scripts folder, `#lua` runs a line of Lua, and
//! `#scripts` lists what is loaded.

use super::{split_first_word, InputResult, APP_DATA_DIR};
use crate::profile::Profile;
use crate::script;

pub(super) fn slash_script(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "load" => slash_script_load(profile, rest, lua),
        "reload" => slash_script_reload(profile, lua),
        "" => InputResult::error("usage #script load <name> | #script reload"),
        other => InputResult::error(format!("unknown #script subcommand `{other}`")),
    }
}

fn slash_script_load(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let app_data = APP_DATA_DIR.get().map(std::path::PathBuf::as_path);
    slash_script_load_in(profile, args, lua, app_data)
}

/// [`slash_script_load`] over the app data folder `app_data` in place of
/// [`APP_DATA_DIR`], so a test can load from a folder of its own.
pub(super) fn slash_script_load_in(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
    app_data: Option<&std::path::Path>,
) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return InputResult::error("usage #script load <name>");
    }
    let Some(app_data) = app_data else {
        return InputResult::error("could not resolve scripts directory");
    };
    let path = script_path_for(app_data, name);
    let code = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return InputResult::error(format!("read failed: {e} ({})", path.display())),
    };
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.load_script(name, code) {
        Ok(o) => o,
        Err(e) => return InputResult::error(format!("script error: {e}")),
    };
    lua.append(script::apply_actions(profile, outcome));
    InputResult::echo_line(format!("loaded {}", path.display()))
}

fn slash_script_reload(profile: &mut Profile, lua: &mut script::ApplyResult) -> InputResult {
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.reload_scripts() {
        Ok(o) => o,
        Err(e) => return InputResult::error(format!("reload error: {e}")),
    };
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
    let outcome = match profile.script.eval(code, "#lua") {
        Ok(o) => o,
        Err(e) => return InputResult::error(format!("lua error: {e}")),
    };
    lua.append(script::apply_actions(profile, outcome));
    InputResult::empty()
}

/// The file `#script load <name>` reads, `<app_data>/scripts/<name>.lua`
/// under the app data folder `app_data`.
fn script_path_for(app_data: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = app_data.join("scripts");
    if std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        dir.join(name)
    } else {
        dir.join(format!("{name}.lua"))
    }
}
