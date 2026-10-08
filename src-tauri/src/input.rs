//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection. [`run_typed_line`] runs a line you
//! type from end to end and hands its bytes to the session, and a `#walk`
//! in it to the walker there.

mod automation;
pub(crate) mod profile;
mod prompt;
mod script;
mod slash;
pub(crate) mod target;
mod tick;
mod vars;
pub(crate) mod walk;

use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use vosh_automation::alias::{ExpandError, ExpandStep};
use vosh_prompt::PromptConfig;
use walk::WalkCommand;

use crate::app::events::{self, HELP_OPEN};
use crate::app::state::{AppState, SharedState};
use crate::disk::save::settle_line_effects;
use crate::output;
use crate::profile::live::Profile;
use crate::profile::switch::read_shared_layer;
use crate::prompt::request_prompt_repaint;
use crate::script::{run_alias_body, ApplyResult};
use crate::session::connection::{Connection, QuickKey};
use crate::session::effects::{collect_script_result, run_lines_locked, Collected, LinesRun};
use crate::sessions::Session;
use crate::tick::TickConfig;

use slash::handle_slash;
use target::{run_target_clear, run_target_cycle, run_target_set};

/// What the input pipeline produced.
pub(crate) struct InputResult {
    /// Commands to send to the server, already terminated with CRLF.
    pub(crate) bytes: Vec<u8>,
    /// Local lines to echo back to the terminal pane (without CRLF added).
    /// The session layer wraps each line in CRLF before emitting.
    pub(crate) echo: Vec<String>,
    /// A `#walk` the line ran, for the walker in the session, after
    /// `bytes`. What followed it in the line rides inside it.
    pub(crate) walk: Option<WalkCommand>,
}

impl InputResult {
    /// Echo `lines` and send nothing, what most slash commands return.
    fn echo_lines(lines: Vec<String>) -> Self {
        Self {
            bytes: Vec::new(),
            echo: lines,
            walk: None,
        }
    }

    /// Echo one line and send nothing.
    fn echo_line(line: impl Into<String>) -> Self {
        Self::echo_lines(vec![line.into()])
    }

    /// Echo `message` in brackets, the way a command says it failed, and
    /// send nothing.
    fn error(message: impl std::fmt::Display) -> Self {
        Self::echo_line(format!("[{message}]"))
    }

    /// Send `bytes` and echo nothing.
    fn send(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            echo: Vec::new(),
            walk: None,
        }
    }

    /// Send nothing and echo nothing.
    fn empty() -> Self {
        Self::send(Vec::new())
    }
}

/// The two `#profile` commands that lay a profile over the live one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileReplace {
    /// `#profile reset` lays the defaults over it.
    Reset,
    /// `#profile load` lays its file over it.
    Load,
}

/// Which of `#profile reset` and `#profile load` `line` is, tokenized
/// exactly like the slash dispatcher, so the persist-suppression
/// decision in [`run_typed_line`] cannot drift from what actually
/// executes ("#profile  reset" and "# profile load" count too).
pub(crate) fn profile_replace(line: &str) -> Option<ProfileReplace> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (cmd, rest) = split_first_word(rest);
    if cmd != "profile" {
        return None;
    }
    match split_first_word(rest).0 {
        "reset" => Some(ProfileReplace::Reset),
        "load" => Some(ProfileReplace::Load),
        _ => None,
    }
}

/// Who asked for a line the input pipeline runs, which decides the
/// slash commands it may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineFrom {
    /// You: a line you type, a Settings timer or the tick command.
    You,
    /// Your own Lua through `mud.input`: a `#lua` line, the Lua of a
    /// trigger or an alias, or a script from `#script load`.
    YourLua,
    /// A plugin through `mud.input`.
    Plugin,
}

impl LineFrom {
    /// Who asked for a `mud.input` line that the Lua of `owner` queued.
    pub(crate) fn lua(owner: &vosh_script::Owner) -> Self {
        match owner {
            vosh_script::Owner::Plugin(_) => LineFrom::Plugin,
            _ => LineFrom::YourLua,
        }
    }
}

/// The command in `line` that Lua may not run through `mud.input`, read
/// the way the slash dispatcher reads it: `#script load`, which runs a
/// file, `#script reload`, which runs every loaded file again,
/// `#import-tintin`, which reads a file, and `#profile`, which saves,
/// loads or blanks your profile. None for any other line.
pub(crate) fn kept_from_lua(line: &str) -> Option<&'static str> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (cmd, rest) = split_first_word(rest);
    match cmd {
        "profile" => Some("#profile"),
        "import-tintin" => Some("#import-tintin"),
        "script" => match split_first_word(rest).0 {
            "load" => Some("#script load"),
            "reload" => Some("#script reload"),
            _ => None,
        },
        _ => None,
    }
}

/// Why Vosh does not run the slash command `line` that `from` asks for,
/// read the way the slash dispatcher reads it, or None when it runs. A
/// plugin runs no slash command but `#echo`. Your own Lua runs any but
/// those [`kept_from_lua`] names, and it cannot set a quick key or the
/// tick command to a slash command, which would later run as yours.
fn slash_refusal(from: LineFrom, line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix('#')?;
    let (cmd, args) = split_first_word(rest);
    let runs_slash = |text: &str| text.trim_start().starts_with('#');
    match from {
        LineFrom::You => None,
        LineFrom::Plugin => (!matches!(cmd, "echo" | "showme"))
            .then(|| format!("Vosh never runs #{cmd} for a plugin.")),
        LineFrom::YourLua => {
            if let Some(command) = kept_from_lua(line) {
                return Some(format!("Vosh runs {command} only when you type it."));
            }
            let (word, text) = split_first_word(args);
            match cmd {
                "qkey" if word != "clear" && runs_slash(text) => {
                    Some("Vosh sets a quick key to a # command only when you type it.".to_string())
                }
                "tick" if word == "fire" && runs_slash(text) => Some(
                    "Vosh sets the tick command to a # command only when you type it.".to_string(),
                ),
                _ => None,
            }
        }
    }
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
/// profile comes back from [`run_line_from`].
pub(crate) fn may_replace_profile(state: &AppState, line: &str) -> bool {
    !state
        .loadout_mode
        .load(std::sync::atomic::Ordering::Acquire)
        && profile_replace(line).is_some()
}

/// What the terminal prints when you send a line with no connection.
pub(crate) const NOT_CONNECTED: &[u8] = b"\r\n[not connected]\r\n";

/// Run a line you typed in `session`, the body of
/// `session_send_input`. `#help` and `#logs` take their short cuts. Any
/// other line runs through the pipeline under the profile lock and the
/// connection's. Then the prompt repaints when the line changed how it
/// looks, the line's saves and events go out with your target when it
/// changed, and what it sends and echoes is delivered.
pub(crate) async fn run_typed_line<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Arc<Session>,
    line: &str,
) -> Result<(), String> {
    // What the plugins printed at launch, if nothing showed it yet.
    crate::app::plugins::show_launch_lines(app, session);
    // `#help <words>` opens Help on those words. The topics live in the
    // page, so the main window searches them, opens Help on the best
    // match, or says in the terminal that none matched.
    if let Some(words) = help_query(line) {
        return app
            .emit_to("main", HELP_OPEN, words)
            .map_err(|e| e.to_string());
    }
    // `#logs` works on the log store, not the profile, and can take a
    // while on a large log, so it runs on its own task and echoes when
    // done.
    if let Some(command) = logs_command(line) {
        crate::logs::forget_passwords::start(app, session, command);
        return Ok(());
    }
    // `#profile reset` and `#profile load` replace the live profile
    // wholesale, panes and tracked affects included. Loadout mode turns them
    // into echoes, so there they change nothing. The profile file they
    // read holds none of the shared settings, so global.toml goes back
    // over the result the way a switch lays it.
    let shared_layer = if may_replace_profile(state, line) {
        read_shared_layer(state).await
    } else {
        None
    };
    // The line runs the way a line from a timer, the tick or Lua runs.
    let (
        open,
        quick,
        LinesRun {
            apply,
            shown,
            effects,
            replaced_by,
        },
    ) = {
        let mut profile = session.lock_profile().await;
        let mut connection = session.connection.lock();
        // A quick key echoes its command from here, since the page leaves
        // its echo out, so that echo lands after the prompt.
        let quick = fires_quick_key(&connection, line);
        let run = run_lines_locked(
            state,
            &mut profile,
            &mut connection,
            [(LineFrom::You, line)],
            shared_layer.as_ref(),
        );
        (profile.open().clone(), quick, run)
    };
    // `#prompt draw` and `#prompt show` change the prompt on screen at
    // once, and `#prompt default` draws the new design there. A typed
    // line runs outside the session loop, so it waits for the repaint.
    if shown.repaint {
        request_prompt_repaint(session).await;
    }

    settle_line_effects(app, session, &open, effects, replaced_by).await;

    if let Some(payload) = shown.target {
        session.emit(app, events::TARGET, &payload);
    }

    deliver_script_result(app, session, apply.ran_under(&open), quick).await
}

/// Apply a script result outside the session loop, the way every path
/// applies one, then print its echo lines on the terminal, send its bytes
/// to the game `session` runs and hand a `#walk` to its walker after them.
/// With no connection the terminal says so. `quick` says the lines start
/// with a quick key's echo, which lands after the prompt. Any other line
/// starts a row of its own.
async fn deliver_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    apply: ApplyResult,
    quick: bool,
) -> Result<(), String> {
    let Collected {
        bytes,
        echoes,
        walk,
    } = collect_script_result(app, session, apply).await;
    if quick {
        output::echo_command(app, session, &echoes);
    } else {
        output::echo_lines(app, session, &echoes);
    }

    if bytes.is_empty() && walk.is_none() {
        return Ok(());
    }

    let mut current = session.slot.lock().await;
    if let Some(handle) = current.as_ref() {
        let sent = bytes.is_empty() || handle.send(bytes);
        if sent && walk.map_or(true, |walk| handle.walk(walk)) {
            return Ok(());
        }
        // The game closed the connection and the session ended, but its
        // handle stayed here. A send fails only once the session loop has
        // returned, so its teardown is done and nothing needs to wait on
        // it. Take the handle out, so this line and every one after it
        // finds no connection, as after a disconnect.
        *current = None;
        output::emit_output(app, session, NOT_CONNECTED.to_vec());
        return Ok(());
    }
    // With no connection no walk is under way, so `#walk` and `#walk stop`
    // say so, and anything that would reach the game says it cannot.
    let reaches_game = match &walk {
        Some(WalkCommand::Start { .. }) => true,
        Some(WalkCommand::Stop { rest, .. } | WalkCommand::Status { rest }) => !rest.is_empty(),
        None => false,
    };
    if matches!(
        walk,
        Some(WalkCommand::Stop { key: false, .. } | WalkCommand::Status { .. })
    ) {
        output::echo_lines(
            app,
            session,
            &[crate::session::walk::NOT_WALKING.to_string()],
        );
    }
    if !bytes.is_empty() || reaches_game {
        output::emit_output(app, session, NOT_CONNECTED.to_vec());
    }
    Ok(())
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
    /// The caller hands it to `session::effects::apply_script_result`
    /// after the line's own output.
    pub(crate) lua: ApplyResult,
    /// A `#profile reset`, or a `#profile load` that read its file,
    /// replaced the live profile.
    pub(crate) replaced: bool,
    /// The tick settings before the line, when it changed them, like
    /// `#tick warn at 10`.
    pub(crate) tick_before: Option<TickConfig>,
    /// The `[prompt]` table the line left in the engine, when it changed
    /// it without laying another profile over, like `#prompt draw off`.
    pub(crate) prompt: Option<PromptConfig>,
}

/// [`run_line_from`] for a line you type, for a test.
#[cfg(test)]
pub(crate) fn run_line(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    line: &str,
) -> Ran {
    run_line_from(state, profile, c, line, LineFrom::You)
}

/// Run `line`, which `from` asks for, through the input pipeline: what
/// to send, what to echo, what the Lua it ran asks for, whether it
/// replaced the live profile, and whether it changed the tick settings
/// or the `[prompt]` table, for [`LineEffects::note_ran`]. The target
/// words and the quick keys read and set your target on `c`. Who asked
/// decides the slash commands it may run, those a quick key in it
/// expands to included.
pub(crate) fn run_line_from(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    line: &str,
    from: LineFrom,
) -> Ran {
    let mut replaced = false;
    let mut lua = ApplyResult::default();
    let tick_before = profile.tick.config.clone();
    let prompt_before = c.prompt.revision();
    let result = process_line(state, profile, c, line, from, &mut replaced, &mut lua);
    let tick_before = (profile.tick.config != tick_before).then_some(tick_before);
    let prompt =
        (!replaced && c.prompt.revision() != prompt_before).then(|| c.prompt.config().clone());
    Ran {
        result,
        lua,
        replaced,
        tick_before,
        prompt,
    }
}

/// What a run of input lines asks of the saved profile. Every path that
/// runs a line through [`run_line_from`] notes each line here in order: typed
/// input, a Settings timer command, the tick auto-fire command, and a
/// Lua `mud.input` line. So `#alias` or `#trigger` from a timer reaches
/// disk the way the same line typed at the prompt does. Lua that changed
/// durable state marks the profile dirty where its result is applied,
/// after these.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct LineEffects {
    /// A `#profile reset` or `#profile load` replaced the live profile,
    /// which leaves it diverged from disk on purpose.
    pub(crate) replaced: bool,
    /// A slash command came after the last replace, or with none.
    pub(crate) dirty: bool,
    /// The tick settings before the first line that changed them, like a
    /// `#tick` command. The status line and the Settings Tick card show
    /// them, so every window hears the new settings once the lines have
    /// run, and every other session on the profile follows them.
    pub(crate) tick_before: Option<TickConfig>,
    /// The `[prompt]` table the last line that changed it left in its
    /// engine, like `#prompt draw off`. The engine of every other session
    /// on the profile takes what you chose in it.
    pub(crate) prompt: Option<PromptConfig>,
}

impl LineEffects {
    /// Note one line [`run_line_from`] ran: [`Self::note`] with whether it
    /// replaced the live profile, whether it changed the tick settings or
    /// the `[prompt]` table, and whether the Lua bodies of its script
    /// aliases changed durable state.
    pub(crate) fn note_ran(&mut self, line: &str, ran: &Ran) {
        self.note(line, ran.replaced);
        if self.tick_before.is_none() {
            self.tick_before.clone_from(&ran.tick_before);
        }
        if ran.prompt.is_some() {
            self.prompt.clone_from(&ran.prompt);
        }
        if ran.lua.durable_changed {
            self.dirty = true;
        }
    }

    /// Note one line that ran through [`run_line_from`], with whether it
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
        if profile_replace(line).is_some() {
            return;
        }
        // A `#walk` changes nothing a save keeps, and what it holds runs
        // as an alias's pieces do, where a `#` command goes out as text.
        let walk = line
            .trim_start()
            .strip_prefix('#')
            .and_then(walk::slash_walk_args)
            .is_some();
        if line.trim_start().starts_with('#') && !walk {
            self.dirty = true;
        }
    }
}

/// Run the input pipeline against the given profile and return what to send
/// and what to echo locally. The app runs every line through [`run_line_from`],
/// which also says whether the line replaced the profile. A test that
/// needs no app state of its own runs here, on a fresh one, with a fresh
/// connection, so what the line leaves on it, such as a target, a Lua
/// global or a recording, drops with it.
#[cfg(test)]
pub(crate) fn process(profile: &mut Profile, line: &str) -> InputResult {
    run_line(
        &AppState::default(),
        profile,
        &mut Connection::default(),
        line,
    )
    .result
}

/// The body of [`run_line_from`]. Sets `replaced` when a `#profile reset`
/// or a `#profile load` that read its file replaced the live profile, and
/// adds to `lua` what the Lua the line ran asks for. A script alias
/// body's sends and echo lines go in the result instead, where you typed
/// it.
fn process_line(
    state: &AppState,
    profile: &mut Profile,
    c: &mut Connection,
    line: &str,
    from: LineFrom,
    replaced: &mut bool,
    lua: &mut ApplyResult,
) -> InputResult {
    let trimmed = line.trim_start();

    // Slash commands target the local profile. Lua runs only those its
    // owner may, here where a quick key's expansion arrives too.
    if let Some(rest) = trimmed.strip_prefix('#') {
        if let Some(refusal) = slash_refusal(from, trimmed) {
            return InputResult::echo_lines(crate::script::lua_error_lines(&refusal).collect());
        }
        return handle_slash(state, profile, c, rest, replaced, lua);
    }

    // A bare Enter sends a blank line to the server. MUDs use this to
    // advance prompts and paginated output.
    if trimmed.is_empty() {
        return InputResult::send(b"\r\n".to_vec());
    }

    // Target keywords work bare (no `#` prefix) so they feel like
    // commands rather than slash builtins. `tar`/`tarn`/`tarp`/
    // `tarclear` are reserved at registration time so they can't be
    // shadowed by aliases or quick-keys.
    let (head, rest) = split_first_word(trimmed);
    match head {
        "tar" => return run_target_set(c, rest),
        "tarn" => return run_target_cycle(c, 1),
        "tarp" => return run_target_cycle(c, -1),
        "tarclear" => return run_target_clear(c),
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
    if let Some(qk) = quick_key(c, head) {
        let target = c.target.name.clone().unwrap_or_default();
        if target.is_empty() {
            return InputResult::error("no target — set one with `tar <name|index>` first");
        }
        let expansion = format!("{} {}", qk.verb, target);
        let mut inner = process_line(state, profile, c, &expansion, from, replaced, lua);
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
    if let Some(recorder) = c.recording_macro.as_mut() {
        recorder.commands.push(trimmed.to_string());
    }

    // Plain input. Interpolate variables, then expand aliases, then encode.
    // An alias that runs Lua runs its body where it stands, so what the
    // body sends goes out in the order you typed the line.
    let interpolated = c.var_view(profile).interpolate(trimmed);
    let steps = match profile
        .aliases
        .expand_line_full(&interpolated, &c.plugin_aliases, c.stop_key)
    {
        Ok(steps) => steps,
        Err(ExpandError::RecursionLimit(depth)) => {
            return InputResult::error(format!("alias recursion limit hit ({depth})"));
        }
    };
    run_expanded(profile, c, steps, lua)
}

/// Run the steps a line expanded to, in order: each command goes out as
/// text and each script alias body runs where it stands, adding to `lua`
/// what it asks for besides its sends and echo lines. A command that is a
/// `#walk` runs it, and the steps after it ride in the walk, so they wait
/// for it to end and run where it takes you. Every other `#` command
/// goes out as text, as it always did. The walker runs what a walk held
/// this way once you arrive.
pub(crate) fn run_expanded(
    profile: &mut Profile,
    c: &mut Connection,
    steps: Vec<ExpandStep>,
    lua: &mut ApplyResult,
) -> InputResult {
    let mut bytes = Vec::new();
    let mut echo = Vec::new();
    let mut steps = steps.into_iter();
    while let Some(step) = steps.next() {
        match step {
            ExpandStep::Command(cmd) => {
                if let Some(args) = walk::walk_args(&cmd) {
                    let mut walked = walk::walk_result(args, steps.collect());
                    echo.append(&mut walked.echo);
                    return InputResult {
                        bytes,
                        echo,
                        walk: walked.walk,
                    };
                }
                bytes.extend_from_slice(cmd.as_bytes());
                bytes.extend_from_slice(b"\r\n");
            }
            ExpandStep::Script(call) => {
                let mut apply = run_alias_body(profile, c, &call);
                bytes.append(&mut apply.send_bytes);
                echo.append(&mut apply.echoes);
                lua.append(apply);
            }
        }
    }
    InputResult {
        bytes,
        echo,
        walk: None,
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

/// The quick key a line that starts with `head` fires: one of that
/// name with a verb set.
fn quick_key<'a>(c: &'a Connection, head: &str) -> Option<&'a QuickKey> {
    c.target
        .quick_keys
        .iter()
        .find(|q| q.name == head && !q.verb.is_empty())
}

/// Whether `line`, typed, fires a quick key, whose echo Vosh draws in
/// place of the page's, as [`process_line`] reads it: its first word
/// names a quick key and it is neither a `#` command nor a target word.
fn fires_quick_key(c: &Connection, line: &str) -> bool {
    let trimmed = line.trim_start();
    let (head, _) = split_first_word(trimmed);
    !trimmed.starts_with('#')
        && !matches!(head, "tar" | "tarn" | "tarp" | "tarclear")
        && quick_key(c, head).is_some()
}

/// The echo of a command you send, as the command line draws it: the
/// mark from [`echo_mark`], then the command in the Command color when
/// one is set, faint when Dim sent commands is on. The mark keeps its own
/// color and never dims. Mirrors `commandEcho` and `echoMark` in
/// src/input/maskedInput.ts, so a quick key echoes like a typed command.
/// The bytes for each case sit in fixtures/input/echo-marks.json. An
/// empty line echoes as itself.
pub(crate) fn command_echo(line: &str, ui: &crate::profile::ui::UiConfig) -> String {
    if line.is_empty() {
        return String::new();
    }
    let mark = echo_mark(ui);
    let color = ui.input_echo_color.as_deref().and_then(echo_rgb);
    match (ui.input_echo_dim, color) {
        (true, Some((r, g, b))) => format!("{mark}\x1b[2;38;2;{r};{g};{b}m{line}\x1b[0m"),
        (true, None) => format!("{mark}\x1b[2m{line}\x1b[0m"),
        (false, Some((r, g, b))) => format!("{mark}\x1b[38;2;{r};{g};{b}m{line}\x1b[0m"),
        (false, None) => format!("{mark}{line}"),
    }
}

/// The mark before each command you send, empty while it is off or your
/// own text is blank: the Mark color, or the theme's bright black (SGR
/// 90) when none is set, then `›`, `>` or your own text, then a space and
/// a reset. Each renderer leaves it out when the row your echo lands on
/// already ends in `>`, as a game's prompt such as `Account name> ` does
/// (`TermGrid::local_write` and `TermGrid::session_output` in the native
/// grid, which strips these bytes, and the page's `RegionWriter`).
pub(crate) fn echo_mark(ui: &crate::profile::ui::UiConfig) -> String {
    let text = match ui.input_echo_mark.as_str() {
        "off" => return String::new(),
        "gt" => ">",
        "own" => ui.input_echo_mark_text.as_str(),
        _ => "\u{203a}",
    };
    if text.is_empty() {
        return String::new();
    }
    match ui.input_echo_mark_color.as_deref().and_then(echo_rgb) {
        Some((r, g, b)) => format!("\x1b[38;2;{r};{g};{b}m{text} \x1b[0m"),
        None => format!("\x1b[90m{text} \x1b[0m"),
    }
}

/// Tell the native grid of `session` the `mark` [`echo_mark`] gave, so
/// the grid leaves out exactly that mark after a prompt that ends in `>`.
pub(crate) fn keep_echo_mark(session: crate::sessions::SessionId, mark: String) {
    #[cfg(any(native_surface, test))]
    crate::native::grid::set_echo_mark(session, mark.into_bytes());
    #[cfg(not(any(native_surface, test)))]
    let _ = (session, mark);
}

/// The red, green and blue of a Command or Mark color, the six hex digits
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

#[cfg(test)]
mod tests;
