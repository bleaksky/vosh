//! Auto reconnect (Alerts Q13 and Q14, with Sessions Q8 and Q10). After a
//! drop while you play, Vosh dials the same world again, 3 seconds after
//! the drop, then 6, 12, 24, 48 and 60 seconds after each try before, 8
//! tries in all, one socket at a time. The series ends at the first try
//! that connects, which reaches the game's first prompt, and you log in
//! yourself, since Vosh sends nothing on a redial and stores no password.
//! Each session runs a series of its own.
//!
//! [`LinkWatch`] follows what decides it on the connection, under the
//! connection lock the line pipeline takes anyway: whether you play, from
//! Char.Status or the vitals the game sends only in play until a line
//! says you stepped away to the account menu, a closing line of the game
//! since the last prompt, a quit of yours, and the game's question
//! whether to connect anyway to a character another link plays.
//!
//! Vosh never dials after your Disconnect, a quit you sent in the last
//! 10 seconds, a closing line, or when another session took the
//! character, and never while the profile has Reconnect when the link
//! drops off. Disconnect, Connect and closing the session end a series.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::alert::presets::{self, Link};
use crate::app::events;
use crate::app::state::SharedState;
use crate::output::emit_output;
use crate::profile::live::Profile;
use crate::sessions::{Address, Session};

use super::effects::framed_echoes;

/// The waits before each try, the first from the drop and each other one
/// from the end of the try before.
pub(crate) const WAITS: [Duration; 8] = [
    Duration::from_secs(3),
    Duration::from_secs(6),
    Duration::from_secs(12),
    Duration::from_secs(24),
    Duration::from_secs(48),
    Duration::from_secs(60),
    Duration::from_secs(60),
    Duration::from_secs(60),
];

/// How many tries a series makes.
const TRIES: usize = WAITS.len();

/// A quit of yours this recent means a drop is the quit.
const QUIT_GRACE: Duration = Duration::from_secs(10);

/// A Y to the game's question this recent took the character another
/// session played.
const TAKE_GRACE: Duration = Duration::from_secs(10);

/// The lines the game closes the link on while you play, matched as a
/// whole line anywhere since the last prompt, since a reset can throw
/// away the last bytes. Your quit and the idle auto quit alike print the
/// first (`act_comm.c:3250`, `update.c:4051`), and a ban the second
/// (`update.c:5142`, 5160).
const CLOSING_LINES: [&str; 2] = [
    "You have escaped from the Forsaken Lands.",
    "This account has been banned.",
];

/// The line `quit menu` prints as you step away to the account menu
/// (`act_comm.c:3248`), which ends play on a link that stays open.
const LEFT_PLAY: &str = "You step away from the Forsaken Lands and return to your account menu.";

/// The line the game prints when you pick a character another link plays
/// (`comm.c:6531`). A Y after it closes the other link.
const ALREADY_PLAYING: &str = "That character is already playing.";

/// What the redial follows on one connection. It sits on the session's
/// [`Connection`](super::connection::Connection), since the line pipeline
/// changes it, and starts over at each connect.
#[derive(Debug, Default)]
pub(crate) struct LinkWatch {
    /// You play, from Char.Status or the vitals the game sends only in
    /// play, until you step away to the account menu.
    playing: bool,
    /// A closing line came since the last prompt.
    closing: bool,
    /// When a quit of yours left.
    quit_at: Option<Instant>,
    /// The game asked whether to connect anyway, so a Y next takes the
    /// character another link plays.
    asked: bool,
    /// When a Y of yours took a character another link played.
    pub(crate) took_at: Option<Instant>,
    /// Another session logged in as the character this one plays, on the
    /// same host and port, so the game closes this link. Only a link that
    /// plays takes the mark, and it holds until you step away or log in
    /// again on this link.
    taken: bool,
}

/// Why a drop while you play does not redial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Why {
    /// You sent a quit in the seconds before.
    Quit,
    /// The game closed the link with a line that says so.
    Closing,
    /// Another session took the character.
    Taken,
    /// Reconnect when the link drops is off in the profile.
    Off,
}

impl LinkWatch {
    /// A packet `package` came. The game sends Char.Status as you log in
    /// and the vitals only while you play, so either one means you play,
    /// a reconnect to a character left link dead among it, which sends no
    /// Char.Status.
    pub(crate) fn gmcp(&mut self, package: &str) {
        if package == "Char.Status" || package == "Char.Vitals" {
            self.playing = true;
        }
    }

    /// A complete line of the game came.
    pub(crate) fn line(&mut self, plain: &str) {
        let line = plain.trim();
        if CLOSING_LINES.contains(&line) {
            self.closing = true;
        } else if line == LEFT_PLAY {
            self.playing = false;
            self.taken = false;
        } else if line.starts_with(ALREADY_PLAYING) {
            self.asked = true;
        }
    }

    /// Your prompt came, so a closing line before it no longer counts.
    pub(crate) fn prompt(&mut self) {
        self.closing = false;
    }

    /// You sent `bytes`, a line or more. A quit is spelled out in full,
    /// as the game asks, and a Y answers its question.
    pub(crate) fn sent(&mut self, bytes: &[u8], now: Instant) {
        let text = String::from_utf8_lossy(bytes);
        for line in text.split(['\r', '\n']).filter(|l| !l.trim().is_empty()) {
            let line = line.trim();
            let word = line.split_whitespace().next().unwrap_or_default();
            if word.eq_ignore_ascii_case("quit") {
                self.quit_at = Some(now);
            }
            if self.asked && line.starts_with(['y', 'Y']) {
                self.took_at = Some(now);
            }
            self.asked = false;
        }
    }

    /// Whether you play now.
    pub(crate) fn playing(&self) -> bool {
        self.playing
    }

    /// You logged in on this link as a character it had not named yet,
    /// so no mark another session left on what this link played before
    /// holds, and the game asks nothing now.
    pub(crate) fn logged_in(&mut self) {
        self.taken = false;
        self.asked = false;
    }

    /// Another session logged in as the character this link plays, which
    /// the game closes it for. A link that does not play keeps no mark,
    /// as at the account menu, where the character is free to take.
    fn take(&mut self) {
        if self.playing {
            self.taken = true;
        }
    }

    /// What a drop at `now` while you play means: None when it is
    /// unexpected, so a redial may follow, or why it is expected.
    pub(crate) fn expected(&self, now: Instant) -> Option<Why> {
        if self.taken {
            Some(Why::Taken)
        } else if self.closing {
            Some(Why::Closing)
        } else if self
            .quit_at
            .is_some_and(|at| now.duration_since(at) < QUIT_GRACE)
        {
            Some(Why::Quit)
        } else {
            None
        }
    }
}

/// `session://reconnect`: where the redial of a session stands, for the
/// reconnect notice of the page half.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ReconnectPayload {
    /// The next try dials in `seconds`.
    Waiting {
        #[serde(rename = "try")]
        number: usize,
        tries: usize,
        seconds: u64,
    },
    /// A try dials now.
    Dialing {
        #[serde(rename = "try")]
        number: usize,
        tries: usize,
    },
    /// A try failed, for the reason the terminal line gives.
    Failed {
        #[serde(rename = "try")]
        number: usize,
        tries: usize,
        reason: String,
    },
    /// A try connected and the game's prompt waits for your login.
    Reached {
        #[serde(rename = "try")]
        number: usize,
    },
    /// Every try failed, and Vosh stopped trying.
    Stopped { tries: usize },
    /// Your Disconnect, a Connect or a close ended the series.
    Cancelled,
    /// The link dropped while you played and Vosh does not dial, for
    /// `why`.
    Declined { why: Why },
}

/// Whether a redial opened the link that runs and the game's prompt, the
/// first text on it, has yet to come. The session keeps it, and only this
/// module reads or sets it, so the three turns of the link ring from here.
#[derive(Debug, Default)]
pub(crate) struct AwaitingPrompt(AtomicBool);

impl AwaitingPrompt {
    fn set(&self, awaiting: bool) {
        self.0.store(awaiting, Ordering::Release);
    }
}

/// The first text came on the link that runs. When a redial opened it,
/// that text is the game's prompt, which waits for your login, and the
/// Connection preset raises its alert once.
pub(crate) fn reached_prompt(session: &Session, p: &Profile) -> Option<crate::alert::Alert> {
    if !session.awaiting_game_prompt.0.swap(false, Ordering::AcqRel) {
        return None;
    }
    Some(presets::connection(p, Link::Ready))
}

/// A series that runs for a session: its task, the wake that dials at
/// once, and what it redials.
pub(crate) struct Redial {
    task: tokio::task::JoinHandle<()>,
    now: Arc<Notify>,
    lost: Lost,
}

/// What a drop left a series to redial: where the link ran and the
/// character it played. Each try forgets both on the session as it dials,
/// so the series keeps its own copy.
#[derive(Debug, Clone)]
struct Lost {
    address: Address,
    character: Option<String>,
    /// When the link dropped.
    at: Instant,
}

impl Lost {
    /// Whether the link ran at `here`, the host and port of another link.
    fn at(&self, here: &(String, u16)) -> bool {
        self.address.host == here.0 && self.address.port == here.1
    }
}

impl Redial {
    /// Whether the series redials `character` at `here`.
    fn redials(&self, here: &(String, u16), character: &str) -> bool {
        self.lost.at(here) && self.lost.character.as_deref() == Some(character)
    }

    /// Dial at once in place of the wait under way. Pressed while a try
    /// dials, it does nothing, so the wait after that try runs its time.
    pub(crate) fn now(&self) {
        self.now.notify_waiters();
    }

    /// Whether the series ended, at a try that connected or after its
    /// last try.
    pub(crate) fn ended(&self) -> bool {
        self.task.is_finished()
    }

    /// End the series, and wait until it has. Returns true when the end
    /// cut it short, and false when it had ended on its own, at a try
    /// that connected or after its last try.
    pub(crate) async fn end(self) -> bool {
        self.task.abort();
        self.task.await.is_err_and(|e| e.is_cancelled())
    }
}

/// The connection of `session` ended, for `reason`, or with none for your
/// Disconnect, and `watch` holds what it followed. Ring the Connection
/// preset for a drop while you play, and start a series of redials unless
/// the drop was expected or the profile has Reconnect off. Takes the
/// session map to find a session that took the character, so call it
/// with no lock held.
pub(crate) async fn after_drop<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<Session>,
    reason: Option<&str>,
    watch: LinkWatch,
) {
    if reason.is_none() || !watch.playing() {
        return;
    }
    let now = Instant::now();
    let Some(state) = app.try_state::<SharedState>() else {
        return;
    };
    let state = state.inner().clone();
    let why = watch
        .expected(now)
        .or_else(|| took_elsewhere(&state, session, now).then_some(Why::Taken));
    if why == Some(Why::Taken) {
        print(app, session, &taken_line(session.character().as_deref()));
    }
    let (lost, on) = {
        let p = session.lock_profile().await;
        (presets::connection(&p, Link::Lost), p.reconnect.is_on())
    };
    if let Some(why) = why {
        session.emit(app, events::RECONNECT, &ReconnectPayload::Declined { why });
        return;
    }
    crate::alert::ring(app, session, vec![lost]);
    if !on {
        session.emit(
            app,
            events::RECONNECT,
            &ReconnectPayload::Declined { why: Why::Off },
        );
        return;
    }
    let Some(address) = session
        .address
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return;
    };
    let lost = Lost {
        address,
        character: session.character(),
        at: now,
    };
    let wake = Arc::new(Notify::new());
    let task = tokio::spawn(series(
        app.clone(),
        state,
        Arc::clone(session),
        lost.clone(),
        Arc::clone(&wake),
    ));
    session.start_redial(Redial {
        task,
        now: wake,
        lost,
    });
}

/// Whether another session on the host and port of `session` answered Y
/// to the game's question in the seconds before, which closes this link
/// with no line (Sessions Q8).
fn took_elsewhere(state: &SharedState, session: &Session, now: Instant) -> bool {
    let Some(here) = session.live_address() else {
        return false;
    };
    state.other_sessions(session.id).iter().any(|other| {
        other.live_address().as_ref() == Some(&here)
            && other
                .connection
                .lock()
                .link
                .took_at
                .is_some_and(|at| now.duration_since(at) < TAKE_GRACE)
    })
}

/// A login in `session` named `character`: every other session that
/// plays that character on the same host and port loses it to this one,
/// so its close counts as expected, and one waiting to redial stops and
/// says so (Sessions Q8). A session at the account menu plays no
/// character, so it loses none. Takes the session map, so call it with no
/// lock held.
pub(crate) async fn took_character<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &SharedState,
    session: &Session,
    character: &str,
) {
    let Some(here) = session.live_address() else {
        return;
    };
    for other in state.other_sessions(session.id) {
        let plays = other.live_address().as_ref() == Some(&here)
            && other.character().as_deref() == Some(character);
        if plays {
            other.connection.lock().link.take();
        }
        // A series matches on what the drop left it, since each try
        // forgets the address and the character on the session.
        let redial =
            other.take_redial_if(|redial| !redial.ended() && redial.redials(&here, character));
        if let Some(redial) = redial {
            if redial.end().await {
                stop_taken(app, &other, Some(character));
            }
        }
    }
}

/// Whether, since the drop `lost` names, another session on its host and
/// port answered Y to the game's question, or plays its character there
/// now. Either one took the character this series would redial (Sessions
/// Q8). The Y names no character, so it counts for whichever character
/// the game asked about.
fn taken_since(state: &SharedState, session: &Session, lost: &Lost) -> bool {
    state.other_sessions(session.id).iter().any(|other| {
        let Some(there) = other.live_address() else {
            return false;
        };
        if !lost.at(&there) {
            return false;
        }
        let (took, playing) = {
            let c = other.connection.lock();
            (c.link.took_at, c.link.playing())
        };
        took.is_some_and(|at| at >= lost.at)
            || (playing && lost.character.is_some() && other.character() == lost.character)
    })
}

/// End the series of `session` for a character another session took, and
/// say so on its terminal and to the page.
fn stop_taken<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, character: Option<&str>) {
    session.awaiting_game_prompt.set(false);
    print(app, session, &taken_line(character));
    session.emit(
        app,
        events::RECONNECT,
        &ReconnectPayload::Declined { why: Why::Taken },
    );
}

/// End the series `session` runs, if any, and say so to the page.
/// Returns true when the cancel cut a series short.
pub(crate) async fn cancel<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session) -> bool {
    let cut = match session.take_redial() {
        Some(redial) => redial.end().await,
        None => false,
    };
    if cut {
        session.emit(app, events::RECONNECT, &ReconnectPayload::Cancelled);
    }
    // A try the cancel cut short leaves no ring for the next link.
    session.awaiting_game_prompt.set(false);
    cut
}

/// The redials of one series, each after its wait, until one connects or
/// the tries run out. A redial starts a session loop, whose end may start
/// a series, so the future is boxed to name its type.
fn series<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: SharedState,
    session: Arc<Session>,
    lost: Lost,
    wake: Arc<Notify>,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(run_series(app, state, session, lost, wake))
}

async fn run_series<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: SharedState,
    session: Arc<Session>,
    lost: Lost,
    wake: Arc<Notify>,
) {
    let address = &lost.address;
    for (at, wait) in WAITS.iter().enumerate() {
        let number = at + 1;
        session.emit(
            &app,
            events::RECONNECT,
            &ReconnectPayload::Waiting {
                number,
                tries: TRIES,
                seconds: wait.as_secs(),
            },
        );
        tokio::select! {
            () = sleep(&state, &session, *wait) => {}
            () = wake.notified() => {}
        }
        // Another session may have taken the character since the drop.
        if taken_since(&state, &session, &lost) {
            stop_taken(&app, &session, lost.character.as_deref());
            return;
        }
        session.emit(
            &app,
            events::RECONNECT,
            &ReconnectPayload::Dialing {
                number,
                tries: TRIES,
            },
        );
        // The first text of the new link is the game's prompt, which the
        // Connection alert rings for.
        session.awaiting_game_prompt.set(true);
        let dialed = super::dial(
            &app,
            &state,
            &session,
            address.host.clone(),
            address.port,
            address.tls,
            true,
        )
        .await;
        match dialed {
            Ok(()) => {
                session.emit(
                    &app,
                    events::RECONNECT,
                    &ReconnectPayload::Reached { number },
                );
                return;
            }
            Err(reason) => {
                session.awaiting_game_prompt.set(false);
                let reason = plain_reason(&reason);
                print(
                    &app,
                    &session,
                    &format!("[reconnect] Try {number} failed ({reason})"),
                );
                session.emit(
                    &app,
                    events::RECONNECT,
                    &ReconnectPayload::Failed {
                        number,
                        tries: TRIES,
                        reason,
                    },
                );
            }
        }
    }
    print(
        &app,
        &session,
        &format!("[reconnect] Vosh stopped after {TRIES} tries."),
    );
    session.emit(
        &app,
        events::RECONNECT,
        &ReconnectPayload::Stopped { tries: TRIES },
    );
    let stopped = presets::connection(&*session.lock_profile().await, Link::Stopped);
    crate::alert::ring(&app, &session, vec![stopped]);
}

/// Wait `wait` before a try, on the tokio clock, or in a test build on
/// the clock a test holds when it holds one.
async fn sleep(state: &SharedState, session: &Session, wait: Duration) {
    #[cfg(test)]
    {
        let clock = state
            .redial_clock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(clock) = clock {
            let (done, waited) = tokio::sync::oneshot::channel();
            if clock.send((session.id, wait, done)).is_ok() {
                let _ = waited.await;
                return;
            }
        }
    }
    let _ = (state, session);
    tokio::time::sleep(wait).await;
}

/// In a test build, a try that connected waits here before it takes the
/// slot while a test holds the gate. It blocks its thread, so an end of
/// the series cannot land on it until the test lets it go on.
#[cfg(test)]
pub(crate) fn hold_try(state: &SharedState) {
    let gate = state
        .redial_gate
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(gate) = gate {
        let (go_on, wait) = std::sync::mpsc::channel();
        if gate.send(go_on).is_ok() {
            let _ = wait.recv();
        }
    }
}

/// The reason a try failed, as the terminal line gives it.
fn plain_reason(reason: &str) -> String {
    if reason.contains("timed out") {
        "no answer in 10 seconds".into()
    } else if reason.contains("refused") {
        "the game refused the connection".into()
    } else {
        reason.to_string()
    }
}

/// What a session that lost `character` to another session prints.
fn taken_line(character: Option<&str>) -> String {
    match character {
        Some(character) => format!(
            "[reconnect] Another session logged in as {character}, so Vosh does not reconnect here."
        ),
        None => "[reconnect] Another session took this character, so Vosh does not reconnect here."
            .into(),
    }
}

fn print<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session, line: &str) {
    emit_output(app, session, framed_echoes(&[line]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waits_run_three_six_twelve_twenty_four_forty_eight_then_sixty() {
        let secs: Vec<u64> = WAITS.iter().map(Duration::as_secs).collect();
        assert_eq!(secs, [3, 6, 12, 24, 48, 60, 60, 60]);
        assert!(secs.iter().sum::<u64>() < 5 * 60, "about five minutes");
    }

    #[test]
    fn you_play_from_char_status_or_your_vitals_until_you_step_away() {
        let mut watch = LinkWatch::default();
        assert!(!watch.playing());
        watch.gmcp("Char.Vitals");
        assert!(
            watch.playing(),
            "a link dead character sends no Char.Status"
        );
        watch.line(LEFT_PLAY);
        assert!(!watch.playing());
        watch.gmcp("Room.Info");
        assert!(!watch.playing());
        watch.gmcp("Char.Status");
        assert!(watch.playing());
    }

    #[test]
    fn a_closing_line_counts_until_the_next_prompt() {
        let now = Instant::now();
        let mut watch = LinkWatch::default();
        watch.line("You have escaped from the Forsaken Lands.");
        assert_eq!(watch.expected(now), Some(Why::Closing));
        watch.prompt();
        assert_eq!(watch.expected(now), None);
        watch.line("This account has been banned.");
        assert_eq!(watch.expected(now), Some(Why::Closing));
    }

    #[test]
    fn a_quit_counts_for_ten_seconds_and_only_spelled_out() {
        let now = Instant::now();
        let mut watch = LinkWatch::default();
        watch.sent(b"qui\r\n", now);
        assert_eq!(watch.expected(now), None, "qui is no quit");
        watch.sent(b"look\r\nQUIT\r\n", now);
        assert_eq!(
            watch.expected(now + Duration::from_secs(9)),
            Some(Why::Quit)
        );
        assert_eq!(watch.expected(now + QUIT_GRACE), None);
    }

    #[test]
    fn a_y_after_the_game_asks_takes_the_character() {
        let now = Instant::now();
        let mut watch = LinkWatch::default();
        watch.sent(b"y\r\n", now);
        assert_eq!(watch.took_at, None, "the game asked nothing");
        watch.line(ALREADY_PLAYING);
        watch.sent(b"Y\r\n", now);
        assert_eq!(watch.took_at, Some(now));
        let mut watch = LinkWatch::default();
        watch.line(ALREADY_PLAYING);
        watch.sent(b"n\r\n", now);
        watch.sent(b"y\r\n", now);
        assert_eq!(watch.took_at, None, "a No ends the question");
    }

    #[test]
    fn only_a_link_that_plays_takes_the_mark_and_it_ends_with_the_play() {
        let now = Instant::now();
        let mut watch = LinkWatch::default();
        watch.take();
        assert_eq!(watch.expected(now), None, "nothing plays here yet");
        watch.gmcp("Char.Status");
        watch.take();
        assert_eq!(watch.expected(now), Some(Why::Taken));
        watch.line(LEFT_PLAY);
        assert_eq!(watch.expected(now), None, "you stepped away");
        watch.gmcp("Char.Status");
        watch.take();
        watch.logged_in();
        assert_eq!(watch.expected(now), None, "a login on this link");
    }

    #[tokio::test]
    async fn reconnect_now_ends_a_wait_under_way_and_leaves_none_for_the_next() {
        let wake = Arc::new(Notify::new());
        let redial = Redial {
            task: tokio::spawn(async {}),
            now: Arc::clone(&wake),
            lost: Lost {
                address: Address {
                    host: "127.0.0.1".into(),
                    port: 4000,
                    tls: false,
                },
                character: None,
                at: Instant::now(),
            },
        };
        // Pressed while a try dials, with no wait under way.
        redial.now();
        let next = tokio::time::timeout(Duration::from_millis(50), wake.notified()).await;
        assert!(next.is_err(), "the next wait runs its time");
        let waiting = wake.notified();
        redial.now();
        tokio::time::timeout(Duration::from_millis(50), waiting)
            .await
            .expect("the wait under way ends");
    }

    #[test]
    fn a_failed_try_reads_plainly() {
        assert_eq!(
            plain_reason("connect timed out after 10s"),
            "no answer in 10 seconds"
        );
        assert_eq!(
            plain_reason("io error: Connection refused (os error 61)"),
            "the game refused the connection"
        );
    }
}
