//! Room triggers, played through the session's own steps.
//!
//! A child of `session`, so it drives the same private steps the socket
//! loop runs: the GMCP step, the Line pass and the GA step. The looks come
//! from fixtures/room-colors/looks.json, each one the way the Aabahran
//! server prints it, with its Room.Chars packet first where the server
//! sends one.

use super::*;

/// One event of a look in the fixture.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum LookEvent {
    Gmcp {
        gmcp: String,
        data: serde_json::Value,
    },
    Line {
        line: String,
        room: bool,
    },
    Prompt {
        prompt: String,
    },
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct LookCase {
    pub(super) name: String,
    pub(super) events: Vec<LookEvent>,
}

#[derive(Debug, serde::Deserialize)]
struct LookFile {
    cases: Vec<LookCase>,
}

/// Every look in fixtures/room-colors/looks.json.
pub(super) fn looks() -> Vec<LookCase> {
    let text = include_str!("../../fixtures/room-colors/looks.json");
    serde_json::from_str::<LookFile>(text)
        .expect("looks.json reads")
        .cases
}

/// The wire bytes of `events`: each packet as a GMCP subnegotiation, each
/// line with the `\n\r` the server ends it with, and each prompt with a
/// GA after it.
pub(super) fn wire(events: &[LookEvent]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for event in events {
        match event {
            LookEvent::Gmcp { gmcp, data } => {
                bytes.extend_from_slice(&[
                    telnet_codes::IAC,
                    telnet_codes::SB,
                    telnet_option::GMCP,
                ]);
                bytes.extend_from_slice(format!("{gmcp} {data}").as_bytes());
                bytes.extend_from_slice(&[telnet_codes::IAC, telnet_codes::SE]);
            }
            LookEvent::Line { line, .. } => {
                bytes.extend_from_slice(line.as_bytes());
                bytes.extend_from_slice(b"\n\r");
            }
            LookEvent::Prompt { prompt } => {
                bytes.extend_from_slice(prompt.as_bytes());
                bytes.extend_from_slice(&[telnet_codes::IAC, telnet_codes::GA]);
            }
        }
    }
    bytes
}

/// One socket read of `data` through the steps the session runs for each
/// event, then the end of the read. Returns what the terminal gets.
pub(super) fn read(p: &mut Profile, data: &[u8]) -> String {
    let mut parser = Parser::new();
    let mut acc = LineAccumulator::new();
    let mut batch = ReadBatch::new(output_count());
    let now = Instant::now();
    for event in parser.feed(data) {
        match event {
            TelnetEvent::Data(bytes) => {
                for line in acc.feed(&bytes) {
                    let plain = vosh_ansi::plain_text(&line.bytes);
                    let _ = line_step(p, &mut batch, line, plain, now, None);
                }
            }
            TelnetEvent::Subnegotiation { option, payload } if option == telnet_option::GMCP => {
                let msg = vosh_gmcp::parse(&payload).expect("every packet parses");
                let _ = gmcp_step(p, &msg, now);
            }
            TelnetEvent::Command(byte) if byte == telnet_codes::GA || byte == telnet_codes::EOR => {
                let _ = marker_step(p, &mut acc, &mut batch, now, None);
            }
            _ => {}
        }
    }
    let _ = partial_step(p, &mut acc, &mut batch, now, None);
    String::from_utf8(batch.out.bytes).expect("the output is text")
}

/// What the terminal shows for `events` when each room line takes
/// `room_line` and every other line and prompt shows as sent.
pub(super) fn expected(events: &[LookEvent], room_line: &dyn Fn(&str) -> String) -> String {
    let mut out = String::new();
    for event in events {
        match event {
            LookEvent::Gmcp { .. } => {}
            LookEvent::Line { line, room: true } => {
                out.push_str(&room_line(line));
                out.push_str("\r\n");
            }
            LookEvent::Line { line, room: false } => {
                out.push_str(line);
                out.push_str("\r\n");
            }
            LookEvent::Prompt { prompt } => {
                out.push_str(prompt);
                out.push_str("\r\n");
            }
        }
    }
    out
}

/// A trigger on `target` that colors a whole line yellow.
fn yellow(name: &str, target: vosh_trigger::TriggerTarget) -> vosh_trigger::Trigger {
    vosh_trigger::Trigger {
        name: name.to_string(),
        patterns: vec![vosh_trigger::TriggerPattern {
            pattern: "^.+$".to_string(),
            enabled: true,
        }],
        priority: 4,
        enabled: true,
        actions: vec![vosh_trigger::TriggerAction::Highlight {
            style: vosh_trigger::HighlightStyle {
                fg: Some(vosh_trigger::NamedColor::Yellow),
                ..Default::default()
            },
        }],
        preset: None,
        group: None,
        target,
    }
}

#[test]
fn a_room_trigger_colors_the_things_and_people_of_each_look_and_nothing_else() {
    let cases = looks();
    assert!(cases.len() >= 6);
    for case in &cases {
        let mut p = Profile::default();
        p.triggers
            .set(yellow("room", vosh_trigger::TriggerTarget::Room))
            .unwrap();
        let shown = read(&mut p, &wire(&case.events));
        let want = expected(&case.events, &|line| {
            format!("\x1b[33m{}\x1b[0m", vosh_ansi::plain_text(line.as_bytes()))
        });
        assert_eq!(shown, want, "{}", case.name);
    }
}

#[test]
fn an_exits_line_someone_says_opens_no_look() {
    let mut p = Profile::default();
    p.triggers
        .set(yellow("room", vosh_trigger::TriggerTarget::Room))
        .unwrap();
    // languages.c, $n says with the text in `# bold yellow.
    let said = "Tolliver says '\x1b[0;1;33m[Exits: south]\x1b[0;0m'";
    let helm = "     A black-steel helm is here, gleaming darkly.";
    let shown = read(&mut p, format!("{said}\n\r{helm}\n\r").as_bytes());
    assert_eq!(shown, format!("{said}\r\n{helm}\r\n"));
}

#[test]
fn line_triggers_still_see_every_line_of_a_look() {
    let case = &looks()[0];
    let mut p = Profile::default();
    p.triggers
        .set(yellow("all", vosh_trigger::TriggerTarget::Line))
        .unwrap();
    let shown = read(&mut p, &wire(&case.events));
    for event in &case.events {
        if let LookEvent::Line { line, .. } = event {
            let plain = vosh_ansi::plain_text(line.as_bytes());
            if !plain.is_empty() {
                assert!(
                    shown.contains(&format!("\x1b[33m{plain}\x1b[0m\r\n")),
                    "{plain}"
                );
            }
        }
    }
}
