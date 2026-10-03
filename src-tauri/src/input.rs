//! Input pipeline. Takes a typed command line, applies variable
//! interpolation and alias expansion, and returns the bytes to send to the
//! server. Recognizes a small set of slash commands that target the local
//! profile rather than the connection. [`run_typed_line`] runs a line you
//! type from end to end and hands its bytes to the session.

mod automation;
mod profile;
mod prompt;
mod script;
mod slash;
pub(crate) mod target;
mod tick;
mod vars;

use tauri::{AppHandle, Emitter};
use vosh_automation::alias::{ExpandError, ExpandStep};

use crate::app::events::{self, ListChanges, ListRevisions, HELP_OPEN};
use crate::app::state::{note_ui_config_replaced, SharedState};
use crate::disk::save::settle_line_effects;
use crate::output;
use crate::profile::switch::read_shared_layer;
use crate::profile::Profile;
use crate::prompt::{prompt_look, request_prompt_repaint};
use crate::script::{run_alias_body, ApplyResult};
use crate::session::{self, TargetPayload};

use slash::handle_slash;
use target::{run_target_clear, run_target_cycle, run_target_set};

/// What the input pipeline produced.
pub(crate) struct InputResult {
    /// Commands to send to the server, already terminated with CRLF.
    pub(crate) bytes: Vec<u8>,
    /// Local lines to echo back to the terminal pane (without CRLF added).
    /// The session layer wraps each line in CRLF before emitting.
    pub(crate) echo: Vec<String>,
}

impl InputResult {
    /// Echo `lines` and send nothing, what most slash commands return.
    fn echo_lines(lines: Vec<String>) -> Self {
        Self {
            bytes: Vec::new(),
            echo: lines,
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
        }
    }

    /// Send nothing and echo nothing.
    fn empty() -> Self {
        Self::send(Vec::new())
    }
}

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
/// decision in [`run_typed_line`] cannot drift from what actually
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

/// What the terminal prints when you send a line with no connection.
pub(crate) const NOT_CONNECTED: &[u8] = b"\r\n[not connected]\r\n";

/// Run a line you typed, the body of `session_send_input`. `#help` and
/// `#logs` take their short cuts. Any other line runs through the
/// pipeline under the profile lock. Then the prompt repaints when the
/// line changed how it looks, the line's saves and events go out with
/// your target when it changed, and what it sends and echoes is
/// delivered.
pub(crate) async fn run_typed_line<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    line: &str,
) -> Result<(), String> {
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
        crate::logs::forget_passwords::start(app, command);
        return Ok(());
    }
    // `#profile reset` and `#profile load` replace the live profile
    // wholesale, panes and tracked affects included. Path B turns them
    // into echoes, so there they change nothing.
    let mut effects = LineEffects::default();
    // The profile file they read holds none of the shared settings, so
    // global.toml goes back over the result the way a switch lays it.
    let shared_layer = if may_replace_profile(line) {
        read_shared_layer(state).await
    } else {
        None
    };
    let (apply, target_after, look_changed) = {
        let mut profile = state.profile.lock().await;
        let lists_before = ListRevisions::of(&profile);
        let look_before = prompt_look(&profile);
        let before_name = profile.target.name.clone();
        let before_idx = profile.target.room_idx;
        let before_keys = profile.target.quick_keys.clone();
        let ran = match &shared_layer {
            Some(layer) => layer.keep_across(&mut profile, |p| run_line(p, line)),
            None => run_line(&mut profile, line),
        };
        // Only a reset, or a load that read its file, replaced the
        // profile. A load that failed leaves it for the saves to write.
        effects.note_ran(line, &ran);
        if ran.replaced {
            note_ui_config_replaced();
        }
        let after_name = profile.target.name.clone();
        let after_idx = profile.target.room_idx;
        let after_keys = profile.target.quick_keys.clone();
        let changed =
            before_name != after_name || before_idx != after_idx || before_keys != after_keys;
        let payload = if changed {
            Some(TargetPayload {
                name: after_name,
                room_idx: after_idx,
                quick_keys: after_keys,
            })
        } else {
            None
        };
        // The line's own bytes and echo lines, with what the Lua bodies
        // of its script aliases send among them, then all else the Lua it
        // ran asks for. #trigger, #alias, and the Lua they run change the
        // lists an open Settings page shows, so the result carries every
        // list the line changed.
        let mut apply = session::line_script_result(ran);
        apply.lists = ListChanges::since(lists_before, &profile);
        let look_changed = prompt_look(&profile) != look_before;
        (apply, payload, look_changed)
    };
    // `#prompt draw` and `#prompt show` change the prompt on screen at
    // once, and `#prompt default` draws the new design there.
    if look_changed {
        request_prompt_repaint(state).await;
    }

    settle_line_effects(app, effects).await;

    if let Some(payload) = target_after {
        let _ = app.emit(events::TARGET, payload);
    }

    deliver_script_result(app, state, apply).await
}

/// Apply a script result outside the session loop, the way every path
/// applies one, then print its echo lines on the terminal and send its
/// bytes to the game. With no connection the terminal says so.
async fn deliver_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    apply: ApplyResult,
) -> Result<(), String> {
    let (bytes, echoes) =
        session::collect_script_result(app, &state.profile, &state.script_timers, apply).await;
    output::echo_lines(app, &echoes);

    if bytes.is_empty() {
        return Ok(());
    }

    let mut current = state.session.lock().await;
    if let Some(handle) = current.as_ref() {
        if handle.send(bytes) {
            return Ok(());
        }
        // The game closed the connection and the session ended, but its
        // handle stayed here. A send fails only once the session loop has
        // returned, so its teardown is done and nothing needs to wait on
        // it. Take the handle out, so this line and every one after it
        // finds no connection, as after a disconnect.
        *current = None;
    }
    output::emit_output(app, NOT_CONNECTED.to_vec());
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
        return InputResult::send(b"\r\n".to_vec());
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
            return InputResult::error("no target — set one with `tar <name|index>` first");
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
            return InputResult::error(format!("alias recursion limit hit ({depth})"));
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

#[cfg(test)]
mod tests;
