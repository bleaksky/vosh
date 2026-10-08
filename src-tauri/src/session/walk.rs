//! The walker, the one speedwalk of a connection. It sends one step
//! at a time and keeps one step in flight. Each step waits for the
//! Room.Info of the room it reaches, which the game sends with the look
//! after a move, before the next step leaves. When the Map.Tiles that came
//! with the last Room.Info lists the step's exit in the `ex` of the cell
//! you stand on, the step expects that room. Without tiles any new room
//! will do.
//!
//! The walk stops on a line the game prints when a step fails, a room
//! other than the one the step expects, Char.Combat with a target, a
//! Char.State position other than standing, the blind or dark look,
//! `#walk stop` or Esc, or a command you send the game. A step with no
//! Room.Info for [`BACKSTOP`] stops it too, since a trap or a hold can
//! keep a step waiting for about six seconds (`act_move.c:730` to `797`).
//! A new walk while a step is in flight takes over once that step lands.
//!
//! The walker only decides. Each event returns a [`WalkOut`] with the
//! step to send, the lines to print and what the walk held to run once
//! you arrive, and the session does the IO. [`Walker::progress`] says
//! where the walk stands, which the session tells the page when it
//! changes.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::time::Instant;
use vosh_automation::alias::ExpandStep;

use crate::input::walk::{steps_text, Dir, WalkCommand, WalkPlan};
use crate::script::ApplyResult;
use crate::sessions::Session;

/// How long a step waits for its Room.Info before the walker gives up
/// on the walk.
pub(crate) const BACKSTOP: Duration = Duration::from_secs(10);

/// What `#walk` and `#walk stop` say when no walk is under way.
pub(crate) const NOT_WALKING: &str = "[walk] You are not walking.";

/// The lines the game answers a step with when the step does not move
/// you. `move_char` in `act_move.c` prints the first group, and
/// `interpret` in `interp.c` the second before the command runs, the
/// last eight when your position is below standing (`interp.c:1396`). A
/// closed door reads `The $d is closed.`, which [`is_failure`] reads
/// for any door.
const FAILURES: &[&str] = &[
    "But you haven't got any legs!",
    "Alas, you cannot go that way.",
    "What?  And leave your beloved master?",
    "That room is private right now.",
    "Don't think so.",
    "Ghosts are not allowed in there!",
    "You've spent too much time in the cells.  Wait a while.",
    "A magical barrier crackles as it stops you.",
    "You failed to find a path.",
    "You lack the concentration required.",
    "You can't fly.",
    "You need a boat to go there.",
    "You sense danger in that direction.",
    "As you approach a host of guards come into view.  To proceed would be certain death!",
    "You have been restricted to this area by your fate!",
    "You are too exhausted.",
    "You're totally frozen!",
    "You're too busy collecting bird droppings!",
    "You are too hurt to do anything.",
    "Why?!? You're so happy right now.",
    "You decide to dance instead!",
    "You can't do that while underground!",
    "You're in a state of self-induced catalepsy.  You can't do that!",
    "This command has been temporarily disabled.",
    "Lie still; you are DEAD.",
    "You are hurt far too bad for that.",
    "You are too stunned to do that.",
    "In your dreams, or what?",
    "Nah... You feel too relaxed...",
    "You are still meditating.",
    "Better stand up first.",
    "No way!  You are still fighting!",
];

/// The look you get after a step you cannot see: blind
/// (`act_info.c:2360`), and in a dark room (`act_info.c:2373`), which
/// the game ends with a space. Neither sends Room.Info.
const UNSEEN: &[&str] = &["You can't see a thing!", "It is pitch black ..."];

/// True when `line`, its trailing spaces gone, is a line the game prints
/// when a step does not move you.
fn is_failure(line: &str) -> bool {
    if FAILURES.contains(&line) {
        return true;
    }
    // `act( "The $d is closed.", ...)` in `move_char` (`act_move.c:343`),
    // where `$d` is the door's whole keyword, or `door` when it has none
    // (`comm.c:7224`). A container prints the same line (`act_obj.c:846`),
    // but only for a command you send, which stops the walk first.
    line.strip_prefix("The ")
        .and_then(|rest| rest.strip_suffix(" is closed."))
        .is_some_and(|door| !door.is_empty())
}

/// What the walker asks of the session after an event.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct WalkOut {
    /// The step to send the game, with its line end, after what
    /// `release` runs.
    pub(crate) send: Vec<u8>,
    /// Lines to print.
    pub(crate) lines: Vec<String>,
    /// What followed `#walk` in its line, to run now: the walk arrived,
    /// or a `#walk stop` or `#walk` ran.
    pub(crate) release: Vec<ExpandStep>,
}

/// The walk under way.
#[derive(Debug)]
struct Walk {
    plan: WalkPlan,
    /// The steps that went as planned. A step that went dark counts, as
    /// it moved you, and one that failed or led elsewhere does not.
    done: usize,
    rest: Vec<ExpandStep>,
}

impl Walk {
    fn total(&self) -> usize {
        self.plan.steps.len()
    }
}

/// A walk waiting for the step in flight to land, which then takes over.
#[derive(Debug)]
struct Next {
    plan: WalkPlan,
    rest: Vec<ExpandStep>,
}

/// The step on its way. It outlives a walk that stops, so a new walk
/// waits for it to land.
#[derive(Debug, Clone, Copy)]
struct Flight {
    /// The room it left, when Vosh knew it.
    from: Option<i64>,
    /// The room it should reach, when the tiles or a route said.
    expect: Option<i64>,
    sent_at: Instant,
}

/// How a step in flight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// The room it should reach, or a new room when nothing said which.
    Arrived,
    /// Another room.
    Elsewhere,
    /// The game said the step failed, so you stand where you were.
    Failed,
    /// The blind or dark look, so the step moved you somewhere unseen.
    Unseen,
}

/// Why a walk stopped, as its line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Why {
    /// Anything that needs no more words.
    Plain,
    LostSight,
    LostTrack,
}

/// Where the walk stands, as the page hears it on
/// [`crate::app::events::WALK`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WalkProgress {
    /// No walk, since the last one arrived or none ran.
    #[default]
    Idle,
    /// A walk under way, or one waiting for the step in flight to land.
    /// `left` is the steps still to go as a `#walk` string, and `route`
    /// is true for a click on the map.
    Walking {
        done: usize,
        total: usize,
        left: String,
        route: bool,
    },
    /// The last walk stopped early, after `done` of its `total` steps.
    Stopped { done: usize, total: usize, why: Why },
}

/// The walker. See the module notes.
#[derive(Debug, Default)]
pub(crate) struct Walker {
    walk: Option<Walk>,
    flight: Option<Flight>,
    next: Option<Next>,
    /// How the last walk stopped, until the next one starts.
    stopped: Option<(usize, usize, Why)>,
    /// The room the last Room.Info named, while Vosh knows where you are.
    room: Option<i64>,
    /// The exits the tiles that came with that Room.Info list from the
    /// cell you stand on.
    exits: Option<HashMap<Dir, i64>>,
    /// The exits of the tiles that came since, which belong to the next
    /// Room.Info. The game sends Map.Tiles before Room.Info in a look.
    fresh: Option<HashMap<Dir, i64>>,
}

impl Walker {
    /// True while a step is in flight, so the lines the game sends matter.
    pub(crate) fn watching(&self) -> bool {
        self.flight.is_some()
    }

    /// When the step in flight gives up waiting for its Room.Info.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.flight.map(|f| f.sent_at + BACKSTOP)
    }

    /// Where the walk stands: the walk under way, else the one waiting
    /// to take over, else how the last one stopped.
    pub(crate) fn progress(&self) -> WalkProgress {
        let walking = |plan: &WalkPlan, done: usize| WalkProgress::Walking {
            done,
            total: plan.steps.len(),
            left: steps_text(&plan.steps[done..]),
            route: plan.route.is_some(),
        };
        match (&self.walk, &self.next, self.stopped) {
            (Some(walk), _, _) => walking(&walk.plan, walk.done),
            (None, Some(next), _) => walking(&next.plan, 0),
            (None, None, Some((done, total, why))) => WalkProgress::Stopped { done, total, why },
            (None, None, None) => WalkProgress::Idle,
        }
    }

    /// Run a `#walk` line, or Esc.
    pub(crate) fn command(&mut self, command: WalkCommand, now: Instant) -> WalkOut {
        let mut out = WalkOut::default();
        match command {
            WalkCommand::Start { plan, rest } => {
                if self.flight.is_some() {
                    self.next = Some(Next { plan, rest });
                    self.stopped = None;
                } else {
                    self.start(plan, rest, now, &mut out);
                }
            }
            WalkCommand::Stop { key, rest } => {
                if !self.stop(Why::Plain, &mut out) && !key {
                    out.lines.push(NOT_WALKING.to_string());
                }
                out.release = rest;
            }
            WalkCommand::Status { rest } => {
                let line = match (&self.walk, &self.next) {
                    (Some(walk), _) => left_line(walk.total() - walk.done, walk.total()),
                    (None, Some(next)) => left_line(next.plan.steps.len(), next.plan.steps.len()),
                    (None, None) => NOT_WALKING.to_string(),
                };
                out.lines.push(line);
                out.release = rest;
            }
        }
        out
    }

    /// A Map.Tiles packet. It belongs to the Room.Info that follows it.
    pub(crate) fn tiles(&mut self, data: &Value) {
        self.fresh = here_exits(data);
    }

    /// A Room.Info packet. A room other than the one the step left lands
    /// the step in flight, and one the step should not reach stops the
    /// walk. The room it left again is a look where you stand, such as
    /// one you typed, so the step waits on.
    pub(crate) fn room_info(&mut self, data: &Value, now: Instant) -> WalkOut {
        let mut out = WalkOut::default();
        let Some(num) = data.get("num").and_then(Value::as_i64) else {
            return out;
        };
        let exits = self.fresh.take();
        let Some(flight) = self.flight else {
            self.room = Some(num);
            self.exits = exits;
            return out;
        };
        self.exits = exits;
        if flight.from == Some(num) {
            return out;
        }
        self.flight = None;
        self.room = Some(num);
        let landing = if flight.expect.map_or(true, |expect| expect == num) {
            Landing::Arrived
        } else {
            Landing::Elsewhere
        };
        self.landed(landing, now, &mut out);
        out
    }

    /// A line of game text, without its colors. While a step is in
    /// flight, a line that says it failed lands it where you stand, and
    /// the blind or dark look lands it somewhere Vosh cannot see. A line
    /// that ends a prompt Vosh did not read comes as [`answer`] leaves it.
    pub(crate) fn line(&mut self, plain: &str, now: Instant) -> WalkOut {
        let mut out = WalkOut::default();
        if self.flight.is_none() {
            return out;
        }
        let line = plain.trim_end();
        let landing = if is_failure(line) {
            Landing::Failed
        } else if UNSEEN.contains(&line) {
            Landing::Unseen
        } else {
            return out;
        };
        self.flight = None;
        if landing == Landing::Unseen {
            self.lose_room();
        }
        self.landed(landing, now, &mut out);
        out
    }

    /// A Char.Combat packet. One with a target stops the walk.
    pub(crate) fn combat(&mut self, data: &Value) -> WalkOut {
        let fighting = data
            .get("target")
            .and_then(Value::as_str)
            .is_some_and(|target| !target.trim().is_empty());
        self.stop_when(fighting)
    }

    /// A Char.State packet. Any position but standing stops the walk.
    pub(crate) fn state(&mut self, data: &Value) -> WalkOut {
        let down = data
            .get("position")
            .and_then(Value::as_str)
            .is_some_and(|position| position != "standing");
        self.stop_when(down)
    }

    /// You sent the game `bytes` from the command line. A command stops
    /// the walk, and a bare Enter, which asks the game for nothing, does
    /// not.
    pub(crate) fn typed(&mut self, bytes: &[u8]) -> WalkOut {
        let command = bytes
            .split(|b| *b == b'\n')
            .any(|line| line.iter().any(|b| !b.is_ascii_whitespace()));
        self.stop_when(command)
    }

    /// The backstop. A step with no Room.Info by its deadline stops the
    /// walk, and Vosh no longer knows where you are.
    pub(crate) fn expire(&mut self, now: Instant) -> WalkOut {
        let mut out = WalkOut::default();
        if self.deadline().map_or(true, |deadline| deadline > now) {
            return out;
        }
        self.flight = None;
        self.lose_room();
        self.stop(Why::LostTrack, &mut out);
        out
    }

    fn stop_when(&mut self, stop: bool) -> WalkOut {
        let mut out = WalkOut::default();
        if stop {
            self.stop(Why::Plain, &mut out);
        }
        out
    }

    /// Vosh no longer knows the room you stand in or its exits.
    fn lose_room(&mut self) {
        self.room = None;
        self.exits = None;
        self.fresh = None;
    }

    /// Start `plan` from where you stand, now that no step is in flight.
    /// A route planned from another room drops.
    fn start(&mut self, plan: WalkPlan, rest: Vec<ExpandStep>, now: Instant, out: &mut WalkOut) {
        if plan.steps.is_empty() {
            return;
        }
        if let Some(route) = &plan.route {
            if self.room != Some(route.start) || route.rooms.len() != plan.steps.len() {
                return;
            }
        }
        self.walk = Some(Walk {
            plan,
            done: 0,
            rest,
        });
        self.stopped = None;
        self.send_next(now, out);
    }

    /// Send the next step of the walk under way and put it in flight.
    fn send_next(&mut self, now: Instant, out: &mut WalkOut) {
        let Some(walk) = &self.walk else {
            return;
        };
        let dir = walk.plan.steps[walk.done];
        let expect = match &walk.plan.route {
            Some(route) => route.rooms.get(walk.done).copied(),
            None => self
                .exits
                .as_ref()
                .and_then(|exits| exits.get(&dir).copied()),
        };
        self.flight = Some(Flight {
            from: self.room,
            expect,
            sent_at: now,
        });
        out.send.push(dir.letter() as u8);
        out.send.extend_from_slice(b"\r\n");
    }

    /// The step in flight ended as `landing`. The walk goes on, arrives,
    /// or stops, and a walk waiting to take over starts from here. A walk
    /// that arrives lets go of what it held first, even with a walk
    /// waiting.
    fn landed(&mut self, landing: Landing, now: Instant, out: &mut WalkOut) {
        if let Some(mut walk) = self.walk.take() {
            if matches!(landing, Landing::Arrived | Landing::Unseen) {
                walk.done += 1;
            }
            match landing {
                Landing::Arrived if walk.done == walk.total() => {
                    self.stopped = None;
                    out.release = walk.rest;
                }
                Landing::Arrived if self.next.is_none() => {
                    self.walk = Some(walk);
                    self.send_next(now, out);
                    return;
                }
                Landing::Unseen => self.halt(&walk, Why::LostSight, out),
                Landing::Arrived | Landing::Elsewhere | Landing::Failed => {
                    self.halt(&walk, Why::Plain, out);
                }
            }
        }
        if let Some(next) = self.next.take() {
            self.start(next.plan, next.rest, now, out);
        }
    }

    /// Stop the walk under way, or the one waiting to take over, and say
    /// so. False when there was none.
    fn stop(&mut self, why: Why, out: &mut WalkOut) -> bool {
        let next = self.next.take();
        if let Some(walk) = self.walk.take() {
            self.halt(&walk, why, out);
            return true;
        }
        if let Some(next) = next {
            let waiting = Walk {
                plan: next.plan,
                done: 0,
                rest: next.rest,
            };
            self.halt(&waiting, why, out);
            return true;
        }
        false
    }

    /// `walk` stopped early for `why`. Say so, and keep how it stopped.
    fn halt(&mut self, walk: &Walk, why: Why, out: &mut WalkOut) {
        out.lines.push(stopped_line(walk, why));
        self.stopped = Some((walk.done, walk.total(), why));
    }
}

/// The answer to a step in `line`, the line that ended the partial
/// `partial`, both without colors. The game starts no new row for its
/// answer to a command (`comm.c:2111`), so the answer runs on from the
/// prompt before it when no IAC GA ended that prompt and Vosh did not
/// read it. The answer is what follows the partial, and a line that does
/// not start with the partial is all answer.
pub(crate) fn answer<'a>(line: &'a str, partial: Option<&str>) -> &'a str {
    partial
        .and_then(|partial| line.strip_prefix(partial))
        .unwrap_or(line)
}

/// `step` or `steps`, as `total` takes.
fn steps_word(total: usize) -> &'static str {
    if total == 1 {
        "step"
    } else {
        "steps"
    }
}

/// What bare `#walk` says while you walk.
fn left_line(left: usize, total: usize) -> String {
    format!("[walk] {left} of {total} {} left.", steps_word(total))
}

/// What Vosh prints when `walk` stops early.
fn stopped_line(walk: &Walk, why: Why) -> String {
    let mut line = String::from("[walk] Stopped");
    if why != Why::LostTrack {
        let total = walk.total();
        let _ = write!(
            line,
            " after {} of {total} {}",
            walk.done,
            steps_word(total)
        );
    }
    if !walk.rest.is_empty() {
        line.push_str(", so Vosh did not send the rest of the line");
    }
    line.push('.');
    match why {
        Why::Plain => {}
        Why::LostSight => line.push_str(" Vosh lost sight of the room."),
        Why::LostTrack => line.push_str(" Vosh lost track of the walk."),
    }
    line
}

/// The `ex` of the cell you stand on in a Map.Tiles packet, the cell
/// whose `h` is 1, as the room each exit leads to. None when no cell is
/// yours.
fn here_exits(data: &Value) -> Option<HashMap<Dir, i64>> {
    let here = data
        .get("g")?
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .flatten()
        .find(|cell| cell.get("h").and_then(Value::as_i64) == Some(1))?;
    let exits = here
        .get("ex")
        .and_then(Value::as_object)
        .map(|ex| {
            ex.iter()
                .filter_map(|(key, room)| {
                    let mut letters = key.chars();
                    let dir = Dir::from_letter(letters.next()?)?;
                    letters.next().is_none().then_some(())?;
                    Some((dir, room.as_i64()?))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(exits)
}

/// Run what a walk held, the steps after `#walk` in its line, as the
/// line would have run them, under the profile lock and the connection's.
/// A `#walk` among them comes back in the result's `walk`.
pub(super) async fn release(session: &Session, rest: Vec<ExpandStep>) -> ApplyResult {
    let mut lua = ApplyResult::default();
    let (open, result) = {
        let mut p = session.lock_profile().await;
        let mut c = session.connection.lock();
        let result = crate::input::run_expanded(&mut p, &mut c, rest, &mut lua);
        (p.open().clone(), result)
    };
    let mut apply = ApplyResult {
        send_bytes: result.bytes,
        echoes: result.echo,
        walk: result.walk,
        ..ApplyResult::default()
    };
    apply.append(lua);
    apply.ran_under(&open)
}
