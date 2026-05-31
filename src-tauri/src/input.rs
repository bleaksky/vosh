//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection.

use tokio::time::Instant;
use vosh_alias::{Alias, ExpandError};
use vosh_trigger::{HighlightStyle, NamedColor, Trigger, TriggerAction};
use vosh_vars::Scope;

use crate::profile::{MacroRecorder, Profile, QuickKey, RoomChar};
use crate::profile_config::ProfileConfig;
use crate::script_state;
use crate::tintin_import;

/// What the input pipeline produced.
pub(crate) struct InputResult {
    /// Commands to send to the server, already terminated with CRLF.
    pub(crate) bytes: Vec<u8>,
    /// Local lines to echo back to the terminal pane (without CRLF added).
    /// The session layer wraps each line in CRLF before emitting.
    pub(crate) echo: Vec<String>,
}

const HELP_TEXT: &str = "\
slash commands:
  #alias <name> <expansion>            define or replace an alias
  #unalias <name>                      remove an alias
  #aliases                             list aliases
  #var <name> <value>                  set a session variable
  #var <name>                          show a variable
  #unvar <name>                        remove a variable
  #vars                                list variables
  #trigger <name> {pattern} <action>   define or replace a trigger
  #untrigger <name>                    remove a trigger
  #triggers                            list triggers
  #tick                                show tick timer state
  #tick interval <secs>                set the tick interval
  #tick reset                          reset the timer now
  #tick on {pattern}                   also reset on a regex match
  #tick off                            clear the regex reset pattern
  #tick fire <command>                 run a command on each fire
  #tick nofire                         clear the auto-fire command
  #tick sound on|off                   toggle the tick beep
  #tick disable                        stop the tick timer
  #tick enable                         start the tick timer
  #tick warn                           show the warning settings
  #tick warn at <secs>                 echo a warning at <secs> before fire
  #tick warn message <text>            customize the warning text
  #tick warn color <name>              color the warning (red, bright-red, ...)
  #tick warn off                       disable the warning
  #script load <name>                  load <name>.lua from the scripts dir
  #script reload                       re-run all loaded scripts
  #scripts                             list loaded scripts and Lua triggers
  #lua <code>                          evaluate Lua inline
  #profile save                        save the current profile to disk
  #profile load                        replace state with the saved profile
  #profile reset                       wipe aliases, vars, triggers, tick
  #import-tintin <path>                import #alias and #variable from a .tin
  #record <name>                       start recording typed commands into a macro
  #record                              show current recording status
  #record cancel                       discard the in-progress recording
  #endrec                              stop recording and save as alias <name>
  tar                                  list current target and chars in room
  tar <N> | tar <substr>               set target by index or partial name
  tarn / tarp                          cycle to next/previous char in room
  tarclear                             clear the current target
  #qkey <name> <verb>                  configure a quick-key (gg/xx/zz/tt by default)
  #qkey clear <name>                   remove a quick-key
  #qkeys                               list quick-key bindings
  #help                                show this list
trigger actions:
  highlight <color> [bold] [underline] [inverse] [bg:<color>]
  gag
  replace <template>
  send <template>
  route <pane>
parameter substitution: %0 entire args, %1..%9 positional, %1-..%9- Nth onward, %% literal %
variable substitution: $name or ${name}, $$ literal $
trigger captures: $0 full match, $1..$9 positional groups, ${name} named group\
";

/// Run the input pipeline against the given profile and return what to send
/// and what to echo locally.
pub(crate) fn process(profile: &mut Profile, line: &str) -> InputResult {
    let trimmed = line.trim_start();

    // Slash commands target the local profile.
    if let Some(rest) = trimmed.strip_prefix('#') {
        return handle_slash(profile, rest);
    }

    // A bare Enter sends a blank line to the server. MUDs use this to
    // advance prompts and paginated output.
    if trimmed.is_empty() {
        return InputResult {
            bytes: b"\r\n".to_vec(),
            echo: Vec::new(),
        };
    }

    // Target keywords work bare (no `#` prefix) so they feel like
    // commands rather than slash builtins. `tar`/`tarn`/`tarp`/
    // `tarclear` are reserved at registration time so they can't be
    // shadowed by aliases or quick-keys.
    let (head, rest) = split_first_word(trimmed);
    match head {
        "tar" => return run_target_set(profile, rest),
        "tarn" => return run_target_cycle(profile, 1),
        "tarp" => return run_target_cycle(profile, -1),
        "tarclear" => return run_target_clear(profile),
        _ => {}
    }

    // Quick-keys expand BEFORE alias expansion, BEFORE var
    // interpolation, so `gg` becomes `<verb> <target>` and then runs
    // through the normal pipeline (so aliases inside the verb still
    // expand, vars inside still interpolate).
    //
    // Only fire when the quick-key actually has a verb configured.
    // An unconfigured quick-key falls through to alias expansion (and
    // then to the MUD if no alias matches), so a default-but-unused
    // name like `gg` does not shadow a user alias of the same name
    // with a "no verb is set" error.
    if let Some(qk) = profile
        .target
        .quick_keys
        .iter()
        .find(|q| q.name == head && !q.verb.is_empty())
    {
        let target = profile.target.name.clone().unwrap_or_default();
        if target.is_empty() {
            return error_echo("no target — set one with `tar <name|index>` first".to_string());
        }
        let expansion = format!("{} {}", qk.verb, target);
        let mut inner = process(profile, &expansion);
        // Echo the resolved line like any other typed command. The
        // frontend suppresses its own echo for quick-keys, so this is
        // the only echo that lands.
        inner.echo.insert(0, expansion);
        return inner;
    }

    // Capture into the macro recorder if one is active. Pre-expansion
    // so the recorded macro stays high-level: a recorded `fb dragon`
    // re-expands through the alias engine on replay rather than
    // freezing the alias definition at record time.
    if let Some(recorder) = profile.recording_macro.as_mut() {
        recorder.commands.push(trimmed.to_string());
    }

    // Plain input. Interpolate variables, then expand aliases, then encode.
    let interpolated = profile.vars.interpolate(trimmed);
    let commands = match profile.aliases.expand_line(&interpolated) {
        Ok(cmds) => cmds,
        Err(ExpandError::RecursionLimit(depth)) => {
            return error_echo(format!("alias recursion limit hit ({depth})"));
        }
    };

    let mut bytes = Vec::new();
    for cmd in commands {
        bytes.extend_from_slice(cmd.as_bytes());
        bytes.extend_from_slice(b"\r\n");
    }
    InputResult {
        bytes,
        echo: Vec::new(),
    }
}

fn handle_slash(profile: &mut Profile, rest: &str) -> InputResult {
    let (cmd, args) = split_first_word(rest);
    match cmd {
        "alias" => slash_alias(profile, args),
        "unalias" => slash_unalias(profile, args),
        "aliases" => slash_aliases_list(profile),
        "var" => slash_var(profile, args),
        "unvar" => slash_unvar(profile, args),
        "vars" => slash_vars_list(profile),
        "trigger" => slash_trigger(profile, args),
        "untrigger" => slash_untrigger(profile, args),
        "triggers" => slash_triggers_list(profile),
        "tick" => slash_tick(profile, args),
        "script" => slash_script(profile, args),
        "scripts" => slash_scripts_list(profile),
        "lua" => slash_lua(profile, args),
        "profile" => slash_profile(profile, args),
        "import-tintin" => slash_import_tintin(profile, args),
        "record" => slash_record(profile, args),
        "endrec" => slash_endrec(profile),
        "target" => slash_target(profile, args),
        "tarn" => run_target_cycle(profile, 1),
        "tarp" => run_target_cycle(profile, -1),
        "tarclear" => run_target_clear(profile),
        "qkey" => slash_qkey(profile, args),
        "qkeys" => slash_qkeys_list(profile),
        "help" => echo_lines(HELP_TEXT.lines()),
        "" => error_echo("missing slash command. try #help".to_string()),
        other => error_echo(format!("unknown slash command #{other}. try #help")),
    }
}

fn slash_alias(profile: &mut Profile, args: &str) -> InputResult {
    let (name, expansion) = split_first_word(args);
    if name.is_empty() {
        return error_echo("usage #alias <name> <expansion>".to_string());
    }
    if expansion.is_empty() {
        return error_echo("usage #alias <name> <expansion>".to_string());
    }
    // Reserved target keywords and existing quick-keys can't be
    // shadowed — the input pipeline checks both before alias
    // expansion, so an alias with the same name would silently never
    // fire.
    if is_target_keyword(name) {
        return error_echo(format!(
            "`{name}` is a target keyword — pick another alias name"
        ));
    }
    if profile.target.quick_keys.iter().any(|q| q.name == name) {
        return error_echo(format!(
            "quick-key `{name}` exists — `#qkey clear {name}` first if you want this name"
        ));
    }
    profile.aliases.set(Alias::new(name, expansion));
    echo_one(format!("alias {name} set"))
}

fn slash_record(profile: &mut Profile, args: &str) -> InputResult {
    let trimmed = args.trim();
    // `#record` with no args prints status.
    if trimmed.is_empty() {
        return match &profile.recording_macro {
            Some(r) => echo_one(format!(
                "recording `{}` ({} command(s) captured) — `#endrec` to save, `#record cancel` to discard",
                r.name,
                r.commands.len(),
            )),
            None => echo_one("not recording. usage: #record <name>".to_string()),
        };
    }
    // `#record cancel` aborts an in-progress recording.
    if trimmed == "cancel" {
        return match profile.recording_macro.take() {
            Some(r) => echo_one(format!(
                "recording cancelled — `{}` was at {} command(s)",
                r.name,
                r.commands.len(),
            )),
            None => error_echo("not recording — nothing to cancel".to_string()),
        };
    }
    if profile.recording_macro.is_some() {
        return error_echo(
            "already recording — `#endrec` to save or `#record cancel` to discard".to_string(),
        );
    }
    let name = trimmed.split_whitespace().next().unwrap_or("");
    if name.is_empty() {
        return error_echo("usage #record <name>".to_string());
    }
    profile.recording_macro = Some(MacroRecorder {
        name: name.to_string(),
        commands: Vec::new(),
    });
    echo_one(format!(
        "recording `{name}` — every command you type is captured until `#endrec`"
    ))
}

fn slash_endrec(profile: &mut Profile) -> InputResult {
    let Some(recorder) = profile.recording_macro.take() else {
        return error_echo("not recording. start with `#record <name>`".to_string());
    };
    if recorder.commands.is_empty() {
        return error_echo(format!(
            "recording `{}` had no commands — discarded",
            recorder.name
        ));
    }
    let expansion = recorder.commands.join(";");
    let name = recorder.name.clone();
    let count = recorder.commands.len();
    profile.aliases.set(Alias::new(name.clone(), expansion));
    echo_one(format!(
        "saved macro `{name}` ({count} command(s)) — invoke by typing `{name}`"
    ))
}

// ── Target system ────────────────────────────────────────────────

const TARGET_KEYWORDS: &[&str] = &["tar", "tarn", "tarp", "tarclear"];

fn is_target_keyword(name: &str) -> bool {
    TARGET_KEYWORDS.contains(&name)
}

/// Recompute `room_idx` from the current room snapshot. Called whenever
/// the target name changes or the room chars push refreshes the list.
///
/// Matching is **substring, case-insensitive**: typing `tar helg`
/// stores "helg" as the name (so commands use the user's keyword)
/// but resolves `room_idx` to whichever char contains "helg" so the
/// `>` marker shows on the right chip. First match wins.
pub(crate) fn refresh_target_idx(profile: &mut Profile) {
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
    // works in alias expansions. Char.Combat's `target_name` stays
    // separate (it's the server-confirmed combat target).
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

fn run_target_set(profile: &mut Profile, args: &str) -> InputResult {
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
    // like `tar helg` are what the user actually wants to send back
    // as `kill helg` rather than the full `The Baron Helgardium`.
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

fn run_target_cycle(profile: &mut Profile, step: i32) -> InputResult {
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

fn run_target_clear(profile: &mut Profile) -> InputResult {
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
fn slash_target(profile: &mut Profile, args: &str) -> InputResult {
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

fn slash_qkey(profile: &mut Profile, args: &str) -> InputResult {
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

fn slash_qkeys_list(profile: &Profile) -> InputResult {
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

fn slash_unalias(profile: &mut Profile, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return error_echo("usage #unalias <name>".to_string());
    }
    if profile.aliases.remove(name) {
        echo_one(format!("alias {name} removed"))
    } else {
        error_echo(format!("alias {name} not found"))
    }
}

fn slash_aliases_list(profile: &Profile) -> InputResult {
    let aliases = profile.aliases.list();
    if aliases.is_empty() {
        return echo_one("no aliases defined".to_string());
    }
    let mut lines = Vec::with_capacity(aliases.len() + 1);
    lines.push(format!("{} alias(es):", aliases.len()));
    for a in aliases {
        let mark = if a.enabled { ' ' } else { '*' };
        lines.push(format!("  {mark} {} -> {}", a.name, a.expansion));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

fn slash_var(profile: &mut Profile, args: &str) -> InputResult {
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

fn slash_unvar(profile: &mut Profile, args: &str) -> InputResult {
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

fn slash_trigger(profile: &mut Profile, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return error_echo("usage #trigger <name> {pattern} <action> [args]".to_string());
    }
    let Some((pattern, after_pattern)) = parse_braced_pattern(rest) else {
        return error_echo("usage #trigger <name> {pattern} <action> [args]".to_string());
    };
    let action = match parse_action(after_pattern) {
        Ok(a) => a,
        Err(msg) => return error_echo(msg),
    };
    let trigger = Trigger {
        name: name.to_string(),
        patterns: vec![vosh_trigger::TriggerPattern {
            pattern,
            enabled: true,
        }],
        priority: 0,
        enabled: true,
        actions: vec![action],
        preset: None,
    };
    match profile.triggers.set(trigger) {
        Ok(()) => echo_one(format!("trigger {name} set")),
        Err(e) => error_echo(format!("trigger {name} rejected: {e}")),
    }
}

fn slash_untrigger(profile: &mut Profile, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return error_echo("usage #untrigger <name>".to_string());
    }
    if profile.triggers.remove(name) {
        echo_one(format!("trigger {name} removed"))
    } else {
        error_echo(format!("trigger {name} not found"))
    }
}

fn slash_triggers_list(profile: &Profile) -> InputResult {
    let triggers = profile.triggers.list();
    if triggers.is_empty() {
        return echo_one("no triggers defined".to_string());
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
        lines.push(format!(
            "  {mark} [{:>3}] {} /{}/ -> {action}",
            t.priority,
            t.name,
            t.first_pattern(),
        ));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

/// Parse a `{pattern}` block. Supports `\}` to escape a closing brace inside
/// the pattern. Returns the pattern (escapes resolved) plus the remainder
/// after the closing brace.
fn parse_braced_pattern(input: &str) -> Option<(String, &str)> {
    let trimmed = input.trim_start();
    let mut chars = trimmed.char_indices();
    let (_, first) = chars.next()?;
    if first != '{' {
        return None;
    }
    let mut pattern = String::new();
    let mut last_end = 0;
    while let Some((i, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, next)) = chars.next() {
                if next == '}' || next == '\\' {
                    pattern.push(next);
                    continue;
                }
                pattern.push(ch);
                pattern.push(next);
                continue;
            }
            pattern.push(ch);
            continue;
        }
        if ch == '}' {
            last_end = i + 1;
            break;
        }
        pattern.push(ch);
    }
    if last_end == 0 {
        return None;
    }
    Some((pattern, trimmed[last_end..].trim_start()))
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
            format!("highlight {}", parts.join(" "))
        }
        TriggerAction::Gag => "gag".to_string(),
        TriggerAction::Replace { template } => format!("replace `{template}`"),
        TriggerAction::Send { template } => format!("send `{template}`"),
        TriggerAction::Route { pane } => format!("route {pane}"),
    }
}

fn slash_profile(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, _rest) = split_first_word(args);
    match cmd {
        "save" => match profile_path() {
            Some(path) => {
                let snapshot = ProfileConfig::from_profile(profile);
                match snapshot.save(&path) {
                    Ok(()) => echo_one(format!("profile saved to {}", path.display())),
                    Err(e) => error_echo(format!("save failed: {e}")),
                }
            }
            None => error_echo("could not resolve profile path".to_string()),
        },
        "load" => match profile_path() {
            Some(path) => match ProfileConfig::load(&path) {
                Ok(snapshot) => {
                    let warnings = snapshot.apply_to(profile);
                    let mut lines = vec![format!("profile loaded from {}", path.display())];
                    for w in warnings {
                        lines.push(format!("  {w}"));
                    }
                    InputResult {
                        bytes: Vec::new(),
                        echo: lines,
                    }
                }
                Err(e) => error_echo(format!("load failed: {e}")),
            },
            None => error_echo("could not resolve profile path".to_string()),
        },
        "reset" => {
            let blank = ProfileConfig::default();
            let _ = blank.apply_to(profile);
            echo_one("profile reset to defaults".to_string())
        }
        "" => error_echo("usage #profile save | load | reset".to_string()),
        other => error_echo(format!("unknown #profile subcommand `{other}`")),
    }
}

fn slash_import_tintin(profile: &mut Profile, args: &str) -> InputResult {
    let path = args.trim();
    if path.is_empty() {
        return error_echo("usage #import-tintin <path>".to_string());
    }
    let expanded = expand_home(path);
    let report = match tintin_import::import_file(&expanded) {
        Ok(r) => r,
        Err(e) => return error_echo(format!("read failed: {e}")),
    };
    for alias in &report.aliases {
        profile.aliases.set(alias.clone());
    }
    for (name, value) in &report.vars {
        profile
            .vars
            .set(Scope::Profile, name.clone(), value.clone());
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
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

fn profile_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let base = std::path::PathBuf::from(home);
    Some(
        base.join("Library")
            .join("Application Support")
            .join("com.aabahran.vosh")
            .join("profile.toml"),
    )
}

fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

fn slash_script(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "load" => slash_script_load(profile, rest),
        "reload" => slash_script_reload(profile),
        "" => error_echo("usage #script load <name> | #script reload".to_string()),
        other => error_echo(format!("unknown #script subcommand `{other}`")),
    }
}

fn slash_script_load(profile: &mut Profile, args: &str) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return error_echo("usage #script load <name>".to_string());
    }
    let Some(path) = script_path_for(name) else {
        return error_echo("could not resolve scripts directory".to_string());
    };
    let code = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return error_echo(format!("read failed: {e} ({})", path.display())),
    };
    script_state::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.load_script(name, code) {
        Ok(o) => o,
        Err(e) => return error_echo(format!("script error: {e}")),
    };
    let apply = script_state::apply_actions(profile, outcome);
    let mut echoes = vec![format!("loaded {}", path.display())];
    echoes.extend(apply.echoes);
    InputResult {
        bytes: apply.send_bytes,
        echo: echoes,
    }
}

fn slash_script_reload(profile: &mut Profile) -> InputResult {
    script_state::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.reload_scripts() {
        Ok(o) => o,
        Err(e) => return error_echo(format!("reload error: {e}")),
    };
    let apply = script_state::apply_actions(profile, outcome);
    let mut echoes = vec!["scripts reloaded".to_string()];
    echoes.extend(apply.echoes);
    InputResult {
        bytes: apply.send_bytes,
        echo: echoes,
    }
}

fn slash_scripts_list(profile: &Profile) -> InputResult {
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
        for t in triggers {
            let mark = if t.enabled { ' ' } else { '*' };
            lines.push(format!(
                "  {mark} [{:>3}] {} /{}/",
                t.priority, t.name, t.pattern
            ));
        }
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

fn slash_lua(profile: &mut Profile, args: &str) -> InputResult {
    let code = args.trim_start();
    if code.is_empty() {
        return error_echo("usage #lua <code>".to_string());
    }
    script_state::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.eval(code, "#lua") {
        Ok(o) => o,
        Err(e) => return error_echo(format!("lua error: {e}")),
    };
    let apply = script_state::apply_actions(profile, outcome);
    InputResult {
        bytes: apply.send_bytes,
        echo: apply.echoes,
    }
}

fn script_path_for(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let base = std::path::PathBuf::from(home);
    // macOS specific for the demo. Phase 9 will move to the OS-aware app
    // data dir Tauri already exposes for the map store.
    let dir = base
        .join("Library")
        .join("Application Support")
        .join("com.aabahran.vosh")
        .join("scripts");
    let with_lua = if std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        dir.join(name)
    } else {
        dir.join(format!("{name}.lua"))
    };
    Some(with_lua)
}

fn slash_tick(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    let now = Instant::now();
    match cmd {
        "" => slash_tick_show(profile, now),
        "interval" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.set_interval(secs, now);
                echo_one(format!("tick interval set to {secs}s"))
            }
            _ => error_echo("usage #tick interval <secs>".to_string()),
        },
        "reset" => {
            profile.tick.reset(now);
            echo_one("tick reset".to_string())
        }
        "on" => {
            let Some((pattern, _rest)) = parse_braced_pattern(rest) else {
                return error_echo("usage #tick on {pattern}".to_string());
            };
            match profile.tick.set_reset_pattern(Some(pattern.clone())) {
                Ok(()) => echo_one(format!("tick will reset on /{pattern}/")),
                Err(e) => error_echo(format!("invalid regex: {e}")),
            }
        }
        "off" => {
            let _ = profile.tick.set_reset_pattern(None);
            echo_one("tick reset pattern cleared".to_string())
        }
        "fire" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.auto_fire = None;
                echo_one("tick auto-fire cleared".to_string())
            } else {
                profile.tick.config.auto_fire = Some(trimmed.to_string());
                echo_one(format!("tick auto-fire set to: {trimmed}"))
            }
        }
        "nofire" => {
            profile.tick.config.auto_fire = None;
            echo_one("tick auto-fire cleared".to_string())
        }
        "sound" => match rest.trim() {
            "on" => {
                profile.tick.config.sound = true;
                echo_one("tick sound on".to_string())
            }
            "off" => {
                profile.tick.config.sound = false;
                echo_one("tick sound off".to_string())
            }
            _ => error_echo("usage #tick sound on|off".to_string()),
        },
        "disable" => {
            profile.tick.disable();
            echo_one("tick disabled".to_string())
        }
        "enable" => {
            profile.tick.enable(now);
            echo_one("tick enabled".to_string())
        }
        "warn" => slash_tick_warn(profile, rest),
        other => error_echo(format!("unknown #tick subcommand `{other}`")),
    }
}

fn slash_tick_warn(profile: &mut Profile, args: &str) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "" => {
            let cfg = &profile.tick.config;
            let mut lines = Vec::new();
            match cfg.warn_at_secs {
                Some(s) => lines.push(format!("tick warn at {s}s before fire")),
                None => lines.push("tick warn: off".to_string()),
            }
            lines.push(format!(
                "  message: {}",
                cfg.warn_message.as_deref().unwrap_or("(default)")
            ));
            lines.push(format!(
                "  color:   {}",
                cfg.warn_color.as_deref().unwrap_or("bright-red")
            ));
            InputResult {
                bytes: Vec::new(),
                echo: lines,
            }
        }
        "at" => match rest.trim().parse::<u64>() {
            Ok(secs) if secs > 0 => {
                profile.tick.config.warn_at_secs = Some(secs);
                echo_one(format!("tick warn set to {secs}s before fire"))
            }
            _ => error_echo("usage #tick warn at <secs>".to_string()),
        },
        "off" => {
            profile.tick.config.warn_at_secs = None;
            echo_one("tick warn disabled".to_string())
        }
        "message" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_message = None;
                echo_one("tick warn message cleared (default applies)".to_string())
            } else {
                profile.tick.config.warn_message = Some(trimmed.to_string());
                echo_one(format!("tick warn message set to: {trimmed}"))
            }
        }
        "color" => {
            let trimmed = rest.trim();
            if trimmed.is_empty() {
                profile.tick.config.warn_color = None;
                echo_one("tick warn color cleared (default bright-red)".to_string())
            } else {
                profile.tick.config.warn_color = Some(trimmed.to_string());
                echo_one(format!("tick warn color set to: {trimmed}"))
            }
        }
        other => error_echo(format!(
            "unknown #tick warn subcommand `{other}`. usage: at <secs> | off | message <text> | color <name>"
        )),
    }
}

fn slash_tick_show(profile: &Profile, now: Instant) -> InputResult {
    let cfg = &profile.tick.config;
    let mut lines = Vec::new();
    let state = if cfg.enabled { "enabled" } else { "disabled" };
    lines.push(format!(
        "tick {state}, interval {}s",
        cfg.interval.as_secs()
    ));
    if let Some(remaining) = profile.tick.remaining(now) {
        lines.push(format!("  remaining {}s", remaining.as_secs()));
    } else {
        lines.push("  remaining (not running)".to_string());
    }
    if let Some(p) = &cfg.reset_pattern {
        lines.push(format!("  reset on /{p}/"));
    } else {
        lines.push("  no reset pattern".to_string());
    }
    if let Some(f) = &cfg.auto_fire {
        lines.push(format!("  auto-fire: {f}"));
    } else {
        lines.push("  auto-fire: (none)".to_string());
    }
    lines.push(format!("  sound {}", if cfg.sound { "on" } else { "off" }));
    match cfg.warn_at_secs {
        Some(s) => {
            let msg = cfg.warn_message.as_deref().unwrap_or("(default)");
            let color = cfg.warn_color.as_deref().unwrap_or("bright-red");
            lines.push(format!("  warn at {s}s | {color} | {msg}"));
        }
        None => lines.push("  warn (off)".to_string()),
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

fn slash_vars_list(profile: &Profile) -> InputResult {
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

fn split_first_word(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => {
            let (head, tail) = trimmed.split_at(idx);
            (head, tail.trim_start())
        }
        None => (trimmed, ""),
    }
}

fn error_echo(message: String) -> InputResult {
    InputResult {
        bytes: Vec::new(),
        echo: vec![format!("[{message}]")],
    }
}

fn echo_one(message: String) -> InputResult {
    InputResult {
        bytes: Vec::new(),
        echo: vec![message],
    }
}

fn echo_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> InputResult {
    InputResult {
        bytes: Vec::new(),
        echo: lines.into_iter().map(str::to_string).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_input_appends_crlf() {
        let mut p = Profile::default();
        let r = process(&mut p, "look");
        assert_eq!(r.bytes, b"look\r\n");
        assert!(r.echo.is_empty());
    }

    #[test]
    fn empty_input_sends_bare_crlf() {
        let mut p = Profile::default();
        let r = process(&mut p, "");
        assert_eq!(r.bytes, b"\r\n");
        assert!(r.echo.is_empty());
    }

    #[test]
    fn whitespace_only_input_sends_bare_crlf() {
        let mut p = Profile::default();
        let r = process(&mut p, "   ");
        assert_eq!(r.bytes, b"\r\n");
    }

    #[test]
    fn alias_expansion_runs_through_pipeline() {
        let mut p = Profile::default();
        p.aliases.set(Alias::new("greet", "wave;bow"));
        let r = process(&mut p, "greet");
        assert_eq!(r.bytes, b"wave\r\nbow\r\n");
    }

    #[test]
    fn variables_substitute_before_alias_expansion() {
        let mut p = Profile::default();
        p.vars.set(Scope::Session, "target", "goblin");
        p.aliases.set(Alias::new("hit", "kick %0"));
        let r = process(&mut p, "hit $target");
        assert_eq!(r.bytes, b"kick goblin\r\n");
    }

    #[test]
    fn semicolon_in_user_input_splits_into_two_sends() {
        let mut p = Profile::default();
        let r = process(&mut p, "look;sip water");
        assert_eq!(r.bytes, b"look\r\nsip water\r\n");
    }

    #[test]
    fn slash_alias_sets_and_lists() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#alias greet wave;bow");
        let r = process(&mut p, "#aliases");
        assert!(r.echo.iter().any(|l| l.contains("greet -> wave;bow")));
    }

    fn rc(name: &str, npc: bool) -> RoomChar {
        RoomChar {
            name: name.to_string(),
            npc,
        }
    }

    #[test]
    fn tar_by_index_sets_target_and_idx() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("Bob", false), rc("ogre", true)]);
        let r = process(&mut p, "tar 2");
        assert_eq!(p.target.name.as_deref(), Some("ogre"));
        assert_eq!(p.target.room_idx, Some(2));
        assert!(r.echo.iter().any(|l| l.contains("ogre")));
    }

    #[test]
    fn tar_string_keeps_literal_resolves_idx_via_substring() {
        // Non-numeric `tar <string>` stores the user's literal keyword
        // (so `kill ${target}` sends `kill helg`, which the MUD's
        // keyword matcher handles), but still resolves room_idx via
        // case-insensitive substring so the `>` marker lands on the
        // matching chip.
        let mut p = Profile::default();
        set_room_chars(
            &mut p,
            vec![rc("The Baron Helgardium", true), rc("ogre", true)],
        );
        let _ = process(&mut p, "tar helg");
        assert_eq!(p.target.name.as_deref(), Some("helg"));
        assert_eq!(p.target.room_idx, Some(1));
    }

    #[test]
    fn tar_unknown_keeps_literal_with_no_idx() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("Bob", false)]);
        let _ = process(&mut p, "tar Alice");
        assert_eq!(p.target.name.as_deref(), Some("Alice"));
        assert_eq!(p.target.room_idx, None);
    }

    #[test]
    fn target_syncs_to_var_store_for_interpolation() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("Bob", false)]);
        let _ = process(&mut p, "tar 1");
        // `${target}` should now interpolate to "Bob".
        let r = process(&mut p, "cast 'bless' ${target}");
        assert_eq!(r.bytes, b"cast 'bless' Bob\r\n");
    }

    #[test]
    fn tarn_cycles_forward_and_wraps() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("A", true), rc("B", true), rc("C", true)]);
        let _ = process(&mut p, "tarn");
        assert_eq!(p.target.name.as_deref(), Some("A"));
        let _ = process(&mut p, "tarn");
        assert_eq!(p.target.name.as_deref(), Some("B"));
        let _ = process(&mut p, "tarn");
        let _ = process(&mut p, "tarn");
        assert_eq!(p.target.name.as_deref(), Some("A"));
    }

    #[test]
    fn tarclear_drops_target_and_var() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("Bob", false)]);
        let _ = process(&mut p, "tar 1");
        let _ = process(&mut p, "tarclear");
        assert!(p.target.name.is_none());
        assert!(p.vars.get("target").is_none());
    }

    #[test]
    fn quick_key_expands_to_verb_plus_target() {
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("ogre", true)]);
        let _ = process(&mut p, "tar 1");
        let _ = process(&mut p, "#qkey gg kick");
        let r = process(&mut p, "gg");
        assert_eq!(r.bytes, b"kick ogre\r\n");
    }

    #[test]
    fn quick_key_uses_literal_keyword_not_full_name() {
        // `tar helg` keeps "helg" as the target. Quick-keys should
        // expand to `<verb> helg` so the MUD's keyword matcher
        // resolves it on its side rather than getting the full
        // descriptor "The Baron Helgardium".
        let mut p = Profile::default();
        set_room_chars(&mut p, vec![rc("The Baron Helgardium", true)]);
        let _ = process(&mut p, "tar helg");
        let _ = process(&mut p, "#qkey gg cast 'fireball'");
        let r = process(&mut p, "gg");
        assert_eq!(r.bytes, b"cast 'fireball' helg\r\n");
    }

    #[test]
    fn quick_key_without_target_errors() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#qkey gg kick");
        let r = process(&mut p, "gg");
        assert!(r.bytes.is_empty());
        assert!(r.echo.iter().any(|l| l.contains("no target")));
    }

    #[test]
    fn alias_cannot_shadow_quick_key() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#qkey gg kick");
        let r = process(&mut p, "#alias gg cast 'fireball'");
        assert!(r.echo.iter().any(|l| l.contains("quick-key")));
        assert!(p.aliases.get("gg").is_none());
    }

    #[test]
    fn qkey_cannot_shadow_alias() {
        // Use a name that isn't a default quick-key slot so the alias
        // can register first, then verify qkey refuses to shadow it.
        let mut p = Profile::default();
        let _ = process(&mut p, "#alias kk kick");
        let r = process(&mut p, "#qkey kk kick");
        assert!(r.echo.iter().any(|l| l.contains("alias")));
        assert!(p.target.quick_keys.iter().all(|q| q.name != "kk"));
    }

    #[test]
    fn qkey_cannot_use_reserved_target_keyword() {
        let mut p = Profile::default();
        let r = process(&mut p, "#qkey tar foo");
        assert!(r.echo.iter().any(|l| l.contains("target keyword")));
    }

    #[test]
    fn default_quick_keys_are_present_but_empty() {
        let p = Profile::default();
        let names: Vec<&str> = p
            .target
            .quick_keys
            .iter()
            .map(|q| q.name.as_str())
            .collect();
        assert_eq!(names, ["gg", "xx", "zz", "tt"]);
        assert!(p.target.quick_keys.iter().all(|q| q.verb.is_empty()));
    }

    #[test]
    fn record_captures_then_saves_alias() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#record buff");
        let _ = process(&mut p, "cast 'sanctuary' self");
        let _ = process(&mut p, "cast 'haste' self");
        let _ = process(&mut p, "cast 'bless' self");
        let _ = process(&mut p, "#endrec");
        let alias = p.aliases.list();
        let buff = alias
            .iter()
            .find(|a| a.name == "buff")
            .expect("alias saved");
        assert_eq!(
            buff.expansion,
            "cast 'sanctuary' self;cast 'haste' self;cast 'bless' self"
        );
    }

    #[test]
    fn record_skips_slash_lines() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#record probe");
        let _ = process(&mut p, "look");
        // A slash command shouldn't be captured.
        let _ = process(&mut p, "#aliases");
        let _ = process(&mut p, "score");
        let _ = process(&mut p, "#endrec");
        let buff = p
            .aliases
            .list()
            .into_iter()
            .find(|a| a.name == "probe")
            .expect("alias saved");
        assert_eq!(buff.expansion, "look;score");
    }

    #[test]
    fn record_cancel_discards() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#record nope");
        let _ = process(&mut p, "kill rabbit");
        let _ = process(&mut p, "#record cancel");
        assert!(p.recording_macro.is_none());
        assert!(p.aliases.list().iter().all(|a| a.name != "nope"));
    }

    #[test]
    fn slash_unalias_removes() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#alias greet wave");
        let _ = process(&mut p, "#unalias greet");
        let r = process(&mut p, "greet");
        assert_eq!(r.bytes, b"greet\r\n");
    }

    #[test]
    fn slash_var_set_and_show() {
        let mut p = Profile::default();
        let r = process(&mut p, "#var hp 100");
        assert!(r.echo.iter().any(|l| l == "var hp set"));
        let r = process(&mut p, "#var hp");
        assert!(r.echo.iter().any(|l| l == "hp = 100"));
    }

    #[test]
    fn slash_help_lists_commands() {
        let mut p = Profile::default();
        let r = process(&mut p, "#help");
        assert!(r.echo.iter().any(|l| l.contains("#alias")));
        assert!(r.echo.iter().any(|l| l.contains("#var")));
    }

    #[test]
    fn unknown_slash_returns_error_echo() {
        let mut p = Profile::default();
        let r = process(&mut p, "#nope");
        assert!(r.bytes.is_empty());
        assert!(r.echo.iter().any(|l| l.contains("unknown slash command")));
    }

    #[test]
    fn alias_recursion_returns_error_echo_not_panic() {
        let mut p = Profile::default();
        p.aliases.set(Alias::new("loop", "loop"));
        let r = process(&mut p, "loop");
        assert!(r.bytes.is_empty());
        assert!(r.echo.iter().any(|l| l.contains("recursion limit")));
    }

    #[test]
    fn slash_trigger_highlight_registers() {
        let mut p = Profile::default();
        let r = process(&mut p, "#trigger tells {tells you} highlight cyan bold");
        assert!(r.echo.iter().any(|l| l == "trigger tells set"));
        assert_eq!(p.triggers.len(), 1);
        let trig = p.triggers.get("tells").unwrap();
        match trig.actions.first() {
            Some(TriggerAction::Highlight { style }) => {
                assert_eq!(style.fg, Some(NamedColor::Cyan));
                assert!(style.bold);
            }
            _ => panic!("expected highlight action"),
        }
    }

    #[test]
    fn slash_trigger_gag_registers() {
        let mut p = Profile::default();
        let r = process(&mut p, "#trigger spam {tingle} gag");
        assert!(r.echo.iter().any(|l| l == "trigger spam set"));
        assert!(matches!(
            p.triggers.get("spam").unwrap().actions.first(),
            Some(TriggerAction::Gag)
        ));
    }

    #[test]
    fn slash_trigger_send_with_capture() {
        let mut p = Profile::default();
        let r = process(
            &mut p,
            r"#trigger loot {The (\w+) is DEAD} send loot $1 from corpse",
        );
        assert!(r.echo.iter().any(|l| l == "trigger loot set"));
        match p.triggers.get("loot").unwrap().actions.first() {
            Some(TriggerAction::Send { template }) => {
                assert_eq!(template, "loot $1 from corpse");
            }
            _ => panic!("expected send action"),
        }
    }

    #[test]
    fn slash_trigger_invalid_regex_rejected() {
        let mut p = Profile::default();
        let r = process(&mut p, "#trigger bad {[unclosed} gag");
        assert!(r.echo.iter().any(|l| l.contains("rejected")));
        assert_eq!(p.triggers.len(), 0);
    }

    #[test]
    fn slash_untrigger_removes() {
        let mut p = Profile::default();
        let _ = process(&mut p, "#trigger spam {tingle} gag");
        let _ = process(&mut p, "#untrigger spam");
        assert_eq!(p.triggers.len(), 0);
    }

    #[test]
    fn parse_braced_pattern_handles_escaped_close() {
        let (pattern, rest) = parse_braced_pattern(r"{a\}b} send hi").unwrap();
        assert_eq!(pattern, "a}b");
        assert_eq!(rest, "send hi");
    }
}
