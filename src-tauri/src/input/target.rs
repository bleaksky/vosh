//! Target words and quick keys. `tar` and its kin pick whom your
//! commands aim at from the characters in the room, and a quick key
//! like `gg` sends its verb at that target.

use vosh_automation::vars::Scope;

use super::{echo_one, error_echo, split_first_word, InputResult};
use crate::profile::{Profile, QuickKey, RoomChar};

const TARGET_KEYWORDS: &[&str] = &["tar", "tarn", "tarp", "tarclear"];

pub(super) fn is_target_keyword(name: &str) -> bool {
    TARGET_KEYWORDS.contains(&name)
}

/// Recompute `room_idx` from the current room snapshot. Called whenever
/// the target name changes or the room chars push refreshes the list.
///
/// Matching is **substring, case-insensitive**: typing `tar gris`
/// stores "gris" as the name (so commands use the user's keyword)
/// but resolves `room_idx` to whichever char contains "gris" so the
/// `>` marker shows on the right chip. First match wins.
fn refresh_target_idx(profile: &mut Profile) {
    profile.target.room_idx = match &profile.target.name {
        None => None,
        Some(name) => {
            let lower = name.to_ascii_lowercase();
            profile
                .room_chars
                .iter()
                .position(|c| c.name.to_ascii_lowercase().contains(&lower))
                .map(|i| i + 1)
        }
    };
    // Mirror the user target into the variable store so `${target}`
    // works in alias expansions and Lua `mud.var("target")` reads it.
    // Clearing the target removes the variable rather than leaving it
    // empty, so an alias that names it keeps the token as typed.
    // Char.Combat's `target_name` stays separate (it's the
    // server-confirmed combat target).
    if let Some(name) = &profile.target.name {
        profile.vars.set(Scope::Session, "target", name.clone());
    } else {
        profile.vars.remove("target");
    }
}

pub(crate) fn set_room_chars(profile: &mut Profile, chars: Vec<RoomChar>) {
    profile.room_chars = chars;
    refresh_target_idx(profile);
}

pub(super) fn run_target_set(profile: &mut Profile, args: &str) -> InputResult {
    let arg = args.trim();
    if arg.is_empty() {
        return list_targets(profile);
    }
    // Numeric → pick from room chars by 1-based index. This is the
    // one path that resolves to the full server-supplied name, since
    // an index alone isn't usable as a command keyword.
    if let Ok(n) = arg.parse::<usize>() {
        if n == 0 || n > profile.room_chars.len() {
            return error_echo(format!(
                "no char #{n} in room (have {})",
                profile.room_chars.len()
            ));
        }
        let name = profile.room_chars[n - 1].name.clone();
        profile.target.name = Some(name.clone());
        refresh_target_idx(profile);
        return echo_one(format!("target: {name}"));
    }
    // Non-numeric → use the literal string the user typed. The MUD
    // parses commands with its own keyword matching, so short forms
    // like `tar gris` are what the user actually wants to send back
    // as `kill gris` rather than the full `The Baron Grisvald`.
    // We still look for a containing room char to drive the `>`
    // marker on the room chip but don't substitute the name.
    profile.target.name = Some(arg.to_string());
    refresh_target_idx(profile);
    if profile.target.room_idx.is_some() {
        echo_one(format!("target: {arg}"))
    } else {
        echo_one(format!("target: {arg} (not in room)"))
    }
}

pub(super) fn run_target_cycle(profile: &mut Profile, step: i32) -> InputResult {
    let n = profile.room_chars.len();
    if n == 0 {
        return error_echo("no chars in room to cycle through".to_string());
    }
    let current = profile.target.room_idx.unwrap_or(0) as i32;
    let count = n as i32;
    // 1-based wraparound. step=+1 goes forward, -1 backward.
    let next = if current == 0 {
        if step >= 0 {
            1
        } else {
            count
        }
    } else {
        let raw = current + step;
        if raw < 1 {
            count
        } else if raw > count {
            1
        } else {
            raw
        }
    };
    let name = profile.room_chars[(next - 1) as usize].name.clone();
    profile.target.name = Some(name.clone());
    refresh_target_idx(profile);
    echo_one(format!("target: {name} (#{next}/{count})"))
}

pub(super) fn run_target_clear(profile: &mut Profile) -> InputResult {
    if profile.target.name.is_none() {
        return echo_one("no target to clear".to_string());
    }
    profile.target.name = None;
    refresh_target_idx(profile);
    echo_one("target cleared".to_string())
}

fn list_targets(profile: &Profile) -> InputResult {
    let mut lines: Vec<String> = Vec::new();
    match &profile.target.name {
        Some(t) => lines.push(format!("current target: {t}")),
        None => lines.push("no target set".to_string()),
    }
    if profile.room_chars.is_empty() {
        lines.push("(no Room.Chars data yet)".to_string());
    } else {
        lines.push(format!("{} char(s) in room:", profile.room_chars.len()));
        for (i, c) in profile.room_chars.iter().enumerate() {
            let marker = if Some(i + 1) == profile.target.room_idx {
                ">"
            } else {
                " "
            };
            let kind = if c.npc { "npc" } else { "pc" };
            lines.push(format!("  {marker} {:>2}. {} [{kind}]", i + 1, c.name));
        }
        lines.push("usage: tar <N> | tar <substring> | tarn | tarp | tarclear".to_string());
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

/// `#target <args>` mirrors the bare `tar` shortcut.
pub(super) fn slash_target(profile: &mut Profile, args: &str) -> InputResult {
    let trimmed = args.trim();
    if trimmed == "clear" {
        return run_target_clear(profile);
    }
    if trimmed == "next" {
        return run_target_cycle(profile, 1);
    }
    if trimmed == "prev" {
        return run_target_cycle(profile, -1);
    }
    run_target_set(profile, args)
}

pub(super) fn slash_qkey(profile: &mut Profile, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return error_echo("usage: #qkey <name> <verb>  |  #qkey clear <name>".to_string());
    }
    if name == "clear" {
        let target = rest.trim();
        if target.is_empty() {
            return error_echo("usage: #qkey clear <name>".to_string());
        }
        let before = profile.target.quick_keys.len();
        profile.target.quick_keys.retain(|q| q.name != target);
        if profile.target.quick_keys.len() == before {
            return error_echo(format!("quick-key `{target}` not found"));
        }
        return echo_one(format!("quick-key `{target}` removed"));
    }
    // Reserved keywords and existing aliases can't be shadowed.
    if is_target_keyword(name) {
        return error_echo(format!(
            "`{name}` is a target keyword — pick another quick-key name"
        ));
    }
    if profile.aliases.get(name).is_some() {
        return error_echo(format!(
            "alias `{name}` exists — `#unalias {name}` first if you want this name"
        ));
    }
    let verb = rest.trim();
    if verb.is_empty() {
        return error_echo(format!("usage: #qkey {name} <verb>"));
    }
    // Update in place if it exists, otherwise append.
    match profile
        .target
        .quick_keys
        .iter_mut()
        .find(|q| q.name == name)
    {
        Some(qk) => qk.verb = verb.to_string(),
        None => profile.target.quick_keys.push(QuickKey {
            name: name.to_string(),
            verb: verb.to_string(),
        }),
    }
    echo_one(format!("quick-key `{name}` -> {verb}"))
}

pub(super) fn slash_qkeys_list(profile: &Profile) -> InputResult {
    if profile.target.quick_keys.is_empty() {
        return echo_one("no quick-keys defined".to_string());
    }
    let mut lines = vec![format!("{} quick-key(s):", profile.target.quick_keys.len())];
    for qk in &profile.target.quick_keys {
        let verb = if qk.verb.is_empty() {
            "(unset)"
        } else {
            qk.verb.as_str()
        };
        lines.push(format!("  {:>4}  ->  {verb}", qk.name));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}
