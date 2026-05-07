//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection.

use mudclient_alias::{Alias, ExpandError};
use mudclient_trigger::{HighlightStyle, NamedColor, Trigger, TriggerAction};
use mudclient_vars::Scope;
use tokio::time::Instant;

use crate::profile::Profile;

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
  #tick on {pattern}                   reset on a regex match
  #tick off                            clear the reset pattern
  #tick fire <command>                 run a command on each fire
  #tick nofire                         clear the auto-fire command
  #tick sound on|off                   toggle the tick beep
  #tick disable                        stop the tick timer
  #tick enable                         start the tick timer
  #help                                show this list
trigger actions:
  highlight <color> [bold] [underline] [inverse] [bg:<color>]
  gag
  replace <template>
  send <template>
  route <pane>
parameter substitution: %0 entire args, %1..%9 positional, %% literal %
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
    profile.aliases.set(Alias::new(name, expansion));
    echo_one(format!("alias {name} set"))
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
        pattern,
        priority: 0,
        enabled: true,
        action,
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
        let action = describe_action(&t.action);
        lines.push(format!(
            "  {mark} [{:>3}] {} /{}/ -> {action}",
            t.priority, t.name, t.pattern,
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
        other => error_echo(format!("unknown #tick subcommand `{other}`")),
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
        match &trig.action {
            TriggerAction::Highlight { style } => {
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
            p.triggers.get("spam").unwrap().action,
            TriggerAction::Gag
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
        match &p.triggers.get("loot").unwrap().action {
            TriggerAction::Send { template } => {
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
