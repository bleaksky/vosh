//! Where your prompt shows, played through the session's own steps.
//!
//! A child of `session`, so it drives the same private steps the socket
//! loop runs: the Line pass, the GA step, the end of a read, a send, a
//! local write and a repaint. The golden test holds the payloads the
//! webview gets with your prompt in the text to the ones this build sent
//! before the other two choices existed, byte for byte.

use super::*;

/// The PROMPT the fake Aabahran prints, the one the wire fixtures carry.
const CODES: &str = vosh_prompt::testkit::mud::PROMPT;
/// The PROMPT with no line end that `prompt all` prints.
const CODES_ALL: &str = vosh_prompt::testkit::mud::PROMPT_ALL;
/// Draws the hp the capture read, so each byte of a draw is known.
const HP: &str = "<%hp>";

/// A profile that reads Aabahran's codes `prompt` and draws `template`
/// over them while `draw` is on, started the way a connection starts it.
fn profile(prompt: &str, template: &str, draw: bool) -> Profile {
    let mut p = Profile::default();
    p.set_prompt_config(vosh_prompt::PromptConfig {
        draw,
        template: template.to_string(),
        capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
            prompt: prompt.to_string(),
            ..vosh_prompt::config::AabahranCapture::default()
        }),
        ..vosh_prompt::PromptConfig::default()
    });
    start_prompt(&mut p, false);
    p
}

/// A synthetic socket read from fixtures/prompt/aabahran/wire.
fn wire_fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../fixtures/prompt/aabahran/wire/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Where to cut `bytes` in two: after every byte of text, and around and
/// inside each GMCP packet, as the session tests cut them.
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

/// One read's worth of what the session hands on: the output, the log
/// rows and the lines kept for scrollback, in order.
#[derive(Debug, Default)]
struct Read {
    out: Output,
    log: Vec<String>,
    kept: Vec<Vec<u8>>,
}

/// The session's state for one connection, fed through its own steps.
struct Session {
    p: Profile,
    acc: LineAccumulator,
    parser: vosh_telnet::Parser,
}

impl Session {
    fn new(p: Profile) -> Self {
        Self {
            p,
            acc: LineAccumulator::new(),
            parser: vosh_telnet::Parser::new(),
        }
    }

    /// One socket read of raw wire bytes, then the end of the read and
    /// the hold's deadline before the next one.
    fn read(&mut self, data: &[u8]) -> Read {
        let mut batch = ReadBatch::new(0);
        batch.out = Output::new(false);
        let now = Instant::now();
        let mut kept = Vec::new();
        for event in self.parser.feed(data) {
            match event {
                TelnetEvent::Data(bytes) => {
                    for line in self.acc.feed(&bytes) {
                        let plain = vosh_ansi::plain_text(&line.bytes);
                        for step in line_step(&mut self.p, &mut batch, line, plain, now, Some(1)) {
                            kept.extend(step.scrollback);
                        }
                    }
                }
                TelnetEvent::Subnegotiation { option, payload }
                    if option == telnet_option::GMCP =>
                {
                    let msg = vosh_gmcp::parse(&payload).expect("every packet parses");
                    let _ = gmcp_step(&mut self.p, &msg, now);
                }
                TelnetEvent::Command(byte)
                    if byte == telnet_codes::GA || byte == telnet_codes::EOR =>
                {
                    for step in marker_step(&mut self.p, &mut self.acc, &mut batch, now, Some(1)) {
                        kept.extend(step.scrollback);
                    }
                }
                _ => {}
            }
        }
        if let Some(step) = partial_step(&mut self.p, &mut self.acc, &mut batch, now, Some(1)) {
            kept.extend(step.scrollback);
        }
        if batch.hold {
            hold_step(&mut self.p, &mut self.acc, &mut batch.out);
        }
        self.p.prompt.stage.finish(&batch.out);
        Read {
            out: batch.out,
            log: batch.log.into_iter().map(|row| row.text).collect(),
            kept,
        }
    }

    /// You send `line`: held lines let go first, then the send step.
    fn send(&mut self, line: &str) -> Read {
        let mut batch = ReadBatch::new(0);
        batch.out = Output::new(false);
        let mut kept = Vec::new();
        for step in let_go_held(&mut self.p, &mut batch, Instant::now(), Some(1)) {
            kept.extend(step.scrollback);
        }
        let _ = send_step(&mut self.p, &self.acc, format!("{line}\r\n").as_bytes(), 0);
        self.acc.forget_partial();
        Read {
            out: batch.out,
            log: batch.log.into_iter().map(|row| row.text).collect(),
            kept,
        }
    }

    /// The webview wrote to the terminal itself, such as your echo.
    fn local_write(&mut self) {
        self.p.prompt.stage.close();
    }

    /// The `[prompt]` table changed, so the open row repaints.
    fn repaint(&mut self) -> Output {
        repaint_step(&mut self.p, false, Instant::now())
    }
}

/// FNV-1a over `bytes`, folded into `hash`. Fixed, so a digest taken on
/// one toolchain holds on the next.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// The JSON the webview gets for `out`, or nothing when the session
/// would send nothing.
fn payload(out: &Output) -> Option<String> {
    (!out.is_empty())
        .then(|| serde_json::to_string(&OutputPayload::from_output(out)).expect("it serializes"))
}

/// Every payload a session sends for `reads` of `bytes`, cut after each
/// offset in `at`, with the log rows and the kept lines, folded into one
/// digest.
fn digest_reads(hash: u64, profile: &dyn Fn() -> Profile, bytes: &[u8], at: &[usize]) -> u64 {
    let mut session = Session::new(profile());
    let mut hash = hash;
    for read in vosh_prompt::testkit::reads(bytes, at) {
        let read = session.read(read);
        hash = fnv(hash, payload(&read.out).unwrap_or_default().as_bytes());
        hash = fnv(hash, b"\x00");
        for row in &read.log {
            hash = fnv(hash, row.as_bytes());
            hash = fnv(hash, b"\x01");
        }
        for line in &read.kept {
            hash = fnv(hash, line);
            hash = fnv(hash, b"\x02");
        }
    }
    hash
}

/// One read, and two cut at every place [`cuts`] names, folded into one
/// digest for the fixture.
fn digest_fixture(profile: &dyn Fn() -> Profile, name: &str) -> u64 {
    let bytes = wire_fixture(name);
    let mut hash = digest_reads(FNV_START, profile, &bytes, &[]);
    for at in cuts(&bytes) {
        hash = digest_reads(hash, profile, &bytes, &[at]);
    }
    hash
}

/// A short play: a quiet pulse, your `look`, its reply and prompt, a
/// fight pulse, text you did not ask for, Enter on an empty line, a
/// design change that repaints, drawing off, and a repaint with nothing
/// open. Every payload, log row and kept line, folded into one digest.
fn digest_play(template: &str, draw: bool) -> u64 {
    let mut session = Session::new(profile(CODES, template, draw));
    let mut hash = FNV_START;
    let take = |hash: &mut u64, read: &Read| {
        *hash = fnv(*hash, payload(&read.out).unwrap_or_default().as_bytes());
        *hash = fnv(*hash, b"\x00");
        for row in &read.log {
            *hash = fnv(*hash, row.as_bytes());
        }
        for line in &read.kept {
            *hash = fnv(*hash, line);
        }
    };
    let quiet = wire_fixture("quiet");
    let read = session.read(&quiet);
    take(&mut hash, &read);
    let read = session.send("look");
    take(&mut hash, &read);
    session.local_write();
    let read = session.read(
        b"The Bank of Aabahran\n\r[Exits: south]\n\r\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9",
    );
    take(&mut hash, &read);
    let read = session.read(&wire_fixture("fight-tank"));
    take(&mut hash, &read);
    let read = session.read(
        b"\n\rTarvik tells you 'back soon'\n\r\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9",
    );
    take(&mut hash, &read);
    let read = session.send("");
    take(&mut hash, &read);
    let read = session.read(b"\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9");
    take(&mut hash, &read);
    let mut config = session.p.prompt.config().clone();
    config.template = "hp %hp> ".into();
    session.p.set_prompt_config(config);
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    let mut config = session.p.prompt.config().clone();
    config.draw = !config.draw;
    session.p.set_prompt_config(config);
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    session.local_write();
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    hash
}

/// The digests this build sent with your prompt in the text, taken at
/// c740298 before `[prompt] show` existed. A change to any payload, log
/// row or kept line in the default moves one of them.
const TODAY: &[(&str, u64)] = &[
    ("quiet/draw", 0x6b7a_023c_8b04_abfd),
    ("quiet/off", 0x2b93_76d7_c674_7515),
    ("fight-tank/draw", 0xc44a_b93c_9e2c_0a87),
    ("fight-tank/off", 0x885b_db18_633e_e5d9),
    ("lament-new/draw", 0xdc59_7629_5335_37a8),
    ("lament-new/off", 0xf332_2494_56e3_44bc),
    ("lament-243cac5c/draw", 0xf917_f52c_57c8_7d18),
    ("lament-243cac5c/off", 0x5a37_7528_0160_dcec),
    ("lament-older/draw", 0xe73a_35ff_086f_4382),
    ("lament-older/off", 0x97a4_0c9d_3f96_6357),
    ("prompt-all-next/draw", 0x4725_06ae_9099_82dd),
    ("prompt-all-next/off", 0x3e7e_727e_1ce8_7e5c),
    ("ga/draw", 0x78fb_4fb1_a66d_dbbf),
    ("ga/off", 0xc7e3_d17a_0134_5d9d),
    ("login-new/draw", 0x32ea_6a55_902e_d2e0),
    ("login-new/off", 0x7944_f589_c34a_31d6),
    ("prompt-x-new/draw", 0x1bad_38ad_84be_c30e),
    ("prompt-x-new/off", 0xfbe5_12b8_1b87_5e7c),
    ("prompts-off-new/draw", 0x76ce_32ae_ec99_c147),
    ("prompts-off-new/off", 0x76ce_32ae_ec99_c147),
    ("play/draw", 0x6254_1a27_8206_5293),
    ("play/off", 0x92b6_97ce_7920_2a31),
    ("play/wide", 0x0178_5623_acd2_18ea),
    ("fake/draw", 0xf420_67e4_71b0_4f7c),
    ("fake/off", 0xf136_3c56_a088_17c2),
];

fn digests() -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for case in vosh_prompt::testkit::wire::CASES {
        let prompt = if case.prompt == CODES_ALL {
            CODES_ALL
        } else {
            CODES
        };
        for (draw, label) in [(true, "draw"), (false, "off")] {
            let make = || profile(prompt, HP, draw);
            out.push((
                format!("{}/{label}", case.name),
                digest_fixture(&make, case.name),
            ));
        }
    }
    out.push(("play/draw".into(), digest_play(HP, true)));
    out.push(("play/off".into(), digest_play(HP, false)));
    out.push(("play/wide".into(), digest_play("%hp/%maxhp hp ", true)));
    for (draw, label) in [(true, "draw"), (false, "off")] {
        let steps = fake_play(profile(CODES, DETAILED, draw));
        out.push((format!("fake/{label}"), digest_steps(&steps)));
    }
    out
}

/// The Detailed preset of the prompt editor, which ends on a piece.
const DETAILED: &str = "%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %opponent_cond%nl%{end}%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv %{c:8}tick%c_default %tick%{if:exits} %{c:8}[%c_default%exits%{c:8}]%c_default%{end} %{gold}g%{if:missing} %c_3%missing missing%c_default%{end}";

/// One step of a scripted play: a socket read's payload, or a line you
/// send, which the webview echoes before the next read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Output(Option<String>),
    Send(String),
}

/// A few minutes of play on the fake Aabahran, new build, in its wire
/// order: the login pulse, `look`, a tell you did not ask for, `fight`,
/// two combat rounds, Enter on an empty line, then `fight` again to end
/// it and someone arriving.
fn fake_play(p: Profile) -> Vec<Step> {
    use vosh_prompt::testkit::{Build, Mud, Options};
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut session = Session::new(p);
    let mut steps = Vec::new();
    let read = |session: &mut Session, bytes: &[u8], steps: &mut Vec<Step>| {
        steps.push(Step::Output(payload(&session.read(bytes).out)));
    };
    let login = mud.login();
    read(&mut session, &login, &mut steps);
    let send = |session: &mut Session, mud: &mut Mud, line: &str, steps: &mut Vec<Step>| {
        steps.push(Step::Send(line.to_string()));
        let _ = session.send(line);
        session.local_write();
        for write in mud.command(line) {
            read(session, &write.bytes, steps);
        }
    };
    send(&mut session, &mut mud, "look", &mut steps);
    let tell = mud.pulse_later("Tarvik tells you 'grabbing my bank box, back soon'");
    read(&mut session, &tell, &mut steps);
    send(&mut session, &mut mud, "fight", &mut steps);
    for round in [
        "Your slash hits a Blackwatch guard.",
        "A Blackwatch guard's pierce misses you.",
    ] {
        let bytes = mud.pulse_later(round);
        read(&mut session, &bytes, &mut steps);
    }
    send(&mut session, &mut mud, "", &mut steps);
    send(&mut session, &mut mud, "fight", &mut steps);
    let arrives = mud.pulse_later(vosh_prompt::testkit::wire::ARRIVES);
    read(&mut session, &arrives, &mut steps);
    steps
}

/// The fake play's steps folded into one digest.
fn digest_steps(steps: &[Step]) -> u64 {
    let mut hash = FNV_START;
    for step in steps {
        match step {
            Step::Output(json) => hash = fnv(hash, json.as_deref().unwrap_or("").as_bytes()),
            Step::Send(line) => hash = fnv(hash, line.as_bytes()),
        }
        hash = fnv(hash, b"\x00");
    }
    hash
}

/// Write each scripted play as JSON for the screenshot harness, when
/// `VOSH_WRITE_PLAYS` names a folder. Nothing otherwise.
#[test]
fn write_plays_for_the_screenshot_harness() {
    let Ok(dir) = std::env::var("VOSH_WRITE_PLAYS") else {
        return;
    };
    let plays = [
        ("text-detailed", profile(CODES, DETAILED, true)),
        ("text-off", profile(CODES, DETAILED, false)),
    ];
    for (name, p) in plays {
        let steps: Vec<serde_json::Value> = fake_play(p)
            .into_iter()
            .map(|step| match step {
                Step::Output(json) => serde_json::json!({
                    "output": json.map(|j| serde_json::from_str::<serde_json::Value>(&j).expect("json")),
                }),
                Step::Send(line) => serde_json::json!({ "send": line }),
            })
            .collect();
        let path = std::path::Path::new(&dir).join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&steps).expect("json"))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn in_the_text_every_payload_log_row_and_kept_line_stays_as_today() {
    let now = digests();
    let moved: Vec<String> = now
        .iter()
        .filter(|(name, hash)| !TODAY.contains(&(name.as_str(), *hash)))
        .map(|(name, hash)| format!("{name} is now 0x{hash:016x}"))
        .collect();
    assert!(moved.is_empty(), "{moved:#?}");
    assert_eq!(now.len(), TODAY.len());
}
