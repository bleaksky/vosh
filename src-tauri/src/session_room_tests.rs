//! Room triggers and the Room and time colors preset, played through the
//! session's own steps.
//!
//! A child of `session`, so it drives the same private steps the socket
//! loop runs: the GMCP step, the Line pass and the GA step. The looks and
//! lines come from fixtures/room-colors, each one the way the Aabahran
//! server prints it, with its Room.Chars packet first where the server
//! sends one. The preset's triggers come from preset.json, which
//! presets.test.ts holds to src/lib/presets.ts.

use super::*;

/// One event of a look in looks.json.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum LookEvent {
    Gmcp {
        gmcp: String,
        data: serde_json::Value,
    },
    Line {
        line: String,
        room: bool,
        /// The color the Room and time colors preset gives a line that
        /// is not a room line, if any.
        #[serde(default)]
        preset: Option<String>,
    },
    Prompt {
        prompt: String,
    },
}

#[derive(Debug, serde::Deserialize)]
struct LookCase {
    name: String,
    events: Vec<LookEvent>,
}

#[derive(Debug, serde::Deserialize)]
struct LookFile {
    cases: Vec<LookCase>,
}

/// Every look in fixtures/room-colors/looks.json.
fn looks() -> Vec<LookCase> {
    let text = include_str!("../../fixtures/room-colors/looks.json");
    serde_json::from_str::<LookFile>(text)
        .expect("looks.json reads")
        .cases
}

/// One line in lines.json.
#[derive(Debug, serde::Deserialize)]
struct PresetLine {
    line: String,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default, rename = "match")]
    span: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct LineFile {
    lines: Vec<PresetLine>,
}

/// Every line in fixtures/room-colors/lines.json.
fn preset_lines() -> Vec<PresetLine> {
    let text = include_str!("../../fixtures/room-colors/lines.json");
    serde_json::from_str::<LineFile>(text)
        .expect("lines.json reads")
        .lines
}

/// The triggers of the Room and time colors preset, from preset.json.
fn preset_triggers() -> Vec<vosh_trigger::Trigger> {
    #[derive(serde::Deserialize)]
    struct PresetFile {
        triggers: Vec<vosh_trigger::Trigger>,
    }
    let text = include_str!("../../fixtures/room-colors/preset.json");
    serde_json::from_str::<PresetFile>(text)
        .expect("preset.json reads")
        .triggers
}

/// A profile with the Room and time colors preset installed.
fn preset_profile() -> Profile {
    let mut p = Profile::default();
    for trigger in preset_triggers() {
        p.triggers
            .set(trigger)
            .expect("every preset pattern compiles");
    }
    p
}

/// The wire bytes of `events`: each packet as a GMCP subnegotiation, each
/// line with the `\n\r` the server ends it with, and each prompt with a
/// GA after it.
fn wire(events: &[LookEvent]) -> Vec<u8> {
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
fn read(p: &mut Profile, data: &[u8]) -> String {
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

/// What the terminal shows for `events`, each line as `shows` gives it
/// from the line as sent, whether it is a room line, and the preset color
/// looks.json names for it. Each prompt shows as sent.
fn expected(events: &[LookEvent], shows: &dyn Fn(&str, bool, Option<&str>) -> String) -> String {
    let mut out = String::new();
    for event in events {
        match event {
            LookEvent::Gmcp { .. } => {}
            LookEvent::Line { line, room, preset } => {
                out.push_str(&shows(line, *room, preset.as_deref()));
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

/// `line` without its ANSI codes, wrapped in `open` and a reset.
fn wrapped(open: &str, line: &str) -> String {
    format!("{open}{}\x1b[0m", vosh_ansi::plain_text(line.as_bytes()))
}

/// The SGR open the preset uses for a color named in a fixture.
fn open_for(color: &str) -> &'static str {
    match color {
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        other => panic!("no preset color {other}"),
    }
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
        let want = expected(&case.events, &|line, room, _| {
            if room {
                wrapped("\x1b[33m", line)
            } else {
                line.to_string()
            }
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

#[test]
fn the_preset_opens_a_look_on_the_line_the_session_reads_as_its_exits() {
    let exits = preset_triggers()
        .into_iter()
        .find(|t| t.name == "room.exits")
        .expect("the preset colors the exits");
    assert_eq!(
        exits.first_pattern(),
        crate::room_block::EXITS_PATTERN,
        "the preset and the session read the same exits line"
    );
}

#[test]
fn the_preset_colors_each_look_as_the_mockups_draw_it() {
    for case in &looks() {
        let mut p = preset_profile();
        let shown = read(&mut p, &wire(&case.events));
        let want = expected(&case.events, &|line, room, preset| match (room, preset) {
            (true, _) => wrapped(open_for("yellow"), line),
            (false, Some(color)) => wrapped(open_for(color), line),
            (false, None) => line.to_string(),
        });
        assert_eq!(shown, want, "{}", case.name);
    }
}

#[test]
fn the_preset_colors_each_line_it_names_and_leaves_every_near_miss_alone() {
    let lines = preset_lines();
    assert!(lines.iter().any(|l| l.trigger.is_none()));
    for case in &lines {
        let mut p = preset_profile();
        let shown = read(&mut p, format!("{}\n\r", case.line).as_bytes());
        let plain = vosh_ansi::plain_text(case.line.as_bytes());
        let want = match case.trigger.as_deref() {
            None => case.line.clone(),
            Some(trigger) => {
                let span = case.span.as_deref().expect("a colored line names its span");
                let open = match trigger {
                    "room.exits" => "\x1b[32m",
                    "time.of_day" => "\x1b[34m",
                    "wiznet.tag" => "\x1b[1;35m",
                    other => panic!("no preset trigger {other}"),
                };
                assert!(plain.starts_with(span), "{plain}");
                format!("{open}{span}\x1b[0m{}", &plain[span.len()..])
            }
        };
        assert_eq!(shown, format!("{want}\r\n"), "{plain}");
    }
}
