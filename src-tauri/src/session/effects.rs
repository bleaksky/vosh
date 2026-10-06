//! The one Lua effects applier. Every path that runs Lua hands what it
//! asks for to [`apply_script_result`], which sends its bytes, echoes its
//! lines, hands a `#walk` to the walker, keeps its timers, shows its
//! prompt values and its plugins' panes, and runs its `mud.input` lines
//! through the input pipeline. Every line runs through the pipeline here, typed or from a
//! Settings timer, the tick or `mud.input`, and says what it changed
//! outside the terminal text.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tokio::time::Instant;
use tracing::{info, warn};

use crate::app::events::{self, broadcast_list_changes, ListChanges, ListRevisions};
use crate::app::state::{AppState, SharedState};
use crate::input::walk::WalkCommand;
use crate::input::{self, LineFrom};
use crate::output::emit_output;
use crate::profile::live::Profile;
use crate::profile::shared::SharedLayer;
use crate::script::output::LuaOutputPayload;
use crate::script::ApplyResult;
use crate::sessions::Session;
use crate::tick::TickStep;

use super::batch::ReadBatch;
use super::connection::Connection;
use super::prompt_view::emit_prompt_vars;
use super::socket::Stream;
use super::walk::{self, Walker};
use super::TargetPayload;

/// Where a step writes to the terminal: the batch of the read it runs
/// in, or straight out for work outside a read, such as a timer.
pub(super) enum OutputSink<'a> {
    Batch(&'a mut ReadBatch),
    Direct,
}

impl OutputSink<'_> {
    /// Write `bytes` to the terminal of `session`.
    fn write<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session, bytes: Vec<u8>) {
        match self {
            OutputSink::Batch(batch) => batch.out.text(&bytes),
            OutputSink::Direct => emit_output(app, session, bytes),
        }
    }

    /// A prompt var of `session` changed. A read sends the prompt vars
    /// once after its output, and anything else sends them now.
    async fn prompt_vars<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session) {
        match self {
            OutputSink::Batch(batch) => batch.prompt_vars = true,
            OutputSink::Direct => emit_prompt_vars(app, session, true).await,
        }
    }

    /// A plugin of `session` changed its panes. A read sends what changed
    /// once after its output, and anything else sends it now.
    fn lua_panes<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session) {
        match self {
            OutputSink::Batch(batch) => batch.lua_panes = true,
            OutputSink::Direct => emit_lua_panes(app, session),
        }
    }
}

/// Send what the plugins of `session` changed in their panes since the
/// last send, on `session://lua-panes`.
pub(super) fn emit_lua_panes<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session) {
    let changes = session.connection.lock().lua_panes.take_changes();
    if let Some(changes) = changes {
        session.emit(app, events::LUA_PANES, &changes);
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

    fn echo<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        session: &Session,
        lines: Vec<String>,
    ) {
        match self {
            ScriptIo::Session(_, sink, _) => sink.write(app, session, framed_echoes(&lines)),
            ScriptIo::Collect { echoes, .. } => echoes.extend(lines),
        }
    }

    async fn prompt_vars<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session) {
        match self {
            ScriptIo::Session(_, sink, _) => sink.prompt_vars(app, session).await,
            ScriptIo::Collect { .. } => emit_prompt_vars(app, session, true).await,
        }
    }

    fn lua_panes<R: tauri::Runtime>(&mut self, app: &AppHandle<R>, session: &Session) {
        match self {
            ScriptIo::Session(_, sink, _) => sink.lua_panes(app, session),
            ScriptIo::Collect { .. } => emit_lua_panes(app, session),
        }
    }

    /// Hand `command` to the walker: in the session it sends the step and
    /// prints the walker's lines, and returns what a `#walk stop` or a
    /// bare `#walk` let go of, run. Anywhere else the walk collects for
    /// the caller, the later of two taking over.
    async fn walk<R: tauri::Runtime>(
        &mut self,
        app: &AppHandle<R>,
        session: &Session,
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
                    sink.write(app, session, framed_echoes(&out.lines));
                }
                if out.release.is_empty() {
                    return Ok(None);
                }
                Ok(Some(walk::release(session, out.release).await))
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
/// command, a plugin load and the Scripts console. Sends and echoes flow
/// to `io`, the `[lua]` lines reach the Scripts page, timers register
/// with the shared list, values a script gave your prompt reach the
/// windows, and `mud.input` lines are run through the input pipeline
/// so they pick up aliases and slash commands too, with all their own Lua
/// asks for applied in turn.
pub(super) async fn apply_script_result<R: tauri::Runtime>(
    app: &AppHandle<R>,
    io: &mut ScriptIo<'_, '_>,
    session: &Arc<Session>,
    apply: ApplyResult,
) -> std::io::Result<()> {
    let mut apply = apply;
    let mut depth = 0;
    let mut budget = InputBudget::new();
    loop {
        // Durable Lua changes (mud.alias, set_var, group toggles fired
        // by triggers or timers) ride the same debounced save the slash
        // commands use, or they would never reach disk.
        mark_durable(app, &apply);
        // A Lua `mud.alias` changes the list an open Settings page shows,
        // while the profile it ran under is in front. Every path names
        // that profile through `ran_under`.
        if let Some(open) = &apply.profile {
            broadcast_list_changes(app, open, apply.lists);
        }

        if !apply.send_bytes.is_empty() {
            io.send(&apply.send_bytes).await?;
        }
        if !apply.echoes.is_empty() {
            io.echo(app, session, std::mem::take(&mut apply.echoes));
        }
        // A `#walk` goes after the bytes of its line. What a `#walk stop`
        // or a bare `#walk` let go of runs right after it, and its own
        // timers and `mud.input` lines join this result's.
        let mut walking = apply.walk.take();
        while let Some(command) = walking.take() {
            let Some(mut released) = io.walk(app, session, command).await? else {
                continue;
            };
            mark_durable(app, &released);
            if let Some(open) = &released.profile {
                broadcast_list_changes(app, open, std::mem::take(&mut released.lists));
            }
            if !released.send_bytes.is_empty() {
                io.send(&std::mem::take(&mut released.send_bytes)).await?;
            }
            if !released.echoes.is_empty() {
                io.echo(app, session, std::mem::take(&mut released.echoes));
            }
            walking = released.walk.take();
            apply.append(released);
        }
        // The Scripts page shows the `[lua]` lines in its Output.
        if !apply.lua_lines.is_empty() {
            let lines = std::mem::take(&mut apply.lua_lines);
            session.emit(app, events::LUA_OUTPUT, &LuaOutputPayload { lines });
        }
        // It shows a plugin Vosh stopped as stopped in its list too.
        if std::mem::take(&mut apply.plugin_stopped) {
            events::broadcast(app, events::PLUGINS_CHANGED, &());
        }
        if !apply.new_timers.is_empty() || !apply.cancel_timers.is_empty() {
            // New timers go in before the cancels run, so a timer that
            // one result both starts and cancels never fires. Lua never
            // gives two timers the same id, so a cancel only ever takes
            // the timer it names.
            let mut guard = session.lua_timers.lock().await;
            guard.extend(apply.new_timers);
            guard.retain(|t| !apply.cancel_timers.contains(&t.timer_id));
        }
        if apply.prompt_vars_changed {
            io.prompt_vars(app, session).await;
        }
        if std::mem::take(&mut apply.panes_changed) {
            io.lua_panes(app, session);
        }
        for owner in std::mem::take(&mut apply.ended) {
            crate::alert::end_owner(app, session, &owner);
        }
        crate::alert::ring(app, session, std::mem::take(&mut apply.alerts));
        if apply.inputs.is_empty() {
            return Ok(());
        }
        if depth == MUD_INPUT_DEPTH {
            warn!(depth, "mud.input went too deep");
            io.echo(
                app,
                session,
                vec![format!("[mud.input recursion limit hit ({depth})]")],
            );
            return Ok(());
        }
        depth += 1;
        let (inputs, dropped) = budget.take(std::mem::take(&mut apply.inputs));
        if !dropped.is_empty() {
            io.echo(app, session, dropped);
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
        let (
            open,
            LinesRun {
                apply: next,
                shown,
                effects,
                replaced_by,
            },
        ) = {
            let mut p = session.lock_profile().await;
            let mut c = session.connection.lock();
            let run = run_lines_locked(
                &state,
                &mut p,
                &mut c,
                inputs.iter().map(|(from, line)| (*from, line.as_str())),
                shared.as_ref(),
            );
            (p.open().clone(), run)
        };
        crate::disk::save::settle_line_effects(app, session, &open, effects, replaced_by).await;
        shown.send(app, session);
        apply = next.ran_under(&open);
    }
}

/// Mark the profile `apply` ran under to save, when its Lua changed what
/// the profile saves.
fn mark_durable<R: tauri::Runtime>(app: &AppHandle<R>, apply: &ApplyResult) {
    debug_assert!(
        !apply.durable_changed || apply.profile.is_some(),
        "a durable change names the profile it ran under"
    );
    if let Some(open) = apply.profile.as_ref().filter(|_| apply.durable_changed) {
        crate::disk::save::mark_profile_dirty(app, open);
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
    session: &Arc<Session>,
    apply: ApplyResult,
) -> Collected {
    let mut collected = Collected::default();
    let mut io = ScriptIo::Collect {
        bytes: &mut collected.bytes,
        echoes: &mut collected.echoes,
        walk: &mut collected.walk,
    };
    // Collecting writes to no stream, so it never fails.
    if let Err(e) = apply_script_result(app, &mut io, session, apply).await {
        warn!(error = %e, "applying a script result failed");
    }
    collected
}

/// Deliver what Lua that ran outside the session loop asks for, `apply`,
/// as at a profile switch or from the Scripts console: its lines print
/// in the terminal of `session`, and what it sends goes to the game from
/// a task of its own when the session runs a connection. A typed line
/// delivers its own way, since it says so when no game listens.
pub(crate) async fn deliver_detached<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    apply: ApplyResult,
) {
    let Collected {
        bytes,
        echoes,
        walk,
    } = collect_script_result(app, session, apply).await;
    crate::output::echo_lines(app, session, &echoes);
    if bytes.is_empty() && walk.is_none() {
        return;
    }
    // A login switches profiles inside the session task, and a
    // disconnect holds the session lock while it waits for that task to
    // end. So the bytes and a #walk go from a task of their own, and
    // neither a switch nor the console waits on the lock.
    let session = Arc::clone(session);
    tokio::spawn(async move {
        let delivered = session.slot.lock().await.as_ref().is_some_and(|handle| {
            (bytes.is_empty() || handle.send(bytes)) && walk.map_or(true, |walk| handle.walk(walk))
        });
        if !delivered {
            info!("Lua output outside the session loop has no game to go to");
        }
    });
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
/// saved profile. Call with the profile lock held, and the connection's
/// after it. A `#profile reset`, or a `#profile load` that reads its file,
/// swaps the live UI config and panes, so the panes generation moves in
/// the same step. The profile file it reads holds none of the shared
/// settings, so `shared` goes back over the result.
pub(super) fn run_and_note_line(
    state: &AppState,
    p: &mut Profile,
    c: &mut Connection,
    from: LineFrom,
    line: &str,
    effects: &mut input::LineEffects,
    shared: Option<&SharedLayer>,
) -> input::Ran {
    let ran = match shared.filter(|_| input::may_replace_profile(state, line)) {
        Some(layer) => layer.keep_across(p, |p| input::run_line_from(state, p, c, line, from)),
        None => input::run_line_from(state, p, c, line, from),
    };
    effects.note_ran(line, &ran);
    if ran.replaced {
        state.bump_panes_generation();
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
    fn of(c: &Connection) -> Self {
        Self {
            target: TargetPayload::of(c),
            look: crate::prompt::prompt_look(c),
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
    /// What `c` changed since `before` was taken.
    fn since(before: Shown, c: &Connection) -> Self {
        let after = Shown::of(c);
        Self {
            repaint: after.look != before.look,
            target: (after.target != before.target).then_some(after.target),
        }
    }

    /// Ask `session` for the repaint and send the target, for lines the
    /// session loop runs. The repaint request goes through the session
    /// handle from a task of its own, since `session_disconnect` holds the
    /// handle's lock while it waits for this session to end.
    fn send<R: tauri::Runtime>(self, app: &AppHandle<R>, session: &Arc<Session>) {
        if self.repaint {
            let session = Arc::clone(session);
            tokio::spawn(async move { crate::prompt::request_prompt_repaint(&session).await });
        }
        if let Some(payload) = self.target {
            session.emit(app, events::TARGET, &payload);
        }
    }
}

/// What lines run through the input pipeline produced under the profile
/// lock and the connection's.
pub(crate) struct LinesRun {
    /// What the lines ask for, with every list they changed.
    pub(crate) apply: ApplyResult,
    /// What they changed outside the terminal text.
    pub(crate) shown: ShownChanges,
    pub(crate) effects: input::LineEffects,
    /// Which of `#profile reset` and `#profile load` last laid a profile
    /// over the live one, read from its line the way the choice to lay
    /// global.toml back is.
    pub(crate) replaced_by: Option<input::ProfileReplace>,
}

/// The part of [`run_fired_command`] that runs under the profile lock
/// and the connection's: the input pipeline, which runs the Lua bodies of
/// any script aliases in the command where they stand, and all the Lua it
/// ran asks for.
pub(super) fn run_fired_locked(
    state: &AppState,
    p: &mut Profile,
    c: &mut Connection,
    command: &str,
    shared: Option<&SharedLayer>,
) -> LinesRun {
    run_lines_locked(state, p, c, [(LineFrom::You, command)], shared)
}

/// Run `lines` through the input pipeline under the profile lock and the
/// connection's, each for whoever asked for it, as [`line_script_result`]
/// reads it. Every path runs its lines here: a typed line, a Settings
/// timer, the tick command and `mud.input`.
pub(crate) fn run_lines_locked<'a>(
    state: &AppState,
    p: &mut Profile,
    c: &mut Connection,
    lines: impl IntoIterator<Item = (LineFrom, &'a str)>,
    shared: Option<&SharedLayer>,
) -> LinesRun {
    let lists_before = ListRevisions::of(p, c);
    let shown_before = Shown::of(c);
    let mut effects = input::LineEffects::default();
    let mut apply = ApplyResult::default();
    let mut replaced_by = None;
    for (from, line) in lines {
        let ran = run_and_note_line(state, p, c, from, line, &mut effects, shared);
        if ran.replaced {
            replaced_by = input::profile_replace(line);
        }
        apply.append(line_script_result(ran));
    }
    apply.lists = ListChanges::since(lists_before, p, c);
    LinesRun {
        apply,
        shown: ShownChanges::since(shown_before, c),
        effects,
        replaced_by,
    }
}

/// Run one command produced by a timer (or any non-typed source) through
/// the full input pipeline and deliver its results to `io` through
/// [`apply_script_result`]: echo lines to the terminal and bytes to the
/// server, with what the Lua bodies of its script aliases send among them
/// in the order the command names them, and all else the Lua it ran asks
/// for.
/// Mirrors the typed-input handler so a timer command behaves exactly
/// like the same line typed at the prompt, including `#lua` and
/// script-bodied aliases.
pub(super) async fn run_fired_command<R: tauri::Runtime>(
    app: &AppHandle<R>,
    io: &mut ScriptIo<'_, '_>,
    session: &Arc<Session>,
    command: &str,
) -> std::io::Result<()> {
    let shared = crate::profile::switch::shared_layer_for_lines(app, [command]).await;
    let state = app.state::<SharedState>();
    let (
        open,
        LinesRun {
            apply,
            shown,
            effects,
            replaced_by,
        },
    ) = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let run = run_fired_locked(&state, &mut p, &mut c, command, shared.as_ref());
        (p.open().clone(), run)
    };
    crate::disk::save::settle_line_effects(app, session, &open, effects, replaced_by).await;
    shown.send(app, session);
    apply_script_result(app, io, session, apply.ran_under(&open)).await
}

/// Report a tick step of `session` on `session://tick`, so the frontend
/// counts and plays the sound when it fired, then run its Send each tick
/// command through the full input pipeline like a timer command.
pub(super) async fn deliver_tick_step<R: tauri::Runtime>(
    app: &AppHandle<R>,
    io: &mut ScriptIo<'_, '_>,
    session: &Arc<Session>,
    step: TickStep,
) -> std::io::Result<()> {
    session.emit(app, events::TICK, &step.payload);
    if let Some(command) = step.command {
        run_fired_command(app, io, session, &command).await?;
    }
    Ok(())
}
