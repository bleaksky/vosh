//! Room triggers and the Room, time and weather colors preset, played
//! through the session's own steps.
//!
//! Inside `session`, so it drives the same private steps the socket
//! loop runs: the GMCP step, the Line pass and the GA step. The looks and
//! lines come from fixtures/room-colors, each one the way the Aabahran
//! server prints it, with its Room.Info, Room.Chars and Room.Items
//! packets where the server sends them, before the text up to d50e4a24
//! and after the people since. The preset's triggers come from
//! preset.json, which presets.test.ts holds to src/automation/presets.ts.

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
        /// The line of the person the case targets.
        #[serde(default)]
        target: bool,
        /// The color the Room, time and weather colors preset gives a line
        /// that is not a room line, if any.
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
    /// What you gave `tar` before the look, if anything.
    #[serde(default)]
    target: Option<String>,
    events: Vec<LookEvent>,
}

#[derive(Debug, serde::Deserialize)]
struct LookFile {
    cases: Vec<LookCase>,
}

/// Every look in fixtures/room-colors/looks.json.
fn looks() -> Vec<LookCase> {
    let text = include_str!("../../../../fixtures/room-colors/looks.json");
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
    let text = include_str!("../../../../fixtures/room-colors/lines.json");
    serde_json::from_str::<LineFile>(text)
        .expect("lines.json reads")
        .lines
}

/// The triggers of the Room, time and weather colors preset, from preset.json.
fn preset_triggers() -> Vec<vosh_automation::trigger::Trigger> {
    #[derive(serde::Deserialize)]
    struct PresetFile {
        triggers: Vec<vosh_automation::trigger::Trigger>,
    }
    let text = include_str!("../../../../fixtures/room-colors/preset.json");
    serde_json::from_str::<PresetFile>(text)
        .expect("preset.json reads")
        .triggers
}

/// A profile with the Room, time and weather colors preset installed.
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

/// The socket reads of one connection, each through the steps the
/// session runs for each event, with the Room.Chars each event finds ahead
/// in its read, then the end of the read.
#[derive(Default)]
struct Reads {
    parser: Parser,
    acc: LineAccumulator,
}

impl Reads {
    /// One read of `data`, with your target, the room list and the room
    /// look in `c`. Returns what the read writes to the terminal.
    fn read(&mut self, p: &mut Profile, c: &mut Connection, data: &[u8]) -> Output {
        let mut batch = ReadBatch::new(false);
        let now = Instant::now();
        let events = self.parser.feed(data);
        let ahead = room_chars_ahead(&events);
        for (i, event) in events.into_iter().enumerate() {
            batch.room_ahead = ahead.get(i).cloned().flatten();
            match event {
                TelnetEvent::Data(bytes) => {
                    for line in self.acc.feed(&bytes) {
                        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
                        let _ = line_step(p, c, &mut batch, line, plain, now, None);
                    }
                }
                TelnetEvent::Subnegotiation { option, payload }
                    if option == telnet_option::GMCP =>
                {
                    let msg = vosh_protocol::gmcp::parse(&payload).expect("every packet parses");
                    let _ = gmcp_step(p, c, &msg, now);
                    if msg.package == "Room.Chars" {
                        let _ = recolor_target(p, c, &mut batch.out);
                    }
                }
                TelnetEvent::Command(byte)
                    if byte == telnet_codes::GA || byte == telnet_codes::EOR =>
                {
                    let _ = marker_step(p, c, &mut self.acc, &mut batch, now, None);
                }
                _ => {}
            }
        }
        let _ = partial_step(p, c, &mut self.acc, &mut batch, now, None);
        c.prompt.stage.finish(&mut batch.out);
        batch.out
    }
}

/// One socket read of `data` on a new connection. Returns what the
/// terminal gets.
fn read(p: &mut Profile, c: &mut Connection, data: &[u8]) -> String {
    let out = Reads::default().read(p, c, data);
    assert!(out.replace.is_none());
    String::from_utf8(out.bytes).expect("the output is text")
}

/// The screen after `outs`, each a read's output: a replace writes over
/// its region from the region's mark on, as a renderer does while nothing
/// came after the region, and the marks show nothing.
fn screen(outs: &[Output]) -> String {
    let mut screen = String::new();
    for out in outs {
        if let Some(replace) = &out.replace {
            let mark = String::from_utf8(vosh_prompt::stage::mark(replace.gen)).unwrap();
            let at = screen.rfind(&mark).expect("the region the replace writes");
            assert!(
                !screen[at + mark.len()..].contains("\x1b]7717;"),
                "no region after the one replaced"
            );
            screen.truncate(at);
            screen.push_str(std::str::from_utf8(&replace.bytes).unwrap());
        }
        screen.push_str(std::str::from_utf8(&out.bytes).unwrap());
    }
    while let Some(at) = screen.find("\x1b]7717;") {
        let end = at + screen[at..].find('\x07').expect("a mark ends") + 1;
        screen.replace_range(at..end, "");
    }
    screen
}

/// `events` in one read for each run of lines that a look's packets
/// follow and one for the rest, so the packets come a read later than
/// the people they place.
fn reads_apart(events: &[LookEvent]) -> Vec<Vec<u8>> {
    let mut reads = vec![Vec::new()];
    let mut after_line = false;
    for event in events {
        let gmcp = matches!(event, LookEvent::Gmcp { .. });
        if gmcp && after_line {
            reads.push(Vec::new());
        }
        after_line = matches!(event, LookEvent::Line { .. });
        reads
            .last_mut()
            .expect("a read")
            .extend(wire(std::slice::from_ref(event)));
    }
    reads
}

/// What the terminal shows for `events`, each line as `shows` gives it
/// from the line as sent, what it is to the look, and the preset color
/// looks.json names for it. Each prompt shows as sent.
fn expected(events: &[LookEvent], shows: &dyn Fn(&str, Listed, Option<&str>) -> String) -> String {
    let mut out = String::new();
    for event in events {
        match event {
            LookEvent::Gmcp { .. } => {}
            LookEvent::Line {
                line,
                room,
                target,
                preset,
            } => {
                let listed = match (room, target) {
                    (_, true) => Listed::Target,
                    (true, false) => Listed::Room,
                    (false, false) => Listed::No,
                };
                out.push_str(&shows(line, listed, preset.as_deref()));
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

/// What a line of a look is, as looks.json marks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listed {
    /// No line the room lists.
    No,
    /// An army, a thing or a person the room lists.
    Room,
    /// The line of the person the case targets, a room line too.
    Target,
}

/// A connection that targets what `case` gives `tar`, if anything.
fn targeting(case: &LookCase) -> Connection {
    let mut c = Connection::default();
    c.target.name = case.target.clone();
    c
}

/// `line` without its ANSI codes, wrapped in `open` and a reset. A line
/// the game ends with its own reset, as `show_room_armies` ends each army
/// line, keeps that reset in place of the highlight's.
fn wrapped(open: &str, line: &str) -> String {
    let plain = vosh_protocol::ansi::plain_text(line.as_bytes());
    if line.ends_with(GAME_RESET) {
        format!("{open}{plain}{GAME_RESET}")
    } else {
        format!("{open}{plain}\x1b[0m")
    }
}

/// The reset the server sends for two backticks.
const GAME_RESET: &str = "\x1b[0;0m";

/// `line` as sent under the base color `open`, which opens it, comes back
/// after each of the game's resets, and closes with a reset. The fixtures
/// reset with `ESC[0;0m`, the code the server sends for two backticks.
fn based(open: &str, line: &str) -> String {
    format!(
        "{open}{}\x1b[0m",
        line.replace("\x1b[0;0m", &format!("\x1b[0;0m{open}"))
    )
}

/// The SGR open the preset uses for a color named in a fixture.
fn open_for(color: &str) -> &'static str {
    match color {
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "bright_red" => "\x1b[91m",
        other => panic!("no preset color {other}"),
    }
}

/// A trigger on `target` that colors a whole line yellow.
fn yellow(
    name: &str,
    target: vosh_automation::trigger::TriggerTarget,
) -> vosh_automation::trigger::Trigger {
    vosh_automation::trigger::Trigger {
        priority: 4,
        target,
        ..vosh_automation::trigger::Trigger::new(
            name,
            "^.+$",
            vosh_automation::trigger::TriggerAction::Highlight {
                style: vosh_automation::trigger::HighlightStyle {
                    fg: Some(vosh_automation::trigger::NamedColor::Yellow),
                    ..Default::default()
                },
            },
        )
    }
}

#[test]
fn a_room_trigger_colors_the_things_and_people_of_each_look_and_nothing_else() {
    let cases = looks();
    assert!(cases.len() >= 6);
    for case in &cases {
        // The line of your target is a room line too.
        let mut c = targeting(case);
        let mut p = Profile::default();
        p.triggers
            .set(yellow(
                "room",
                vosh_automation::trigger::TriggerTarget::Room,
            ))
            .unwrap();
        let shown = read(&mut p, &mut c, &wire(&case.events));
        let want = expected(&case.events, &|line, listed, _| match listed {
            Listed::Room | Listed::Target => wrapped("\x1b[33m", line),
            Listed::No => line.to_string(),
        });
        assert_eq!(shown, want, "{}", case.name);
    }
}

#[test]
fn a_your_target_trigger_colors_the_line_of_your_target_and_nothing_else() {
    let cases = looks();
    assert!(cases.iter().filter(|c| c.target.is_some()).count() >= 4);
    for case in &cases {
        let mut c = targeting(case);
        let mut p = Profile::default();
        let mut target = yellow(
            "target",
            vosh_automation::trigger::TriggerTarget::RoomTarget,
        );
        target.priority = 5;
        target.actions = vec![vosh_automation::trigger::TriggerAction::Highlight {
            style: vosh_automation::trigger::HighlightStyle {
                fg: Some(vosh_automation::trigger::NamedColor::BrightRed),
                ..Default::default()
            },
        }];
        p.triggers.set(target).unwrap();
        p.triggers
            .set(yellow(
                "room",
                vosh_automation::trigger::TriggerTarget::Room,
            ))
            .unwrap();
        let shown = read(&mut p, &mut c, &wire(&case.events));
        let want = expected(&case.events, &|line, listed, _| match listed {
            Listed::Target => wrapped("\x1b[91m", line),
            Listed::Room => wrapped("\x1b[33m", line),
            Listed::No => line.to_string(),
        });
        assert_eq!(shown, want, "{}", case.name);
    }
}

#[test]
fn with_no_target_no_line_is_the_line_of_your_target() {
    let case = looks()
        .into_iter()
        .find(|c| c.target.is_some())
        .expect("a look with a target");
    let mut p = Profile::default();
    p.triggers
        .set(yellow(
            "target",
            vosh_automation::trigger::TriggerTarget::RoomTarget,
        ))
        .unwrap();
    let shown = read(&mut p, &mut Connection::default(), &wire(&case.events));
    assert_eq!(
        shown,
        expected(&case.events, &|line, _, _| line.to_string())
    );
}

#[test]
fn an_exits_line_someone_says_opens_no_look() {
    let mut p = Profile::default();
    p.triggers
        .set(yellow(
            "room",
            vosh_automation::trigger::TriggerTarget::Room,
        ))
        .unwrap();
    // languages.c, $n says with the text in `# bold yellow.
    let said = "Tolliver says '\x1b[0;1;33m[Exits: south]\x1b[0;0m'";
    let helm = "     A black-steel helm is here, gleaming darkly.";
    let shown = read(
        &mut p,
        &mut Connection::default(),
        format!("{said}\n\r{helm}\n\r").as_bytes(),
    );
    assert_eq!(shown, format!("{said}\r\n{helm}\r\n"));
}

#[test]
fn line_triggers_still_see_every_line_of_a_look() {
    let case = &looks()[0];
    let mut p = Profile::default();
    p.triggers
        .set(yellow("all", vosh_automation::trigger::TriggerTarget::Line))
        .unwrap();
    let shown = read(&mut p, &mut Connection::default(), &wire(&case.events));
    for event in &case.events {
        if let LookEvent::Line { line, .. } = event {
            // The yellow opens on each line's text, after any codes the
            // game sent ahead of it, a reset first where the game had set
            // a color.
            let plain = vosh_protocol::ansi::plain_text(line.as_bytes());
            if !plain.is_empty() {
                assert!(
                    shown.contains(&format!("\x1b[33m{plain}"))
                        || shown.contains(&format!("\x1b[0;33m{plain}")),
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
        crate::session::room_block::EXITS_PATTERN,
        "the preset and the session read the same exits line"
    );
}

#[test]
fn the_preset_colors_each_look_as_the_mockups_draw_it() {
    for case in &looks() {
        let mut p = preset_profile();
        let mut c = targeting(case);
        let shown = read(&mut p, &mut c, &wire(&case.events));
        // Your target in bright red over the room yellow.
        assert_eq!(shown, preset_expected(&case.events), "{}", case.name);
    }
}

/// What the preset shows for `events`, as looks.json marks each line.
fn preset_expected(events: &[LookEvent]) -> String {
    expected(events, &|line, listed, preset| match (listed, preset) {
        (Listed::Target, _) => based(open_for("bright_red"), line),
        (Listed::Room, _) => based(open_for("yellow"), line),
        (Listed::No, Some("green")) => based(open_for("green"), line),
        (Listed::No, Some(color)) => wrapped(open_for(color), line),
        (Listed::No, None) => line.to_string(),
    })
}

/// Whether the game sends the packets of `case` after the text, as it
/// does since d50e4a24.
fn packets_follow(case: &LookCase) -> bool {
    case.events
        .windows(2)
        .any(|pair| matches!(pair, [LookEvent::Line { .. }, LookEvent::Gmcp { .. }]))
}

#[test]
fn a_walk_and_a_goto_color_your_target_on_the_first_look_where_the_packets_follow() {
    for name in [
        "a move west into a full room after packets that follow",
        "an immortal's goto from the bank to the crossroads after packets that follow",
    ] {
        let case = looks()
            .into_iter()
            .find(|c| c.name.starts_with(name))
            .expect("the case");
        assert!(packets_follow(&case));
        let target = case
            .events
            .iter()
            .filter(|e| matches!(e, LookEvent::Line { target: true, .. }))
            .count();
        assert_eq!(target, 1, "{name} marks your target once");
        let mut p = preset_profile();
        let mut c = targeting(&case);
        let shown = read(&mut p, &mut c, &wire(&case.events));
        assert_eq!(shown, preset_expected(&case.events), "{name}");
    }
}

#[test]
fn packets_a_read_after_the_people_color_your_target_again_in_place() {
    let mut split = 0;
    for case in &looks() {
        let mut p = preset_profile();
        let mut c = targeting(case);
        let mut reads = Reads::default();
        let outs: Vec<Output> = reads_apart(&case.events)
            .iter()
            .map(|data| reads.read(&mut p, &mut c, data))
            .collect();
        split += usize::from(outs.len() > 1);
        assert_eq!(
            screen(&outs),
            preset_expected(&case.events),
            "{}",
            case.name
        );
        // A replace comes only for a target whose line showed in the
        // read before its packet.
        let replaced = outs.iter().filter(|out| out.replace.is_some()).count();
        let late = packets_follow(case)
            && case
                .events
                .iter()
                .any(|e| matches!(e, LookEvent::Line { target: true, .. }));
        assert_eq!(replaced > 0, late, "{}", case.name);
    }
    assert!(split >= 4);
}

#[test]
fn where_the_packets_come_first_no_row_waits_to_be_colored_again() {
    for case in looks().iter().filter(|c| !packets_follow(c)) {
        let mut p = preset_profile();
        let mut c = targeting(case);
        let out = Reads::default().read(&mut p, &mut c, &wire(&case.events));
        let text = String::from_utf8(out.bytes).unwrap();
        assert!(!text.contains("\x1b]7717;"), "{}", case.name);
    }
}

#[test]
fn the_ring_keeps_your_target_as_the_screen_shows_it_after_the_packet() {
    let mut ring = crate::logs::Scrollback::default();
    ring.push(b"A young werebeast stands here, leaning on his spear.".to_vec());
    ring.push(b"Tolliver is resting here.".to_vec());
    ring.recolor(&vosh_prompt::stage::Recolored {
        was: Some(b"A young werebeast stands here, leaning on his spear.".to_vec()),
        now: Some(b"\x1b[91mA young werebeast stands here, leaning on his spear.\x1b[0m".to_vec()),
        after: 1,
    });
    // A line that no longer reads as it showed stays.
    ring.recolor(&vosh_prompt::stage::Recolored {
        was: Some(b"Maren is resting here.".to_vec()),
        now: Some(b"\x1b[91mMaren is resting here.\x1b[0m".to_vec()),
        after: 0,
    });
    assert_eq!(
        ring.lines().collect::<Vec<_>>(),
        [
            &b"\x1b[91mA young werebeast stands here, leaning on his spear.\x1b[0m"[..],
            &b"Tolliver is resting here."[..],
        ]
    );
}

/// The `WiZNET` tag as `act_wiz.c` sends it after the bold white of its
/// W, the grey i and then ZNET in bold white again.
const WIZNET_TAG: &str = "W\x1b[0;1;30mi\x1b[0;1;37mZNET";

#[test]
fn the_preset_colors_each_line_it_names_and_leaves_every_near_miss_alone() {
    let lines = preset_lines();
    assert!(lines.iter().any(|l| l.trigger.is_none()));
    for case in &lines {
        let mut p = preset_profile();
        let shown = read(
            &mut p,
            &mut Connection::default(),
            format!("{}\n\r", case.line).as_bytes(),
        );
        let plain = vosh_protocol::ansi::plain_text(case.line.as_bytes());
        let want = match case.trigger.as_deref() {
            None => case.line.clone(),
            // The exits line keeps the game's codes, such as a trap's red +.
            Some("room.exits") => {
                assert_eq!(case.span.as_deref(), Some(plain.as_str()));
                based(open_for("green"), &case.line)
            }
            // The game sends a time of day message with no codes, and the
            // blue wraps it whole.
            Some("time.of_day") => {
                assert_eq!(case.span.as_deref(), Some(case.line.as_str()));
                format!("\x1b[34m{}\x1b[0m", case.line)
            }
            // A change in the weather turns the weather blue whole, in
            // place of the bold white the game sends with a change in the
            // sky. With no ground the blue draws as the preset sets it.
            Some("weather.change") => {
                assert_eq!(case.span.as_deref(), Some(plain.as_str()));
                format!("\x1b[38;2;143;167;217m{plain}\x1b[0m")
            }
            // The tag turns bold magenta whole, the grey i in it too, and
            // the rest of the line keeps the codes the game sent, the
            // colors of the message included.
            Some("wiznet.tag") => {
                assert_eq!(case.span.as_deref(), Some("WiZNET"));
                assert!(case.line.contains(WIZNET_TAG), "{plain}");
                case.line.replacen(WIZNET_TAG, "\x1b[0;1;35mWiZNET", 1)
            }
            Some(other) => panic!("no preset trigger {other}"),
        };
        assert_eq!(shown, format!("{want}\r\n"), "{plain}");
    }
}

#[test]
fn your_own_highlight_on_a_name_draws_over_the_room_color() {
    let mut p = preset_profile();
    let mut name = yellow("friend", vosh_automation::trigger::TriggerTarget::Line);
    name.patterns[0].pattern = "Tolliver".to_string();
    name.priority = 5;
    name.actions = vec![vosh_automation::trigger::TriggerAction::Highlight {
        style: vosh_automation::trigger::HighlightStyle {
            fg: Some(vosh_automation::trigger::NamedColor::Cyan),
            ..Default::default()
        },
    }];
    p.triggers.set(name).unwrap();
    let shown = read(
        &mut p,
        &mut Connection::default(),
        b"\xff\xfa\xc9Room.Chars [{\"name\":\"Tolliver\",\"npc\":false}]\xff\xf0\
          [Exits: south]\n\rTolliver is resting here.\n\r",
    );
    assert_eq!(
        shown,
        "\x1b[32m[Exits: south]\x1b[0m\r\n\
         \x1b[33m\x1b[36mTolliver\x1b[0m\x1b[33m is resting here.\x1b[0m\r\n"
    );
}

#[test]
fn a_line_finds_the_next_room_chars_of_its_read_and_none_past_a_ga() {
    let chars = b"\xff\xfa\xc9Room.Chars [{\"name\":\"Maren\",\"npc\":false}]\xff\xf0";
    let mut data =
        b"<1020hp 800m 930mv> \xff\xf9[Exits: south]\n\rMaren is resting here.\n\r".to_vec();
    data.extend_from_slice(chars);
    data.extend_from_slice(b"\n\r<1020hp 800m 930mv> \xff\xf9Orla arrives from the south.\n\r");
    let events = Parser::new().feed(&data);
    let ahead = room_chars_ahead(&events);
    let names: Vec<Option<Vec<String>>> = ahead
        .iter()
        .map(|chars| {
            chars
                .as_ref()
                .map(|c| c.iter().map(|ch| ch.name.clone()).collect())
        })
        .collect();
    let maren = Some(vec!["Maren".to_string()]);
    // The prompt, its GA, the look, the packet, the prompt after it, its
    // GA and the arrival. The prompt before the GA finds none.
    assert_eq!(names.len(), 7, "{events:?}");
    assert_eq!(names, [None, maren.clone(), maren, None, None, None, None]);
    // A read with no Room.Chars looks ahead for nothing.
    assert_eq!(
        room_chars_ahead(&Parser::new().feed(b"Orla arrives.\n\r")),
        [None; 0]
    );
}
