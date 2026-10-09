//! Where your prompt shows, played through the session's own steps.
//!
//! Inside `session`, so it drives the same private steps the socket
//! loop runs: the Line pass, the GA step, the end of a read, a send, a
//! local write and a repaint. The golden test holds the payloads the
//! webview gets with your prompt in the text to the ones this build sent
//! before the other two choices existed, byte for byte.

use super::*;
use crate::output::{base64_encode, OutputPayload};
use vosh_prompt::testkit::designs::DETAILED;

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

/// Every payload a session sends for `reads` of `bytes`, cut after each
/// offset in `at`, with the log rows and the kept lines, folded into one
/// digest.
fn digest_reads(hash: u64, profile: &dyn Fn() -> Live, bytes: &[u8], at: &[usize]) -> u64 {
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
fn digest_fixture(profile: &dyn Fn() -> Live, name: &str) -> u64 {
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
        b"\n\rQuenby tells you 'back soon'\n\r\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9",
    );
    take(&mut hash, &read);
    let read = session.send("");
    take(&mut hash, &read);
    let read = session.read(b"\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9");
    take(&mut hash, &read);
    let mut config = session.c.prompt.config().clone();
    config.template = "hp %hp> ".into();
    take_config(&mut session.p, &mut session.c, config);
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    let mut config = session.c.prompt.config().clone();
    config.draw = !config.draw;
    take_config(&mut session.p, &mut session.c, config);
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    session.local_write();
    let out = session.repaint();
    hash = fnv(hash, payload(&out).unwrap_or_default().as_bytes());
    hash
}

/// The digests this build sent with your prompt in the text, taken at
/// c740298 before `[prompt] show` existed. A change to any payload, log
/// row or kept line in the default moves one of them. The play and fake
/// digests were taken again when the tell in those plays came from an
/// invented name of the same length, which moves no other byte.
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
    ("play/draw", 0xb6b1_2efa_a899_905a),
    ("play/off", 0xa85b_aac6_79e2_64ee),
    ("play/wide", 0x3540_8029_97dc_bbd7),
    ("fake/draw", 0x3bef_33ab_eb32_df7b),
    ("fake/off", 0x4ce7_97fb_5fe8_d121),
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
fn fake_play(live: Live) -> Vec<Step> {
    use vosh_prompt::testkit::{Build, Mud, Options};
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut session = Session::new(live);
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
    let tell = mud.pulse_later("Quenby tells you 'grabbing my bank box, back soon'");
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

/// A Prompts trigger that asks to send `seen` for every prompt, so a test
/// can count what Prompts triggers saw.
fn counting((mut p, c): Live) -> Live {
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            target: vosh_automation::trigger::TriggerTarget::Prompt,
            ..vosh_automation::trigger::Trigger::new(
                "count",
                ".",
                vosh_automation::trigger::TriggerAction::Send {
                    template: "seen".into(),
                },
            )
        })
        .expect("the trigger compiles");
    (p, c)
}

/// Everything a session hands on for `bytes` cut at `at`, read by the
/// profile `make` gives: the payloads, the log rows, the kept lines and
/// what Prompts triggers sent.
fn play_reads(make: &dyn Fn() -> Live, bytes: &[u8], at: &[usize]) -> Vec<Read> {
    replay(&mut Session::new(make()), bytes, at)
}

/// [`play_reads`] on a new connection of `session`.
fn replay(session: &mut Session, bytes: &[u8], at: &[usize]) -> Vec<Read> {
    session.restart();
    vosh_prompt::testkit::reads(bytes, at)
        .into_iter()
        .map(|read| session.read(read))
        .collect()
}

/// The plain text of every byte the reads wrote to the text, held line
/// ends included.
fn text_of(reads: &[Read]) -> String {
    let mut bytes = Vec::new();
    for read in reads {
        if let Some(replace) = &read.out.replace {
            bytes.extend_from_slice(&replace.bytes);
        }
        bytes.extend_from_slice(&read.out.bytes);
        bytes.extend_from_slice(&read.out.hold);
    }
    vosh_protocol::ansi::plain_text(&bytes)
}

#[test]
fn pinned_logs_keeps_and_triggers_every_prompt_as_the_text_does_at_every_split() {
    use vosh_prompt::PromptShow;
    for case in vosh_prompt::testkit::wire::CASES {
        let prompt = if case.prompt == CODES_ALL {
            CODES_ALL
        } else {
            CODES
        };
        let bytes = wire_fixture(case.name);
        for draw in [true, false] {
            let text = || counting(profile(prompt, HP, draw));
            let pinned = || showing(counting(profile(prompt, HP, draw)), PromptShow::Pinned);
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            let mut in_text = Session::new(text());
            let mut in_band = Session::new(pinned());
            for at in &splits {
                let want = replay(&mut in_text, &bytes, at);
                let got = replay(&mut in_band, &bytes, at);
                let label = format!("{} draw {draw} cut {at:?}", case.name);
                let log =
                    |reads: &[Read]| reads.iter().flat_map(|r| r.log.clone()).collect::<Vec<_>>();
                let kept = |reads: &[Read]| {
                    reads
                        .iter()
                        .flat_map(|r| r.kept.clone())
                        .collect::<Vec<_>>()
                };
                let sends = |reads: &[Read]| {
                    reads
                        .iter()
                        .flat_map(|r| r.sends.clone())
                        .collect::<Vec<_>>()
                };
                assert_eq!(log(&got), log(&want), "log rows, {label}");
                assert_eq!(kept(&got), kept(&want), "kept lines, {label}");
                assert_eq!(sends(&got), sends(&want), "Prompts triggers, {label}");
                // In one read no prompt reaches the text: not the design,
                // not the game's prompt, not the tank line above it. A
                // read that ends partway through a prompt paints that
                // part, which the next one erases, so the renderer tests
                // hold the screens of split reads.
                if at.is_empty() {
                    let shown = text_of(&got);
                    for prompt_text in ["<1020>", "<765>", "<?>", "mv]", "mv> ", "Tester:"] {
                        assert!(
                            !shown.contains(prompt_text),
                            "{prompt_text} in {shown:?}, {label}"
                        );
                    }
                }
            }
            // One read's prompts reach the band, one Prompts trigger send
            // each.
            let got = play_reads(&pinned, &bytes, &[]);
            let pins: Vec<&Vec<u8>> = got.iter().filter_map(|r| r.out.pin.as_ref()).collect();
            let recognized = got.iter().map(|r| r.sends.len()).sum::<usize>();
            if case.name == "prompts-off-new" {
                assert!(pins.is_empty(), "{}", case.name);
            } else {
                assert!(!pins.is_empty(), "{} draw {draw}", case.name);
                assert!(recognized >= 1, "{} draw {draw}", case.name);
            }
        }
    }
}

#[test]
fn a_pinned_band_shows_the_design_or_the_game_prompt_with_its_tank_line() {
    use vosh_prompt::PromptShow;
    let fight = wire_fixture("fight-tank");
    let pinned = |draw| showing(profile(CODES, HP, draw), PromptShow::Pinned);
    let reads = play_reads(&|| pinned(true), &fight, &[]);
    let pin = reads[0].out.pin.clone().expect("the band");
    // The design reads nothing on the tank line, so it shows as sent.
    assert_eq!(
        vosh_protocol::ansi::plain_text(&pin),
        "Tester: [===|===|===|---]\r\n<765>"
    );
    assert_eq!(
        text_of(&reads),
        "A Blackwatch guard attacks you!\r\nA Blackwatch guard has quite a few wounds. \r\n\r\n"
    );
    assert!(reads[0].out.bytes.ends_with(b"wounds. "));
    assert_eq!(reads[0].out.hold, b"\r\n\r\n");
    // The band carries where each piece of the design landed on it,
    // under the tank line, and so does the payload.
    assert_eq!(
        band_pieces(&reads[0].out),
        Some(vec![(0, 1, 0, 1), (1, 1, 1, 3), (2, 1, 4, 1)])
    );
    let sent: serde_json::Value =
        serde_json::from_str(&payload(&reads[0].out).expect("a payload")).expect("json");
    assert_eq!(sent["pin_spans"][1]["piece"], 1);
    assert_eq!(sent["pin_spans"][1]["row"], 1);
    assert_eq!(sent["pin_spans"][1]["width"], 3);
    // A design that reads the tank takes the whole band.
    let make = || {
        showing(
            profile(CODES, "%tank %{tank_hp:pct}%% <%hp>", true),
            PromptShow::Pinned,
        )
    };
    let reads = play_reads(&make, &fight, &[]);
    let pin = reads[0].out.pin.clone().expect("the band");
    assert_eq!(vosh_protocol::ansi::plain_text(&pin), "Tester 75% <765>");
    assert_eq!(
        band_pieces(&reads[0].out).map(|pieces| pieces.iter().map(|p| p.1).max()),
        Some(Some(0)),
        "every piece on the first row"
    );
    // Drawing off, the band holds the game's lines as sent, with no
    // pieces in it.
    let reads = play_reads(&|| pinned(false), &fight, &[]);
    let pin = reads[0].out.pin.clone().expect("the band");
    assert_eq!(
        vosh_protocol::ansi::plain_text(&pin),
        "Tester: [===|===|===|---]\r\n[765/1020hp 800/800mn 930/930mv]"
    );
    assert_eq!(reads[0].out.pin_spans, None);
    let sent: serde_json::Value =
        serde_json::from_str(&payload(&reads[0].out).expect("a payload")).expect("json");
    assert!(sent.get("pin_spans").is_none());
}

/// A design that draws the tank line itself, as Same as the game does.
const TANK_DESIGN: &str = "%{if:tank}%tank: %{tank_hp:game}%nl%{end}<%hp>";

/// Rows that show the tank line, drawn or as sent.
fn tank_rows(rows: &[String]) -> usize {
    rows.iter().filter(|r| r.starts_with("Tester:")).count()
}

/// Take `template` as the design, as the card saves an edit, and repaint.
fn edit_design(session: &mut Session, template: &str) -> Output {
    let mut config = session.c.prompt.config().clone();
    config.template = template.to_string();
    take_config(&mut session.p, &mut session.c, config);
    session.repaint()
}

#[test]
fn an_edit_that_starts_or_stops_reading_the_tank_line_shows_it_once() {
    use vosh_prompt::PromptShow;
    let fight = wire_fixture("fight-tank");
    for show in [PromptShow::Text, PromptShow::Lifted] {
        for (from, to) in [(HP, TANK_DESIGN), (TANK_DESIGN, HP)] {
            let label = format!("{show:?} from {from:?} to {to:?}");
            let mut session = Session::new(showing(profile(CODES, from, true), show));
            let mut grid = crate::native::grid::TermGrid::new(80, 40);
            grid.session_output(&session.read(&fight).out);
            assert_eq!(tank_rows(&rows_of(&grid)), 1, "{label}");
            let out = edit_design(&mut session, to);
            grid.session_output(&out);
            let rows = rows_of(&grid);
            assert_eq!(tank_rows(&rows), 1, "{label}: {rows:?}");
            assert_eq!(rows.last().map(String::as_str), Some("<765>"), "{label}");
            // Your echo lands after it, and history keeps it once.
            session.local_write();
            grid.local_write(b"look\r\n");
            assert_eq!(tank_rows(&rows_of(&grid)), 1, "{label}");
            // The design the edit made reads the fight as a fresh read of
            // the same prompt would.
            let mut fresh = Session::new(showing(profile(CODES, to, true), show));
            let mut want = crate::native::grid::TermGrid::new(80, 40);
            want.session_output(&fresh.read(&fight).out);
            assert_eq!(rows, rows_of(&want), "{label}");
        }
    }
    // Pinned, the band shows it once.
    for (from, to) in [(HP, TANK_DESIGN), (TANK_DESIGN, HP)] {
        let label = format!("pinned from {from:?} to {to:?}");
        let mut session = Session::new(showing(profile(CODES, from, true), PromptShow::Pinned));
        let _ = session.read(&fight);
        let out = edit_design(&mut session, to);
        let band = vosh_protocol::ansi::plain_text(out.pin.as_deref().expect("the band"));
        let rows: Vec<String> = band.split("\r\n").map(str::to_string).collect();
        assert_eq!(tank_rows(&rows), 1, "{label}: {rows:?}");
        let mut fresh = Session::new(showing(profile(CODES, to, true), PromptShow::Pinned));
        let want = fresh.read(&fight).out;
        assert_eq!(out.pin, want.pin, "{label}");
        assert_eq!(out.pin_spans, want.pin_spans, "{label}");
    }
}

/// The pieces on the band `out` shows: piece, row, column and width.
fn band_pieces(out: &Output) -> Option<Vec<(usize, usize, usize, usize)>> {
    out.pin_spans.as_ref().map(|spans| {
        spans
            .iter()
            .map(|s| (s.piece, s.row, s.col, s.width))
            .collect()
    })
}

#[test]
fn a_prompts_trigger_that_hides_the_prompt_leaves_the_band_empty() {
    use vosh_prompt::PromptShow;
    let (mut p, c) = showing(profile(CODES, HP, false), PromptShow::Pinned);
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            target: vosh_automation::trigger::TriggerTarget::Prompt,
            ..vosh_automation::trigger::Trigger::new(
                "hide",
                "hp",
                vosh_automation::trigger::TriggerAction::Gag,
            )
        })
        .expect("the trigger compiles");
    let mut session = Session::new((p, c));
    let read = session.read(&wire_fixture("quiet"));
    assert_eq!(read.out.pin.as_deref(), Some(&b""[..]));
}

#[test]
fn enter_on_an_empty_line_while_pinned_moves_nothing_and_updates_the_band() {
    use vosh_prompt::testkit::{Build, Mud, Options};
    use vosh_prompt::PromptShow;
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut session = Session::new(showing(profile(CODES, HP, true), PromptShow::Pinned));
    let login = session.read(&mud.login());
    assert!(!login.out.hold.is_empty(), "expected entries");
    assert!(session.c.prompt.stage.swallows(), "armed after login");
    // Enter on an empty line: the webview echoes nothing while pinned,
    // so only the send reaches the session.
    let _ = session.send("");
    assert!(session.c.prompt.stage.swallows(), "armed after the send");
    for write in mud.command("") {
        let read = session.read(&write.bytes);
        assert!(read.out.bytes.is_empty(), "{:?}", read.out);
        let leftover = &read.out.hold;
        assert!(leftover.is_empty(), "{leftover:?}");
        assert!(read.out.replace.is_none());
        assert!(read.out.pin.is_some());
        assert!(!read.out.is_empty(), "the band still goes out");
    }
    // A typed line lands where the prompt was, and its reply follows.
    let _ = session.send("look");
    session.local_write();
    let reply = mud.command("look");
    let read = session.read(&reply[0].bytes);
    assert!(read.out.bytes.starts_with(b"The Bank of Aabahran"));
}

#[test]
fn a_repaint_while_pinned_goes_to_the_band_and_a_change_of_place_moves_the_prompt() {
    use vosh_prompt::PromptShow;
    let mut session = Session::new(profile(CODES, HP, true));
    let _ = session.read(&wire_fixture("quiet"));
    assert!(session.c.prompt.stage.open_row().is_some());
    // You choose Pinned: the open row is erased and goes to the band.
    show_at(&mut session.p, &mut session.c, PromptShow::Pinned);
    let out = session.repaint();
    assert!(out
        .replace
        .as_ref()
        .is_some_and(|r| r.bytes.is_empty() && !r.fresh));
    assert_eq!(
        out.pin
            .as_deref()
            .map(vosh_protocol::ansi::plain_text)
            .as_deref(),
        Some("<1020>")
    );
    // A design change only redraws the band.
    let mut config = session.c.prompt.config().clone();
    config.template = "hp %hp> ".into();
    take_config(&mut session.p, &mut session.c, config);
    let out = session.repaint();
    assert!(out.bytes.is_empty() && out.replace.is_none());
    assert_eq!(
        out.pin
            .as_deref()
            .map(vosh_protocol::ansi::plain_text)
            .as_deref(),
        Some("hp 1020> ")
    );
    assert_eq!(
        band_pieces(&out),
        Some(vec![(0, 0, 0, 3), (1, 0, 3, 4), (2, 0, 7, 2)])
    );
    // Back to the text: the prompt comes back at the cursor.
    show_at(&mut session.p, &mut session.c, PromptShow::Text);
    let out = session.repaint();
    assert_eq!(out.pin.as_deref(), Some(&b""[..]));
    assert_eq!(
        drawn_after_mark(&out.bytes).as_deref(),
        Some("hp 1020> \x1b[0m")
    );
    assert!(session.c.prompt.stage.open_row().is_some());
}

/// The bytes after the last region mark, as text.
fn drawn_after_mark(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let at = text.rfind("\x1b]7717;o;")?;
    let rest = &text[at..];
    let end = rest.find('\x07')?;
    Some(rest[end + 1..].to_string())
}

/// The screen a native grid `columns` wide shows after `reads`, rows
/// trimmed, up to the last row that shows anything, and where its cursor
/// sits.
fn grid_screen(reads: &[Read], columns: usize) -> (Vec<String>, (i32, usize)) {
    let mut grid = crate::native::grid::TermGrid::new(columns, 60);
    for read in reads {
        grid.session_output(&read.out);
    }
    let mut rows: Vec<String> = (0..grid.screen_lines())
        .map(|line| grid.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    (rows, grid.cursor())
}

/// The pinned screen of each stream in one read, drawing on, 80 wide.
fn pinned_screen_of(name: &str) -> Vec<String> {
    let (_, bytes, prompt) = pinned_streams()
        .into_iter()
        .find(|(n, _, _)| n == name)
        .expect("the stream");
    let make = || showing(profile(prompt, HP, true), vosh_prompt::PromptShow::Pinned);
    grid_screen(&play_reads(&make, &bytes, &[]), 80).0
}

#[test]
fn pinned_screens_are_the_same_at_every_split_on_the_native_grid() {
    use vosh_prompt::PromptShow;
    for (name, bytes, prompt) in pinned_streams() {
        for draw in [true, false] {
            let mut session = Session::new(showing(profile(prompt, HP, draw), PromptShow::Pinned));
            // The rows match. A read that ends partway through a prompt
            // paints that part, and the next read erases it, so while you
            // wait the cursor can sit a row lower than after one read.
            for columns in [40, 12] {
                let whole = grid_screen(&replay(&mut session, &bytes, &[]), columns).0;
                for at in cuts(&bytes) {
                    let split = grid_screen(&replay(&mut session, &bytes, &[at]), columns).0;
                    assert_eq!(
                        split, whole,
                        "{name} draw {draw} {columns} wide, cut after {at}"
                    );
                }
            }
        }
    }
}

#[test]
fn pinned_screens_keep_every_row_but_the_prompts() {
    const ROOM: [&str; 3] = [
        "The Bank of Aabahran",
        "  Marble counters line the hall, and a clerk nods at you.",
        "[Exits: south]",
    ];
    // The room, and the blank before the prompt waits with the prompt's
    // own line end, so the text ends on the room.
    assert_eq!(pinned_screen_of("quiet"), ROOM);
    // The battle line stays, and the tank line goes with the prompt.
    assert_eq!(
        pinned_screen_of("fight-tank"),
        [
            "A Blackwatch guard attacks you!",
            "A Blackwatch guard has quite a few wounds."
        ]
    );
    // Two prompts in one read: the one the next pulse completes took its
    // line end, and the text you did not ask for follows the blank.
    assert_eq!(
        pinned_screen_of("prompt-all-next"),
        [
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "",
            "A Blackwatch guard arrives from the south."
        ]
    );
    assert_eq!(
        pinned_screen_of("login-then-tell"),
        [
            "Welcome to the fake Aabahran, Tester.",
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "",
            "Quenby tells you 'back soon'"
        ]
    );
    assert_eq!(
        pinned_screen_of("three-fight-pulses"),
        [
            "A Blackwatch guard attacks you!",
            "A Blackwatch guard has quite a few wounds.",
            "",
            "Your slash hits a Blackwatch guard.",
            "A Blackwatch guard has quite a few wounds.",
            "",
            "A Blackwatch guard's pierce misses you.",
            "A Blackwatch guard has quite a few wounds."
        ]
    );
    // Compact prints no blank before a prompt, so none appears.
    assert_eq!(
        pinned_screen_of("compact"),
        [
            "Welcome to the fake Aabahran, Tester.",
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "Quenby tells you 'back soon'"
        ]
    );
    // No prompts at all: the same as the text.
    assert_eq!(
        pinned_screen_of("prompts-off-new")[..4],
        ["You will no longer see prompts.", "", "", "Pulse 1 of 3."]
    );
}

#[test]
fn while_you_wait_the_text_ends_on_its_last_line() {
    use vosh_prompt::PromptShow;
    let make = || showing(profile(CODES, HP, true), PromptShow::Pinned);
    let reads = play_reads(&make, &wire_fixture("quiet"), &[]);
    let (rows, cursor) = grid_screen(&reads, 80);
    assert_eq!(cursor, (2, rows[2].len()));
}

#[test]
fn your_echo_takes_the_row_the_prompt_held() {
    use vosh_prompt::testkit::{Build, Mud, Options};
    use vosh_prompt::PromptShow;
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut session = Session::new(showing(profile(CODES, HP, true), PromptShow::Pinned));
    let mut grid = crate::native::grid::TermGrid::new(60, 30);
    grid.session_output(&session.read(&mud.login()).out);
    // You type look: the echo lands after the held line ends, and the
    // reply follows it.
    let _ = session.send("look");
    grid.local_write(b"look\r\n");
    session.local_write();
    for write in mud.command("look") {
        grid.session_output(&session.read(&write.bytes).out);
    }
    // Enter on an empty line echoes nothing and moves nothing.
    let _ = session.send("");
    for write in mud.command("") {
        grid.session_output(&session.read(&write.bytes).out);
    }
    let tell = mud.pulse_later("Quenby tells you 'back soon'");
    grid.session_output(&session.read(&tell).out);
    let mut rows: Vec<String> = (0..grid.screen_lines())
        .map(|line| grid.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    assert_eq!(
        rows,
        [
            "Welcome to the fake Aabahran, Tester.",
            "The Bank of Aabahran",
            "  Marble counters line the hall, and a clerk nods at you.",
            "[Exits: south]",
            "",
            "look",
            "The Bank of Aabahran",
            "  Marble counters line the hall, and a clerk nods at you.",
            "[Exits: south]",
            "",
            "Quenby tells you 'back soon'"
        ]
    );
}

/// The pinned payloads of every stream in [`pinned_streams`], as one read
/// and as two cut at every place [`cuts`] names, with the native grid's
/// screen of one read at 40 and 12 wide. The webview test replays them
/// into xterm and holds its screens to the grid's.
fn pinned_splits() -> serde_json::Value {
    use vosh_prompt::PromptShow;
    let mut streams = Vec::new();
    for (name, bytes, prompt) in pinned_streams() {
        for draw in [true, false] {
            let mut session = Session::new(showing(profile(prompt, HP, draw), PromptShow::Pinned));
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            let payloads: Vec<Vec<serde_json::Value>> = splits
                .iter()
                .map(|at| {
                    replay(&mut session, &bytes, at)
                        .iter()
                        .filter(|read| !read.out.is_empty())
                        .map(|read| {
                            serde_json::to_value(OutputPayload::from_output(&read.out))
                                .expect("it serializes")
                        })
                        .collect()
                })
                .collect();
            let whole = replay(&mut session, &bytes, &[]);
            let screens: serde_json::Map<String, serde_json::Value> = [40, 12]
                .into_iter()
                .map(|columns| {
                    (
                        columns.to_string(),
                        serde_json::json!(grid_screen(&whole, columns).0),
                    )
                })
                .collect();
            streams.push(serde_json::json!({
                "name": name,
                "draw": draw,
                "screens": screens,
                "splits": payloads,
            }));
        }
    }
    serde_json::json!({ "streams": streams })
}

/// The file the webview test reads: the JSON of [`pinned_splits`],
/// gzipped, as base64 text, since the webview test can import text only.
fn pinned_splits_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/prompt/aabahran/pinned/splits.b64")
}

/// Write [`pinned_splits`] for the webview test when
/// `VOSH_WRITE_PINNED_SPLITS` is set. Nothing otherwise.
#[test]
fn write_the_pinned_splits_for_the_webview() {
    use std::io::Write as _;
    if std::env::var("VOSH_WRITE_PINNED_SPLITS").is_err() {
        return;
    }
    let text = serde_json::to_string(&pinned_splits()).expect("json");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(text.as_bytes()).expect("gzip");
    let encoded = base64_encode(&gz.finish().expect("gzip"));
    let mut lines: Vec<&str> = encoded
        .as_bytes()
        .chunks(100)
        .map(|c| std::str::from_utf8(c).expect("ascii"))
        .collect();
    lines.push("");
    let path = pinned_splits_path();
    std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
    std::fs::write(&path, lines.join("\n")).expect("the file");
}

#[test]
fn the_pinned_splits_the_webview_replays_are_what_the_session_sends() {
    use std::io::Read as _;
    let stored = std::fs::read_to_string(pinned_splits_path()).expect(
        "fixtures/prompt/aabahran/pinned/splits.b64, written with VOSH_WRITE_PINNED_SPLITS=1",
    );
    let bytes = base64_decode(&stored);
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .expect("gzip");
    let stored: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert!(
        stored == pinned_splits(),
        "the session changed, so write the file again with VOSH_WRITE_PINNED_SPLITS=1"
    );
}

#[test]
fn lifted_prompts_stay_in_the_text_with_marks_that_take_no_room() {
    use vosh_prompt::PromptShow;
    for (name, bytes, prompt) in pinned_streams() {
        for draw in [true, false] {
            let mut text = Session::new(counting(profile(prompt, HP, draw)));
            let mut lifted = Session::new(showing(
                counting(profile(prompt, HP, draw)),
                PromptShow::Lifted,
            ));
            let mut splits = vec![Vec::new()];
            splits.extend(cuts(&bytes).into_iter().map(|at| vec![at]));
            for at in &splits {
                let want = replay(&mut text, &bytes, at);
                let got = replay(&mut lifted, &bytes, at);
                let label = format!("{name} draw {draw} cut {at:?}");
                let flat = |reads: &[Read]| {
                    (
                        reads.iter().flat_map(|r| r.log.clone()).collect::<Vec<_>>(),
                        reads
                            .iter()
                            .flat_map(|r| r.kept.clone())
                            .collect::<Vec<_>>(),
                        reads
                            .iter()
                            .flat_map(|r| r.sends.clone())
                            .collect::<Vec<_>>(),
                    )
                };
                assert_eq!(flat(&got), flat(&want), "{label}");
                // The screen is the text's, save the one space a lift that
                // ends on a glyph keeps before your echo.
                for columns in [40, 12] {
                    assert_eq!(
                        grid_screen(&got, columns).0,
                        grid_screen(&want, columns).0,
                        "{label} {columns} wide"
                    );
                }
            }
            // Every prompt in one read carries a start and an end.
            let got = replay(&mut lifted, &bytes, &[]);
            let all: Vec<u8> = got.iter().flat_map(|r| r.out.bytes.clone()).collect();
            let text_all = String::from_utf8_lossy(&all);
            let starts = text_all.matches("\x1b]7717;l;").count();
            let ends = text_all.matches("\x1b]7717;e;").count();
            assert_eq!(starts, ends, "{name} draw {draw}");
            if name != "prompts-off-new" {
                assert!(starts >= 1, "{name} draw {draw}");
            }
        }
    }
}

#[test]
fn an_echo_that_ends_the_prompt_row_takes_the_row_a_pinned_prompt_left() {
    use vosh_prompt::testkit::{Build, Mud, Options};
    use vosh_prompt::PromptShow;
    const ROOM: [&str; 3] = [
        "The Bank of Aabahran",
        "  Marble counters line the hall, and a clerk nods at you.",
        "[Exits: south]",
    ];
    let mut mud = Mud::playing(Options::new(Build::New));
    let login = mud.login();
    let tell = mud.pulse_later("Quenby tells you 'back soon'");
    let play = |show, draw| {
        let mut session = Session::new(showing(profile(CODES, HP, draw), show));
        let mut grid = crate::native::grid::TermGrid::new(80, 40);
        grid.session_output(&session.read(&login).out);
        // The tick warning, framed to end the prompt's row first.
        let warn = session.emitted(b"\r\n\x1b[33mTICK IN 5s\x1b[0m\r\n");
        grid.session_output(&warn);
        grid.session_output(&session.read(&tell).out);
        // An error notice the page writes, framed the same way.
        grid.local_write(b"\r\n\x1b[31m[Not connected]\x1b[0m\r\n");
        session.local_write();
        rows_of(&grid)
    };
    // In the text each echo ends the drawn prompt's row.
    let text = play(PromptShow::Text, true);
    assert_eq!(
        text,
        [
            "Welcome to the fake Aabahran, Tester.",
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "",
            "<1020>",
            "TICK IN 5s",
            "",
            "Quenby tells you 'back soon'",
            "",
            "<1020>",
            "[Not connected]"
        ]
    );
    // Pinned shows those rows without the prompts' own, drawing on or
    // off.
    let want: Vec<String> = text.into_iter().filter(|r| r != "<1020>").collect();
    for draw in [true, false] {
        assert_eq!(play(PromptShow::Pinned, draw), want, "draw {draw}");
    }
}

/// Whether the row a pinned prompt left is open after `out`, the way the
/// page tracks it from each payload (src/stores/session/pinnedPromptStore.ts)
/// to decide whether Enter on an empty line echoes a line end.
fn pin_row_after(open: bool, out: &Output) -> bool {
    match out.pin_row {
        Some(open) => open,
        None => open && !vosh_prompt::stage::close_pin_row(&out.bytes).1,
    }
}

#[test]
fn enter_on_an_empty_line_ends_the_row_of_a_prompt_left_in_the_text() {
    use vosh_prompt::PromptShow;
    // The pager and the note editor, as Aabahran writes them: no GA and
    // no line end, and the next page with no line end before it. Vosh
    // does not read either as your prompt, so both stay in the text.
    let pager = (
        b"\n\rLine one of a long help.\n\rLine two.\n\r\r[Hit Return to continue]\r".to_vec(),
        b"Line three.\n\rLine four.\n\r\n\r[1020/1020hp 800/800mn 930/930mv]\n\r\xff\xf9".to_vec(),
    );
    let editor = (
        b"\n\rEnter your note. End with @.\n\r> ".to_vec(),
        b"> ".to_vec(),
    );
    for (name, (first, next)) in [("pager", &pager), ("editor", &editor)] {
        let play = |show, draw| {
            let mut session = Session::new(showing(profile(CODES, HP, draw), show));
            let mut grid = crate::native::grid::TermGrid::new(80, 40);
            let mut open = false;
            for bytes in [wire_fixture("quiet"), first.clone()] {
                let read = session.read(&bytes);
                open = pin_row_after(open, &read.out);
                grid.session_output(&read.out);
            }
            for _ in 0..2 {
                // Enter on an empty line.
                if !open {
                    grid.local_write(b"\r\n");
                    session.local_write();
                }
                let _ = session.send("");
                let read = session.read(next);
                open = pin_row_after(open, &read.out);
                grid.session_output(&read.out);
            }
            rows_of(&grid)
        };
        // Pinned shows the rows the text shows with drawing on, without
        // the prompts' own, and holds the line ends before the last one.
        let text = play(PromptShow::Text, true);
        let mut want: Vec<String> = text.iter().filter(|r| *r != "<1020>").cloned().collect();
        while want.last().is_some_and(String::is_empty) {
            want.pop();
        }
        for draw in [true, false] {
            let got = play(PromptShow::Pinned, draw);
            assert_eq!(got, want, "{name} draw {draw}, in the text {text:#?}");
        }
    }
}

/// `p` with a Prompts trigger on `hp` that does `action`.
fn prompts_trigger((mut p, c): Live, action: vosh_automation::trigger::TriggerAction) -> Live {
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            target: vosh_automation::trigger::TriggerTarget::Prompt,
            ..vosh_automation::trigger::Trigger::new("on-prompt", "hp", action)
        })
        .expect("the trigger compiles");
    (p, c)
}

#[test]
fn leaving_pinned_with_drawing_off_keeps_what_prompts_triggers_did() {
    use vosh_automation::trigger::TriggerAction;
    use vosh_prompt::PromptShow;
    let replace = TriggerAction::Replace {
        template: "HITPOINTS".into(),
    };
    for action in [TriggerAction::Gag, replace] {
        let make = |show| {
            showing(
                prompts_trigger(profile(CODES, HP, false), action.clone()),
                show,
            )
        };
        let quiet = wire_fixture("quiet");
        // In the text all along.
        let mut text = Session::new(make(PromptShow::Text));
        let mut want = crate::native::grid::TermGrid::new(80, 30);
        want.session_output(&text.read(&quiet).out);
        // Pinned, then back before anything else lands.
        for back in [PromptShow::Text, PromptShow::Lifted] {
            let mut session = Session::new(make(PromptShow::Pinned));
            let mut grid = crate::native::grid::TermGrid::new(80, 30);
            let read = session.read(&quiet);
            let band = read.out.pin.clone().expect("the band");
            grid.session_output(&read.out);
            // A repaint while pinned keeps the band as the trigger left it.
            let again = session.repaint();
            assert!(
                again.pin.is_none() || again.pin.as_ref() == Some(&band),
                "{action:?}"
            );
            show_at(&mut session.p, &mut session.c, back);
            let out = session.repaint();
            grid.session_output(&out);
            assert_eq!(
                rows_of(&grid),
                rows_of(&want),
                "{action:?} back to {back:?}"
            );
        }
    }
}

/// Each lift on `grid`, as its rows' text, top first.
fn lifted_rows(grid: &crate::native::grid::TermGrid) -> Vec<Vec<String>> {
    let mut lifts: Vec<(u64, Vec<String>)> = Vec::new();
    for span in grid.lift_spans(-1000, 1000) {
        let row: String = grid
            .row_string(usize::try_from(span.line).expect("on screen"))
            .chars()
            .skip(span.first)
            .take(span.end - span.first)
            .collect();
        match lifts.iter_mut().find(|(id, _)| *id == span.id) {
            Some((_, rows)) => rows.push(row),
            None => lifts.push((span.id, vec![row])),
        }
    }
    lifts.into_iter().map(|(_, rows)| rows).collect()
}

#[test]
fn changing_where_your_prompt_shows_mid_fight_moves_the_tank_line_with_it() {
    use vosh_prompt::testkit::{Build, Mud, Options};
    use vosh_prompt::PromptShow;
    for (draw, columns) in [(true, 80), (false, 80), (true, 12)] {
        for (from, to) in [
            (PromptShow::Text, PromptShow::Pinned),
            (PromptShow::Lifted, PromptShow::Pinned),
            (PromptShow::Text, PromptShow::Lifted),
            (PromptShow::Pinned, PromptShow::Text),
            (PromptShow::Pinned, PromptShow::Lifted),
        ] {
            let mut mud = Mud::playing(Options::new(Build::New));
            let fight = wire_fixture("fight-tank");
            let tell = mud.pulse_later("Quenby tells you 'back soon'");
            // The whole play shown as `to`, or started as `from` and
            // switched right after the fight's prompt.
            let play = |switch: bool| {
                let start = if switch { from } else { to };
                let mut session = Session::new(showing(profile(CODES, HP, draw), start));
                let mut grid = crate::native::grid::TermGrid::new(columns, 40);
                grid.session_output(&session.read(&fight).out);
                let mut band = None;
                if switch {
                    show_at(&mut session.p, &mut session.c, to);
                    let out = session.repaint();
                    band = out.pin.clone();
                    grid.session_output(&out);
                }
                let at_switch = (rows_of(&grid), lifted_rows(&grid));
                grid.session_output(&session.read(&tell).out);
                grid.local_write(b"look\r\n");
                session.local_write();
                (at_switch, rows_of(&grid), band)
            };
            let label = format!("draw {draw} {columns} wide {from:?} to {to:?}");
            let (want_at, want, _) = play(false);
            let (got_at, got, band) = play(true);
            if to == PromptShow::Pinned && draw {
                // The band holds the tank line, and the text does not.
                let band = band.expect("the band");
                assert_eq!(
                    vosh_protocol::ansi::plain_text(&band),
                    "Tester: [===|===|===|---]\r\n<765>",
                    "{label}"
                );
            }
            // Drawing off, a prompt in the text leaves no open row, so it
            // stays as it shows and the next prompt goes where you chose.
            if draw || from == PromptShow::Pinned {
                assert_eq!(got_at, want_at, "{label}");
                assert_eq!(got, want, "{label}");
            }
        }
    }
}

/// What the native grid shows after you send `line` with the prompt
/// pinned and the game answers it, from the screen login left: its rows,
/// how many rows its cursor sits below the last that shows anything, and
/// its rows once a tell comes after.
fn pinned_after(line: &str) -> (Vec<String>, i32, Vec<String>) {
    use vosh_prompt::testkit::{Build, Mud, Options};
    use vosh_prompt::PromptShow;
    let mut mud = Mud::playing(Options::new(Build::New));
    let mut session = Session::new(showing(profile(CODES, HP, true), PromptShow::Pinned));
    let mut grid = crate::native::grid::TermGrid::new(60, 30);
    grid.session_output(&session.read(&mud.login()).out);
    let _ = session.send(line);
    grid.local_write(
        format!(
            "{}{line}\r\n",
            crate::input::echo_mark(&crate::profile::ui::UiConfig::default())
        )
        .as_bytes(),
    );
    session.local_write();
    for write in mud.command(line) {
        grid.session_output(&session.read(&write.bytes).out);
    }
    let rows = rows_of(&grid);
    let last = i32::try_from(rows.len()).expect("few rows") - 1;
    let below = grid.cursor().0 - last;
    let tell = mud.pulse_later("Quenby tells you 'back soon'");
    grid.session_output(&session.read(&tell).out);
    (rows, below, rows_of(&grid))
}

#[test]
fn a_blank_enter_while_pinned_leaves_the_text_where_a_command_does() {
    // The game answers a blank line with a line end and the prompt
    // (comm.c process_output), and `look` with its lines, a line end and
    // the prompt. Either way the text waits on its last row, your echo
    // for the blank line.
    let (look, look_below, look_then) = pinned_after("look");
    let (blank, blank_below, blank_then) = pinned_after("");
    assert_eq!(look.last().map(String::as_str), Some("[Exits: south]"));
    assert_eq!(blank.last().map(String::as_str), Some("\u{203a}"));
    assert_eq!(blank_below, look_below, "look {look:?}\nblank {blank:?}");
    assert_eq!(look_below, 0);
    // What comes next starts after the empty row the game sent before
    // the prompt, as it did before.
    let tell = "Quenby tells you 'back soon'".to_string();
    assert_eq!(look_then[look.len()..], [String::new(), tell.clone()]);
    assert_eq!(blank_then[blank.len()..], [String::new(), tell]);
}
