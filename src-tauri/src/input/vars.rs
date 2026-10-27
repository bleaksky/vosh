//! The `#var`, `#unvar` and `#vars` commands on your variables, and
//! `#list` and `#lists` on your lists. `#var` sets a session variable,
//! which stays in the session you type it in, and `#list` makes a session
//! list, which `#list <name> keep` moves to the profile. `#unvar` removes
//! a variable or a list.

use vosh_automation::vars::{Scope, LIST_ITEMS};

use super::{split_first_word, InputResult};
use crate::profile::live::Profile;
use crate::session::connection::Connection;

pub(super) fn slash_var(profile: &Profile, c: &mut Connection, args: &str) -> InputResult {
    let (name, value) = split_first_word(args);
    if name.is_empty() {
        return InputResult::error("usage #var <name> [value]");
    }
    if value.is_empty() {
        return match c.var_view(profile).get(name) {
            Some(v) => InputResult::echo_line(format!("{name} = {v}")),
            None => InputResult::error(format!("var {name} not set")),
        };
    }
    c.vars.set(name, value);
    InputResult::echo_line(format!("var {name} set"))
}

/// `#unvar` removes the name from both scopes, so the profile's value
/// goes for every session on the profile.
pub(super) fn slash_unvar(profile: &mut Profile, c: &mut Connection, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return InputResult::error("usage #unvar <name>");
    }
    let session = c.vars.remove(name);
    let shared = profile.vars.remove(name);
    if session || shared {
        InputResult::echo_line(format!("var {name} removed"))
    } else {
        InputResult::error(format!("var {name} not set"))
    }
}

pub(super) fn slash_vars_list(profile: &Profile, c: &Connection) -> InputResult {
    let mut entries: Vec<_> = c
        .var_view(profile)
        .iter()
        .map(|(k, v, scope)| (k.to_string(), v.to_string(), scope))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.is_empty() {
        return InputResult::echo_line("no variables defined");
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
    InputResult::echo_lines(lines)
}

/// The most bytes one list item holds, the variable limit.
const ITEM_BYTES: usize = 4 * 1024;

/// Whether `name` can name a variable or a list, as `$name` reads one.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The place `word` names in a list of `len` items, counted from 1, or
/// from the end below 0. `room` lets it name the place after the last,
/// as an insert can.
fn place(word: &str, len: usize, room: bool) -> Option<usize> {
    let n: i64 = word.parse().ok()?;
    let len_i = i64::try_from(len).ok()?;
    let at = if n < 0 { len_i + n } else { n - 1 };
    let top = if room { len_i } else { len_i - 1 };
    (0..=top).contains(&at).then(|| usize::try_from(at).ok())?
}

fn items_phrase(n: usize) -> String {
    if n == 1 {
        "1 item".to_string()
    } else {
        format!("{n} items")
    }
}

/// The list `name` in the scope that holds it, a new session list when
/// `create` asks and none does.
fn list_mut<'a>(
    profile: &'a mut Profile,
    c: &'a mut Connection,
    name: &str,
    create: bool,
) -> Option<&'a mut Vec<String>> {
    if c.vars.list(name).is_some() {
        return c.vars.list_mut(name);
    }
    if profile.vars.list(name).is_some() {
        return profile.vars.list_mut(name);
    }
    if !create {
        return None;
    }
    c.vars.set_list(name, Vec::new());
    c.vars.list_mut(name)
}

/// `#list <name> [add|insert|delete|set|find|sort|clear|keep] ...`, on the
/// list of that name in the scope that holds it. A new list is the
/// session's, as `#var` makes a session variable, and `keep` moves it
/// to the profile, so it lasts past the session. A variable of the same
/// name refuses, since a name is one or the other.
pub(super) fn slash_list(profile: &mut Profile, c: &mut Connection, args: &str) -> InputResult {
    const USAGE: &str =
        "usage #list <name> [add|insert <n>|delete <n>|set <n>|find|sort|clear|keep] [items]";
    let (name, rest) = split_first_word(args);
    if !is_name(name) {
        return InputResult::error(USAGE);
    }
    let (cmd, rest) = split_first_word(rest);
    if c.var_view(profile).get(name).is_some() {
        return InputResult::error(format!(
            "{name} is a variable. Remove it with #unvar {name} first"
        ));
    }
    let items = vosh_automation::vars::split_items(rest);
    if let Some(long) = items.iter().find(|item| item.len() > ITEM_BYTES) {
        return InputResult::error(format!(
            "an item holds at most {ITEM_BYTES} bytes, and one holds {}",
            long.len()
        ));
    }
    let full = |list: &Vec<String>, more: usize| list.len() + more > LIST_ITEMS;
    match cmd {
        "" => show_list(profile, c, name),
        "add" => {
            if items.is_empty() {
                return InputResult::error(USAGE);
            }
            let Some(list) = list_mut(profile, c, name, true) else {
                return InputResult::error(USAGE);
            };
            if full(list, items.len()) {
                return InputResult::error(format!("a list holds at most {LIST_ITEMS} items"));
            }
            list.extend(items);
            InputResult::echo_line(format!("{name} holds {}", items_phrase(list.len())))
        }
        "insert" => {
            let (at, rest) = split_first_word(rest);
            let items = vosh_automation::vars::split_items(rest);
            if items.is_empty() {
                return InputResult::error(USAGE);
            }
            let Some(list) = list_mut(profile, c, name, true) else {
                return InputResult::error(USAGE);
            };
            let Some(at) = place(at, list.len(), true) else {
                return InputResult::error(format!("{name} has no place {at}"));
            };
            if full(list, items.len()) {
                return InputResult::error(format!("a list holds at most {LIST_ITEMS} items"));
            }
            let len = items.len();
            list.splice(at..at, items);
            InputResult::echo_line(format!(
                "{name} holds {}, with {} at {}",
                items_phrase(list.len()),
                items_phrase(len),
                at + 1
            ))
        }
        "delete" | "set" => {
            let (at, rest) = split_first_word(rest);
            let Some(list) = list_mut(profile, c, name, false) else {
                return InputResult::error(format!("list {name} not set"));
            };
            let Some(at) = place(at, list.len(), false) else {
                return InputResult::error(format!("{name} has no item {at}"));
            };
            if cmd == "delete" {
                let gone = list.remove(at);
                return InputResult::echo_line(format!("removed {gone} from {name}"));
            }
            let item = vosh_automation::vars::split_items(rest);
            let [item] = item.as_slice() else {
                return InputResult::error(format!("usage #list {name} set <n> <item>"));
            };
            list[at].clone_from(item);
            InputResult::echo_line(format!("{name} item {} is {item}", at + 1))
        }
        "find" => {
            let [item] = items.as_slice() else {
                return InputResult::error(format!("usage #list {name} find <item>"));
            };
            let Some((list, _)) = c.var_view(profile).list(name) else {
                return InputResult::error(format!("list {name} not set"));
            };
            match list.iter().position(|it| it == item) {
                Some(at) => InputResult::echo_line(format!("{item} is item {} of {name}", at + 1)),
                None => InputResult::error(format!("{item} is not in {name}")),
            }
        }
        "sort" | "clear" => {
            let Some(list) = list_mut(profile, c, name, false) else {
                return InputResult::error(format!("list {name} not set"));
            };
            if cmd == "sort" {
                list.sort_by_key(|item| item.to_lowercase());
                InputResult::echo_line(format!("{name} sorted"))
            } else {
                list.clear();
                InputResult::echo_line(format!("{name} cleared"))
            }
        }
        "keep" => match c.vars.take_list(name) {
            Some(list) => {
                profile.vars.set_list(name, list);
                InputResult::echo_line(format!("{name} kept in the profile"))
            }
            None if profile.vars.list(name).is_some() => {
                InputResult::echo_line(format!("{name} is in the profile already"))
            }
            None => InputResult::error(format!("list {name} not set")),
        },
        _ => InputResult::error(USAGE),
    }
}

/// `#list <name>`: the items of the list, one a line.
fn show_list(profile: &Profile, c: &Connection, name: &str) -> InputResult {
    let Some((items, scope)) = c.var_view(profile).list(name) else {
        return InputResult::error(format!("list {name} not set"));
    };
    if items.is_empty() {
        return InputResult::echo_line(format!("list {name} is empty"));
    }
    let scope = match scope {
        Scope::Profile => "profile",
        Scope::Session => "session",
    };
    let mut lines = vec![format!(
        "list {name} ({scope}), {}:",
        items_phrase(items.len())
    )];
    let width = items.len().to_string().len();
    for (at, item) in items.iter().enumerate() {
        lines.push(format!("  {:>width$}  {item}", at + 1));
    }
    InputResult::echo_lines(lines)
}

/// `#lists`: every list, with its scope and how many items it holds.
pub(super) fn slash_lists(profile: &Profile, c: &Connection) -> InputResult {
    let mut entries: Vec<_> = c
        .var_view(profile)
        .lists()
        .map(|(name, items, scope)| (name.to_string(), items.len(), scope))
        .collect();
    if entries.is_empty() {
        return InputResult::echo_line("no lists defined");
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!("{} list(s):", entries.len()));
    for (name, len, scope) in entries {
        let s = match scope {
            Scope::Profile => "profile",
            Scope::Session => "session",
        };
        lines.push(format!("  {s:<7} {name}  {}", items_phrase(len)));
    }
    InputResult::echo_lines(lines)
}
