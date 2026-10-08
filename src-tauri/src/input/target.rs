//! Target words and quick keys. `tar` and its kin pick whom your
//! commands aim at from the characters in the room, and a quick key
//! like `gg` sends its verb at that target. Both live on the
//! [`Connection`], and its session variable `target` mirrors the target.

use serde_json::Value;
use vosh_automation::alias::AliasStore;

use super::{split_first_word, InputResult};
use crate::session::connection::{Connection, QuickKey, RoomChar};

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
fn refresh_target_idx(c: &mut Connection) {
    c.target.room_idx = target_place(c.target.name.as_deref(), &c.room_chars);
    // Mirror the user target into the session's variables so `${target}`
    // works in alias expansions and Lua `mud.var("target")` reads it.
    // Clearing the target removes the variable rather than leaving it
    // empty, so an alias that names it keeps the token as typed. A
    // profile variable of the name stays, for every session on the
    // profile. Char.Combat's `target_name` stays separate (it's the
    // server-confirmed combat target).
    if let Some(name) = &c.target.name {
        c.vars.set("target", name.clone());
    } else {
        c.vars.remove("target");
    }
}

/// The place from 1 in `chars` of the target `name`, the first whose
/// name holds it, ignoring case, as `refresh_target_idx` finds it.
pub(crate) fn target_place(name: Option<&str>, chars: &[RoomChar]) -> Option<usize> {
    let lower = name?.to_ascii_lowercase();
    chars
        .iter()
        .position(|ch| ch.name.to_ascii_lowercase().contains(&lower))
        .map(|i| i + 1)
}

/// The characters a Room.Chars packet lists, in its order. An entry
/// without a name, or with an empty one, is skipped. The server may send
/// `npc` as a bool, a string or a number, so each form reads, and an
/// entry without it counts as a player.
pub(crate) fn read_room_chars(entries: &[Value]) -> Vec<RoomChar> {
    entries
        .iter()
        .filter_map(|v| {
            let obj = v.as_object()?;
            let name = obj.get("name").and_then(|n| n.as_str())?.to_string();
            if name.is_empty() {
                return None;
            }
            let npc = match obj.get("npc") {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => s == "1" || s == "true",
                Some(Value::Number(n)) => n.as_i64().is_some_and(|x| x != 0),
                _ => false,
            };
            Some(RoomChar { name, npc })
        })
        .collect()
}

/// Keep `chars` as the room list and find your target in it again.
pub(crate) fn set_room_chars(c: &mut Connection, chars: Vec<RoomChar>) {
    c.room_chars = chars;
    refresh_target_idx(c);
}

pub(super) fn run_target_set(c: &mut Connection, args: &str) -> InputResult {
    let arg = args.trim();
    if arg.is_empty() {
        return list_targets(c);
    }
    // Numeric → pick from room chars by 1-based index. This is the
    // one path that resolves to the full server-supplied name, since
    // an index alone isn't usable as a command keyword.
    if let Ok(n) = arg.parse::<usize>() {
        if n == 0 || n > c.room_chars.len() {
            return InputResult::error(format!(
                "no char #{n} in room (have {})",
                c.room_chars.len()
            ));
        }
        let name = c.room_chars[n - 1].name.clone();
        c.target.name = Some(name.clone());
        refresh_target_idx(c);
        return InputResult::echo_line(format!("target: {name}"));
    }
    // Non-numeric → use the literal string the user typed. The MUD
    // parses commands with its own keyword matching, so short forms
    // like `tar gris` are what the user actually wants to send back
    // as `kill gris` rather than the full `The Baron Grisvald`.
    // We still look for a containing room char to drive the `>`
    // marker on the room chip but don't substitute the name.
    c.target.name = Some(arg.to_string());
    refresh_target_idx(c);
    if c.target.room_idx.is_some() {
        InputResult::echo_line(format!("target: {arg}"))
    } else {
        InputResult::echo_line(format!("target: {arg} (not in room)"))
    }
}

pub(super) fn run_target_cycle(c: &mut Connection, step: i32) -> InputResult {
    let n = c.room_chars.len();
    if n == 0 {
        return InputResult::error("no chars in room to cycle through");
    }
    let current = c.target.room_idx.unwrap_or(0) as i32;
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
    let name = c.room_chars[(next - 1) as usize].name.clone();
    c.target.name = Some(name.clone());
    refresh_target_idx(c);
    InputResult::echo_line(format!("target: {name} (#{next}/{count})"))
}

pub(super) fn run_target_clear(c: &mut Connection) -> InputResult {
    if c.target.name.is_none() {
        return InputResult::echo_line("no target to clear");
    }
    c.target.name = None;
    refresh_target_idx(c);
    InputResult::echo_line("target cleared")
}

fn list_targets(c: &Connection) -> InputResult {
    let mut lines: Vec<String> = Vec::new();
    match &c.target.name {
        Some(t) => lines.push(format!("current target: {t}")),
        None => lines.push("no target set".to_string()),
    }
    if c.room_chars.is_empty() {
        lines.push("(no Room.Chars data yet)".to_string());
    } else {
        lines.push(format!("{} char(s) in room:", c.room_chars.len()));
        for (i, ch) in c.room_chars.iter().enumerate() {
            let marker = if Some(i + 1) == c.target.room_idx {
                ">"
            } else {
                " "
            };
            let kind = if ch.npc { "npc" } else { "pc" };
            lines.push(format!("  {marker} {:>2}. {} [{kind}]", i + 1, ch.name));
        }
        lines.push("usage: tar <N> | tar <substring> | tarn | tarp | tarclear".to_string());
    }
    InputResult::echo_lines(lines)
}

/// `#target <args>` mirrors the bare `tar` shortcut.
pub(super) fn slash_target(c: &mut Connection, args: &str) -> InputResult {
    let trimmed = args.trim();
    if trimmed == "clear" {
        return run_target_clear(c);
    }
    if trimmed == "next" {
        return run_target_cycle(c, 1);
    }
    if trimmed == "prev" {
        return run_target_cycle(c, -1);
    }
    run_target_set(c, args)
}

/// `#qkey`, which sets a quick key on `c`, or clears one. A name your
/// `aliases` use stays theirs.
pub(super) fn slash_qkey(c: &mut Connection, aliases: &AliasStore, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return InputResult::error("usage: #qkey <name> <verb>  |  #qkey clear <name>");
    }
    if name == "clear" {
        let target = rest.trim();
        if target.is_empty() {
            return InputResult::error("usage: #qkey clear <name>");
        }
        let before = c.target.quick_keys.len();
        c.target.quick_keys.retain(|q| q.name != target);
        if c.target.quick_keys.len() == before {
            return InputResult::error(format!("quick-key `{target}` not found"));
        }
        return InputResult::echo_line(format!("quick-key `{target}` removed"));
    }
    // Reserved keywords and existing aliases can't be shadowed.
    if is_target_keyword(name) {
        return InputResult::error(format!(
            "`{name}` is a target keyword — pick another quick-key name"
        ));
    }
    if aliases.get(name).is_some() {
        return InputResult::error(format!(
            "alias `{name}` exists — `#unalias {name}` first if you want this name"
        ));
    }
    let verb = rest.trim();
    if verb.is_empty() {
        return InputResult::error(format!("usage: #qkey {name} <verb>"));
    }
    // Update in place if it exists, otherwise append.
    match c.target.quick_keys.iter_mut().find(|q| q.name == name) {
        Some(qk) => qk.verb = verb.to_string(),
        None => c.target.quick_keys.push(QuickKey {
            name: name.to_string(),
            verb: verb.to_string(),
        }),
    }
    InputResult::echo_line(format!("quick-key `{name}` -> {verb}"))
}

pub(super) fn slash_qkeys_list(c: &Connection) -> InputResult {
    if c.target.quick_keys.is_empty() {
        return InputResult::echo_line("no quick-keys defined");
    }
    let mut lines = vec![format!("{} quick-key(s):", c.target.quick_keys.len())];
    for qk in &c.target.quick_keys {
        let verb = if qk.verb.is_empty() {
            "(unset)"
        } else {
            qk.verb.as_str()
        };
        lines.push(format!("  {:>4}  ->  {verb}", qk.name));
    }
    InputResult::echo_lines(lines)
}
