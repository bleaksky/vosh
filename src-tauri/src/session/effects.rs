//! The one Lua effects applier. Every path that runs Lua hands what it
//! asks for to [`apply_script_result`], which sends its bytes, echoes its
//! lines, hands a `#walk` to the walker, keeps its timers, shows its
//! prompt values and runs its `mud.input` lines through the input
//! pipeline. Every line runs through the pipeline here, typed or from a
//! Settings timer, the tick or `mud.input`, and says what it changed
//! outside the terminal text.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::warn;

use crate::app::events::{self, broadcast_list_changes, ListChanges, ListRevisions};
use crate::app::state::{AppState, SharedState};
use crate::input::walk::WalkCommand;
use crate::input::{self, LineFrom};
use crate::output::emit_output;
use crate::profile::live::Profile;
use crate::profile::shared::SharedLayer;
use crate::script::{ApplyResult, SharedTimers};
use crate::tick::TickStep;

use super::batch::ReadBatch;
use super::connection::Stream;
use super::prompt_view::emit_prompt_vars;
use super::walk::{self, Walker};
use super::TargetPayload;

/// Where a step writes to the terminal: the batch of the read it runs
/// in, or straight out for work outside a read, such as a timer.
pub(super) enum OutputSink<'a> {
    Batch(&'a mut ReadBatch),
    Direct,
}

impl OutputSink<'_> {
    /// Write `bytes` to the terminal.
    fn write<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, bytes: Vec<u8>) {
        match self {
            OutputSink::Batch(batch) => batch.out.text(&bytes),
            OutputSink::Direct => emit_output(app, bytes),
        }
    }

    /// A prompt var changed. A read sends the prompt vars once after its
    /// output, and anything else sends them now.
    async fn prompt_vars<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        profile: &Arc<Mutex<Profile>>,
    ) {
        match self {
            OutputSink::Batch(batch) => batch.prompt_vars = true,
            OutputSink::Direct => emit_prompt_vars(app, profile, true).await,
        }
    }
}

/// Where the bytes, echo lines and `#walk` a script result asks for go.
pub(super) enum ScriptIo<'a, 'b> {
    /// The session loop: its connection, the read's batch or the
    /// terminal, and its walker.
    Session(&'a mut Stream, &'a mut OutputSink<'b>, &'a mut Walker),
    /// Anywhere else, such as a typed line or a plugin load. The bytes,
    /// echo lines and walk collect for the caller, which sends and prints
    /// them with its own and hands the walk to the session.
    Collect {
        bytes: &'a mut Vec<u8>,
        echoes: &'a mut Vec<String>,
        walk: &'a mut Option<WalkCommand>,
    },
}

impl ScriptIo<'_, '_> {
    async fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            ScriptIo::Session(stream, ..) => {
                stream.write_all(bytes).await?;
                stream.flush().await
            }
            ScriptIo::Collect { bytes: out, .. } => {
                out.extend_from_slice(bytes);
                Ok(())
            }
        }
    }

    fn echo<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, lines: Vec<String>) {
        match self {
            ScriptIo::Session(_, sink, _) => sink.write(app, framed_echoes(&lines)),
            ScriptIo::Collect { echoes, .. } => echoes.extend(lines),
        }
    }

    async fn prompt_vars<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        profile: &Arc<Mutex<Profile>>,
    ) {
        match self {
            ScriptIo::Session(_, sink, _) => sink.prompt_vars(app, profile).await,
            ScriptIo::Collect { .. } => emit_prompt_vars(app, profile, true).await,
        }
    }

    /// Hand `command` to the walker: in the session it sends the step and
    /// prints the walker's lines, and returns what a `#walk stop` or a
    /// bare `#walk` let go of, run. Anywhere else the walk collects for
    /// the caller, the later of two taking over.
    async fn walk<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        profile: &Arc<Mutex<Profile>>,
        command: WalkCommand,
    ) -> std::io::Result<Option<ApplyResult>> {
        match self {
            ScriptIo::Session(stream, sink, walker) => {
                let out = walker.command(command, Instant::now());
                if !out.send.is_empty() {
                    stream.write_all(&out.send).await?;
                    stream.flush().await?;
                }
                if !out.lines.is_empty() {
                    sink.write(app, framed_echoes(&out.lines));
                }
                if out.release.is_empty() {
                    return Ok(None);
                }
                Ok(Some(walk::release(profile, out.release).await))
            }
            ScriptIo::Collect { walk, .. } => {
                **walk = Some(command);
                Ok(None)
            }
        }
    }
}

/// How many rounds of `mud.input` lines one script result runs, each
/// round the lines the Lua of the round before asked for. Lua that keeps
/// asking stops here, at the depth an alias may go.
const MUD_INPUT_DEPTH: usize = vosh_automation::alias::DEFAULT_MAX_DEPTH;

/// How many `mud.input` lines one script result runs in all its rounds.
/// Each call may queue 100 lines, so Lua whose lines each ask for as
/// many again would otherwise run 100 times more Lua each round.
const MUD_INPUT_LINES: usize = 100;

/// The `mud.input` lines one script result may still run.
#[derive(Debug)]
pub(super) struct InputBudget {
    left: usize,
    /// Vosh said it dropped lines.
    told: bool,
}

impl InputBudget {
    pub(super) fn new() -> Self {
        Self {
            left: MUD_INPUT_LINES,
            told: false,
        }
    }

    /// The lines of the next round that fit what is left, and the first
    /// time lines do not fit, the terminal lines that say Vosh dropped
    /// them.
    pub(super) fn take(
        &mut self,
        mut lines: Vec<(LineFrom, String)>,
    ) -> (Vec<(LineFrom, String)>, Vec<String>) {
        let mut said = Vec::new();
        if lines.len() > self.left {
            lines.truncate(self.left);
            if !self.told {
                self.told = true;
                warn!(
                    lines = MUD_INPUT_LINES,
                    "mud.input asked for too many lines"
                );
                said = crate::script::lua_error_lines(&format!(
                    "Vosh ran {MUD_INPUT_LINES} lines from mud.input and dropped the rest."
                ))
                .collect();
            }
        }
        self.left -= lines.len();
        (lines, said)
    }
}

/// Perform the IO and timer bookkeeping a script result asks for. Every
/// path that runs Lua applies its result here: the game's lines and
/// GMCP, Lua timers, the lines you type, a Settings timer, the tick
/// command, and a plugin load. Sends and echoes flow to `io`, timers
/// register with the shared list, values a script gave your prompt reach
/// the windows, and `mud.input` lines are run through the input pipeline
/// so they pick up aliases and slash commands too, with all their own Lua
/// asks for applied in turn.
pub(super) async fn apply_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    io: &mut ScriptIo<'_, '_>,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
    apply: ApplyResult,
) -> std::io::Result<()> {
    let mut apply = apply;
    let mut depth = 0;
    let mut budget = InputBudget::new();
    loop {
        // Durable Lua changes (mud.alias, set_var, group toggles fired
        // by triggers or timers) ride the same debounced save the slash
        // commands use, or they would never reach disk.
        if apply.durable_changed {
            crate::disk::save::mark_profile_dirty(app);
        }
        // A Lua `mud.alias` changes the list an open Settings page shows.
        broadcast_list_changes(app, apply.lists);

        if !apply.send_bytes.is_empty() {
            io.send(&apply.send_bytes).await?;
        }
        if !apply.echoes.is_empty() {
            io.echo(app, std::mem::take(&mut apply.echoes));
        }
        // A `#walk` goes after the bytes of its line. What a `#walk stop`
        // or a bare `#walk` let go of runs right after it, and its own
        // timers and `mud.input` lines join this result's.
        let mut walking = apply.walk.take();
        while let Some(command) = walking.take() {
            let Some(mut released) = io.walk(app, profile, command).await? else {
                continue;
            };
            if released.durable_changed {
                crate::disk::save::mark_profile_dirty(app);
            }
            broadcast_list_changes(app, std::mem::take(&mut released.lists));
            if !released.send_bytes.is_empty() {
                io.send(&std::mem::take(&mut released.send_bytes)).await?;
            }
            if !released.echoes.is_empty() {
                io.echo(app, std::mem::take(&mut released.echoes));
            }
            walking = released.walk.take();
            apply.append(released);
        }
        if !apply.new_timers.is_empty() || !apply.cancel_timers.is_empty() {
            // New timers go in before the cancels run, so a timer that
            // one result both starts and cancels never fires. Lua never
            // gives two timers the same id, so a cancel only ever takes
            // the timer it names.
            let mut guard = lua_timers.lock().await;
            guard.extend(apply.new_timers);
            guard.retain(|t| !apply.cancel_timers.contains(&t.timer_id));
        }
        if apply.prompt_vars_changed {
            io.prompt_vars(app, profile).await;
        }
        if apply.inputs.is_empty() {
            return Ok(());
        }
        if depth == MUD_INPUT_DEPTH {
            warn!(depth, "mud.input went too deep");
            io.echo(
                app,
                vec![format!("[mud.input recursion limit hit ({depth})]")],
            );
            return Ok(());
        }
        depth += 1;
        let (inputs, dropped) = budget.take(std::mem::take(&mut apply.inputs));
        if !dropped.is_empty() {
            io.echo(app, dropped);
        }
        if inputs.is_empty() {
            return Ok(());
        }
        let shared = crate::profile::switch::shared_layer_for_lines(
            app,
            inputs.iter().map(|(_, line)| line.as_str()),
        )
        .await;
        let state = app.state::<SharedState>();
        let LinesRun {
            apply: next,
            shown,
            effects,
        } = {
            let mut p = profile.lock().await;
            run_lines_locked(
                &state,
                &mut p,
                inputs.iter().map(|(from, line)| (*from, line.as_str())),
                shared.as_ref(),
            )
        };
        crate::disk::save::settle_line_effects(app, effects).await;
        shown.send(app);
        apply = next;
    }
}

/// What a script result outside the session loop leaves for its caller:
/// the bytes for the game, the echo lines for the terminal, and a `#walk`
/// for the walker in the session, which goes after the bytes.
#[derive(Debug, Default)]
pub(crate) struct Collected {
    pub(crate) bytes: Vec<u8>,
    pub(crate) echoes: Vec<String>,
    pub(crate) walk: Option<WalkCommand>,
}

/// [`apply_script_result`] outside the session loop, as for a typed line
/// or a plugin load. Returns what the caller sends, prints and hands the
/// session.
pub(crate) async fn collect_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
    apply: ApplyResult,
) -> Collected {
    let mut collected = Collected::default();
    let mut io = ScriptIo::Collect {
        bytes: &mut collected.bytes,
        echoes: &mut collected.echoes,
        walk: &mut collected.walk,
    };
    // Collecting writes to no stream, so it never fails.
    if let Err(e) = apply_script_result(app, &mut io, profile, lua_timers, apply).await {
        warn!(error = %e, "applying a script result failed");
    }
    collected
}

/// Echo lines outside a trigger's own line, each on its own row with a
/// line end before the first.
pub(super) fn framed_echoes<S: AsRef<str>>(lines: &[S]) -> Vec<u8> {
    let mut buf = Vec::new();
    for line in lines {
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(line.as_ref().as_bytes());
    }
    buf.extend_from_slice(b"\r\n");
    buf
}

/// Run `line` through the input pipeline and note what it asks of the
/// saved profile. Call with the profile lock held. A `#profile reset`,
/// or a `#profile load` that reads its file, swaps the live UI config
/// and panes, so their generations move in the same step. The profile
/// file it reads holds none of the shared settings, so `shared` goes
/// back over the result.
pub(super) fn run_and_note_line(
    state: &AppState,
    p: &mut Profile,
    from: LineFrom,
    line: &str,
    effects: &mut input::LineEffects,
    shared: Option<&SharedLayer>,
) -> input::Ran {
    let ran = match shared.filter(|_| input::may_replace_profile(state, line)) {
        Some(layer) => layer.keep_across(p, |p| input::run_line_from(state, p, line, from)),
        None => input::run_line_from(state, p, line, from),
    };
    effects.note_ran(line, &ran);
    if ran.replaced {
        state.note_ui_config_replaced();
    }
    ran
}

/// What one line the input pipeline ran asks for, as one script result:
/// its own bytes and echo lines, with what the Lua bodies of its script
/// aliases send among them in the order you typed them, then all else
/// the Lua it ran asks for.
pub(crate) fn line_script_result(ran: input::Ran) -> ApplyResult {
    let input::Ran { result, lua, .. } = ran;
    let mut apply = ApplyResult {
        send_bytes: result.bytes,
        echoes: result.echo,
        walk: result.walk,
        ..ApplyResult::default()
    };
    apply.append(lua);
    apply
}

/// What a line can change that shows outside the terminal text: the
/// target display and how your prompt looks.
struct Shown {
    target: TargetPayload,
    look: (bool, String, vosh_prompt::PromptShow),
}

impl Shown {
    fn of(p: &Profile) -> Self {
        Self {
            target: TargetPayload::of(p),
            look: crate::prompt::prompt_look(p),
        }
    }
}

/// What lines changed outside the terminal text.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ShownChanges {
    /// The target display, sent on `session://target`.
    pub(crate) target: Option<TargetPayload>,
    /// Your prompt looks different, so the open row repaints.
    pub(crate) repaint: bool,
}

impl ShownChanges {
    /// What `p` changed since `before` was taken.
    fn since(before: Shown, p: &Profile) -> Self {
        let after = Shown::of(p);
        Self {
            repaint: after.look != before.look,
            target: (after.target != before.target).then_some(after.target),
        }
    }

    /// Ask for the repaint and send the target, for lines the session
    /// loop runs. The repaint request goes through the session handle
    /// from a task of its own, since `session_disconnect` holds the
    /// handle's lock while it waits for this session to end.
    fn send<R: tauri::Runtime>(self, app: &AppHandle<R>) {
        if self.repaint {
            let state = app
                .state::<crate::app::state::SharedState>()
                .inner()
                .clone();
            tokio::spawn(async move { crate::prompt::request_prompt_repaint(&state).await });
        }
        if let Some(payload) = self.target {
            let _ = app.emit(events::TARGET, payload);
        }
    }
}

/// What lines run through the input pipeline produced under the profile
/// lock.
pub(crate) struct LinesRun {
    /// What the lines ask for, with every list they changed.
    pub(crate) apply: ApplyResult,
    /// What they changed outside the terminal text.
    pub(crate) shown: ShownChanges,
    pub(crate) effects: input::LineEffects,
}

/// The part of [`run_fired_command`] that runs under the profile lock:
/// the input pipeline, which runs the Lua bodies of any script aliases
/// in the command where they stand, and all the Lua it ran asks for.
pub(super) fn run_fired_locked(
    state: &AppState,
    p: &mut Profile,
    command: &str,
    shared: Option<&SharedLayer>,
) -> LinesRun {
    run_lines_locked(state, p, [(LineFrom::You, command)], shared)
}

/// Run `lines` through the input pipeline under the profile lock, each
/// for whoever asked for it, as [`line_script_result`] reads it. Every
/// path runs its lines here: a typed line, a Settings timer, the tick
/// command and `mud.input`.
pub(crate) fn run_lines_locked<'a>(
    state: &AppState,
    p: &mut Profile,
    lines: impl IntoIterator<Item = (LineFrom, &'a str)>,
    shared: Option<&SharedLayer>,
) -> LinesRun {
    let lists_before = ListRevisions::of(p);
    let shown_before = Shown::of(p);
    let mut effects = input::LineEffects::default();
    let mut apply = ApplyResult::default();
    for (from, line) in lines {
        let ran = run_and_note_line(state, p, from, line, &mut effects, shared);
        apply.append(line_script_result(ran));
    }
    apply.lists = ListChanges::since(lists_before, p);
    LinesRun {
        apply,
        shown: ShownChanges::since(shown_before, p),
        effects,
    }
}

/// Run one command produced by a timer (or any non-typed source) through
/// the full input pipeline and deliver its results through
/// [`apply_script_result`]: echo lines to the terminal and bytes to the
/// server, with what the Lua bodies of its script aliases send among them
/// in the order the command names them, and all else the Lua it ran asks
/// for.
/// Mirrors the typed-input handler so a timer command behaves exactly
/// like the same line typed at the prompt, including `#lua` and
/// script-bodied aliases.
pub(super) async fn run_fired_command<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    walker: &mut Walker,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
    command: &str,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    let shared = crate::profile::switch::shared_layer_for_lines(app, [command]).await;
    let state = app.state::<SharedState>();
    let LinesRun {
        apply,
        shown,
        effects,
    } = {
        let mut p = profile.lock().await;
        run_fired_locked(&state, &mut p, command, shared.as_ref())
    };
    crate::disk::save::settle_line_effects(app, effects).await;
    shown.send(app);
    let mut io = ScriptIo::Session(stream, sink, walker);
    apply_script_result(app, &mut io, profile, lua_timers, apply).await
}

/// Report a tick step on `session://tick`, so the frontend counts and
/// plays the sound when it fired, then run its Send each tick command
/// through the full input pipeline like a timer command.
pub(super) async fn deliver_tick_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    stream: &mut Stream,
    walker: &mut Walker,
    profile: &Arc<Mutex<Profile>>,
    lua_timers: &SharedTimers,
    step: TickStep,
    sink: &mut OutputSink<'_>,
) -> std::io::Result<()> {
    if let Err(e) = app.emit(events::TICK, &step.payload) {
        warn!(error = %e, "failed to emit tick payload");
    }
    if let Some(command) = step.command {
        run_fired_command(app, stream, walker, profile, lua_timers, &command, sink).await?;
    }
    Ok(())
}
