//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection.

mod automation;
mod profile;
mod prompt;
mod script;
pub(crate) mod target;
mod tick;
mod vars;

use vosh_automation::alias::{ExpandError, ExpandStep};

use crate::profile::Profile;
use crate::script::{run_alias_body, ApplyResult};

use automation::{
    slash_alias, slash_aliases_list, slash_endrec, slash_group, slash_groups_list, slash_record,
    slash_trigger, slash_triggers_list, slash_unalias, slash_untrigger,
};
use profile::{slash_import_tintin, slash_profile};
use prompt::{slash_prompt, slash_unprompt};
use script::{slash_lua, slash_script, slash_scripts_list};
use target::{
    run_target_clear, run_target_cycle, run_target_set, slash_qkey, slash_qkeys_list, slash_target,
};
use tick::slash_tick;
use vars::{slash_unvar, slash_var, slash_vars_list};

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

/// Set at startup (and at migration time) when Path B is live: the
/// catalog owns authored items and persistence is automatic, so the
/// legacy #profile save/load/reset trio switches to echo-only.
pub(crate) static PATH_B_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The app data folder, set once at startup from Tauri's `app_data_dir`,
/// the folder that holds every other file Vosh keeps. `#profile save`,
/// `#profile load` and `#script load` find their files under it.
pub(crate) static APP_DATA_DIR: std::sync::OnceLock<std::path::PathBuf> =
    std::sync::OnceLock::new();

/// True when `line` is `#profile reset` or `#profile load`, tokenized
/// exactly like the slash dispatcher, so the persist-suppression
/// decision in `session_send_input` cannot drift from what actually
/// executes ("#profile  reset" and "# profile load" count too).
pub(crate) fn is_profile_reset_or_load(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let (cmd, rest) = split_first_word(rest);
    if cmd != "profile" {
        return false;
    }
    let (sub, _) = split_first_word(rest);
    matches!(sub, "reset" | "load")
}

/// What a `#logs` line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogsCommand {
    /// `#logs forget-passwords` counts the lines that hold a password.
    Preview,
    /// `#logs forget-passwords now` blanks them.
    Forget,
    /// Anything else after `#logs`.
    Usage,
}

/// The `#logs` command `line` asks for, or None when it is no `#logs`
/// line. Typed input runs it before the pipeline, since it works on the
/// log store rather than the profile.
pub(crate) fn logs_command(line: &str) -> Option<LogsCommand> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (cmd, rest) = split_first_word(rest);
    if cmd != "logs" {
        return None;
    }
    let mut words = rest.split_whitespace();
    Some(match (words.next(), words.next(), words.next()) {
        (Some("forget-passwords"), None, None) => LogsCommand::Preview,
        (Some("forget-passwords"), Some("now"), None) => LogsCommand::Forget,
        _ => LogsCommand::Usage,
    })
}

/// The words of a `#help <words>` line, or None for any other line.
/// Typed input opens the Help window on them instead of running the
/// pipeline, since the topics live in the page. `#help` alone runs the
/// pipeline and prints the command summary.
pub(crate) fn help_query(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (cmd, words) = split_first_word(rest);
    // The search reads a phrase, so a run of spaces reads as one.
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    (cmd == "help" && !words.is_empty()).then_some(words)
}

/// True when `line` may replace the live profile: a `#profile reset` or
/// `#profile load` outside loadout mode, which turns the pair into
/// echoes. A caller reads global.toml before such a line runs, to lay the
/// shared settings back over the result. Whether it did replace the
/// profile comes back from [`run_line`].
pub(crate) fn may_replace_profile(line: &str) -> bool {
    !PATH_B_ACTIVE.load(std::sync::atomic::Ordering::Acquire) && is_profile_reset_or_load(line)
}

/// One line run through the input pipeline.
pub(crate) struct Ran {
    pub(crate) result: InputResult,
    /// What the Lua the line ran asks for, from `#lua`, `#script load`,
    /// `#script reload` or the bodies of its script aliases: timers,
    /// `mud.input` lines, prompt values, whether it changed durable
    /// state, which [`LineEffects::note_ran`] notes, and the sends and
    /// echo lines of the slash commands. A script alias body's sends and
    /// echo lines sit in `result` instead, in the order you typed them.
    /// The caller hands it to `session::apply_script_result` after the
    /// line's own output.
    pub(crate) lua: ApplyResult,
    /// A `#profile reset`, or a `#profile load` that read its file,
    /// replaced the live profile.
    pub(crate) replaced: bool,
    /// The line changed the tick settings, like `#tick warn at 10`.
    pub(crate) tick_changed: bool,
}

/// Run `line` through the input pipeline: what to send, what to echo,
/// what the Lua it ran asks for, whether it replaced the live profile,
/// and whether it changed the tick settings, for
/// [`LineEffects::note_ran`].
pub(crate) fn run_line(profile: &mut Profile, line: &str) -> Ran {
    let mut replaced = false;
    let mut lua = ApplyResult::default();
    let tick_before = profile.tick.config.clone();
    let result = process_line(profile, line, &mut replaced, &mut lua);
    let tick_changed = profile.tick.config != tick_before;
    Ran {
        result,
        lua,
        replaced,
        tick_changed,
    }
}

/// What a run of input lines asks of the saved profile. Every path that
/// runs a line through [`run_line`] notes each line here in order: typed
/// input, a Settings timer command, the tick auto-fire command, and a
/// Lua `mud.input` line. So `#alias` or `#trigger` from a timer reaches
/// disk the way the same line typed at the prompt does. Lua that changed
/// durable state marks the profile dirty where its result is applied,
/// after these.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineEffects {
    /// A `#profile reset` or `#profile load` replaced the live profile,
    /// which leaves it diverged from disk on purpose.
    pub(crate) replaced: bool,
    /// A slash command came after the last replace, or with none.
    pub(crate) dirty: bool,
    /// A `#tick` command changed the tick settings. The status line and
    /// the Settings Tick card show them, so every window hears the new
    /// settings once the lines have run.
    pub(crate) tick_changed: bool,
}

impl LineEffects {
    /// Note one line [`run_line`] ran: [`Self::note`] with whether it
    /// replaced the live profile, whether it changed the tick settings,
    /// and whether the Lua bodies of its script aliases changed durable
    /// state.
    pub(crate) fn note_ran(&mut self, line: &str, ran: &Ran) {
        self.note(line, ran.replaced);
        if ran.tick_changed {
            self.tick_changed = true;
        }
        if ran.lua.durable_changed {
            self.dirty = true;
        }
    }

    /// Note one line that ran through [`run_line`], with whether it
    /// replaced the live profile. The line alone cannot say: a `#profile
    /// load` whose file does not read leaves the profile as it was, and
    /// the pending save and the exit flush must still write it.
    pub(crate) fn note(&mut self, line: &str, replaced: bool) {
        if replaced {
            self.replaced = true;
            self.dirty = false;
            return;
        }
        // A `#profile load` that did not read its file changed nothing,
        // and loadout mode turns the pair into echoes, so neither counts
        // as a change to save. Saving after a failed load would write a
        // profile an earlier `#profile reset` blanked.
        if is_profile_reset_or_load(line) {
            return;
        }
        if line.trim_start().starts_with('#') {
            self.dirty = true;
        }
    }
}

/// Run the input pipeline against the given profile and return what to send
/// and what to echo locally. The app runs every line through [`run_line`],
/// which also says whether the line replaced the profile.
#[cfg(test)]
pub(crate) fn process(profile: &mut Profile, line: &str) -> InputResult {
    run_line(profile, line).result
}

/// The body of [`run_line`]. Sets `replaced` when a `#profile reset` or a
/// `#profile load` that read its file replaced the live profile, and adds
/// to `lua` what the Lua the line ran asks for. A script alias body's
/// sends and echo lines go in the result instead, where you typed it.
fn process_line(
    profile: &mut Profile,
    line: &str,
    replaced: &mut bool,
    lua: &mut ApplyResult,
) -> InputResult {
    let trimmed = line.trim_start();

    // Slash commands target the local profile.
    if let Some(rest) = trimmed.strip_prefix('#') {
        return handle_slash(profile, rest, replaced, lua);
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
        let mut inner = process_line(profile, &expansion, replaced, lua);
        // Echo the resolved line like any other typed command, with the
        // caret and the Sent command color. The frontend suppresses its
        // own echo for quick-keys, so this is the only echo that lands.
        inner.echo.insert(0, command_echo(&expansion, &profile.ui));
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
    // An alias that runs Lua runs its body where it stands, so what the
    // body sends goes out in the order you typed the line.
    let interpolated = profile.vars.interpolate(trimmed);
    let steps = match profile.aliases.expand_line_full(&interpolated) {
        Ok(steps) => steps,
        Err(ExpandError::RecursionLimit(depth)) => {
            return error_echo(format!("alias recursion limit hit ({depth})"));
        }
    };

    let mut bytes = Vec::new();
    let mut echo = Vec::new();
    for step in steps {
        match step {
            ExpandStep::Command(cmd) => {
                bytes.extend_from_slice(cmd.as_bytes());
                bytes.extend_from_slice(b"\r\n");
            }
            ExpandStep::Script(call) => {
                let mut apply = run_alias_body(profile, &call);
                bytes.append(&mut apply.send_bytes);
                echo.append(&mut apply.echoes);
                lua.append(apply);
            }
        }
    }
    InputResult { bytes, echo }
}

fn handle_slash(
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
        "logs" => error_echo("type #logs at the input bar".to_string()),
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

/// Print text to the local terminal without sending it to the server.
/// `$vars` interpolate the same way they do in an alias expansion, so a
/// timer or trigger can echo live state (e.g. `#echo hp is $hp`). Empty
/// text echoes a blank line. `#showme` is an accepted alias for muscle
/// memory from `TinTin++` / `Mudlet`.
fn slash_echo(profile: &mut Profile, args: &str) -> InputResult {
    let text = profile.vars.interpolate(args);
    InputResult {
        bytes: Vec::new(),
        echo: vec![text],
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

/// The echo of a command you send, as the command line draws it: a grey
/// `›` and a space while Mark your commands is on, then the command in
/// the Sent command color when one is set. Mirrors `planSubmit` and
/// `colorizeEcho` in src/lib/maskedInput.ts, so a quick key echoes like a
/// typed command. An empty line echoes as itself.
pub(crate) fn command_echo(line: &str, ui: &crate::profile_config::UiConfig) -> String {
    if line.is_empty() {
        return String::new();
    }
    let caret = if ui.input_echo_caret { ECHO_CARET } else { "" };
    match ui.input_echo_color.as_deref().and_then(echo_rgb) {
        Some((r, g, b)) => format!("{caret}\x1b[38;2;{r};{g};{b}m{line}\x1b[0m"),
        None => format!("{caret}{line}"),
    }
}

/// The grey `›` and space before each command you send, in the theme's
/// bright black (SGR 90). The same bytes as `ECHO_CARET` in
/// src/lib/maskedInput.ts.
pub(crate) const ECHO_CARET: &str = "\x1b[90m\u{203a} \x1b[0m";

/// The red, green and blue of a Sent command color, the six hex digits
/// at its start after an optional `#`, or None when it does not read.
fn echo_rgb(color: &str) -> Option<(u8, u8, u8)> {
    let hex = color.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let digits = hex.get(..6)?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
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
mod tests;
