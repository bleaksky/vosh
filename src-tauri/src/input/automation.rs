//! The commands that change your aliases, triggers and groups, and
//! the macro recorder that saves what you type as an alias.

use vosh_automation::trigger::{
    HighlightStyle, MatchMode, NamedColor, Trigger, TriggerAction, TriggerPattern,
};

use super::slash::parse_braced_pattern;
use super::target::is_target_keyword;
use super::{split_first_word, InputResult};
use crate::profile::live::Profile;
use crate::session::connection::{Connection, MacroRecorder};

pub(super) fn slash_alias(profile: &mut Profile, c: &Connection, args: &str) -> InputResult {
    let (name, expansion) = split_first_word(args);
    if name.is_empty() {
        return InputResult::error("usage #alias <name> <expansion>");
    }
    if expansion.is_empty() {
        return InputResult::error("usage #alias <name> <expansion>");
    }
    // Reserved target keywords and existing quick-keys can't be
    // shadowed — the input pipeline checks both before alias
    // expansion, so an alias with the same name would silently never
    // fire.
    if is_target_keyword(name) {
        return InputResult::error(format!(
            "`{name}` is a target keyword — pick another alias name"
        ));
    }
    if c.target.quick_keys.iter().any(|q| q.name == name) {
        return InputResult::error(format!(
            "quick-key `{name}` exists — `#qkey clear {name}` first if you want this name"
        ));
    }
    let label = crate::script::define_alias(profile, c.stop_key, name, expansion);
    InputResult::echo_line(format!("alias {label} set"))
}

/// `#unalias <name> [group]`. With a group it removes the alias of that
/// name in that group. Without one it removes the alias of that name in
/// no group, or the only alias of that name, and asks for the group
/// when more than one group holds the name.
pub(super) fn slash_unalias(profile: &mut Profile, args: &str) -> InputResult {
    let (name, group) = split_first_word(args.trim());
    if name.is_empty() {
        return InputResult::error("usage #unalias <name> [group]");
    }
    let group = Some(group.trim()).filter(|g| !g.is_empty());
    let found = profile.aliases.named(name);
    let target = match group {
        Some(group) => profile.aliases.get_in(Some(group), name),
        None => profile
            .aliases
            .get_in(None, name)
            .or_else(|| (found.len() == 1).then(|| &found[0])),
    };
    let Some(target) = target.cloned() else {
        if group.is_none() && found.len() > 1 {
            let groups: Vec<&str> = found.iter().filter_map(|a| a.group.as_deref()).collect();
            let example = groups.first().copied().unwrap_or_default();
            return InputResult::error(format!(
                "{name} is in {}, so name the group too, like #unalias {name} {example}",
                join_and(&groups)
            ));
        }
        let label = match group {
            Some(group) => format!("{name} in {group}"),
            None => name.to_string(),
        };
        return InputResult::error(format!("alias {label} not found"));
    };
    profile
        .aliases
        .remove(target.group.as_deref(), &target.name);
    InputResult::echo_line(format!("alias {} removed", target.label()))
}

/// `a`, `a and b`, or `a, b and c`.
fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub(super) fn slash_aliases_list(profile: &Profile, c: &Connection) -> InputResult {
    let aliases = profile.aliases.list();
    let from_plugins = c.plugin_aliases.list();
    if aliases.is_empty() && from_plugins.is_empty() {
        return InputResult::echo_line("no aliases defined");
    }
    let count = aliases.len() + from_plugins.len();
    let mut lines = Vec::with_capacity(count + 1);
    lines.push(format!("{count} alias(es):"));
    for a in aliases {
        let mark = if a.enabled { ' ' } else { '*' };
        lines.push(format!("  {mark} {} -> {}", a.label(), a.expansion));
    }
    // A plugin's aliases last while it runs, and Vosh never saves them.
    for (plugin, a) in from_plugins {
        lines.push(format!(
            "    {} -> {} from plugin {plugin}",
            a.name, a.expansion
        ));
    }
    InputResult::echo_lines(lines)
}

pub(super) fn slash_trigger(profile: &mut Profile, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return InputResult::error("usage #trigger <name> {pattern} <action> [args]");
    }
    let Some((pattern, after_pattern)) = parse_braced_pattern(rest) else {
        return InputResult::error("usage #trigger <name> {pattern} <action> [args]");
    };
    let action = match parse_action(after_pattern) {
        Ok(a) => a,
        Err(msg) => return InputResult::error(msg),
    };
    let trigger = Trigger::new(name, pattern, action);
    match profile.triggers.set(trigger) {
        Ok(()) => InputResult::echo_line(format!("trigger {name} set")),
        Err(e) => InputResult::error(format!("trigger {name} rejected: {e}")),
    }
}

pub(super) fn slash_untrigger(profile: &mut Profile, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return InputResult::error("usage #untrigger <name>");
    }
    if profile.triggers.remove(name) {
        InputResult::echo_line(format!("trigger {name} removed"))
    } else {
        InputResult::error(format!("trigger {name} not found"))
    }
}

pub(super) fn slash_triggers_list(profile: &Profile) -> InputResult {
    let triggers = profile.triggers.list();
    if triggers.is_empty() {
        return InputResult::echo_line("no triggers defined");
    }
    let mut lines = Vec::with_capacity(triggers.len() + 1);
    lines.push(format!("{} trigger(s) by priority:", triggers.len()));
    for t in triggers {
        let mark = if t.enabled { ' ' } else { '*' };
        let action = t
            .actions
            .iter()
            .map(describe_action)
            .collect::<Vec<_>>()
            .join(" + ");
        let pattern = t
            .patterns
            .first()
            .map_or_else(|| "//".to_string(), describe_pattern);
        lines.push(format!(
            "  {mark} [{:>3}] {} {pattern} -> {action}",
            t.priority, t.name,
        ));
    }
    InputResult::echo_lines(lines)
}

/// A pattern as `#triggers` lists it. A regex sits between slashes, and
/// Text and Starts with name their mode before the text in quotes, so a
/// dot in them never reads as a regex dot.
fn describe_pattern(row: &TriggerPattern) -> String {
    match row.mode {
        MatchMode::Regex => format!("/{}/", row.pattern),
        MatchMode::Text => format!("text \"{}\"", row.pattern),
        MatchMode::StartsWith => format!("starts with \"{}\"", row.pattern),
    }
}

fn parse_action(input: &str) -> Result<TriggerAction, String> {
    let (kind, rest) = split_first_word(input);
    match kind {
        "highlight" => parse_highlight_action(rest),
        "gag" => Ok(TriggerAction::Gag),
        "replace" => {
            if rest.is_empty() {
                Err("usage replace <template>".to_string())
            } else {
                Ok(TriggerAction::Replace {
                    template: rest.to_string(),
                })
            }
        }
        "send" => {
            if rest.is_empty() {
                Err("usage send <template>".to_string())
            } else {
                Ok(TriggerAction::Send {
                    template: rest.to_string(),
                })
            }
        }
        "route" => {
            if rest.is_empty() {
                Err("usage route <pane>".to_string())
            } else {
                Ok(TriggerAction::Route {
                    pane: rest.to_string(),
                })
            }
        }
        "" => Err("missing action keyword".to_string()),
        other => Err(format!("unknown action `{other}`")),
    }
}

fn parse_highlight_action(input: &str) -> Result<TriggerAction, String> {
    let mut style = HighlightStyle::default();
    for token in input.split_whitespace() {
        match token.to_ascii_lowercase().as_str() {
            "bold" => style.bold = true,
            "underline" => style.underline = true,
            "inverse" => style.inverse = true,
            "wash" => style.wash = true,
            other => {
                if let Some(name) = other.strip_prefix("bg:") {
                    let color =
                        NamedColor::parse(name).ok_or_else(|| format!("unknown color `{name}`"))?;
                    style.bg = Some(color);
                } else if let Some(color) = NamedColor::parse(other) {
                    style.fg = Some(color);
                } else {
                    return Err(format!("unknown highlight token `{other}`"));
                }
            }
        }
    }
    if style.is_empty() {
        return Err("highlight needs at least one color or attribute".to_string());
    }
    Ok(TriggerAction::Highlight { style })
}

fn describe_action(action: &TriggerAction) -> String {
    match action {
        TriggerAction::Highlight { style } => {
            let mut parts = Vec::new();
            if let Some(c) = style.fg {
                parts.push(format!("fg={c:?}"));
            }
            if let Some(c) = style.bg {
                parts.push(format!("bg={c:?}"));
            }
            if style.bold {
                parts.push("bold".to_string());
            }
            if style.underline {
                parts.push("underline".to_string());
            }
            if style.inverse {
                parts.push("inverse".to_string());
            }
            if style.wash {
                parts.push("wash".to_string());
            }
            format!("highlight {}", parts.join(" "))
        }
        TriggerAction::Gag => "gag".to_string(),
        TriggerAction::Replace { template } => format!("replace `{template}`"),
        TriggerAction::Send { template } => format!("send `{template}`"),
        TriggerAction::Route { pane } => format!("route {pane}"),
        TriggerAction::Script { body } => {
            let preview: String = body.chars().take(40).collect();
            let ellipsis = if body.chars().count() > 40 { "…" } else { "" };
            format!("script `{preview}{ellipsis}`")
        }
    }
}

/// `#group <name> [on|off]` — flip a group's enabled state across
/// triggers, aliases, macros and timers in one call. With no on/off
/// arg, echo the current state in each store the group appears in.
pub(super) fn slash_group(profile: &mut Profile, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return InputResult::error("usage #group <name> [on|off]");
    }
    let state = rest.trim();
    match state {
        "" => slash_group_show(profile, name),
        "on" | "off" => {
            let enabled = state == "on";
            let report = crate::script::toggle_group(profile, name, enabled);
            if !report.touched() {
                return InputResult::error(format!(
                    "group `{name}` not found in triggers, aliases, macros, or timers"
                ));
            }
            let mut stores: Vec<&str> = Vec::with_capacity(4);
            if report.triggers {
                stores.push("triggers");
            }
            if report.aliases {
                stores.push("aliases");
            }
            if report.macros {
                stores.push("macros");
            }
            if report.timers {
                stores.push("timers");
            }
            InputResult::echo_line(format!(
                "group `{name}` {} for {}",
                if enabled { "enabled" } else { "disabled" },
                stores.join(" + "),
            ))
        }
        other => InputResult::error(format!(
            "unknown group state `{other}`. usage #group <name> [on|off]"
        )),
    }
}

fn slash_group_show(profile: &Profile, name: &str) -> InputResult {
    use crate::script::GroupState;
    let states = crate::script::group_states(profile, name);
    let [trigger_state, alias_state, macro_state, timer_state] = states;
    if states.iter().all(Option::is_none) {
        return InputResult::error(format!(
            "group `{name}` not found in triggers, aliases, macros, or timers"
        ));
    }
    let mut lines = vec![format!("group `{name}`:")];
    let fmt = |store: &str, state: Option<GroupState>| match state {
        Some(GroupState::On) => format!("  {store}: on"),
        Some(GroupState::Off) => format!("  {store}: off"),
        Some(GroupState::Mixed) => format!("  {store}: partly on"),
        None => format!("  {store}: (none tagged)"),
    };
    lines.push(fmt("triggers", trigger_state));
    lines.push(fmt("aliases ", alias_state));
    lines.push(fmt("macros  ", macro_state));
    lines.push(fmt("timers  ", timer_state));
    InputResult::echo_lines(lines)
}

/// `#groups` — every group that any store, timers included, has at
/// least one entry tagged with, plus the current on/off state per store.
pub(super) fn slash_groups_list(profile: &Profile) -> InputResult {
    use std::collections::BTreeSet;
    let mut names: BTreeSet<String> = BTreeSet::new();
    let trigger_map: std::collections::BTreeMap<String, bool> =
        profile.triggers.groups().into_iter().collect();
    let alias_map: std::collections::BTreeMap<String, bool> =
        profile.aliases.groups().into_iter().collect();
    for g in trigger_map.keys() {
        names.insert(g.clone());
    }
    for g in alias_map.keys() {
        names.insert(g.clone());
    }
    for m in &profile.macros {
        if let Some(g) = &m.group {
            if !g.is_empty() {
                names.insert(g.clone());
            }
        }
    }
    let timer_groups = crate::script::timer_groups(profile);
    names.extend(timer_groups.iter().cloned());
    if names.is_empty() {
        return InputResult::echo_line("no groups defined");
    }
    let mut lines = vec![format!("{} group(s):", names.len())];
    for name in &names {
        let has_macros = profile
            .macros
            .iter()
            .any(|m| m.group.as_deref() == Some(name.as_str()));
        let parts: Vec<String> = [
            (
                "triggers",
                trigger_map
                    .get(name)
                    .copied()
                    .map(|e| if e { "on" } else { "off" }),
            ),
            (
                "aliases",
                alias_map
                    .get(name)
                    .copied()
                    .map(|e| if e { "on" } else { "off" }),
            ),
            (
                "macros",
                if has_macros {
                    Some(if profile.disabled_macro_groups.contains(name) {
                        "off"
                    } else {
                        "on"
                    })
                } else {
                    None
                },
            ),
            (
                "timers",
                timer_groups.contains(name).then(|| {
                    if profile.disabled_timer_groups.contains(name) {
                        "off"
                    } else {
                        "on"
                    }
                }),
            ),
        ]
        .into_iter()
        .filter_map(|(store, state)| state.map(|s| format!("{store}={s}")))
        .collect();
        lines.push(format!("  {name}: {}", parts.join(", ")));
    }
    InputResult::echo_lines(lines)
}

pub(super) fn slash_record(c: &mut Connection, args: &str) -> InputResult {
    let trimmed = args.trim();
    // `#record` with no args prints status.
    if trimmed.is_empty() {
        return match &c.recording_macro {
            Some(r) => InputResult::echo_line(format!(
                "recording `{}` ({} command(s) captured) — `#endrec` to save, `#record cancel` to discard",
                r.name,
                r.commands.len(),
            )),
            None => InputResult::echo_line("not recording. usage: #record <name>"),
        };
    }
    // `#record cancel` aborts an in-progress recording.
    if trimmed == "cancel" {
        return match c.recording_macro.take() {
            Some(r) => InputResult::echo_line(format!(
                "recording cancelled — `{}` was at {} command(s)",
                r.name,
                r.commands.len(),
            )),
            None => InputResult::error("not recording — nothing to cancel"),
        };
    }
    if c.recording_macro.is_some() {
        return InputResult::error(
            "already recording — `#endrec` to save or `#record cancel` to discard",
        );
    }
    let name = trimmed.split_whitespace().next().unwrap_or("");
    if name.is_empty() {
        return InputResult::error("usage #record <name>");
    }
    c.recording_macro = Some(MacroRecorder {
        name: name.to_string(),
        commands: Vec::new(),
    });
    InputResult::echo_line(format!(
        "recording `{name}` — every command you type is captured until `#endrec`"
    ))
}

pub(super) fn slash_endrec(profile: &mut Profile, c: &mut Connection) -> InputResult {
    let Some(recorder) = c.recording_macro.take() else {
        return InputResult::error("not recording. start with `#record <name>`");
    };
    if recorder.commands.is_empty() {
        return InputResult::error(format!(
            "recording `{}` had no commands — discarded",
            recorder.name
        ));
    }
    let expansion = recorder.commands.join(";");
    let name = recorder.name.clone();
    let count = recorder.commands.len();
    crate::script::define_alias(profile, c.stop_key, name.clone(), expansion);
    InputResult::echo_line(format!(
        "saved macro `{name}` ({count} command(s)) — invoke by typing `{name}`"
    ))
}
