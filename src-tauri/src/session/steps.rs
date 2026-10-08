//! The steps the session loop takes for each line, prompt, partial and GA
//! or EOR the game sends, and for each repaint of your prompt. Each takes
//! the [`Connection`], which holds the prompt engine, your target, the
//! room look and the tick's count, and a step that runs triggers or Lua
//! or reads the profile's settings takes the profile too, locked before
//! the connection. None of them sends anything. They write to the read's
//! batch or return the output, and a line step returns what is left for
//! after the locks as a [`LineStep`].

use std::borrow::Cow;
use std::time::Duration;

use tokio::time::Instant;
use tracing::debug;
use vosh_automation::trigger::{readable, LineResult, MatchScope};
use vosh_prompt::stage::{Block, BlockLine, End, Offer, Output};
use vosh_protocol::telnet::Negotiator;
use vosh_script::{Owner, ScriptOutcome};

use crate::profile::live::Profile;
use crate::script::{self, ApplyResult};
use crate::tick::TickStep;

use super::batch::ReadBatch;
use super::connection::Connection;
use super::lines::{Line, LineAccumulator, Partial};
use super::prompt_view::prompt_view;
use super::{highlight_ground, now_ms, room_block};

/// What the Line pass decided for one line that is not your prompt.
struct LinePass {
    result: LineResult,
    /// The tick the line reset, with its Send each tick command.
    tick_step: Option<TickStep>,
    apply: ApplyResult,
}

/// Run one complete line through Line triggers, the tick reset pattern,
/// Lua triggers and the Script bodies the triggers queued, all under the
/// profile and connection locks the caller holds. `plain` is the line
/// without ANSI, so no pattern has to allow for escape bytes and the line
/// is stripped once. `scope` is [`MatchScope::Room`] for a line that
/// lists a room's armies, things or people, so Room triggers run on it
/// too, and [`MatchScope::RoomTarget`] for the line of the person you
/// target.
fn line_pass(
    p: &mut Profile,
    c: &mut Connection,
    bytes: &[u8],
    plain: &str,
    scope: MatchScope,
    now: Instant,
) -> LinePass {
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        bytes,
        plain,
        scope,
        highlight_ground::get(),
        highlight_ground::game(),
        c.stop_key,
    );
    let tick_step = tick_reset(p, c, plain, now);
    script::snapshot_vars(p, c);
    let mut outcome = c.script.match_line(plain);
    script::turn_off_stopped(p, c.stop_key, &outcome);
    // The Lua bodies of this line's Script actions join the outcome the
    // Lua registered triggers wrote, so one apply takes both.
    outcome.append(run_trigger_scripts(p, c, &result));
    let mut apply = script::apply_actions(p, c, outcome);
    // The alerts ride on the match, so a line a trigger hides rings too,
    // and so does a line that names you.
    apply.alerts.extend(
        result
            .alerts
            .iter()
            .map(|alert| crate::alert::Alert::of_trigger(alert, plain)),
    );
    apply.alerts.extend(c.preset_watch.line(p, plain));
    LinePass {
        result,
        tick_step,
        apply,
    }
}

/// The tick step when `plain` matches the tick's Reset on pattern.
fn tick_reset(p: &Profile, c: &mut Connection, plain: &str, now: Instant) -> Option<TickStep> {
    if p.tick.check_reset_match(plain) {
        c.tick.on_game_tick(&p.tick, now)
    } else {
        None
    }
}

/// Run the Lua bodies of the Script actions in `result`, with their
/// captures, and return what they produced. A body Vosh stops turns its
/// trigger off at once, so a later match of the same trigger on this
/// line runs nothing.
pub(super) fn run_trigger_scripts(
    p: &mut Profile,
    c: &mut Connection,
    result: &LineResult,
) -> ScriptOutcome {
    let mut acc = ScriptOutcome::default();
    for call in &result.scripts {
        if p.triggers.is_stopped(&call.source, c.stop_key) {
            continue;
        }
        let owner = Owner::Trigger(call.source.clone());
        let outcome = c.script.run_body(&owner, &call.body, &call.captures);
        script::turn_off_stopped(p, c.stop_key, &outcome);
        acc.append(outcome);
    }
    acc
}

/// What one line, prompt or partial left for the session to do once the
/// profile lock drops: routes, sends, the Lua actions' IO, the tick
/// command, and the scrollback push.
pub(super) struct LineStep {
    pub(super) result: LineResult,
    pub(super) apply: ApplyResult,
    pub(super) tick_step: Option<TickStep>,
    /// The lines to keep in the scrollback ring, each as it shows.
    pub(super) scrollback: Vec<Vec<u8>>,
    /// What Collapse repeated lines made of the line, when it is on, and
    /// the region the run shows in: the line kept starts a run of repeated
    /// lines, or takes the place of the run's line in the ring, the count
    /// before it, as on screen.
    pub(super) repeat: Option<crate::logs::KeptRun>,
}

/// Handle one complete line under the profile lock. The stage reads it
/// as your prompt, alone or as the last line of a prompt that spans lines,
/// through [`prompt_block`], or holds it as the top line of one. Lines it
/// held and let go, then a line that is no prompt, run the Line pass and
/// land in the batch as their triggers left them, each replacing the
/// start an earlier read painted.
pub(super) fn line_step(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    line: Line,
    plain: String,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    c.prompt.note_text();
    // Whether a drop redials reads every line since the last prompt.
    c.link.line(&plain);
    // Without Char.Prompt this session, the game's reply to your own
    // `prompt` tells Vosh your setting, which the capture can take.
    if c.prompt.observing(now_ms()) {
        let before = c.prompt.revision();
        c.prompt
            .observe_line(&line.bytes, &plain, chrono::Local::now().fixed_offset());
        crate::prompt::keep_table(p, c, before);
    }
    let offered = c
        .prompt
        .stage
        .offer(&line.bytes, &plain, line.painted, End::Line);
    let mut steps = released_steps(p, c, batch, offered.released, now, log_session_id);
    match offered.offer {
        Offer::Prompt(block, painted) => {
            steps.push(prompt_block(
                p,
                c,
                batch,
                block,
                painted,
                now,
                log_session_id,
            ));
        }
        Offer::Held => {}
        Offer::Line => steps.push(text_line_step(
            p,
            c,
            batch,
            line.bytes,
            plain,
            Shows::Now(line.painted),
            now,
            log_session_id,
        )),
    }
    steps
}

/// Lines the stage held and let go, each through the Line pass in order.
fn released_steps(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    released: Vec<vosh_prompt::stage::Released>,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    released
        .into_iter()
        .map(|line| {
            text_line_step(
                p,
                c,
                batch,
                line.raw,
                line.plain,
                Shows::Now(line.painted),
                now,
                log_session_id,
            )
        })
        .collect()
}

/// Let go of the lines the stage holds for the rest of a prompt, as your
/// send or a local write does. The end of their read painted them, and
/// what you typed follows them, so each stays as it shows. Each runs the
/// Line pass, so triggers see it and it is logged and kept for
/// scrollback.
pub(super) fn let_go_held(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    c.prompt
        .stage
        .release()
        .into_iter()
        .map(|line| {
            text_line_step(
                p,
                c,
                batch,
                line.raw,
                line.plain,
                Shows::Painted,
                now,
                log_session_id,
            )
        })
        .collect()
}

/// The lines the stage still holds as the session ends. The end of their
/// read painted them, so each is logged and kept for scrollback as it
/// shows. Returns the log rows and the scrollback lines.
pub(super) fn end_held(
    c: &mut Connection,
    log_session_id: Option<i64>,
) -> (Vec<vosh_log::LogEntry>, Vec<Vec<u8>>) {
    let mut log = Vec::new();
    let mut kept = Vec::new();
    for line in c.prompt.stage.release() {
        if let Some(sid) = log_session_id {
            log.push(vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: line.plain,
                raw: Some(line.raw.clone()),
                kind: vosh_log::LineKind::Text,
            });
        }
        kept.push(line.raw);
    }
    (log, kept)
}

/// Where a line that is not your prompt shows.
#[derive(Debug, Clone, Copy)]
enum Shows {
    /// It writes now, over the region an earlier read painted its start
    /// in, if any.
    Now(Option<u64>),
    /// The end of its read painted it whole and output followed it, so it
    /// stays as it shows.
    Painted,
}

/// The scope a complete line that is not your prompt runs in, from the
/// room look tracker, which reads every such line in the order the game
/// sent it. [`MatchScope::RoomTarget`] for the person at the place your
/// target holds in Room.Chars, [`MatchScope::Room`] for any other army,
/// thing or person the look lists, and [`MatchScope::Line`] for any other
/// line. The look's Room.Chars packet comes before its text, so the
/// place is one in this look, the one `tar` marks with `>`.
fn room_scope(c: &mut Connection, plain: &str, bytes: &[u8]) -> MatchScope {
    use room_block::RoomLine;
    match c.room_block.line(plain, bytes) {
        RoomLine::Other => MatchScope::Line,
        RoomLine::Person(place) if c.target.room_idx == Some(place) => MatchScope::RoomTarget,
        RoomLine::Army | RoomLine::Thing | RoomLine::Person(_) => MatchScope::Room,
    }
}

/// A complete line that is not your prompt. It runs the Line pass and
/// lands in the batch as its triggers left it, replacing the region an
/// earlier read painted its start in, or stays as an earlier read
/// painted it (`shows`).
fn text_line_step(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    bytes: Vec<u8>,
    plain: String,
    shows: Shows,
    now: Instant,
    log_session_id: Option<i64>,
) -> LineStep {
    // Every complete line that is not your prompt passes the room look
    // tracker in the order the game sent it, so it knows the lines that
    // list a room's armies, things and people.
    let scope = room_scope(c, &plain, &bytes);
    let LinePass {
        result,
        tick_step,
        mut apply,
    } = line_pass(p, c, &bytes, &plain, scope, now);
    if result.display.is_none() {
        note_gag_without_reader(p, c, batch, &plain, scope);
    }
    // In-place echo replacement. When a trigger gags the line AND its
    // Script action emits one or more `mud.echo(...)` outputs, those
    // echoes land right where the gagged line would have rendered.
    // Without this they fall through to `apply_script_result`, which
    // frames each echo with a line end before and after, and against a
    // gagged line that reads as a blank row followed by the echo.
    let mut shown = Vec::new();
    if result.display.is_none() {
        for echo in apply.echoes.drain(..) {
            shown.extend_from_slice(echo.as_bytes());
            shown.extend_from_slice(b"\r\n");
        }
    }
    let collapse = p.ui.collapse_repeats;
    c.prompt.stage.set_collapse(collapse);
    // In a fight and Attack lines say whether a line of a fight and an
    // attack line join a run. Aabahran sends the pulse's Char.Combat
    // before its text, so the line reads the fight it belongs to. The
    // round that ends a fight comes after the Char.Combat {} that ended
    // it, and its lines are still the fight's until the prompt that ends
    // it. A server that sends the tick after the text sends the first
    // round before the Char.Combat that names your opponent, so an attack
    // line of yours starts the fight's lines until that prompt.
    let rules = vosh_prompt::stage::CollapseRules {
        fights: p.ui.collapse_fight_lines,
        attacks: p.ui.collapse_attack_lines,
    };
    let in_combat = c.prompt.vars.gmcp().fighting();
    if !in_combat && vosh_prompt::aabahran::damage::your_attack_line(&plain) {
        c.fight_head = true;
    }
    let fighting = in_combat || c.fight_tail || c.fight_head;
    let mut repeat = None;
    // Whether the ring keeps the line. While Collapse repeated lines is
    // on, it keeps what the screen shows, so the line end a pinned
    // prompt's row took, which writes nothing, stays out of it.
    let mut ring = true;
    let kept = match shows {
        // Collapse repeated lines shows a line the same as the one before
        // it once, with the count before it. Only what shows collapses,
        // and only a line the rules take, judged by what the game sent.
        // Any other line shows whole and ends the run before it. Its
        // triggers ran above, and it is logged below as any line that
        // shows.
        Shows::Now(painted)
            if collapse
                && result
                    .display
                    .as_ref()
                    .is_some_and(|text| vosh_prompt::stage::collapsible(text.as_bytes()))
                && rules.takes(fighting, &plain) =>
        {
            let text = result.display.as_deref().unwrap_or_default().as_bytes();
            let made = c
                .prompt
                .stage
                .repeat_line(&mut batch.out, &bytes, &plain, painted, text);
            // The run as it shows, its count before it in the colors the
            // text carried into it, and the region it shows in.
            let (shows, gen) = c
                .prompt
                .stage
                .run_shown()
                .unwrap_or_else(|| (text.to_vec(), 0));
            repeat = Some(crate::logs::KeptRun { repeat: made, gen });
            Some(shows)
        }
        Shows::Now(painted) => {
            if let Some(text) = &result.display {
                shown.extend_from_slice(text.as_bytes());
                shown.extend_from_slice(b"\r\n");
            }
            let swallowed = c
                .prompt
                .stage
                .line(&mut batch.out, &bytes, &plain, painted, &shown);
            ring = !(collapse && swallowed);
            result.display.as_ref().map(|text| text.as_bytes().to_vec())
        }
        Shows::Painted => {
            // It shows as the game sent it, whatever its triggers do. What
            // a script echoed in place of a hidden line lands after it.
            c.prompt
                .stage
                .line(&mut batch.out, &bytes, &plain, None, &shown);
            Some(bytes.clone())
        }
    };
    // A line that shows is logged, in one transaction once the socket is
    // quiet, and kept in the ring buffer that becomes scrollback on the
    // next launch. The raw bytes carry ANSI, and the plain text drives
    // the regex search.
    let mut scrollback = Vec::new();
    if let Some(text) = kept {
        if let Some(sid) = log_session_id {
            batch.log.push(vosh_log::LogEntry {
                session_id: sid,
                ts_ms: now_ms(),
                text: plain,
                raw: Some(bytes),
                kind: vosh_log::LineKind::Text,
            });
        }
        if ring {
            scrollback.push(text);
        }
    }
    LineStep {
        result,
        apply,
        tick_step,
        scrollback,
        repeat,
    }
}

/// Your prompt, recognized. Under the profile lock the Line pass takes,
/// in this order: the capture's values merge and the hidden state
/// follows them, Prompts triggers run on its final line so Lua
/// `mud.set_prompt_var` lands before the render, and then the design
/// draws in its place, or with drawing off it shows as sent. Line
/// triggers and Lua `match_line` never see it, and the tick reset
/// pattern still does. A drawn prompt is neither logged nor kept for
/// scrollback, and every line of it that shows is both, as any line that
/// shows. The prompt vars go out after the batch.
fn prompt_block(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    block: Block,
    painted: Option<u64>,
    now: Instant,
    log_session_id: Option<i64>,
) -> LineStep {
    // Your prompt ends any room look before it, and the round that
    // ended a fight, and a closing line no longer ends the link.
    c.room_block.end();
    c.link.prompt();
    c.fight_tail = false;
    c.fight_head = false;
    let disagree = c.prompt.vars.capture(vosh_prompt::Capture {
        values: block.values.clone(),
        raw: Some(block.raw_text()),
    });
    c.prompt.note_prompt(chrono::Local::now().fixed_offset());
    if !disagree.is_empty() {
        debug!(target: "vosh::prompt", fields = ?disagree, "the prompt and GMCP disagree");
    }
    let mut tick_step = None;
    for line in &block.lines {
        if let Some(step) = tick_reset(p, c, &line.plain, now) {
            tick_step.get_or_insert(step);
        }
        // Line triggers no longer see it. Note the ones that would have
        // fired, for the one-time notice.
        let matched = vosh_automation::trigger::matching(
            &p.triggers,
            &line.plain,
            MatchScope::Line,
            c.stop_key,
        );
        c.prompt
            .stage
            .line_triggers_matched(matched.into_iter().map(|t| t.name.as_str()));
    }

    let last = block.final_line().clone();
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        &last.raw,
        &last.plain,
        MatchScope::Prompt,
        highlight_ground::get(),
        highlight_ground::game(),
        c.stop_key,
    );
    if !result.scripts.is_empty() {
        script::snapshot_vars(p, c);
    }
    let outcome = run_trigger_scripts(p, c, &result);
    let mut apply = script::apply_actions(p, c, outcome);
    batch.prompt_vars = true;
    batch.prompt = true;
    // The prompt draws with the packets that came before it.
    batch.gmcp = false;

    let mut before = Vec::new();
    let mut scrollback = Vec::new();
    // Pinned, the prompt leaves the text for the band above the command
    // line. It is logged and kept exactly as it is in the text.
    let pinned = c.prompt.show() == vosh_prompt::PromptShow::Pinned;
    // The away prompt shows as sent, even while Vosh draws.
    if c.prompt.draws() && !block.afk {
        // Echoes land where the prompt was, above the drawn prompt.
        for echo in apply.echoes.drain(..) {
            before.extend_from_slice(echo.as_bytes());
            before.extend_from_slice(b"\r\n");
        }
        // What the open card shows, a preview among them, with the live
        // render behind it, which the region carries as its restore.
        let view = prompt_view(p, c, now);
        // A line above the last one your design reads nothing on has
        // nothing drawn in its place, so it shows as the game sent it.
        let last_index = block.lines.len() - 1;
        let heads_shown: Vec<BlockLine> = block.lines[..last_index]
            .iter()
            .enumerate()
            .filter(|(index, _)| !block.replaced.contains(index))
            .map(|(_, line)| line.clone())
            .collect();
        if pinned {
            c.prompt
                .stage
                .pin_view(&mut batch.out, block, painted, &before, view.stage());
        } else {
            c.prompt
                .stage
                .draw_view(&mut batch.out, block, painted, &before, view.stage());
        }
        for head in &heads_shown {
            keep_shown(batch, &mut scrollback, head, &head.raw, log_session_id);
        }
    } else {
        if result.display.is_none() {
            for echo in apply.echoes.drain(..) {
                before.extend_from_slice(echo.as_bytes());
                before.extend_from_slice(b"\r\n");
            }
        }
        let display = result.display.as_deref().map(str::as_bytes);
        // The lines above the last one show as sent, whatever Prompts
        // triggers do to the last one.
        let heads: Vec<BlockLine> = block.lines[..block.lines.len() - 1].to_vec();
        if pinned {
            c.prompt
                .stage
                .pin_shown(&mut batch.out, block, painted, &before, display);
        } else {
            c.prompt
                .stage
                .show_as_sent(&mut batch.out, block, painted, &before, display);
        }
        for head in &heads {
            keep_shown(batch, &mut scrollback, head, &head.raw, log_session_id);
        }
        if let Some(text) = &result.display {
            keep_shown(
                batch,
                &mut scrollback,
                &last,
                text.as_bytes(),
                log_session_id,
            );
        }
    }
    LineStep {
        result,
        apply,
        tick_step,
        scrollback,
        repeat: None,
    }
}

/// A line of your prompt that shows: logged, in one transaction once the
/// socket is quiet, and kept in the ring buffer that becomes scrollback,
/// as `shown`, what the terminal shows of it.
fn keep_shown(
    batch: &mut ReadBatch,
    scrollback: &mut Vec<Vec<u8>>,
    line: &BlockLine,
    shown: &[u8],
    log_session_id: Option<i64>,
) {
    if let Some(sid) = log_session_id {
        batch.log.push(vosh_log::LogEntry {
            session_id: sid,
            ts_ms: now_ms(),
            text: line.plain.clone(),
            raw: Some(line.raw.clone()),
            kind: vosh_log::LineKind::Text,
        });
    }
    scrollback.push(shown.to_vec());
}

/// A partial Vosh does not read as your prompt, ended by a GA or EOR. It
/// runs through Prompts triggers as it always did and ends its row. A
/// trigger that hides it and sets prompt values while nothing reads your
/// prompt is named once a session.
fn unread_partial(
    p: &mut Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    partial: &Partial,
    plain: &str,
) -> LineStep {
    let game = highlight_ground::game();
    let result = vosh_automation::trigger::process_on_ground(
        &p.triggers,
        &partial.bytes,
        plain,
        MatchScope::Prompt,
        highlight_ground::get(),
        game,
        c.stop_key,
    );
    // What the partial shows when no trigger acts on it, the game's faded
    // 256 colors lifted while Fit game colors is on.
    let bare = game.map_or(Cow::Borrowed(partial.bytes.as_slice()), |game| {
        readable::lift_game_sgr(&partial.bytes, game)
    });
    let effect = match &result.display {
        None => true,
        Some(text) => text.as_bytes() != bare.as_ref(),
    } || !result.sends.is_empty()
        || !result.routes.is_empty()
        || !result.scripts.is_empty();
    let mut apply = ApplyResult::default();
    if effect {
        script::snapshot_vars(p, c);
        let outcome = run_trigger_scripts(p, c, &result);
        apply = script::apply_actions(p, c, outcome);
        // The webview hears every prompt a Prompts trigger acted on.
        batch.prompt_vars = true;
    }
    if result.display.is_none() {
        note_gag_without_reader(p, c, batch, plain, MatchScope::Prompt);
    }
    let mut before = Vec::new();
    if result.display.is_none() {
        for echo in apply.echoes.drain(..) {
            before.extend_from_slice(echo.as_bytes());
            before.extend_from_slice(b"\r\n");
        }
    }
    c.prompt.stage.end_partial(
        &mut batch.out,
        &partial.bytes,
        partial.painted,
        &before,
        result.display.as_deref().map(str::as_bytes),
    );
    LineStep {
        result,
        apply,
        tick_step: None,
        scrollback: Vec::new(),
        repeat: None,
    }
}

/// A GA or EOR arrived. The partial it ends is your prompt when the
/// capture reads it, alone or after held lines, and otherwise goes
/// through [`unread_partial`]. Held lines it does not finish run the Line
/// pass first. The candidates ring records one entry either way.
pub(super) fn marker_step(
    p: &mut Profile,
    c: &mut Connection,
    accumulator: &mut LineAccumulator,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Vec<LineStep> {
    let Some(partial) = accumulator.take_partial() else {
        // A GA after lines the stage held ends them, since the rest of
        // the prompt never came.
        let released = c.prompt.stage.release();
        let steps = released_steps(p, c, batch, released, now, log_session_id);
        c.prompt.record(None, now_ms());
        // The marker ends any room look before it, and the round that
        // ended a fight.
        c.room_block.end();
        c.fight_tail = false;
        c.fight_head = false;
        return steps;
    };
    let plain = vosh_protocol::ansi::plain_text(&partial.bytes);
    let painted = partial.painted.map(|(gen, _)| gen);
    let offered = c
        .prompt
        .stage
        .offer(&partial.bytes, &plain, painted, End::Marker);
    let mut steps = released_steps(p, c, batch, offered.released, now, log_session_id);
    match offered.offer {
        Offer::Prompt(block, painted) => {
            steps.push(prompt_block(
                p,
                c,
                batch,
                block,
                painted,
                now,
                log_session_id,
            ));
            c.prompt.record(None, now_ms());
        }
        Offer::Held | Offer::Line => {
            // The lines it released took the region the partial was
            // painted in, so the partial writes after them.
            let partial = if steps.is_empty() {
                partial
            } else {
                Partial {
                    painted: None,
                    ..partial
                }
            };
            steps.push(unread_partial(p, c, batch, &partial, &plain));
            c.prompt.record(Some((&partial.bytes, &plain)), now_ms());
        }
    }
    c.room_block.end();
    c.fight_tail = false;
    c.fight_head = false;
    steps
}

/// The end of a read. A partial the capture settles on, alone or after
/// held lines, is your prompt now, so it draws in this read and never
/// flashes. Any other partial paints as a region a later read replaces,
/// with the held lines before it. Held lines with no partial after them
/// paint the same way. Then the stage catches up with everything the read
/// wrote.
pub(super) fn partial_step(
    p: &mut Profile,
    c: &mut Connection,
    accumulator: &mut LineAccumulator,
    batch: &mut ReadBatch,
    now: Instant,
    log_session_id: Option<i64>,
) -> Option<LineStep> {
    let mut step = None;
    if let Some(bytes) = accumulator.partial().map(<[u8]>::to_vec) {
        c.prompt.note_text();
        let plain = vosh_protocol::ansi::plain_text(&bytes);
        match c.prompt.stage.settle(&bytes, &plain) {
            Some((block, region)) => {
                let painted = accumulator
                    .take_partial()
                    .and_then(|t| t.painted)
                    .map(|(gen, _)| gen);
                step = Some(prompt_block(
                    p,
                    c,
                    batch,
                    block,
                    region.or(painted),
                    now,
                    log_session_id,
                ));
            }
            // It can still become your prompt, and it was not painted
            // yet, so it waits a moment for the next read.
            None if accumulator.painted().is_none() && c.prompt.stage.live(&plain) => {
                batch.hold = true;
            }
            None => {
                let painted =
                    c.prompt
                        .stage
                        .paint_partial(&mut batch.out, &bytes, accumulator.painted());
                accumulator.set_painted(painted);
            }
        }
    } else {
        c.prompt.stage.end_read(&mut batch.out);
    }
    c.prompt.stage.finish(&mut batch.out);
    step
}

/// A partial that waited for the next read stops waiting: it paints
/// raw, with any held lines before it, as a region a later read
/// replaces.
pub(super) fn hold_step(c: &mut Connection, accumulator: &mut LineAccumulator, out: &mut Output) {
    if let Some(bytes) = accumulator.partial().map(<[u8]>::to_vec) {
        let painted = c
            .prompt
            .stage
            .paint_partial(out, &bytes, accumulator.painted());
        accumulator.set_painted(painted);
    }
    c.prompt.stage.finish(out);
}

/// How long a GMCP packet that changes your prompt waits for text before
/// Vosh repaints your prompt with it, the late GMCP repaint. A prompt that
/// comes in that time draws with the packet, so the repaint never fires.
pub(super) const LATE_REPAINT: Duration = Duration::from_millis(60);

/// When the late GMCP repaint fires, after a read. `waiting`
/// is when it was going to fire, `gmcp` says the read brought packets
/// after the last prompt it read, `prompt` that it read one, and `wrote`
/// that it wrote to the text. What follows the packets draws with them,
/// so in the text and lifted text cancels a repaint an earlier read
/// started, and pinned a prompt does, since the band is not in the text.
/// Packets after the read's last prompt start it while there is an open
/// row or a band to repaint, and one already waiting keeps its time.
/// Whether the packets changed what your prompt shows is decided when it
/// fires, so deciding here draws nothing, and a pulse whose packets and
/// text come in two reads costs no render.
pub(super) fn late_repaint_after(
    c: &Connection,
    waiting: Option<Instant>,
    gmcp: bool,
    prompt: bool,
    wrote: bool,
    now: Instant,
) -> Option<Instant> {
    let pinned = c.prompt.show() == vosh_prompt::PromptShow::Pinned;
    let cancel = if pinned { prompt } else { wrote };
    let waiting = waiting.filter(|_| !cancel);
    if waiting.is_some() || !gmcp {
        return waiting;
    }
    c.prompt.stage.repaintable().then(|| now + LATE_REPAINT)
}

/// The late GMCP repaint fires: your prompt as it shows now, when there is
/// still a row or a band to repaint, which is empty when nothing changed.
/// `other` says output from elsewhere landed since the session last wrote.
pub(super) fn late_repaint_step(
    p: &Profile,
    c: &mut Connection,
    other: bool,
    now: Instant,
) -> Output {
    if !c.prompt.stage.repaintable() {
        return Output::new(other);
    }
    repaint_step(p, c, other, now)
}

/// How long after a clock piece turns to its next second Vosh repaints
/// your prompt with it, so the render never lands a hair early and draws
/// the second before.
pub(super) const CLOCK_SLACK: Duration = Duration::from_millis(5);

/// When the next clock repaint looks at your prompt: when a
/// clock piece in your design next shows another second, or None while
/// your design draws none, so a design without one never wakes the
/// session. The tick turns on its own seconds, counted from its last
/// restart, so the repaint lands on each of them and never skips one.
/// Once a late tick has none left, the seconds since it keep counting
/// up from the same restart. The time and the date turn on the local
/// clock's seconds. With both, the tick sets the pace, so your prompt
/// repaints at most once a second.
pub(super) fn clock_after(p: &Profile, c: &Connection, now: Instant) -> Option<Instant> {
    let clock = c.prompt.clock()?;
    let tick = clock
        .tick
        .then(|| c.tick.remaining(&p.tick, now))
        .flatten()
        .filter(|left| !left.is_zero());
    let late = clock.tick.then(|| c.tick.elapsed(&p.tick, now)).flatten();
    let wait = match (tick, late) {
        (Some(left), _) => match left.as_nanos() % 1_000_000_000 {
            0 => Duration::from_secs(1),
            part => Duration::from_nanos(u64::try_from(part).unwrap_or(0)),
        },
        (None, Some(since)) => {
            let into = since.as_nanos() % 1_000_000_000;
            Duration::from_nanos(u64::try_from(1_000_000_000 - into).unwrap_or(0))
        }
        (None, None) => {
            let into = chrono::Local::now().timestamp_subsec_nanos() % 1_000_000_000;
            Duration::from_nanos(u64::from(1_000_000_000 - into))
        }
    };
    Some(now + wait + CLOCK_SLACK)
}

/// A clock piece shows another second: your prompt as it
/// shows now, the open row while it is the last thing on screen or the
/// band while pinned, and empty when the second changed nothing it
/// shows. The open row waits while `reading`, which says you are
/// selecting text or reading back, and the band does not, since it is not
/// in the text. It waits while the card shows a
/// preview, whose render carries its own restore. Each repaint is a
/// replace with nothing after it, so it never lands on your typed text
/// and never reaches history. `other` says output from elsewhere landed
/// since the session last wrote, which closed the row.
pub(super) fn clock_step(
    p: &Profile,
    c: &mut Connection,
    other: bool,
    reading: bool,
    now: Instant,
) -> Output {
    let pinned = c.prompt.show() == vosh_prompt::PromptShow::Pinned;
    if c.prompt.clock().is_none()
        || c.prompt.preview().is_some()
        || (reading && !pinned)
        || !c.prompt.stage.repaintable()
    {
        return Output::new(other);
    }
    repaint_step(p, c, other, now)
}

/// A line you sent. The candidates ring records the prompt it answers,
/// before the partial goes, and the open row closes, since your typed
/// echo follows it. `sent` is what went to the game, aliases expanded,
/// which opens the observer's window. Returns true when the send started
/// a pulse, on a server that sends no Char.Vitals.
pub(super) fn send_step(
    c: &mut Connection,
    accumulator: &LineAccumulator,
    sent: &[u8],
    at_ms: i64,
) -> bool {
    let partial = accumulator
        .partial()
        .map(|bytes| (bytes.to_vec(), vosh_protocol::ansi::plain_text(bytes)));
    c.prompt.record(
        partial
            .as_ref()
            .map(|(bytes, plain)| (&bytes[..], plain.as_str())),
        at_ms,
    );
    c.prompt.stage.close();
    // A quit of yours, or a Y that takes a character, says how the link
    // may end.
    c.link.sent(sent, Instant::now());
    c.prompt.note_send(&String::from_utf8_lossy(sent), at_ms)
}

/// A window size message. While the card is closed, a new width closes
/// the open row, since each renderer wraps it again at that width, and
/// nothing repaints until the next prompt. A new height wraps
/// nothing, such as when your prompt leaves the pinned band for the text
/// and the terminal grows, so the row stays open and each renderer finds
/// its region where it was. While the card is open, which `card_open`
/// says when it watches your prompt and a preview on the row says too,
/// the row stays open at any size: each renderer finds its region again
/// in its own buffer, so edits and previews keep repainting it and
/// clearing a preview puts the live render back. The size the session
/// already holds, which the webview sends again on every connect, leaves
/// the row open. Pinned, the band is not in the text, so no size closes
/// it.
///
/// A design that pushes part of a row to the right edge (`%{right}`)
/// reaches to the new width, so a new width keeps the row open as the
/// card does and returns true: your prompt draws again at that width, on
/// the row or on the band.
pub(super) fn window_size_step(
    c: &mut Connection,
    negotiator: &mut Negotiator,
    cols: u16,
    rows: u16,
    card_open: bool,
) -> bool {
    let card_open = card_open || c.prompt.preview().is_some();
    let new_width = negotiator.window_size.0 != cols;
    c.prompt.set_cols(usize::from(cols));
    let redraw = new_width && c.prompt.pushes_right();
    if new_width && !card_open && !redraw {
        c.prompt.stage.close();
    }
    negotiator.set_window_size(cols, rows);
    redraw
}

/// Repaint the open row as the `[prompt]` table now says: your design
/// while Vosh draws, else the lines the design replaced, as the game sent
/// them, so turning drawing off shows the game's prompt at once. A
/// preview the open card shows draws in place of the live render, which
/// the row carries as its restore. `other` says output from elsewhere
/// landed since the session last wrote, which closed the row. Returns the
/// repaint, empty when no row is open.
pub(super) fn repaint_step(p: &Profile, c: &mut Connection, other: bool, now: Instant) -> Output {
    let mut out = Output::new(other);
    let view = prompt_view(p, c, now);
    c.prompt.stage.repaint_view(&mut out, view.stage());
    out
}

/// The connection is going. A preview the card shows on your prompt goes
/// with it, so a repaint puts the live render back on the
/// row, or on the band while pinned, since nothing else may land to make
/// the renderers write the restore they hold. Empty with no preview, and
/// when no row or band is left to repaint.
pub(super) fn end_preview_step(
    p: &Profile,
    c: &mut Connection,
    other: bool,
    now: Instant,
) -> Output {
    if c.prompt.preview().is_none() {
        return Output::new(other);
    }
    c.prompt.set_preview(None);
    repaint_step(p, c, other, now)
}

/// A trigger hid a line or partial. When nothing reads your prompt in
/// this profile and the trigger also sets prompt values, it hides your
/// prompt with nothing drawn in its place, so the webview hears its name
/// once a session.
fn note_gag_without_reader(
    p: &Profile,
    c: &mut Connection,
    batch: &mut ReadBatch,
    plain: &str,
    scope: MatchScope,
) {
    if c.prompt.stage.has_recognizer() {
        return;
    }
    for trigger in vosh_automation::trigger::matching(&p.triggers, plain, scope, c.stop_key) {
        if hides_and_reads_prompt(trigger) && c.prompt.stage.gag_without_reader(&trigger.name) {
            batch.gag_without_reader.push(trigger.name.clone());
        }
    }
}

/// The trigger hides what it matches and its script sets prompt values,
/// the shape of a capture trigger.
fn hides_and_reads_prompt(trigger: &vosh_automation::trigger::Trigger) -> bool {
    use vosh_automation::trigger::TriggerAction;
    trigger
        .actions
        .iter()
        .any(|a| matches!(a, TriggerAction::Gag))
        && trigger
            .actions
            .iter()
            .any(|a| matches!(a, TriggerAction::Script { body } if body.contains("set_prompt_var")))
}
