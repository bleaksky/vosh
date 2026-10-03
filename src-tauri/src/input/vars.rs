//! The `#var`, `#unvar` and `#vars` commands on your variables.

use vosh_automation::vars::Scope;

use super::{echo_one, error_echo, split_first_word, InputResult};
use crate::profile::Profile;

pub(super) fn slash_var(profile: &mut Profile, args: &str) -> InputResult {
    let (name, value) = split_first_word(args);
    if name.is_empty() {
        return error_echo("usage #var <name> [value]".to_string());
    }
    if value.is_empty() {
        return match profile.vars.get(name) {
            Some(v) => echo_one(format!("{name} = {v}")),
            None => error_echo(format!("var {name} not set")),
        };
    }
    profile.vars.set(Scope::Session, name, value);
    echo_one(format!("var {name} set"))
}

pub(super) fn slash_unvar(profile: &mut Profile, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return error_echo("usage #unvar <name>".to_string());
    }
    if profile.vars.remove(name) {
        echo_one(format!("var {name} removed"))
    } else {
        error_echo(format!("var {name} not set"))
    }
}

pub(super) fn slash_vars_list(profile: &Profile) -> InputResult {
    let mut entries: Vec<_> = profile
        .vars
        .iter()
        .map(|(k, v, scope)| (k.to_string(), v.to_string(), scope))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.is_empty() {
        return echo_one("no variables defined".to_string());
    }
    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!("{} variable(s):", entries.len()));
    for (name, value, scope) in entries {
        let s = match scope {
            Scope::Profile => "profile",
            Scope::Session => "session",
        };
        lines.push(format!("  {s:<7} {name} = {value}"));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}
