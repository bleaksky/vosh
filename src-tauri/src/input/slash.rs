//! The slash command dispatcher. A line that starts with `#` runs the
//! command its first word names, against the live profile.

use super::automation::{
    slash_alias, slash_aliases_list, slash_endrec, slash_group, slash_groups_list, slash_record,
    slash_trigger, slash_triggers_list, slash_unalias, slash_untrigger,
};
use super::profile::{slash_import_tintin, slash_profile};
use super::prompt::{slash_prompt, slash_unprompt};
use super::script::{slash_lua, slash_script, slash_scripts_list};
use super::target::{
    run_target_clear, run_target_cycle, slash_qkey, slash_qkeys_list, slash_target,
};
use super::tick::slash_tick;
use super::vars::{slash_unvar, slash_var, slash_vars_list};
use super::{split_first_word, InputResult};
use crate::profile::live::Profile;
use crate::script::ApplyResult;

pub(super) fn handle_slash(
    profile: &mut Profile,
    rest: &str,
    replaced: &mut bool,
    lua: &mut ApplyResult,
) -> InputResult {
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
        "prompt" => slash_prompt(profile, args),
        "unprompt" => slash_unprompt(profile),
        "group" => slash_group(profile, args),
        "groups" => slash_groups_list(profile),
        "tick" => slash_tick(profile, args),
        "script" => slash_script(profile, args, lua),
        "scripts" => slash_scripts_list(profile),
        "lua" => slash_lua(profile, args, lua),
        "echo" | "showme" => slash_echo(profile, args),
        "profile" => slash_profile(profile, args, replaced),
        "import-tintin" => slash_import_tintin(profile, args),
        // Typed input runs #logs before the pipeline (see `logs_command`),
        // so only a timer, the tick command, or Lua gets here.
        "logs" => InputResult::error("type #logs at the input bar"),
        "record" => slash_record(profile, args),
        "endrec" => slash_endrec(profile),
        "target" => slash_target(profile, args),
        "tarn" => run_target_cycle(profile, 1),
        "tarp" => run_target_cycle(profile, -1),
        "tarclear" => run_target_clear(profile),
        "qkey" => slash_qkey(profile, args),
        "qkeys" => slash_qkeys_list(profile),
        "help" => InputResult::echo_lines(HELP_TEXT.lines().map(str::to_string).collect()),
        "" => InputResult::error("missing slash command. try #help"),
        other => InputResult::error(format!("unknown slash command #{other}. try #help")),
    }
}

pub(super) const HELP_TEXT: &str = "\
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
  #prompt                              say how Vosh reads your prompt
  #prompt game {setting}               read your prompt from its PROMPT codes
  #prompt fight {setting}              read your fight prompt from its codes
  #prompt draw on|off                  draw your design in place of your
                                       prompt, or show the game's own
  #prompt show text|lifted|pinned      choose where your prompt shows
  #prompt default                      use Vosh's default design and keep
                                       yours as an earlier design
  #prompt {regex}                      read your prompt with a pattern, each
                                       named group like (?<hp>...) a value
  #unprompt                            stop reading your prompt here
  #group <name> on|off                 enable/disable a group across
                                       triggers + aliases + macros
  #group <name>                        show current state of a group
  #groups                              list every group and its state
  #tick                                show tick timer state
  #tick interval <secs>                set how long a tick should take
  #tick reset                          restart the count now
  #tick on {pattern}                   a line that matches is the tick
  #tick off                            clear the regex reset pattern
  #tick fire <command>                 run a command on each tick
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
  #echo <text>                         print text locally (also #showme)
  #profile save                        save the current profile to disk
  #profile load                        replace state with the saved profile
  #profile reset                       wipe aliases, vars, triggers, tick
  #import-tintin <path>                import #alias and #variable from a .tin
  #logs forget-passwords               count the lines where you sent a password
  #logs forget-passwords now           blank those lines in the session log
  #nativesurface on|off|default        force the native renderer on or off
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
  #help <words>                        open Help on those words
trigger actions:
  highlight <color> [bold] [underline] [inverse] [wash] [bg:<color>]
  gag
  replace <template>
  send <template>
  route <pane>
parameter substitution: %0 entire args, %1..%9 positional, %1-..%9- Nth onward, %% literal %
variable substitution: $name or ${name}, $$ literal $
trigger captures: $0 full match, $1..$9 positional groups, ${name} named group\
";

/// Print text to the local terminal without sending it to the server.
/// `$vars` interpolate the same way they do in an alias expansion, so a
/// timer or trigger can echo live state (e.g. `#echo hp is $hp`). Empty
/// text echoes a blank line. `#showme` is an accepted alias for muscle
/// memory from `TinTin++` / `Mudlet`.
fn slash_echo(profile: &mut Profile, args: &str) -> InputResult {
    let text = profile.vars.interpolate(args);
    InputResult::echo_line(text)
}

/// Parse a `{pattern}` block. Supports `\}` to escape a closing brace inside
/// the pattern. Returns the pattern (escapes resolved) plus the remainder
/// after the closing brace.
pub(super) fn parse_braced_pattern(input: &str) -> Option<(String, &str)> {
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
