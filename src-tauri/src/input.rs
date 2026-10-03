//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection.

mod automation;
pub(crate) mod target;

use tokio::time::Instant;
use vosh_automation::alias::{ExpandError, ExpandStep};
use vosh_automation::vars::Scope;
use vosh_prompt::card::sentences::and_list;

use crate::profile::Profile;
use crate::profile_config::ProfileConfig;
use crate::script;
use crate::tintin_import;

use automation::{
    slash_alias, slash_aliases_list, slash_endrec, slash_group, slash_groups_list, slash_record,
    slash_trigger, slash_triggers_list, slash_unalias, slash_untrigger,
};
use target::{
    run_target_clear, run_target_cycle, run_target_set, slash_qkey, slash_qkeys_list, slash_target,
};

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
    pub(crate) lua: script::ApplyResult,
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
    let mut lua = script::ApplyResult::default();
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
    lua: &mut script::ApplyResult,
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
                let mut apply = script::run_alias_body(profile, &call);
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
    lua: &mut script::ApplyResult,
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
        ("draw", rest) => return slash_prompt_draw(profile, rest),
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
/// game.
fn slash_prompt_codes(profile: &mut Profile, args: &str, fight: bool) -> InputResult {
    use vosh_prompt::aabahran::{self, lex, Origin, Which};
    use vosh_prompt::card::sentences;
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
    let mut echo = vec![sentences::reads_sentence(&compiled.reads(which), fight)];
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
    }
}

/// `#prompt draw on|off`: draw your design in place of your prompt, or
/// show the game's own prompt. Turning drawing on with no design draws
/// Vosh's default, as Settings does. The design, the place and the
/// capture stay. With no capture the echo says how to start, since Vosh
/// draws only a prompt it reads.
fn slash_prompt_draw(profile: &mut Profile, args: &str) -> InputResult {
    let draw = match args.trim().to_ascii_lowercase().as_str() {
        "on" => true,
        "off" => false,
        _ => return error_echo("usage #prompt draw on | off".to_string()),
    };
    let mut config = profile.prompt.config().clone();
    if config.draw != draw {
        config.draw = draw;
        if draw && config.template.is_empty() {
            config.template = vosh_prompt::DEFAULT_DESIGN.to_string();
        }
        profile.set_prompt_config(config);
    }
    let mut echo = vec![if draw {
        "Drawing is on. Vosh draws your design in place of your prompt."
    } else {
        "Drawing is off. You see the game's own prompt again."
    }
    .to_string()];
    if draw && profile.prompt.config().capture.is_none() {
        echo.push(PROMPT_NONE.to_string());
    }
    InputResult {
        bytes: Vec::new(),
        echo,
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

/// What `#profile save`, `load`, and `reset` answer between the shared
/// catalog wizard and the relaunch that finishes it. Nothing saves in
/// that window, the profile files hold no aliases, triggers, or macros
/// any more, and the shared catalog loads only at launch.
const PROFILE_MIGRATION_PENDING: &str =
    "Quit Vosh and open it again to finish the move to loadouts.";

/// What `#profile save` answers while another profile write holds
/// [`crate::disk::save::PERSIST_LOCK`].
const PROFILE_SAVE_BUSY: &str = "Vosh is saving this profile. Try again.";

fn slash_profile(profile: &mut Profile, args: &str, replaced: &mut bool) -> InputResult {
    let pending =
        crate::app::state::MIGRATION_RELAUNCH_PENDING.load(std::sync::atomic::Ordering::Acquire);
    let app_data = APP_DATA_DIR.get().map(std::path::PathBuf::as_path);
    slash_profile_with(profile, args, replaced, pending, app_data)
}

/// [`slash_profile`] with `migration_pending` in place of
/// [`crate::app::state::MIGRATION_RELAUNCH_PENDING`] and `app_data` in
/// place of [`APP_DATA_DIR`], so a test can run it after the wizard, or
/// over a folder of its own, without touching what every other test
/// reads.
fn slash_profile_with(
    profile: &mut Profile,
    args: &str,
    replaced: &mut bool,
    migration_pending: bool,
    app_data: Option<&std::path::Path>,
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
        "save" => match app_data.and_then(profile_path) {
            Some(path) => {
                // Every profile file write holds the persist lock. This
                // runs under the profile lock, which the persist takes
                // after the persist lock, so it only tries.
                let Ok(_persist_guard) = crate::disk::save::PERSIST_LOCK.try_lock() else {
                    return error_echo(PROFILE_SAVE_BUSY.to_string());
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
        "load" => match app_data.and_then(profile_path) {
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

/// The active profile's file under the app data folder `app_data`,
/// `<app_data>/profiles/<active>.toml`, whether or not it exists yet.
/// Reads the profile index (`profiles.toml`) to learn which profile is
/// active, and returns `None` when the index does not read or names no
/// active profile.
///
/// It never falls back to the legacy `<app_data>/profile.toml`. Launch
/// writes the index on every install, so that file would only ever be
/// a stray, and a later launch without an index would move it over the
/// default profile.
fn profile_path(app_data: &std::path::Path) -> Option<std::path::PathBuf> {
    let body = std::fs::read_to_string(app_data.join("profiles.toml")).ok()?;
    let value = body.parse::<toml::Value>().ok()?;
    let active = value.get("active")?.as_str()?;
    Some(app_data.join("profiles").join(format!("{active}.toml")))
}

fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

fn slash_script(profile: &mut Profile, args: &str, lua: &mut script::ApplyResult) -> InputResult {
    let (cmd, rest) = split_first_word(args);
    match cmd {
        "load" => slash_script_load(profile, rest, lua),
        "reload" => slash_script_reload(profile, lua),
        "" => error_echo("usage #script load <name> | #script reload".to_string()),
        other => error_echo(format!("unknown #script subcommand `{other}`")),
    }
}

fn slash_script_load(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
) -> InputResult {
    let app_data = APP_DATA_DIR.get().map(std::path::PathBuf::as_path);
    slash_script_load_in(profile, args, lua, app_data)
}

/// [`slash_script_load`] over the app data folder `app_data` in place of
/// [`APP_DATA_DIR`], so a test can load from a folder of its own.
fn slash_script_load_in(
    profile: &mut Profile,
    args: &str,
    lua: &mut script::ApplyResult,
    app_data: Option<&std::path::Path>,
) -> InputResult {
    let name = args.trim();
    if name.is_empty() {
        return error_echo("usage #script load <name>".to_string());
    }
    let Some(app_data) = app_data else {
        return error_echo("could not resolve scripts directory".to_string());
    };
    let path = script_path_for(app_data, name);
    let code = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return error_echo(format!("read failed: {e} ({})", path.display())),
    };
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.load_script(name, code) {
        Ok(o) => o,
        Err(e) => return error_echo(format!("script error: {e}")),
    };
    lua.append(script::apply_actions(profile, outcome));
    echo_one(format!("loaded {}", path.display()))
}

fn slash_script_reload(profile: &mut Profile, lua: &mut script::ApplyResult) -> InputResult {
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.reload_scripts() {
        Ok(o) => o,
        Err(e) => return error_echo(format!("reload error: {e}")),
    };
    lua.append(script::apply_actions(profile, outcome));
    echo_one("scripts reloaded".to_string())
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
        // Every Lua trigger runs at priority 0. The column lines up with
        // the #triggers listing.
        for t in triggers {
            lines.push(format!("    [  0] {} /{}/", t.name, t.pattern));
        }
    }
    InputResult {
        bytes: Vec::new(),
        echo: lines,
    }
}

fn slash_lua(profile: &mut Profile, args: &str, lua: &mut script::ApplyResult) -> InputResult {
    let code = args.trim_start();
    if code.is_empty() {
        return error_echo("usage #lua <code>".to_string());
    }
    script::snapshot_vars(&profile.script, &profile.vars);
    let outcome = match profile.script.eval(code, "#lua") {
        Ok(o) => o,
        Err(e) => return error_echo(format!("lua error: {e}")),
    };
    lua.append(script::apply_actions(profile, outcome));
    InputResult {
        bytes: Vec::new(),
        echo: Vec::new(),
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
    }
}

/// The file `#script load <name>` reads, `<app_data>/scripts/<name>.lua`
/// under the app data folder `app_data`.
fn script_path_for(app_data: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = app_data.join("scripts");
    if std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        dir.join(name)
    } else {
        dir.join(format!("{name}.lua"))
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
