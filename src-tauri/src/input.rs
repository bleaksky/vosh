//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection.

use tokio::time::Instant;
use vosh_alias::ExpandError;
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
    /// Lua bodies queued by script-bodied aliases that fired during
    /// expansion. The session loop runs each through its shared
    /// `ScriptEngine` with `captures[1..]` bound to the alias args.
    /// Empty for non-alias inputs and for template-only aliases.
    pub(crate) scripts: Vec<vosh_alias::AliasScriptCall>,
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
    /// A `#profile reset`, or a `#profile load` that read its file,
    /// replaced the live profile.
    pub(crate) replaced: bool,
    /// The line changed the tick settings, like `#tick warn at 10`.
    pub(crate) tick_changed: bool,
}

/// Run `line` through the input pipeline: what to send, what to echo,
/// whether it replaced the live profile, and whether it changed the
/// tick settings, for [`LineEffects::note_ran`].
pub(crate) fn run_line(profile: &mut Profile, line: &str) -> Ran {
    let mut replaced = false;
    let tick_before = profile.tick.config.clone();
    let result = process_line(profile, line, &mut replaced);
    let tick_changed = profile.tick.config != tick_before;
    Ran {
        result,
        replaced,
        tick_changed,
    }
}

/// What a run of input lines asks of the saved profile. Every path that
/// runs a line through [`run_line`] notes each line here in order: typed
/// input, a Settings timer command, the tick auto-fire command, and a
/// Lua `mud.input` line. So `#alias` or `#trigger` from a timer reaches
/// disk the way the same line typed at the prompt does.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineEffects {
    /// A `#profile reset` or `#profile load` replaced the live profile,
    /// which leaves it diverged from disk on purpose.
    pub(crate) replaced: bool,
    /// A durable change came after the last replace, or with none: a
    /// slash command, or Lua that changed durable state.
    pub(crate) dirty: bool,
    /// A `#tick` command changed the tick settings. The status line and
    /// the Settings Tick card show them, so every window hears the new
    /// settings once the lines have run.
    pub(crate) tick_changed: bool,
}

impl LineEffects {
    /// Note one line [`run_line`] ran: [`Self::note`] with whether it
    /// replaced the live profile, and whether it changed the tick
    /// settings.
    pub(crate) fn note_ran(&mut self, line: &str, ran: &Ran) {
        self.note(line, ran.replaced);
        if ran.tick_changed {
            self.tick_changed = true;
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

    /// Note Lua that ran for these lines, such as the body of a script
    /// alias.
    pub(crate) fn note_script(&mut self, durable_changed: bool) {
        if durable_changed {
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
/// `#profile load` that read its file replaced the live profile.
fn process_line(profile: &mut Profile, line: &str, replaced: &mut bool) -> InputResult {
    let trimmed = line.trim_start();

    // Slash commands target the local profile.
    if let Some(rest) = trimmed.strip_prefix('#') {
        return handle_slash(profile, rest, replaced);
    }

    // A bare Enter sends a blank line to the server. MUDs use this to
    // advance prompts and paginated output.
    if trimmed.is_empty() {
        return InputResult {
            bytes: b"\r\n".to_vec(),
            echo: Vec::new(),
            scripts: Vec::new(),
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
        let mut inner = process_line(profile, &expansion, replaced);
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
        scripts: Vec::new(),
    }
}

fn handle_slash(profile: &mut Profile, rest: &str, replaced: &mut bool) -> InputResult {
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
        "script" => slash_script(profile, args),
        "scripts" => slash_scripts_list(profile),
        "lua" => slash_lua(profile, args),
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
    crate::script_state::define_alias(profile, name, expansion);
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
    crate::script_state::define_alias(profile, name.clone(), expansion);
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

/// Mirror `profile.target.name` into a session var named `target` so
/// `${target}` interpolation in alias / trigger templates and Lua
/// `mud.var("target")` resolve to the live target. Called by every
/// path that sets or clears `profile.target.name`. Removing the var
/// when the target clears (rather than leaving an empty string)
/// matches the alias engine's "unknown var → leave the token
/// alone" semantics.
fn sync_target_var(profile: &mut Profile) {
    match profile.target.name.clone() {
        Some(name) => {
            profile.vars.set(Scope::Session, "target", name);
        }
        None => {
            profile.vars.remove("target");
        }
    }
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
        sync_target_var(profile);
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
    sync_target_var(profile);
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
    sync_target_var(profile);
    echo_one(format!("target: {name} (#{next}/{count})"))
}

fn run_target_clear(profile: &mut Profile) -> InputResult {
    if profile.target.name.is_none() {
        return echo_one("no target to clear".to_string());
    }
    profile.target.name = None;
    refresh_target_idx(profile);
    sync_target_var(profile);
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        group: None,
        target: vosh_trigger::TriggerTarget::Line,
    };
    match profile.triggers.set(trigger) {
        Ok(()) => echo_one(format!("trigger {name} set")),
        Err(e) => error_echo(format!("trigger {name} rejected: {e}")),
    }
}

/// `#prompt {regex}`: read your prompt with a pattern. It becomes the
/// active profile's capture, `[prompt.capture] kind = "regex"`, with
/// `settle` worked out from the pattern, so an anchored pattern that ends
/// in text reads a prompt with no line end at once. Each named group
/// reads into the value of its name, so `(?<hp>\d+)` feeds Health, and a
/// pattern with none still tells Vosh where your prompt is. The switch
/// and the design stay as they are. Older builds wrote a trigger named
/// `prompt-capture` instead, which hid the prompt in every profile.
fn slash_prompt(profile: &mut Profile, args: &str) -> InputResult {
    match split_first_word(args) {
        ("", _) => return prompt_status(profile, chrono::Local::now().fixed_offset()),
        ("game", rest) => return slash_prompt_codes(profile, rest, false),
        ("fight", rest) => return slash_prompt_codes(profile, rest, true),
        ("show", rest) => return slash_prompt_show(profile, rest),
        ("default", rest) => return slash_prompt_default(profile, rest),
        _ => {}
    }
    let Some((pattern, _rest)) = parse_braced_pattern(args) else {
        return error_echo("usage #prompt {regex with named groups like (?<hp>\\d+)}".to_string());
    };
    let regex = match regex::Regex::new(&pattern) {
        Ok(r) => r,
        Err(e) => return error_echo(format!("Vosh cannot read that pattern. {e}")),
    };
    let names: Vec<String> = regex
        .capture_names()
        .flatten()
        .map(str::to_string)
        .collect();
    let capture = vosh_prompt::config::RegexCapture {
        settle: vosh_prompt::capture::settle(&pattern),
        lines: vec![pattern],
        names: std::collections::BTreeMap::new(),
        seen_at: Some(
            chrono::Local::now()
                .fixed_offset()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        ),
        source: Some(vosh_prompt::config::CaptureSource::Typed),
    };
    let mut config = profile.prompt.config().clone();
    config.capture = vosh_prompt::CaptureConfig::Regex(capture);
    profile.set_prompt_config(config);
    echo_one(if names.is_empty() {
        "Vosh reads your prompt with this pattern.".to_string()
    } else {
        format!(
            "Vosh reads {} from your prompt with this pattern.",
            and_list(&names)
        )
    })
}

/// `#prompt game {setting}` and `#prompt fight {setting}`: read your prompt
/// from the codes of your PROMPT or fight prompt setting, typed as you
/// type it in the game. Vosh stores it as the game would and compiles it,
/// then says what it reads and any warning. The capture becomes the
/// active profile's `kind = "aabahran"` with source typed. On the new
/// build the next Char.Prompt replaces it while the capture follows the
/// game (D25).
fn slash_prompt_codes(profile: &mut Profile, args: &str, fight: bool) -> InputResult {
    use vosh_prompt::aabahran::{self, lex, Origin, Which};
    use vosh_prompt::config::{AabahranCapture, CaptureSource};
    use vosh_prompt::CaptureConfig;

    let usage = if fight {
        "usage #prompt fight {your fight prompt setting}"
    } else {
        "usage #prompt game {your PROMPT setting}"
    };
    let Some((typed, _rest)) = parse_braced_pattern(args) else {
        return error_echo(usage.to_string());
    };
    let held = match &profile.prompt.config().capture {
        CaptureConfig::Aabahran(codes) => Some(codes.clone()),
        _ => None,
    };
    if fight && held.is_none() {
        return echo_one(PROMPT_NONE.to_string());
    }
    if !fight && typed.trim().eq_ignore_ascii_case("off") {
        return error_echo(
            "That turns prompts off in the game. Type the prompt setting you use.".to_string(),
        );
    }
    let which = if fight { Which::Fight } else { Which::Prompt };
    let normalized = lex::normalize(&typed, which, profile.prompt.who());
    let codes = held.unwrap_or_default();
    let (prompt, fprompt) = if fight {
        (codes.prompt.clone(), normalized.text)
    } else {
        (normalized.text, codes.fprompt.clone())
    };
    let compiled = match aabahran::compile(&prompt, &fprompt, Origin::Stored, profile.prompt.who())
    {
        Ok(compiled) => compiled,
        Err(e) => return error_echo(e.text),
    };
    let mut config = profile.prompt.config().clone();
    config.capture = CaptureConfig::Aabahran(AabahranCapture {
        prompt: compiled.prompt.clone(),
        fprompt: compiled.fprompt.clone(),
        follow_game: codes.follow_game,
        seen_at: Some(
            chrono::Local::now()
                .fixed_offset()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        ),
        source: Some(CaptureSource::Typed),
    });
    profile.set_prompt_config(config);
    let mut echo = vec![aabahran::reads_sentence(&compiled.reads(which), fight)];
    echo.extend(
        normalized
            .warnings
            .iter()
            .chain(compiled.warnings.iter().filter(|w| w.which == which))
            .map(|w| w.text.clone()),
    );
    InputResult {
        bytes: Vec::new(),
        echo,
        scripts: Vec::new(),
    }
}

/// `#prompt show text|lifted|pinned`: where your prompt shows. In the
/// text as the game sends it, lifted on a band in the text, or pinned on
/// a band above the command line with earlier prompts out of the text.
/// It needs a capture, since Vosh finds your prompt only through one.
fn slash_prompt_show(profile: &mut Profile, args: &str) -> InputResult {
    use vosh_prompt::PromptShow;
    let Some(show) = PromptShow::parse(args) else {
        return error_echo("usage #prompt show text | lifted | pinned".to_string());
    };
    if profile.prompt.config().capture.is_none() {
        return echo_one(PROMPT_NONE.to_string());
    }
    let mut config = profile.prompt.config().clone();
    config.show = show;
    profile.set_prompt_config(config);
    echo_one(show_sentence(show).to_string())
}

/// `#prompt default`: put Vosh's default design in place of the one in
/// this profile. The one you had goes first among the earlier designs,
/// so the card can offer it back, and the switch, the place and the
/// capture stay. The echo says what else it takes to see the design,
/// also when the design is the default already, as in a fresh profile.
fn slash_prompt_default(profile: &mut Profile, args: &str) -> InputResult {
    if !args.trim().is_empty() {
        return error_echo("usage #prompt default".to_string());
    }
    let mut config = profile.prompt.config().clone();
    let had = !config.template.is_empty();
    let changed = config.use_default_design();
    let mut echo = vec![match (changed, had) {
        (false, _) => "Your design is already Vosh's default.",
        (true, true) => {
            "Your design is now Vosh's default. Vosh keeps the one you had as an earlier design."
        }
        (true, false) => "Your design is now Vosh's default.",
    }
    .to_string()];
    if config.capture.is_none() {
        echo.push(PROMPT_NONE.to_string());
    }
    if !config.draw {
        echo.push(
            "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
                .to_string(),
        );
    }
    if changed {
        profile.set_prompt_config(config);
    }
    InputResult {
        bytes: Vec::new(),
        echo,
        scripts: Vec::new(),
    }
}

/// What `#prompt show` says once your prompt shows at `show`.
fn show_sentence(show: vosh_prompt::PromptShow) -> &'static str {
    use vosh_prompt::PromptShow;
    match show {
        PromptShow::Text => "Your prompt shows in the text.",
        PromptShow::Lifted => "Each prompt shows on a raised band in the text.",
        PromptShow::Pinned => "Your latest prompt shows pinned above the command line.",
    }
}

/// What `#prompt` says when nothing reads your prompt in this profile.
const PROMPT_NONE: &str = "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.";
/// What `#prompt` says while you have prompts off in the game.
const PROMPTS_OFF: &str =
    "You turned prompts off in the game. Type prompt in the game to turn them back on.";

/// `#prompt` alone says what reads your prompt in this profile, when it
/// last matched, whether Vosh draws, and whether you turned prompts off
/// in the game. `now` sets the clock the times read in.
fn prompt_status(profile: &Profile, now: chrono::DateTime<chrono::FixedOffset>) -> InputResult {
    use vosh_prompt::{CaptureConfig, Status};
    let engine = &profile.prompt;
    let clock = |at: chrono::DateTime<chrono::FixedOffset>| {
        at.with_timezone(now.offset()).format("%-I:%M").to_string()
    };
    let mut echo = Vec::new();
    let reads = match &engine.config().capture {
        CaptureConfig::None => None,
        CaptureConfig::Aabahran(codes) => Some(format!(
            "Vosh reads your prompt from the codes {}.",
            codes.prompt.trim_end()
        )),
        CaptureConfig::Regex(_) => {
            Some("Vosh reads your prompt with a pattern you pointed at.".to_string())
        }
    };
    match reads {
        None => echo.push(PROMPT_NONE.to_string()),
        Some(reads) => {
            let matched = match engine.last_match_at() {
                Some(at) => format!("It last matched at {}.", clock(at)),
                None => "No prompt has matched since you connected.".to_string(),
            };
            let drawing = if engine.config().draw {
                "Drawing is on."
            } else {
                "Drawing is off."
            };
            let shows = match engine.config().show {
                vosh_prompt::PromptShow::Text => "It shows in the text.",
                vosh_prompt::PromptShow::Lifted => "It shows lifted in the text.",
                vosh_prompt::PromptShow::Pinned => "It shows pinned above the command line.",
            };
            echo.push(format!("{reads} {matched} {drawing} {shows}"));
            // The game showed a PROMPT that the moved pattern could not
            // switch to.
            if let Some(kept) = engine.kept_pattern() {
                echo.push(kept);
            }
            if engine.status() == Status::NotMatching {
                let since = engine
                    .last_match_at()
                    .map_or_else(|| "you connected".to_string(), clock);
                echo.push(format!(
                    "No prompt has matched since {since}. If you changed it in the game, point at it again."
                ));
            }
        }
    }
    if engine.prompts_off() {
        echo.push(PROMPTS_OFF.to_string());
    }
    InputResult {
        bytes: Vec::new(),
        echo,
        scripts: Vec::new(),
    }
}

/// `A`, `A and B`, or `A, B, and C`.
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// `#unprompt`: stop reading your prompt in the active profile. The
/// game's prompt shows again, and the design stays saved.
fn slash_unprompt(profile: &mut Profile) -> InputResult {
    if profile.prompt.config().capture.is_none() {
        return echo_one("Vosh does not read your prompt in this profile.".to_string());
    }
    let mut config = profile.prompt.config().clone();
    config.capture = vosh_prompt::CaptureConfig::None;
    profile.set_prompt_config(config);
    echo_one("Vosh stopped reading your prompt. Your design stays saved.".to_string())
}

/// `#group <name> [on|off]` — flip a group's enabled state across
/// triggers, aliases, and macros in one call. With no on/off arg,
/// echo the current state in each store the group appears in.
fn slash_group(profile: &mut Profile, args: &str) -> InputResult {
    let (name, rest) = split_first_word(args);
    if name.is_empty() {
        return error_echo("usage #group <name> [on|off]".to_string());
    }
    let state = rest.trim();
    match state {
        "" => slash_group_show(profile, name),
        "on" | "off" => {
            let enabled = state == "on";
            let report = crate::script_state::toggle_group(profile, name, enabled);
            if !report.touched() {
                return error_echo(format!(
                    "group `{name}` not found in triggers, aliases, or macros"
                ));
            }
            let mut stores: Vec<&str> = Vec::with_capacity(3);
            if report.triggers {
                stores.push("triggers");
            }
            if report.aliases {
                stores.push("aliases");
            }
            if report.macros {
                stores.push("macros");
            }
            echo_one(format!(
                "group `{name}` {} for {}",
                if enabled { "enabled" } else { "disabled" },
                stores.join(" + "),
            ))
        }
        other => error_echo(format!(
            "unknown group state `{other}`. usage #group <name> [on|off]"
        )),
    }
}

fn slash_group_show(profile: &Profile, name: &str) -> InputResult {
    use crate::script_state::GroupState;
    let [trigger_state, alias_state, macro_state] =
        crate::script_state::group_states(profile, name);
    if trigger_state.is_none() && alias_state.is_none() && macro_state.is_none() {
        return error_echo(format!(
            "group `{name}` not found in triggers, aliases, or macros"
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
    InputResult {
        bytes: Vec::new(),
        echo: lines,
        scripts: Vec::new(),
    }
}

/// `#groups` — every group that any store has at least one entry
/// tagged with, plus the current on/off state per store.
fn slash_groups_list(profile: &Profile) -> InputResult {
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
    if names.is_empty() {
        return echo_one("no groups defined".to_string());
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
        ]
        .into_iter()
        .filter_map(|(store, state)| state.map(|s| format!("{store}={s}")))
        .collect();
        lines.push(format!("  {name}: {}", parts.join(", ")));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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

/// What `#profile save`, `load`, and `reset` answer between the shared
/// catalog wizard and the relaunch that finishes it. Nothing saves in
/// that window, the profile files hold no aliases, triggers, or macros
/// any more, and the shared catalog loads only at launch.
const PROFILE_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts.";

fn slash_profile(profile: &mut Profile, args: &str, replaced: &mut bool) -> InputResult {
    let pending =
        crate::commands::MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    slash_profile_with(profile, args, replaced, pending)
}

/// [`slash_profile`] with `migration_pending` in place of
/// [`crate::commands::MIGRATION_RELAUNCH_PENDING`], so a test can run it
/// after the wizard without touching the flag every other test reads.
fn slash_profile_with(
    profile: &mut Profile,
    args: &str,
    replaced: &mut bool,
    migration_pending: bool,
) -> InputResult {
    let (cmd, _rest) = split_first_word(args);
    if migration_pending && matches!(cmd, "save" | "load" | "reset") {
        return error_echo(PROFILE_MIGRATION_PENDING.to_string());
    }
    // Path B keeps authored items in the catalog and persists them
    // automatically. The legacy save/load/reset trio would write, load,
    // or blank the wrong files there, so it bows out with a pointer.
    if PATH_B_ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
        return match cmd {
            "save" => echo_one("loadout mode saves your changes automatically".to_string()),
            "load" => echo_one("loadout mode loads the catalog at startup".to_string()),
            "reset" => error_echo(
                "profile reset does not apply in loadout mode. delete items from settings instead"
                    .to_string(),
            ),
            "" => error_echo("usage #profile save | load | reset".to_string()),
            other => error_echo(format!("unknown #profile subcommand `{other}`")),
        };
    }
    match cmd {
        "save" => match profile_path() {
            Some(path) => {
                // Every profile file write holds the persist lock. This
                // runs under the profile lock, which the persist takes
                // after the persist lock, so it only tries.
                let Ok(_persist_guard) = crate::commands::PERSIST_LOCK.try_lock() else {
                    return error_echo("Vosh is saving this profile. Try again.".to_string());
                };
                if crate::profile_config::is_unread(&path) {
                    return error_echo(
                        "Vosh could not read this profile file at launch, so it will not save \
                         over it. Fix the file or switch to another profile."
                            .to_string(),
                    );
                }
                let snapshot = ProfileConfig::from_profile(profile);
                match snapshot.save(&path) {
                    Ok(()) => echo_one(format!("profile saved to {}", path.display())),
                    Err(e) => error_echo(format!("save failed: {e}")),
                }
            }
            None => error_echo("could not resolve profile path".to_string()),
        },
        "load" => match profile_path() {
            Some(path) => load_profile_file(profile, &path, replaced),
            None => error_echo("could not resolve profile path".to_string()),
        },
        "reset" => {
            let blank = ProfileConfig::default();
            let _ = blank.apply_to(profile);
            *replaced = true;
            echo_one("profile reset to defaults".to_string())
        }
        "" => error_echo("usage #profile save | load | reset".to_string()),
        other => error_echo(format!("unknown #profile subcommand `{other}`")),
    }
}

/// `#profile load` from `path`. Replaces the live profile, and sets
/// `replaced`, only when the file reads. A file that does not read leaves
/// the live profile as it was.
fn load_profile_file(
    profile: &mut Profile,
    path: &std::path::Path,
    replaced: &mut bool,
) -> InputResult {
    let snapshot = match ProfileConfig::load(path) {
        Ok(snapshot) => snapshot,
        Err(e) => return error_echo(format!("load failed: {e}")),
    };
    // The file reads now and the live profile holds what it says, so the
    // saves may write it again.
    crate::profile_config::release_unread(path);
    let warnings = snapshot.apply_to(profile);
    *replaced = true;
    let mut lines = vec![format!("profile loaded from {}", path.display())];
    for w in warnings {
        lines.push(format!("  {w}"));
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
        scripts: Vec::new(),
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
        scripts: Vec::new(),
    }
}

/// Resolve the active profile's on-disk path. Reads the profile
/// index (`profiles.toml`) to learn which profile is active and
/// returns `<app_data>/profiles/<active>.toml`. Falls back to the
/// legacy `<app_data>/profile.toml` only when no `profiles.toml`
/// index exists — which is the pre-multi-profile layout.
///
/// Before this resolver, `#profile load` and `#profile save`
/// silently routed to the legacy single-file path even after
/// migration, so a "load" overwrote in-memory state with whatever
/// the empty legacy file had (and a "save" wrote the active
/// profile's state into the wrong file). Now they hit the file the
/// user is actually editing.
fn profile_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let app_data = std::path::PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("com.aabahran.vosh");
    let index = app_data.join("profiles.toml");
    if let Ok(body) = std::fs::read_to_string(&index) {
        if let Ok(value) = body.parse::<toml::Value>() {
            if let Some(active) = value.get("active").and_then(|v| v.as_str()) {
                let path = app_data.join("profiles").join(format!("{active}.toml"));
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    Some(app_data.join("profile.toml"))
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
    }
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
        scripts: Vec::new(),
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
                scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
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
        scripts: Vec::new(),
    }
}

fn echo_one(message: String) -> InputResult {
    InputResult {
        bytes: Vec::new(),
        echo: vec![message],
        scripts: Vec::new(),
    }
}

fn echo_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> InputResult {
    InputResult {
        bytes: Vec::new(),
        echo: lines.into_iter().map(str::to_string).collect(),
        scripts: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vosh_alias::Alias;

    fn regex_capture(p: &Profile) -> vosh_prompt::config::RegexCapture {
        match &p.prompt.config().capture {
            vosh_prompt::CaptureConfig::Regex(capture) => capture.clone(),
            other => panic!("a regex capture, got {other:?}"),
        }
    }

    #[test]
    fn prompt_writes_a_regex_capture_to_the_profile() {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
        let ran = run_line(
            &mut p,
            r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)m\]}",
        );
        assert_eq!(
            ran.result.echo,
            ["Vosh reads hp, maxhp, and mana from your prompt with this pattern."]
        );
        let capture = regex_capture(&p);
        assert_eq!(
            capture.lines,
            [r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)m\]"]
        );
        assert!(
            !capture.settle,
            "an unanchored pattern waits for a line end"
        );
        assert_eq!(
            capture.source,
            Some(vosh_prompt::config::CaptureSource::Typed)
        );
        assert!(capture.seen_at.is_some());
        assert!(capture.names.is_empty());
        // The switch and the design stay, and no trigger is written.
        assert!(p.prompt.config().draw);
        assert_eq!(p.prompt.config().template, "%hp");
        assert!(p.triggers.get("prompt-capture").is_none());
        assert!(p.prompt.stage.has_recognizer());

        // An anchored pattern that ends in text settles.
        let ran = run_line(&mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
        assert_eq!(
            ran.result.echo,
            ["Vosh reads hp from your prompt with this pattern."]
        );
        assert!(regex_capture(&p).settle);
        // A pattern with no groups only says where your prompt is.
        let ran = run_line(&mut p, "#prompt {^> $}");
        assert_eq!(
            ran.result.echo,
            ["Vosh reads your prompt with this pattern."]
        );
    }

    #[test]
    fn prompt_with_a_bad_pattern_changes_nothing() {
        let mut p = Profile::default();
        let _ = run_line(&mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
        let before = p.prompt.config().clone();
        let ran = run_line(&mut p, r"#prompt {\[(?<hp>\d+}");
        assert!(
            ran.result.echo[0].starts_with("[Vosh cannot read that pattern."),
            "{:?}",
            ran.result.echo
        );
        assert_eq!(*p.prompt.config(), before);
        let ran = run_line(&mut p, "#prompt {");
        assert!(ran.result.echo[0].starts_with("[usage #prompt"));
    }

    fn codes_of(p: &Profile) -> vosh_prompt::config::AabahranCapture {
        match &p.prompt.config().capture {
            vosh_prompt::CaptureConfig::Aabahran(codes) => codes.clone(),
            other => panic!("an aabahran capture, got {other:?}"),
        }
    }

    #[test]
    fn prompt_game_stores_the_setting_as_the_game_does_and_says_what_it_reads() {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
        let ran = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        assert_eq!(
            ran.result.echo,
            ["Vosh reads Health, Mana, and Moves with their maxes from this prompt. It also reads Tank and Tank health."]
        );
        assert!(ran.result.bytes.is_empty(), "Vosh never sends it");
        let codes = codes_of(&p);
        assert_eq!(codes.prompt, "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c");
        assert_eq!(codes.fprompt, "");
        assert!(codes.follow_game);
        assert_eq!(
            codes.source,
            Some(vosh_prompt::config::CaptureSource::Typed)
        );
        assert!(codes.seen_at.is_some());
        assert!(p.prompt.stage.has_recognizer());
        assert_eq!(p.prompt.config().template, "%hp", "the design stays");

        // As do_prompt stores it: prompt all, and a space added.
        let _ = run_line(&mut p, "#prompt game {all}");
        assert_eq!(codes_of(&p).prompt, "%n%P%C<%hhp %mm %vmv> ");
        let _ = run_line(&mut p, "#prompt game {<%hhp>}");
        assert_eq!(codes_of(&p).prompt, "<%hhp> ");
        // No space around the setting reaches the game.
        let _ = run_line(&mut p, "#prompt game { <%hhp %mm> }");
        assert_eq!(codes_of(&p).prompt, "<%hhp %mm> ");
        let _ = run_line(&mut p, "#prompt game { all }");
        assert_eq!(codes_of(&p).prompt, "%n%P%C<%hhp %mm %vmv> ");
    }

    #[test]
    fn prompt_game_says_every_warning_and_refuses_what_it_cannot_read() {
        let mut p = Profile::default();
        let ran = run_line(&mut p, "#prompt game {<%h%m %vmv>}");
        assert_eq!(
            ran.result.echo,
            [
                "Vosh reads Moves from this prompt.",
                "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
            ]
        );
        // The game keeps a typed backtick only from trust 55.
        trusted(&mut p);
        let before = p.prompt.config().clone();
        let ran = run_line(&mut p, "#prompt game {<`%h>}");
        assert_eq!(
            ran.result.echo,
            ["[A color code runs into %h. Put a space between them in the game.]"]
        );
        assert_eq!(*p.prompt.config(), before);
        let ran = run_line(&mut p, "#prompt game {off}");
        assert_eq!(
            ran.result.echo,
            ["[That turns prompts off in the game. Type the prompt setting you use.]"]
        );
        let ran = run_line(&mut p, "#prompt game");
        assert_eq!(
            ran.result.echo,
            ["[usage #prompt game {your PROMPT setting}]"]
        );
    }

    /// Char.Status for an immortal with trust 55, whose typed backticks
    /// the game keeps.
    fn trusted(p: &mut Profile) {
        p.prompt.observe(
            "Char.Status",
            serde_json::json!({"name": "Tester", "level": 60}),
            chrono::Local::now().fixed_offset(),
        );
    }

    #[test]
    fn prompt_game_stores_what_the_game_keeps_of_your_backticks() {
        // A mortal, or anyone before Char.Status names a level, loses
        // each backtick and the character after it.
        let mut p = Profile::default();
        let _ = run_line(&mut p, "#prompt game {`(240)[%h/%Hhp]}");
        assert_eq!(codes_of(&p).prompt, "240)[%h/%Hhp] ");
        trusted(&mut p);
        let _ = run_line(&mut p, "#prompt game {`(240)[%h/%Hhp]}");
        assert_eq!(codes_of(&p).prompt, "`(240)[%h/%Hhp] ");
    }

    #[test]
    fn prompt_fight_sets_the_fight_prompt_beside_your_prompt() {
        let mut p = Profile::default();
        trusted(&mut p);
        let ran = run_line(&mut p, "#prompt fight {`1%h``hp [%p] >}");
        assert_eq!(
            ran.result.echo,
            ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
        );
        assert!(p.prompt.config().capture.is_none());
        let _ = run_line(&mut p, "#prompt game {<%hhp>}");
        let ran = run_line(&mut p, "#prompt fight {`1%h``hp [%p] >}");
        assert_eq!(
            ran.result.echo,
            ["Vosh reads Health from this fight prompt. It also reads Tank health."]
        );
        let codes = codes_of(&p);
        assert_eq!(codes.prompt, "<%hhp> ");
        assert_eq!(codes.fprompt, "`1%h``hp [%p] > ");
        let _ = run_line(&mut p, "#prompt fight {off}");
        assert_eq!(codes_of(&p).fprompt, "");
        assert_eq!(codes_of(&p).prompt, "<%hhp> ");
    }

    #[test]
    fn prompt_alone_says_how_vosh_reads_your_prompt() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-29T17:30:00-05:00").unwrap();
        let status = |p: &Profile| super::prompt_status(p, now).echo;
        let mut p = Profile::default();
        assert_eq!(
            status(&p),
            ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
        );
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
        let _ = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        assert_eq!(
            status(&p),
            ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. No prompt has matched since you connected. Drawing is on. It shows in the text."]
        );
        p.prompt.connect(true);
        let matched = chrono::DateTime::parse_from_rfc3339("2026-09-29T05:04:00-05:00").unwrap();
        p.prompt.note_prompt(matched);
        assert_eq!(
            status(&p),
            ["Vosh reads your prompt from the codes %n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c. It last matched at 5:04. Drawing is on. It shows in the text."]
        );
        // Three pulses with no prompt.
        for _ in 0..4 {
            p.prompt
                .observe("Char.Vitals", serde_json::json!({"hp": 1}), now);
        }
        assert_eq!(
            status(&p)[1],
            "No prompt has matched since 5:04. If you changed it in the game, point at it again."
        );
        let _ = run_line(&mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
        p.set_prompt_config(vosh_prompt::PromptConfig {
            draw: false,
            ..p.prompt.config().clone()
        });
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": false, "prompt": "%h ", "fprompt": ""}),
            now,
        );
        assert_eq!(
            status(&p),
            [
                "Vosh reads your prompt with a pattern you pointed at. It last matched at 5:04. Drawing is off. It shows in the text.",
                "You turned prompts off in the game. Type prompt in the game to turn them back on."
            ]
        );
    }

    #[test]
    fn prompt_show_picks_where_your_prompt_shows_and_the_status_says_it() {
        use vosh_prompt::PromptShow;
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
        let mut p = Profile::default();
        // With nothing reading your prompt there is nothing to show.
        let ran = run_line(&mut p, "#prompt show pinned");
        assert_eq!(
            ran.result.echo,
            ["Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start."]
        );
        assert_eq!(p.prompt.config().show, PromptShow::Text);

        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
        let _ = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        for (line, show, echo, status) in [
            (
                "#prompt show pinned",
                PromptShow::Pinned,
                "Your latest prompt shows pinned above the command line.",
                "It shows pinned above the command line.",
            ),
            (
                "#prompt show Lifted",
                PromptShow::Lifted,
                "Each prompt shows on a raised band in the text.",
                "It shows lifted in the text.",
            ),
            (
                "#prompt show text",
                PromptShow::Text,
                "Your prompt shows in the text.",
                "It shows in the text.",
            ),
        ] {
            let ran = run_line(&mut p, line);
            assert_eq!(ran.result.echo, [echo], "{line}");
            assert_eq!(p.prompt.config().show, show, "{line}");
            let said = super::prompt_status(&p, now).echo;
            assert!(
                said[0].ends_with(&format!("Drawing is on. {status}")),
                "{said:?}"
            );
        }
        // The design and the capture stay.
        assert_eq!(p.prompt.config().template, "%hp");
        assert!(p.prompt.config().capture.is_aabahran());

        for line in ["#prompt show", "#prompt show sideways"] {
            let ran = run_line(&mut p, line);
            assert_eq!(
                ran.result.echo,
                ["[usage #prompt show text | lifted | pinned]"],
                "{line}"
            );
        }
        assert_eq!(p.prompt.config().show, PromptShow::Text);
        // The help names it.
        assert!(super::HELP_TEXT.contains("#prompt show text|lifted|pinned"));
    }

    #[test]
    fn prompt_default_puts_the_default_design_in_place_and_keeps_yours() {
        use vosh_prompt::{PromptShow, DEFAULT_DESIGN};
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig {
            show: PromptShow::Pinned,
            ..vosh_prompt::PromptConfig::from_legacy(true, "%hp")
        });
        let _ = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        let capture = p.prompt.config().capture.clone();

        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(
            ran.result.echo,
            ["Your design is now Vosh's default. Vosh keeps the one you had as an earlier design."]
        );
        assert!(ran.result.bytes.is_empty());
        let config = p.prompt.config();
        assert_eq!(config.template, DEFAULT_DESIGN);
        assert_eq!(config.previous_templates, ["%hp"]);
        // The switch, the place and the capture stay.
        assert!(config.draw);
        assert_eq!(config.show, PromptShow::Pinned);
        assert_eq!(config.capture, capture);
        assert_eq!(p.ui.prompt_template, DEFAULT_DESIGN);

        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(ran.result.echo, ["Your design is already Vosh's default."]);
        assert_eq!(p.prompt.config().previous_templates, ["%hp"]);

        let ran = run_line(&mut p, "#prompt default please");
        assert_eq!(ran.result.echo, ["[usage #prompt default]"]);
        // The help names it.
        assert!(super::HELP_TEXT.contains("#prompt default "));
    }

    #[test]
    fn prompt_default_says_what_else_it_takes_to_see_the_design() {
        // Drawing off.
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(false, "%hp"));
        let _ = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(
            ran.result.echo,
            [
                "Your design is now Vosh's default. Vosh keeps the one you had as an earlier design.",
                "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
            ]
        );
        assert!(!p.prompt.config().draw);

        // Nothing reads your prompt yet, and there was no design to keep.
        let mut p = Profile::default();
        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(
            ran.result.echo,
            [
                "Your design is now Vosh's default.",
                "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
                "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
            ]
        );
        assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
        assert!(p.prompt.config().previous_templates.is_empty());

        // A fresh profile already holds the default design, and still
        // hears what else it takes.
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::fresh());
        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(
            ran.result.echo,
            [
                "Your design is already Vosh's default.",
                "Vosh does not read your prompt in this profile. Type #prompt game and your prompt setting in braces to start.",
                "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
            ]
        );
        let _ = run_line(&mut p, "#prompt game {%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c}");
        let ran = run_line(&mut p, "#prompt default");
        assert_eq!(
            ran.result.echo,
            [
                "Your design is already Vosh's default.",
                "Turn on Draw your own prompt in Settings under Input, then Prompt, to see it."
            ]
        );
        assert_eq!(p.prompt.config().template, vosh_prompt::DEFAULT_DESIGN);
        assert!(p.prompt.config().previous_templates.is_empty());
    }

    /// A table with the pattern the move from a capture trigger wrote.
    fn migrated() -> vosh_prompt::PromptConfig {
        vosh_prompt::PromptConfig {
            capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
                lines: vec![r"\[(?<hp>\d+)/(?<maxhp>\d+)hp\]".into()],
                source: Some(vosh_prompt::config::CaptureSource::Migrated),
                ..vosh_prompt::config::RegexCapture::default()
            }),
            ..vosh_prompt::PromptConfig::from_legacy(true, "%hp")
        }
    }

    #[test]
    fn prompt_says_in_one_sentence_why_the_moved_pattern_stayed() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
        let mut p = Profile::default();
        p.set_prompt_config(migrated());
        p.prompt.connect(true);
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "<`%h> ", "fprompt": ""}),
            now,
        );
        assert_eq!(*p.prompt.config(), migrated(), "the pattern stays");
        assert_eq!(
            super::prompt_status(&p, now).echo,
            [
                "Vosh reads your prompt with a pattern you pointed at. No prompt has matched since you connected. Drawing is on. It shows in the text.",
                "Vosh kept the pattern from your old capture trigger because a color code runs into %h in the prompt the game sent.",
            ]
        );
        // Once the game sends a prompt Vosh reads, the pattern switches
        // and the reason goes.
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
            now,
        );
        assert_eq!(
            super::prompt_status(&p, now).echo,
            ["Vosh reads your prompt from the codes <%hhp>. No prompt has matched since you connected. Drawing is on. It shows in the text."]
        );
    }

    #[test]
    fn a_pattern_you_set_never_switches_to_the_codes_the_game_sends() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-30T09:00:00-05:00").unwrap();
        let mut p = Profile::default();
        p.set_prompt_config(migrated());
        p.prompt.connect(true);
        let _ = run_line(&mut p, r"#prompt {\[(?<hp>\d+)/(?<maxhp>\d+)hp\]}");
        let typed = p.prompt.config().clone();
        assert!(!typed.capture.is_migrated());
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
            now,
        );
        assert_eq!(*p.prompt.config(), typed);

        // #unprompt leaves nothing to switch.
        let mut p = Profile::default();
        p.set_prompt_config(migrated());
        p.prompt.connect(true);
        let _ = run_line(&mut p, "#unprompt");
        p.prompt.observe(
            "Char.Prompt",
            serde_json::json!({"enabled": true, "prompt": "<%hhp> ", "fprompt": ""}),
            now,
        );
        assert!(p.prompt.config().capture.is_none());
    }

    #[test]
    fn unprompt_stops_reading_and_keeps_the_design() {
        let mut p = Profile::default();
        p.set_prompt_config(vosh_prompt::PromptConfig::from_legacy(true, "%hp"));
        let ran = run_line(&mut p, "#unprompt");
        assert_eq!(
            ran.result.echo,
            ["Vosh does not read your prompt in this profile."]
        );
        let _ = run_line(&mut p, r"#prompt {^<(?<hp>\d+)hp> $}");
        let ran = run_line(&mut p, "#unprompt");
        assert_eq!(
            ran.result.echo,
            ["Vosh stopped reading your prompt. Your design stays saved."]
        );
        assert!(p.prompt.config().capture.is_none());
        assert!(!p.prompt.stage.has_recognizer());
        assert_eq!(p.prompt.config().template, "%hp");
        assert!(p.prompt.config().draw);
    }

    /// Run `lines` through the pipeline the way the typed path does and
    /// note each one.
    fn effects_of(lines: &[&str]) -> LineEffects {
        let mut p = Profile::default();
        let mut effects = LineEffects::default();
        for line in lines {
            let ran = run_line(&mut p, line);
            effects.note_ran(line, &ran);
        }
        effects
    }

    const DIRTY: LineEffects = LineEffects {
        replaced: false,
        dirty: true,
        tick_changed: false,
    };

    const REPLACED: LineEffects = LineEffects {
        replaced: true,
        dirty: false,
        tick_changed: false,
    };

    /// Whether `line` changed the tick settings of `p`.
    fn changes_tick(p: &mut Profile, line: &str) -> bool {
        run_line(p, line).tick_changed
    }

    #[test]
    fn a_tick_command_that_changes_a_setting_says_so() {
        let mut p = Profile::default();
        for line in [
            "#tick warn at 10",
            "#tick warn at 5",
            "#tick warn message duck",
            "#tick warn color red",
            "#tick warn off",
            "#tick interval 40",
            "#tick on {^The sun}",
            "#tick off",
            "#tick fire score",
            "#tick nofire",
            "#tick sound off",
            "#tick disable",
            "#tick enable",
        ] {
            assert!(changes_tick(&mut p, line), "{line}");
        }
    }

    #[test]
    fn a_line_that_leaves_the_tick_settings_alone_says_nothing() {
        let mut p = Profile::default();
        let _ = run_line(&mut p, "#tick warn at 10");
        for line in [
            "look",
            "#tick",
            "#tick warn",
            "#tick reset",
            "#tick warn at 10",
            "#tick warn at nonsense",
            "#tick interval 0",
            "#alias greet wave",
        ] {
            assert!(!changes_tick(&mut p, line), "{line}");
        }
    }

    #[test]
    fn the_effects_remember_a_tick_change_across_the_run() {
        let effects = effects_of(&["#tick warn at 10", "look"]);
        assert!(effects.tick_changed);
        assert!(effects.dirty);
        assert!(!effects_of(&["#tick", "look"]).tick_changed);
    }

    #[test]
    fn slash_commands_mark_the_profile_dirty() {
        for line in [
            "#alias greet wave",
            "#trigger flee {^You flee} send look",
            "  #var x 1",
        ] {
            assert_eq!(effects_of(&[line]), DIRTY, "{line}");
        }
        assert_eq!(effects_of(&["look", "greet"]), LineEffects::default());
    }

    #[test]
    fn profile_save_load_and_reset_wait_for_the_relaunch_after_the_wizard() {
        let mut p = Profile::default();
        p.aliases.set(vosh_alias::Alias::new("kk", "kick %1"));
        for sub in ["save", "load", "reset"] {
            let mut replaced = false;
            let result = slash_profile_with(&mut p, sub, &mut replaced, true);
            assert_eq!(
                result.echo,
                ["[Quit Vosh and open it again to finish the move to loadouts.]"],
                "{sub}"
            );
            assert!(!replaced, "{sub}");
            assert!(p.aliases.get("kk").is_some(), "{sub}");
        }
    }

    #[test]
    fn a_reset_replaces_the_profile_and_saves_nothing() {
        for line in ["#profile reset", "#profile  reset", "# profile reset"] {
            assert_eq!(effects_of(&[line]), REPLACED, "{line}");
        }
        // An edit before the reset is gone with it. An edit after it
        // says the live state is wanted.
        assert_eq!(effects_of(&["#alias a b", "#profile reset"]), REPLACED);
        assert_eq!(
            effects_of(&["#profile reset", "#alias a b"]),
            LineEffects {
                replaced: true,
                dirty: true,
                tick_changed: false,
            }
        );
    }

    #[test]
    fn a_load_replaces_the_profile_only_when_its_file_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Healer.toml");
        std::fs::write(&path, "tracked = = [\n").unwrap();
        let mut p = Profile::default();
        let _ = process(&mut p, "#alias greet wave");

        let mut replaced = false;
        let r = load_profile_file(&mut p, &path, &mut replaced);
        assert!(!replaced);
        assert!(r.echo[0].contains("load failed"), "{:?}", r.echo);
        assert!(p.aliases.get("greet").is_some());
        // So the alias still saves, and a reset before it stays unsaved.
        let mut effects = LineEffects::default();
        effects.note("#alias greet wave", false);
        effects.note("#profile load", replaced);
        assert_eq!(effects, DIRTY);
        let mut effects = REPLACED;
        effects.note("#profile load", replaced);
        assert_eq!(effects, REPLACED);

        ProfileConfig::default().save(&path).unwrap();
        let _ = load_profile_file(&mut p, &path, &mut replaced);
        assert!(replaced);
        assert!(p.aliases.get("greet").is_none());
        let mut effects = DIRTY;
        effects.note("#profile load", replaced);
        assert_eq!(effects, REPLACED);
    }

    #[test]
    fn a_reset_or_load_that_echoes_saves_nothing() {
        // Loadout mode turns the pair into echoes, and an echo is no
        // change to save.
        let mut effects = LineEffects::default();
        effects.note("#profile reset", false);
        effects.note("#profile load", false);
        assert_eq!(effects, LineEffects::default());
    }

    #[test]
    fn durable_lua_marks_the_profile_dirty() {
        let mut effects = effects_of(&["greet"]);
        effects.note_script(false);
        assert_eq!(effects, LineEffects::default());
        effects.note_script(true);
        assert_eq!(effects, DIRTY);
    }

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
    fn slash_alias_replaces_an_alias_in_its_group() {
        let mut p = Profile::default();
        let mut heal = Alias::new("hl", "cast heal");
        heal.group = Some("healing".into());
        p.aliases.set(heal);
        let _ = process(&mut p, "#alias hl cast 'cure light'");
        let hl = p.aliases.get("hl").unwrap();
        assert_eq!(hl.expansion, "cast 'cure light'");
        assert_eq!(hl.group.as_deref(), Some("healing"));
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
    fn a_recording_replaces_an_alias_in_its_group() {
        let mut p = Profile::default();
        let mut buff = Alias::new("buff", "cast 'armor' self");
        buff.group = Some("buffs".into());
        p.aliases.set(buff);
        let _ = process(&mut p, "#record buff");
        let _ = process(&mut p, "cast 'haste' self");
        let _ = process(&mut p, "#endrec");
        let buff = p.aliases.get("buff").unwrap();
        assert_eq!(buff.expansion, "cast 'haste' self");
        assert_eq!(buff.group.as_deref(), Some("buffs"));
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

    #[test]
    fn logs_forget_passwords_reads_like_the_slash_dispatcher() {
        for line in [
            "#logs forget-passwords",
            "  #logs   forget-passwords  ",
            "# logs forget-passwords",
        ] {
            assert_eq!(logs_command(line), Some(LogsCommand::Preview), "{line:?}");
        }
        for line in [
            "#logs forget-passwords now",
            "#logs  forget-passwords   now ",
        ] {
            assert_eq!(logs_command(line), Some(LogsCommand::Forget), "{line:?}");
        }
        for line in [
            "#logs",
            "#logs forget",
            "#logs forget-passwords later",
            "#logs forget-passwords now please",
            "#logs Forget-Passwords",
        ] {
            assert_eq!(logs_command(line), Some(LogsCommand::Usage), "{line:?}");
        }
        for line in [
            "#log forget-passwords",
            "logs forget-passwords",
            "#logsforget-passwords",
            "say #logs forget-passwords",
        ] {
            assert_eq!(logs_command(line), None, "{line:?}");
        }
    }

    #[test]
    fn help_lists_logs_forget_passwords() {
        let lines: Vec<&str> = HELP_TEXT.lines().collect();
        assert!(lines
            .iter()
            .any(|l| l.trim_start().starts_with("#logs forget-passwords ")
                && l.contains("count the lines where you sent a password")));
        assert!(lines.iter().any(
            |l| l.trim_start().starts_with("#logs forget-passwords now ")
                && l.contains("blank those lines in the session log")
        ));
    }

    #[test]
    fn logs_from_a_timer_or_script_says_where_it_runs() {
        // Typed input runs #logs before the pipeline. Only a timer, the
        // tick command, or Lua reaches it here, and those never blank a log.
        let mut p = Profile::default();
        for line in ["#logs forget-passwords now", "#logs"] {
            let r = process(&mut p, line);
            assert!(r.bytes.is_empty());
            assert_eq!(r.echo, vec!["[type #logs at the input bar]".to_string()]);
        }
    }
}
