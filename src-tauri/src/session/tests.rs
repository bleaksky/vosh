//! The session's tests, one file per part of it, and the harness they
//! share. They sit inside `session` so they can drive its private steps,
//! and each file's glob import hands it those steps and the harness.
//!
//! The harness keeps two drivers. [`Session`] reads as the socket loop
//! does, so it finishes the stage at the end of each read and logs rows
//! under a session id. [`Wire`] does neither. The steps tests check what
//! each step hands on before that, so one driver for both would change
//! what they feed.
//!
//! The tests that build or read the native grid run on every platform,
//! since the grid builds for the tests even where the renderer does not.

use std::time::Duration;

use tokio::time::Instant;
use vosh_prompt::stage::Output;
use vosh_protocol::telnet::{
    codes as telnet_codes, option as telnet_option, Event as TelnetEvent, Parser,
};

use super::batch::*;
use super::connection::Connection;
use super::effects::*;
use super::gmcp::*;
use super::lines::{Line, LineAccumulator};
use super::log_sink::*;
use super::perf::*;
use super::prompt_view::*;
use super::steps::*;
use super::*;
use crate::output::OutputPayload;
use crate::profile::live::Profile;
use crate::prompt::take_config;

mod batch;
mod clock;
mod collapse;
mod effects;
mod gmcp;
mod log_kinds;
mod log_sink;
mod pointer;
mod preview;
mod prompt_table;
mod repaint;
mod right;
mod room;
mod show;
mod steps;
mod timers;
mod walk;

/// The PROMPT the fake Aabahran prints, the one the wire fixtures carry.
const CODES: &str = vosh_prompt::testkit::mud::PROMPT;
/// The PROMPT with no line end that `prompt all` prints.
const CODES_ALL: &str = vosh_prompt::testkit::mud::PROMPT_ALL;
/// Draws the hp the capture read, so each byte of a draw is known.
const HP: &str = "<%hp>";

/// A profile and its connection, whose prompt engine took the profile's
/// `[prompt]` table through [`crate::prompt::take_config`] as a load does.
type Live = (Profile, Connection);

/// A profile that reads Aabahran's codes `prompt` and draws `template`
/// over them while `draw` is on, with its connection, started the way a
/// connection starts it.
fn profile(prompt: &str, template: &str, draw: bool) -> Live {
    let mut p = Profile::default();
    let mut c = Connection::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig {
            draw,
            template: template.to_string(),
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: prompt.to_string(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        },
    );
    start_prompt(&mut p, &mut c, false);
    (p, c)
}

/// `live` with its prompt shown at `show`.
fn showing((mut p, mut c): Live, show: vosh_prompt::PromptShow) -> Live {
    show_at(&mut p, &mut c, show);
    (p, c)
}

/// Show your prompt at `show` from now on, as a Settings save does.
fn show_at(p: &mut Profile, c: &mut Connection, show: vosh_prompt::PromptShow) {
    let mut config = c.prompt.config().clone();
    config.show = show;
    take_config(p, c, config);
}

/// A synthetic socket read from fixtures/prompt/aabahran/wire.
fn wire_fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../fixtures/prompt/aabahran/wire/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Where to cut `bytes` in two: after every byte of text, and around
/// and inside each GMCP packet (in its IAC SB GMCP head, halfway
/// through its body, and between its IAC and SE). A cut anywhere else
/// in a packet's body reads the same as the one halfway through it.
fn cuts(bytes: &[u8]) -> Vec<usize> {
    let mut cuts = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 255 && bytes.get(i + 1) == Some(&250) {
            let end = bytes[i..]
                .windows(2)
                .position(|w| w == [255, 240])
                .map_or(bytes.len(), |p| i + p + 2);
            cuts.extend([i, i + 1, i + 2, i + 3, (i + end) / 2, end - 1]);
            i = end;
            continue;
        }
        cuts.push(i);
        i += 1;
    }
    cuts.retain(|&c| c > 0 && c < bytes.len());
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// The streams the pinned screens are checked on: every wire fixture,
/// and pulses the fake game writes back to back into one read, so two
/// prompts pin in one read at many of the cuts.
fn pinned_streams() -> Vec<(String, Vec<u8>, &'static str)> {
    use vosh_prompt::testkit::{Build, Mud, Options};
    let mut streams: Vec<(String, Vec<u8>, &'static str)> = vosh_prompt::testkit::wire::CASES
        .iter()
        .map(|case| {
            let prompt = if case.prompt == CODES_ALL {
                CODES_ALL
            } else {
                CODES
            };
            (case.name.to_string(), wire_fixture(case.name), prompt)
        })
        .collect();
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut quiet_tell = mud.login();
    quiet_tell.extend(mud.pulse_later("Quenby tells you 'back soon'"));
    streams.push(("login-then-tell".into(), quiet_tell, CODES));
    let mut fight = Vec::new();
    for write in mud.command("fight") {
        fight.extend(write.bytes);
    }
    fight.extend(mud.pulse_later("Your slash hits a Blackwatch guard."));
    fight.extend(mud.pulse_later("A Blackwatch guard's pierce misses you."));
    streams.push(("three-fight-pulses".into(), fight, CODES));
    let mut compact = Mud::playing(Options {
        compact: true,
        ..Options::new(Build::New)
    });
    let mut bytes = compact.login();
    bytes.extend(compact.pulse_later("Quenby tells you 'back soon'"));
    streams.push(("compact".into(), bytes, CODES));
    streams
}

/// One read's worth of what the session hands on: the output, the log
/// rows and the lines kept for scrollback, in order, and what triggers
/// asked to send. `gmcp` says GMCP packets came after the last prompt
/// the read brought, and `prompt` that it brought one, as the session
/// loop reads them for the late repaint.
#[derive(Debug, Default)]
struct Read {
    out: Output,
    log: Vec<String>,
    kept: Vec<Vec<u8>>,
    /// What Collapse repeated lines made of each kept line, and the region
    /// its run shows in, in the same order, None for a line it left alone.
    repeats: Vec<Option<crate::logs::KeptRun>>,
    sends: Vec<String>,
    gmcp: bool,
    prompt: bool,
}

/// The session's state for one connection, fed through its own steps.
struct Session {
    p: Profile,
    /// The prompt engine, your target and the room list.
    c: Connection,
    acc: LineAccumulator,
    parser: vosh_protocol::telnet::Parser,
    /// Output from elsewhere reached the terminal since the last read.
    other: bool,
}

impl Session {
    fn new((p, c): Live) -> Self {
        Self {
            p,
            c,
            acc: LineAccumulator::new(),
            parser: vosh_protocol::telnet::Parser::new(),
            other: false,
        }
    }

    /// Output from outside a read, as `emit_output` writes it: a tick
    /// warning, a timer's echo, a slash command's reply. The next read
    /// sees it landed.
    fn emitted(&mut self, bytes: &[u8]) -> Output {
        self.other = true;
        let mut out = Output::new(false);
        out.text(bytes);
        out
    }

    /// A new connection on the same profile, which starts the prompt over
    /// the way a connect does. Cheaper than a new profile, whose script
    /// engine takes a while to start.
    fn restart(&mut self) {
        start_prompt(&mut self.p, &mut self.c, false);
        self.acc = LineAccumulator::new();
        self.parser = vosh_protocol::telnet::Parser::new();
    }

    /// One socket read of raw wire bytes, then the end of the read and
    /// the hold's deadline before the next one.
    fn read(&mut self, data: &[u8]) -> Read {
        let mut batch = ReadBatch::new(std::mem::take(&mut self.other));
        let now = Instant::now();
        let mut kept = Vec::new();
        let mut repeats = Vec::new();
        let mut sends = Vec::new();
        let mut take = |step: LineStep, kept: &mut Vec<Vec<u8>>, repeats: &mut Vec<_>| {
            repeats.extend(step.scrollback.iter().map(|_| step.repeat));
            kept.extend(step.scrollback);
            sends.extend(step.result.sends);
        };
        for event in self.parser.feed(data) {
            match event {
                TelnetEvent::Data(bytes) => {
                    for line in self.acc.feed(&bytes) {
                        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                        for step in line_step(
                            &mut self.p,
                            &mut self.c,
                            &mut batch,
                            line,
                            plain,
                            now,
                            Some(1),
                        ) {
                            take(step, &mut kept, &mut repeats);
                        }
                    }
                }
                TelnetEvent::Subnegotiation { option, payload }
                    if option == telnet_option::GMCP =>
                {
                    batch.gmcp = true;
                    let msg = vosh_protocol::gmcp::parse(&payload).expect("every packet parses");
                    let _ = gmcp_step(&mut self.p, &mut self.c, &msg, now);
                }
                TelnetEvent::Command(byte)
                    if byte == telnet_codes::GA || byte == telnet_codes::EOR =>
                {
                    for step in marker_step(
                        &mut self.p,
                        &mut self.c,
                        &mut self.acc,
                        &mut batch,
                        now,
                        Some(1),
                    ) {
                        take(step, &mut kept, &mut repeats);
                    }
                }
                _ => {}
            }
        }
        if let Some(step) = partial_step(
            &mut self.p,
            &mut self.c,
            &mut self.acc,
            &mut batch,
            now,
            Some(1),
        ) {
            take(step, &mut kept, &mut repeats);
        }
        if batch.hold {
            hold_step(
                &self.p,
                &mut self.c,
                &mut self.acc,
                &mut batch.out,
                &mut batch.reader,
            );
        }
        self.c.prompt.stage.finish(&mut batch.out);
        Read {
            out: batch.out,
            log: batch.log.into_iter().map(|row| row.text).collect(),
            kept,
            repeats,
            sends,
            gmcp: batch.gmcp,
            prompt: batch.prompt,
        }
    }

    /// You send `line`: held lines let go first, then the send step.
    fn send(&mut self, line: &str) -> Read {
        let mut batch = ReadBatch::new(false);
        let mut kept = Vec::new();
        for step in let_go_held(
            &mut self.p,
            &mut self.c,
            &mut batch,
            Instant::now(),
            Some(1),
        ) {
            kept.extend(step.scrollback);
        }
        let _ = send_step(&mut self.c, &self.acc, format!("{line}\r\n").as_bytes(), 0);
        self.acc.forget_partial();
        Read {
            out: batch.out,
            log: batch.log.into_iter().map(|row| row.text).collect(),
            kept,
            ..Read::default()
        }
    }

    /// The webview wrote to the terminal itself, such as your echo, after
    /// everything the session sent.
    fn local_write(&mut self) {
        self.c.prompt.stage.local_write(u64::MAX);
    }

    /// The `[prompt]` table changed, so the open row repaints.
    fn repaint(&mut self) -> Output {
        repaint_step(&self.p, &mut self.c, false, Instant::now())
    }
}

/// A terminal's worth of the session: the profile, its connection and
/// the line accumulator, fed one read at a time through the same steps
/// the session runs.
struct Wire {
    p: Profile,
    /// The prompt engine, your target and the room list.
    c: Connection,
    acc: LineAccumulator,
    /// The telnet parser, for reads of raw wire bytes.
    parser: vosh_protocol::telnet::Parser,
    /// The first generation this wire hands out, so tests can name
    /// the marks by number.
    gen0: u64,
}

impl Wire {
    fn new((p, c): Live) -> Self {
        let mut wire = Self {
            p,
            c,
            acc: LineAccumulator::new(),
            parser: vosh_protocol::telnet::Parser::new(),
            gen0: 0,
        };
        wire.gen0 = wire.c.prompt.stage.next_gen();
        wire
    }

    /// The mark for the nth region this wire hands out, from 1.
    fn mark(&self, n: u64) -> Vec<u8> {
        vosh_prompt::stage::mark(self.gen0 + n)
    }

    /// One socket read: `data`, then `ga` when the read ends in a GA,
    /// then the end of the read. Returns what the terminal gets.
    fn read_with(&mut self, data: &[u8], ga: bool, other: bool) -> ReadBatch {
        let mut batch = ReadBatch::new(other);
        let now = tokio::time::Instant::now();
        for line in self.acc.feed(data) {
            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
            let _ = line_step(&mut self.p, &mut self.c, &mut batch, line, plain, now, None);
        }
        if ga {
            let _ = marker_step(
                &mut self.p,
                &mut self.c,
                &mut self.acc,
                &mut batch,
                now,
                None,
            );
        }
        let _ = partial_step(
            &mut self.p,
            &mut self.c,
            &mut self.acc,
            &mut batch,
            now,
            None,
        );
        // The hold's deadline passes before the next read.
        if batch.hold {
            hold_step(
                &self.p,
                &mut self.c,
                &mut self.acc,
                &mut batch.out,
                &mut batch.reader,
            );
        }
        batch
    }

    /// One socket read that leaves a partial waiting, as the session
    /// does until the next read or the deadline. Returns the batch.
    fn read_holding(&mut self, data: &[u8]) -> ReadBatch {
        let mut batch = ReadBatch::new(false);
        let now = tokio::time::Instant::now();
        for line in self.acc.feed(data) {
            let plain = vosh_protocol::ansi::plain_text(&line.bytes);
            let _ = line_step(&mut self.p, &mut self.c, &mut batch, line, plain, now, None);
        }
        let _ = partial_step(
            &mut self.p,
            &mut self.c,
            &mut self.acc,
            &mut batch,
            now,
            None,
        );
        batch
    }

    fn read(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
        self.read_with(data, false, false).out
    }

    fn read_ga(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
        self.read_with(data, true, false).out
    }

    /// You send a line. Held lines let go first, as in the session.
    fn send(&mut self) {
        let mut batch = ReadBatch::new(false);
        let _ = let_go_held(
            &mut self.p,
            &mut self.c,
            &mut batch,
            tokio::time::Instant::now(),
            None,
        );
        let _ = send_step(&mut self.c, &self.acc, b"look\r\n", 0);
        self.acc.forget_partial();
    }

    /// You send `line` now.
    fn send_line(&mut self, line: &str) {
        let _ = send_step(
            &mut self.c,
            &self.acc,
            format!("{line}\r\n").as_bytes(),
            super::now_ms(),
        );
        self.acc.forget_partial();
    }

    /// One socket read of raw wire bytes, through the telnet parser
    /// and the steps the session runs for each event: text through
    /// the Line pass, each GMCP packet through the GMCP step, and a
    /// GA or EOR through the marker step. Then the end of the read,
    /// and the hold's deadline before the next one.
    fn read_wire(&mut self, data: &[u8]) -> vosh_prompt::stage::Output {
        let mut batch = ReadBatch::new(false);
        let now = tokio::time::Instant::now();
        for event in self.parser.feed(data) {
            match event {
                TelnetEvent::Data(bytes) => {
                    for line in self.acc.feed(&bytes) {
                        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                        let _ =
                            line_step(&mut self.p, &mut self.c, &mut batch, line, plain, now, None);
                    }
                }
                TelnetEvent::Subnegotiation { option, payload }
                    if option == telnet_option::GMCP =>
                {
                    let msg = vosh_protocol::gmcp::parse(&payload).expect("every packet parses");
                    let _ = super::gmcp::gmcp_step(&mut self.p, &mut self.c, &msg, now);
                }
                TelnetEvent::Command(byte)
                    if byte == telnet_codes::GA || byte == telnet_codes::EOR =>
                {
                    let _ = marker_step(
                        &mut self.p,
                        &mut self.c,
                        &mut self.acc,
                        &mut batch,
                        now,
                        None,
                    );
                }
                _ => {}
            }
        }
        let _ = partial_step(
            &mut self.p,
            &mut self.c,
            &mut self.acc,
            &mut batch,
            now,
            None,
        );
        if batch.hold {
            hold_step(
                &self.p,
                &mut self.c,
                &mut self.acc,
                &mut batch.out,
                &mut batch.reader,
            );
        }
        batch.out
    }

    /// One socket read of `events` in order, as the session handles
    /// them, then the end of the read.
    fn read_events(&mut self, events: &[Ev]) -> vosh_prompt::stage::Output {
        let mut batch = ReadBatch::new(false);
        let now = tokio::time::Instant::now();
        for event in events {
            match event {
                Ev::Data(data) => {
                    for line in self.acc.feed(data) {
                        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                        let _ =
                            line_step(&mut self.p, &mut self.c, &mut batch, line, plain, now, None);
                    }
                }
                Ev::Ga => {
                    let _ = marker_step(
                        &mut self.p,
                        &mut self.c,
                        &mut self.acc,
                        &mut batch,
                        now,
                        None,
                    );
                }
            }
        }
        let _ = partial_step(
            &mut self.p,
            &mut self.c,
            &mut self.acc,
            &mut batch,
            now,
            None,
        );
        if batch.hold {
            hold_step(
                &self.p,
                &mut self.c,
                &mut self.acc,
                &mut batch.out,
                &mut batch.reader,
            );
        }
        batch.out
    }
}

/// One event of a socket read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ev {
    Data(Vec<u8>),
    Ga,
}

/// The JSON the webview gets for `out`, or nothing when the session
/// would send nothing.
fn payload(out: &Output) -> Option<String> {
    (!out.is_empty())
        .then(|| serde_json::to_string(&OutputPayload::from_output(out)).expect("it serializes"))
}

/// The rows a grid shows, trimmed, up to the last row that shows anything.
fn rows_of(grid: &crate::native::grid::TermGrid) -> Vec<String> {
    let mut rows: Vec<String> = (0..grid.screen_lines())
        .map(|line| grid.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// `bytes` as plain text, escape codes dropped.
fn plain(bytes: &[u8]) -> String {
    vosh_protocol::ansi::plain_text(bytes)
}

/// What `f` gives, and how many times the session drew your design while
/// it ran.
fn drawing<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let before = RENDERS.with(std::cell::Cell::get);
    let out = f();
    (out, RENDERS.with(std::cell::Cell::get) - before)
}

/// Standard base64 back to bytes, for the stored splits.
fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let clean: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        let n = chunk.iter().fold(0u32, |n, &c| {
            (n << 6) | if c == b'=' { 0 } else { value(c) }
        });
        let n = n << (6 * (4 - chunk.len()));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    out
}
