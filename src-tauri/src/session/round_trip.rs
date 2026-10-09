//! The round trip to the game, which the status line shows.
//! Every [`READ_EVERY`] the session loop reads the kernel's smoothed
//! round trip time for the game socket, see [`kernel`]. The kernel only
//! updates it as the game's machine acknowledges your bytes, so in a
//! hard stall it stays low. The loop also times your oldest line the
//! game has not answered yet, see [`Waits`], and takes that wait when it
//! is longer, so a stall counts up live.
//!
//! A wait counts when the link or the game is not answering, never
//! because the game holds your commands while a skill lags you. The
//! Forsaken Lands reads your bytes every pulse even while `ch->wait`
//! runs (comm.c), so its machine acknowledges them at once, keeps the
//! lines in its buffer and answers each when the lag ends. So the wait
//! counts in two halves. The link half counts while the kernel still
//! holds bytes the game's machine has not acknowledged. The game half
//! counts once the game has sent nothing at all since your line for
//! longer than [`HELD_AT_MOST`], past the lags it puts on you. It times
//! only a line the game owes an answer, one sent while its prompt
//! showed. The note editor shows none and answers no line of the note
//! (board.c), so writing a note never counts. Any bytes from the game,
//! text or GMCP alone, end both. A line you type while an earlier one
//! waits starts no wait of its own, so typing ahead during a bash never
//! counts. Each reading goes to the status
//! line on `session://round-trip` and into the [`RoundTrip`] the
//! connection keeps, which `#lag` reads.
//!
//! A stall is any stretch of readings at [`SLOW`] or more. One that a
//! wait reads begins when the line it counts from left, see [`Sample`],
//! so `#lag` lists its real start and length. The session
//! keeps the last [`KEPT_STALLS`] of the connection, and the readings
//! of the last [`USUAL_OVER`] for the usual round trip, their median.

pub(crate) mod kernel;

use std::collections::VecDeque;
use std::time::Duration;

use chrono::NaiveTime;
use serde::Serialize;
use tokio::time::Instant;

/// How often the session reads the round trip.
pub(crate) const READ_EVERY: Duration = Duration::from_secs(2);

/// From here your commands land a pulse late on Aabahran's 250 ms
/// pulse, so a reading this long or longer is slow and part of a stall.
pub(crate) const SLOW: Duration = Duration::from_millis(300);

/// The readings the usual round trip is the median of.
const USUAL_OVER: Duration = Duration::from_secs(600);

/// The stalls of a connection `#lag` lists, the newest.
const KEPT_STALLS: usize = 20;

/// The longest the game holds a line of yours with nothing sent back
/// under the lags it puts on you in play. The longest in the skill
/// table is 120 beats, 30 seconds (const.c), as long as the `PULSE_TICK`
/// a psalm puts on you when it drags you off bloody (magic.c:6713).
/// `WAIT_STATE2` adds up to `PULSE_VIOLENCE`, 3 seconds, while ghoul
/// touch or paralyze slows you (handler.c:7711), and `WAIT_STATE` takes
/// the longer lag, so lags never stack. The game answers the held line
/// in the pulse the lag ends, so add one 250 ms pulse. Spell lag also
/// scales by `MOD_WAIT_STATE` (magic.c:141), which only an item with
/// `spelllag` raises, and no item in the game's data has one. An
/// immortal can lag you for any time with `lag` (`act_wiz.c`). Those can
/// still read as a stall.
pub(crate) const HELD_AT_MOST: Duration = Duration::from_millis(33_250);

/// How long the oldest line of yours the game has not answered has
/// waited, in two halves, see the module doc. [`super::socket::Stream`] tells it each line you send and
/// each read from the game, and asks it for the reading.
#[derive(Debug, Default)]
pub(crate) struct Waits {
    /// When the oldest line left that the network may still carry, the
    /// oldest since the kernel last showed nothing in flight.
    link: Option<Instant>,
    /// When the oldest line left since the game last sent anything, of
    /// those it owes an answer.
    game: Option<Instant>,
    /// The game's text last ended on its prompt, so it waits for a
    /// command and answers the next line, see [`Waits::prompted`].
    prompted: bool,
}

impl Waits {
    /// A line of yours left at `now`. `reached` says the kernel showed
    /// nothing in flight just before, so every earlier line reached the
    /// game's machine and the wait starts over with this one. Typing
    /// ahead while a skill lags you sends lines the game holds
    /// unanswered, and they never count. Only a line sent while the
    /// game's prompt showed starts the game half.
    pub(crate) fn sent(&mut self, now: Instant, reached: bool) {
        if reached {
            self.link = None;
        }
        self.link.get_or_insert(now);
        if self.prompted {
            self.game.get_or_insert(now);
        }
    }

    /// The game's text ended on its prompt, or on something else. The
    /// game shows its prompt, or a GA or EOR, when it waits for your next
    /// command. An editor that takes lines with no answer, like the note
    /// editor, shows none, so the lines you write there start no game
    /// half.
    pub(crate) fn prompted(&mut self, prompted: bool) {
        self.prompted = prompted;
    }

    /// The game sent something, so the link and the game both answer.
    pub(crate) fn heard(&mut self) {
        self.link = None;
        self.game = None;
    }

    /// The sample at `now` from what the kernel says of the socket, the
    /// longest of the round trip, the link half and the game half.
    /// Nothing in flight means the game's machine has every line, so the
    /// link half starts over with your next line. The game half counts
    /// only past [`HELD_AT_MOST`], since a shorter silence may be a lag
    /// the game holds your line for.
    pub(crate) fn reading(&mut self, kernel: kernel::Reading, now: Instant) -> Sample {
        if !kernel.in_flight {
            self.link = None;
        }
        let wait = |sent: Option<Instant>| {
            sent.map(|s| Sample {
                reading: now.duration_since(s),
                began: Some(s),
            })
        };
        let game = wait(self.game).filter(|w| w.reading > HELD_AT_MOST);
        [wait(self.link), game].into_iter().flatten().fold(
            Sample::kernel(kernel.round_trip),
            |best, w| {
                if w.reading > best.reading {
                    w
                } else {
                    best
                }
            },
        )
    }
}

/// Whether the game's text ends on its prompt, given the `partial` after
/// its last line end. Text there is a prompt waiting for your next
/// command, and escapes alone are no text. See [`Waits::prompted`].
pub(crate) fn ends_on_prompt(partial: Option<&[u8]>) -> bool {
    partial.is_some_and(|p| !vosh_protocol::ansi::plain_text(p).is_empty())
}

/// One reading of the round trip, and when its wait began when the
/// wait for a line of yours is the reading, so a stall lists when the
/// line left rather than when a reading first caught it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sample {
    pub(crate) reading: Duration,
    /// When the line the wait counts from left, None when the kernel's
    /// round trip is the reading.
    pub(crate) began: Option<Instant>,
}

impl Sample {
    /// The kernel's round trip is the reading.
    pub(crate) fn kernel(reading: Duration) -> Self {
        Self {
            reading,
            began: None,
        }
    }
}

/// What `session://round-trip` carries beside the session: the reading
/// in whole milliseconds, or null once the connection ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct RoundTripPayload {
    pub(crate) ms: Option<u32>,
}

impl RoundTripPayload {
    pub(crate) fn of(reading: Option<Duration>) -> Self {
        Self {
            ms: reading.map(|d| u32::try_from(d.as_millis()).unwrap_or(u32::MAX)),
        }
    }
}

/// One stretch of readings at [`SLOW`] or more.
#[derive(Debug, Clone)]
struct Stall {
    /// When it began on your local clock.
    at: NaiveTime,
    began: Instant,
    worst: Duration,
    /// The first reading under [`SLOW`] after it, None while it runs.
    ended: Option<Instant>,
}

/// The round trip of one connection: the latest reading, those of the
/// last ten minutes and the stalls since you connected. The connection
/// keeps it, so `#lag` reads it under the connection lock.
#[derive(Debug, Default)]
pub(crate) struct RoundTrip {
    /// When you connected on your local clock, None while not connected.
    connected_at: Option<NaiveTime>,
    latest: Option<Duration>,
    recent: VecDeque<(Instant, Duration)>,
    stalls: VecDeque<Stall>,
    /// Every stall since you connected, kept or not.
    stall_count: usize,
}

impl RoundTrip {
    /// A connection starts at `at` on your local clock, with nothing read.
    pub(crate) fn connect(&mut self, at: NaiveTime) {
        *self = Self {
            connected_at: Some(at),
            ..Self::default()
        };
    }

    /// The connection ended, and its readings and stalls with it.
    pub(crate) fn disconnect(&mut self) {
        *self = Self::default();
    }

    /// Take `sample` at `now`, `at` on your local clock.
    pub(crate) fn record(&mut self, sample: Sample, now: Instant, at: NaiveTime) {
        let reading = sample.reading;
        self.latest = Some(reading);
        self.recent.push_back((now, reading));
        while self
            .recent
            .front()
            .is_some_and(|(then, _)| now.duration_since(*then) > USUAL_OVER)
        {
            self.recent.pop_front();
        }
        let running = self.stalls.back_mut().filter(|s| s.ended.is_none());
        match running {
            Some(stall) if reading >= SLOW => stall.worst = stall.worst.max(reading),
            Some(stall) => stall.ended = Some(now),
            None if reading >= SLOW => {
                // A wait began when its line left, but never before the
                // last stall ended, so stalls never overlap.
                let after = self.stalls.back().and_then(|s| s.ended);
                let began = sample.began.map_or(now, |b| after.map_or(b, |a| b.max(a)));
                let back =
                    chrono::TimeDelta::from_std(now.duration_since(began)).unwrap_or_default();
                self.stall_count += 1;
                self.stalls.push_back(Stall {
                    at: at - back,
                    began,
                    worst: reading,
                    ended: None,
                });
                if self.stalls.len() > KEPT_STALLS {
                    self.stalls.pop_front();
                }
            }
            None => {}
        }
    }

    /// The median of the readings of the last ten minutes.
    fn usual(&self) -> Option<Duration> {
        let mut readings: Vec<Duration> = self.recent.iter().map(|(_, d)| *d).collect();
        readings.sort_unstable();
        readings.get(readings.len() / 2).copied()
    }

    /// What `#lag` prints at `now`, plain lines like `#tick`.
    pub(crate) fn report(&self, now: Instant) -> Vec<String> {
        let Some(connected_at) = self.connected_at else {
            return vec!["you are not connected, so there is no round trip to show".into()];
        };
        let mut lines = vec![match (self.latest, self.usual()) {
            (Some(latest), Some(usual)) => format!(
                "round trip to the game {}, usually {} over the last 10 minutes",
                shown(latest),
                shown(usual)
            ),
            _ => format!(
                "no round trip yet, Vosh reads it every {} seconds",
                READ_EVERY.as_secs()
            ),
        }];
        let since = format!("since you connected at {}", connected_at.format("%H:%M"));
        lines.push(match self.stall_count {
            0 => format!("no stalls {since}"),
            1 => format!("1 stall {since}"),
            n if n > self.stalls.len() => {
                format!("{n} stalls {since}, the last {} here", self.stalls.len())
            }
            n => format!("{n} stalls {since}"),
        });
        for stall in &self.stalls {
            let at = stall.at.format("%H:%M:%S");
            lines.push(match stall.ended {
                Some(ended) => format!(
                    "  {at}  worst {}, lasted {}",
                    shown(stall.worst),
                    span(ended.duration_since(stall.began))
                ),
                None => format!(
                    "  {at}  {} now, {} so far",
                    shown(self.latest.unwrap_or(stall.worst)),
                    span(now.duration_since(stall.began))
                ),
            });
        }
        lines
    }
}

/// A reading as the status line and `#lag` show it: whole milliseconds
/// under a second, then seconds to one decimal, `38ms` or `1.4s`.
pub(crate) fn shown(reading: Duration) -> String {
    let ms = reading.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", reading.as_secs_f64())
    }
}

/// How long a stall lasted, in whole seconds, then minutes and seconds,
/// then hours and minutes.
fn span(d: Duration) -> String {
    let secs = (d.as_millis() + 500) / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m {}s", secs / 60, secs % 60),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn clock(h: u32, m: u32, s: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, s).expect("a time")
    }

    #[test]
    fn a_wait_counts_only_while_the_link_still_carries_your_line() {
        let sent = Instant::now();
        let now = sent + ms(1500);

        // The game's machine has not acknowledged the line, so the
        // network holds it and the wait is a stall.
        let mut waits = Waits::default();
        waits.sent(sent, true);
        assert_eq!(waits.reading(link(true), now).reading, ms(1500));
        assert_eq!(waits.link, Some(sent));

        // It has the line, and the game holds it while a skill lags you.
        // The reading is the link's, and the wait starts over.
        assert_eq!(waits.reading(link(false), now).reading, ms(38));
        assert_eq!(waits.link, None);

        // A wait shorter than the round trip reads the round trip.
        waits.sent(now - ms(10), true);
        assert_eq!(waits.reading(link(true), now).reading, ms(38));
    }

    /// The kernel reading 38 ms, with bytes in flight or none.
    fn link(in_flight: bool) -> kernel::Reading {
        kernel::Reading {
            round_trip: ms(38),
            in_flight,
        }
    }

    /// The waits of a session whose game shows its prompt, waiting for
    /// your next command.
    fn at_prompt() -> Waits {
        let mut waits = Waits::default();
        waits.prompted(true);
        waits
    }

    #[test]
    fn a_bash_with_lines_typed_ahead_reads_the_round_trip() {
        let start = Instant::now();
        let mut waits = at_prompt();
        // The game answers the bash at once and lags you six seconds.
        waits.sent(start, true);
        waits.heard();
        // You type kick and look, and the game's machine takes each.
        waits.sent(start + ms(400), true);
        waits.sent(start + ms(900), true);
        for i in 1..=3 {
            let reading = waits.reading(link(false), start + READ_EVERY * i).reading;
            assert_eq!(reading, ms(38));
            assert!(reading < SLOW);
        }
        // The lag ends and the game answers kick.
        waits.heard();
        assert_eq!(waits.reading(link(false), start + ms(6500)).reading, ms(38));
    }

    #[test]
    fn a_line_typed_while_one_waits_starts_no_wait_of_its_own() {
        let start = Instant::now();
        let mut waits = Waits::default();
        waits.sent(start, true);
        waits.sent(start + ms(800), false);
        assert_eq!(
            waits.reading(link(true), start + ms(1500)).reading,
            ms(1500)
        );

        // The game's machine took the first, so the link half starts
        // over with the second. The game half keeps the first.
        let mut waits = at_prompt();
        waits.sent(start, true);
        waits.sent(start + ms(5000), true);
        let later = start + HELD_AT_MOST + ms(1000);
        assert_eq!(
            waits.reading(link(false), later).reading,
            HELD_AT_MOST + ms(1000)
        );
    }

    #[test]
    fn a_link_that_holds_your_line_is_a_stall() {
        let start = Instant::now();
        let mut waits = Waits::default();
        waits.sent(start, true);
        let sample = waits.reading(link(true), start + ms(1500));
        assert_eq!(sample.reading, ms(1500));
        assert_eq!(sample.began, Some(start));

        // The stall began when the line left, a second and a half before
        // the reading caught it at 21:14:05.
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        trip.record(sample, start + ms(1500), clock(21, 14, 5));
        waits.heard();
        let answered = waits.reading(link(false), start + ms(3000));
        assert_eq!(answered, Sample::kernel(ms(38)));
        trip.record(answered, start + ms(3000), clock(21, 14, 6));
        assert_eq!(
            trip.report(start + ms(3000))[1..],
            [
                "1 stall since you connected at 19:42",
                "  21:14:03  worst 1.5s, lasted 3s",
            ]
        );
    }

    #[test]
    fn a_wait_stall_begins_no_earlier_than_the_last_stall_ended() {
        let start = Instant::now();
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        trip.record(Sample::kernel(ms(900)), start, clock(21, 14, 1));
        trip.record(Sample::kernel(ms(40)), start + ms(2000), clock(21, 14, 3));
        let wait = Sample {
            reading: ms(5000),
            began: Some(start),
        };
        trip.record(wait, start + ms(5000), clock(21, 14, 6));
        trip.record(Sample::kernel(ms(40)), start + ms(7000), clock(21, 14, 8));
        assert_eq!(
            trip.report(start + ms(7000))[2..],
            [
                "  21:14:01  worst 900ms, lasted 2s",
                "  21:14:03  worst 5.0s, lasted 5s",
            ]
        );
    }

    #[test]
    fn a_game_that_says_nothing_past_the_longest_lag_is_a_stall() {
        let start = Instant::now();
        let mut waits = at_prompt();
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        waits.sent(start, true);
        let mut now = start;
        for i in 1..=17 {
            now = start + READ_EVERY * i;
            let sample = waits.reading(link(false), now);
            if now.duration_since(start) > HELD_AT_MOST {
                assert_eq!(sample.reading, now.duration_since(start));
                assert_eq!(sample.began, Some(start));
            } else {
                assert_eq!(sample, Sample::kernel(ms(38)));
            }
            trip.record(sample, now, clock(21, 14, 2 * i));
        }
        // The stall lists when the line left at 21:14:00, not the
        // reading at 21:14:32 that first caught it.
        assert_eq!(
            trip.report(now),
            [
                "round trip to the game 34.0s, usually 38ms over the last 10 minutes",
                "1 stall since you connected at 19:42",
                "  21:14:00  34.0s now, 34s so far",
            ]
        );
    }

    #[test]
    fn a_line_the_game_answers_in_a_pulse_or_after_a_lag_is_no_stall() {
        let start = Instant::now();
        let mut waits = at_prompt();
        // A trigger sends a line, and the game answers in the pulse.
        waits.sent(start, true);
        waits.heard();
        assert_eq!(
            waits.reading(link(false), start + READ_EVERY).reading,
            ms(38)
        );

        // A timer sends a line while a 120 beat psalm lags you, the
        // longest lag in the skill table (const.c), and paralyze adds a
        // violence pulse (handler.c), 33 seconds in all. The game says
        // nothing until it ends.
        let sent = start + ms(3000);
        waits.sent(sent, true);
        for i in 1..=16 {
            assert_eq!(
                waits.reading(link(false), sent + READ_EVERY * i).reading,
                ms(38)
            );
        }
        assert_eq!(
            waits.reading(link(false), sent + ms(33_250)).reading,
            ms(38)
        );
        waits.heard();
        assert_eq!(
            waits.reading(link(false), sent + ms(33_500)).reading,
            ms(38)
        );
    }

    #[test]
    fn the_note_editor_ends_on_no_prompt_and_play_does() {
        use super::super::lines::LineAccumulator;
        let ends = |bytes: &[u8]| {
            let mut lines = LineAccumulator::new();
            lines.feed(bytes);
            ends_on_prompt(lines.partial())
        };
        // `note write` once you set the subject, as board.c sends it,
        // with the reset the game makes of its color code.
        assert!(!ends(
            b"\n\rEnter text. Type ~\x1b[0m or END\x1b[0m on an empty line to end note.\n\r\
            =======================================================\n\r\x1b[0m"
        ));
        assert!(ends(b"You see Maren here.\n\r\n\r<120hp 98m 210mv> "));
        assert!(ends(b"> "));
    }

    #[test]
    fn a_note_you_write_is_no_stall() {
        let start = Instant::now();
        let mut waits = at_prompt();
        // `note write` opens the editor, which ends on a line of equals
        // signs and shows no prompt (board.c), so the session says the
        // game's text ended on something else.
        waits.sent(start, true);
        waits.heard();
        waits.prompted(false);
        // You write three lines over a minute in a quiet room. The game
        // answers none of them and sends nothing else.
        for at in [2_000, 25_000, 50_000] {
            waits.sent(start + ms(at), true);
        }
        for i in 1..=40 {
            assert_eq!(
                waits.reading(link(false), start + READ_EVERY * i).reading,
                ms(38)
            );
        }
        // The `~` that ends the note gets an answer and the prompt.
        waits.sent(start + ms(80_000), true);
        waits.heard();
        waits.prompted(true);
        assert_eq!(
            waits.reading(link(false), start + ms(80_250)).reading,
            ms(38)
        );
    }

    #[test]
    fn anything_from_the_game_ends_the_wait() {
        let start = Instant::now();
        let mut waits = at_prompt();
        waits.sent(start, true);
        let later = start + HELD_AT_MOST + ms(4000);
        assert_eq!(
            waits.reading(link(true), later).reading,
            HELD_AT_MOST + ms(4000)
        );
        // A GMCP packet alone, with no text, is the game answering.
        waits.heard();
        assert_eq!(waits.reading(link(true), later).reading, ms(38));
    }

    /// A connection at 19:42 that took `readings`, one every two seconds
    /// from `start`, the first at 21:14:01. Returns the time of the last.
    fn run(trip: &mut RoundTrip, start: Instant, readings: &[u64]) -> Instant {
        let mut now = start;
        for (i, r) in readings.iter().enumerate() {
            now = start + READ_EVERY * i as u32;
            let secs = 1 + 2 * i as u32;
            trip.record(
                Sample::kernel(ms(*r)),
                now,
                clock(21, 14 + secs / 60, secs % 60),
            );
        }
        now
    }

    #[test]
    fn a_reading_shows_milliseconds_then_seconds_to_one_decimal() {
        assert_eq!(shown(ms(38)), "38ms");
        assert_eq!(shown(ms(999)), "999ms");
        assert_eq!(shown(ms(1000)), "1.0s");
        assert_eq!(shown(ms(1400)), "1.4s");
        assert_eq!(shown(ms(12_340)), "12.3s");
    }

    #[test]
    fn a_span_reads_seconds_then_minutes_then_hours() {
        assert_eq!(span(ms(6000)), "6s");
        assert_eq!(span(ms(5600)), "6s");
        assert_eq!(span(ms(125_000)), "2m 5s");
        assert_eq!(span(ms(3_720_000)), "1h 2m");
    }

    #[test]
    fn not_connected_says_so_in_one_line() {
        let trip = RoundTrip::default();
        assert_eq!(
            trip.report(Instant::now()),
            ["you are not connected, so there is no round trip to show"]
        );
    }

    #[test]
    fn before_the_first_reading_it_says_none_yet() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 5));
        assert_eq!(
            trip.report(Instant::now()),
            [
                "no round trip yet, Vosh reads it every 2 seconds",
                "no stalls since you connected at 19:42",
            ]
        );
    }

    #[test]
    fn a_fine_connection_has_no_stalls_and_the_usual_is_the_median() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        let now = run(&mut trip, Instant::now(), &[41, 40, 44, 41, 38]);
        assert_eq!(
            trip.report(now),
            [
                "round trip to the game 38ms, usually 41ms over the last 10 minutes",
                "no stalls since you connected at 19:42",
            ]
        );
    }

    #[test]
    fn a_stall_keeps_its_worst_and_how_long_it_lasted() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        // Slow from the third reading for three readings, so six seconds.
        let now = run(&mut trip, Instant::now(), &[40, 41, 900, 1400, 320, 44]);
        assert_eq!(
            trip.report(now),
            [
                "round trip to the game 44ms, usually 320ms over the last 10 minutes",
                "1 stall since you connected at 19:42",
                "  21:14:05  worst 1.4s, lasted 6s",
            ]
        );
    }

    #[test]
    fn a_running_stall_shows_the_reading_now_and_how_long_so_far() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        let now = run(
            &mut trip,
            Instant::now(),
            &[40, 612, 40, 41, 1400, 1200, 42, 44, 900, 1200],
        );
        assert_eq!(
            trip.report(now),
            [
                "round trip to the game 1.2s, usually 612ms over the last 10 minutes",
                "3 stalls since you connected at 19:42",
                "  21:14:03  worst 612ms, lasted 2s",
                "  21:14:09  worst 1.4s, lasted 4s",
                "  21:14:17  1.2s now, 2s so far",
            ]
        );
    }

    #[test]
    fn it_keeps_the_last_twenty_stalls_and_counts_them_all() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        let readings: Vec<u64> = (0..25).flat_map(|_| [500, 40]).collect();
        let now = run(&mut trip, Instant::now(), &readings);
        let report = trip.report(now);
        assert_eq!(
            report[1],
            "25 stalls since you connected at 19:42, the last 20 here"
        );
        assert_eq!(report.len(), 22);
        // The oldest kept is the sixth, which began at the 11th reading.
        assert_eq!(report[2], "  21:14:21  worst 500ms, lasted 2s");
    }

    #[test]
    fn the_usual_round_trip_forgets_readings_older_than_ten_minutes() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        let start = Instant::now();
        trip.record(Sample::kernel(ms(900)), start, clock(19, 43, 0));
        trip.record(Sample::kernel(ms(900)), start + ms(1000), clock(19, 43, 1));
        let later = start + USUAL_OVER + ms(1500);
        trip.record(Sample::kernel(ms(40)), later, clock(19, 53, 2));
        assert_eq!(
            trip.report(later)[0],
            "round trip to the game 40ms, usually 40ms over the last 10 minutes"
        );
    }

    #[test]
    fn a_new_connection_starts_afresh_and_a_disconnect_forgets() {
        let mut trip = RoundTrip::default();
        trip.connect(clock(19, 42, 0));
        run(&mut trip, Instant::now(), &[900, 40]);
        trip.connect(clock(20, 5, 0));
        assert_eq!(
            trip.report(Instant::now())[1],
            "no stalls since you connected at 20:05"
        );
        trip.disconnect();
        assert_eq!(trip.report(Instant::now()).len(), 1);
    }

    #[test]
    fn the_payload_carries_whole_milliseconds_or_null() {
        let json = |p: RoundTripPayload| serde_json::to_string(&p).expect("json");
        assert_eq!(json(RoundTripPayload::of(Some(ms(38)))), r#"{"ms":38}"#);
        assert_eq!(json(RoundTripPayload::of(None)), r#"{"ms":null}"#);
    }
}
