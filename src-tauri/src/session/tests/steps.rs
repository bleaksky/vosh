//! Your prompt and the lines around it, played through the session's
//! own steps from the capture and the draw to the held tank block and
//! the stored wire reads.

use super::*;

/// James's design, with colors by how full and the `%)h` trick that
/// prints a percent sign.
const TEMPLATE: &str = vosh_prompt::testkit::designs::JAMES;

/// The game prompt `%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c` at full.
const PROMPT_LINE: &str = "[1020/1020hp 800/800mn 930/930mv]";

/// The capture the migration writes from the trigger `#prompt` used
/// to write, unanchored as the trigger was.
const CAPTURE: &str =
    r"\[(?<hp>\d+)/(?<maxhp>\d+)hp (?<mana>\d+)/(?<maxmana>\d+)mn (?<move>\d+)/(?<maxmove>\d+)mv\]";

/// A regex capture of `pattern`, as `[prompt.capture]` holds it.
fn regex_capture(pattern: &str, settle: bool) -> vosh_prompt::CaptureConfig {
    vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
        lines: vec![pattern.to_string()],
        settle,
        ..vosh_prompt::config::RegexCapture::default()
    })
}

/// A profile that reads the prompt with the migrated capture and
/// draws `template` in its place.
fn capture_profile(template: &str) -> Live {
    let (mut p, mut c) = Live::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig {
            draw: true,
            template: template.to_string(),
            capture: regex_capture(CAPTURE, false),
            ..vosh_prompt::PromptConfig::default()
        },
    );
    (p, c)
}

/// Run one complete line through the session's line step, with your
/// target on `c`, and return the drawn prompt, the bytes after its region
/// mark, when it drew.
fn draw_line(p: &mut Profile, c: &mut super::Connection, line: &str) -> Option<String> {
    let mut batch = super::ReadBatch::new(false);
    let _ = super::line_step(
        p,
        c,
        &mut batch,
        super::Line {
            bytes: line.as_bytes().to_vec(),
            painted: None,
        },
        vosh_protocol::ansi::plain_text(line.as_bytes()),
        tokio::time::Instant::now(),
        None,
    );
    c.prompt.stage.open_row()?;
    drawn_in(&batch.out.bytes)
}

/// The bytes after the last region mark, as text.
fn drawn_in(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let at = text.rfind("\x1b]7717;o;")?;
    let rest = &text[at..];
    let end = rest.find('\x07')?;
    Some(rest[end + 1..].to_string())
}

/// `ansi` as plain text. These tests write their lines as text, so this
/// takes a str where the harness's `plain` takes bytes.
fn plain(ansi: &str) -> String {
    vosh_protocol::ansi::plain_text(ansi.as_bytes())
}

/// What the game sends: bytes with a GA wherever `*` stands.
fn stream(text: &str) -> Vec<Ev> {
    let mut events = Vec::new();
    for (i, part) in text.split('*').enumerate() {
        if i > 0 {
            events.push(Ev::Ga);
        }
        if !part.is_empty() {
            events.push(Ev::Data(part.as_bytes().to_vec()));
        }
    }
    events
}

/// `events` cut into two reads after `at` bytes, a GA counting as
/// one.
fn split_reads(events: &[Ev], at: usize) -> [Vec<Ev>; 2] {
    let mut reads: [Vec<Ev>; 2] = [Vec::new(), Vec::new()];
    let mut seen = 0;
    for event in events {
        match event {
            Ev::Ga => {
                reads[usize::from(seen >= at)].push(Ev::Ga);
                seen += 1;
            }
            Ev::Data(data) => {
                let cut = at.saturating_sub(seen).min(data.len());
                if cut > 0 {
                    reads[0].push(Ev::Data(data[..cut].to_vec()));
                }
                if cut < data.len() {
                    reads[1].push(Ev::Data(data[cut..].to_vec()));
                }
                seen += data.len();
            }
        }
    }
    reads
}

/// How many cuts `events` has, a GA counting as one byte.
fn stream_len(events: &[Ev]) -> usize {
    events
        .iter()
        .map(|e| match e {
            Ev::Data(data) => data.len(),
            Ev::Ga => 1,
        })
        .sum()
}

/// The screen a native grid `columns` wide shows after `reads`, rows
/// trimmed, up to the last row that shows anything.
fn screen_of(profile: &dyn Fn() -> Live, columns: usize, reads: &[Vec<Ev>]) -> Vec<String> {
    let mut wire = Wire::new(profile());
    let mut grid = crate::native::grid::TermGrid::new(columns, 40);
    for read in reads {
        grid.session_output(&wire.read_events(read));
    }
    let mut rows: Vec<String> = (0..grid.screen_lines())
        .map(|line| grid.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// Cut `text` into two reads at every byte, and check each screen is
/// the one a single read gives, at 40 and 12 wide, for the profile
/// `profile` makes. Returns the 40 wide screen.
fn same_at_every_split(profile: &dyn Fn() -> Live, text: &str) -> Vec<String> {
    let events = stream(text);
    let mut wide = Vec::new();
    for columns in [40, 12] {
        let whole = screen_of(profile, columns, std::slice::from_ref(&events));
        for at in 1..stream_len(&events) {
            let reads = split_reads(&events, at);
            assert_eq!(
                screen_of(profile, columns, &reads),
                whole,
                "{columns} wide, cut after {at}: {reads:?}"
            );
        }
        if columns == 40 {
            wide = whole;
        }
    }
    wide
}

fn with(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn a_line_prompt_draws_in_place_with_no_line_end() {
    let mut wire = Wire::new(capture_profile(HP));
    let out = wire.read(b"You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &wire.mark(1), b"<1020>\x1b[0m"])
    );
    assert_eq!(out.replace, None);
    let open = wire.c.prompt.stage.open_row().expect("the open row");
    assert_eq!(open.gen, wire.gen0 + 1);
}

#[test]
fn the_prompt_draws_the_template_byte_for_byte() {
    let (mut p, mut c) = capture_profile(TEMPLATE);
    let drawn = draw_line(&mut p, &mut c, PROMPT_LINE).expect("the prompt draws");
    assert_eq!(plain(&drawn), "[1020(100%)h 800(100%)m 930(100%)v] ");
    // Health at full in the theme's green, where the first renderer
    // drew 256 color 42. Every other byte is as it drew them.
    assert!(drawn.contains("\x1b[32m100"), "{drawn:?}");
    let first = drawn.replace("\x1b[32m100", "\x1b[38;5;42m100");
    assert_eq!(
        first,
        "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m1020(\x1b[38;5;42m100\x1b[0m\x1b[3m%)h 800(\x1b[38;2;128;200;255m100\x1b[0m\x1b[3m%)m 930(\x1b[38;2;200;255;23m100\x1b[0m\x1b[3m%)v\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m"
    );
    assert!(drawn.ends_with("\x1b[0m"));

    // Any other line shows as sent and draws nothing.
    assert_eq!(draw_line(&mut p, &mut c, "You are hungry."), None);
}

#[test]
fn with_drawing_off_the_prompt_shows_as_sent_and_is_logged() {
    let (mut p, mut c) = capture_profile(HP);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    let mut wire = Wire::new((p, c));
    let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]\n\r");
    assert_eq!(out.bytes, b"[1020/1020hp 800/800mn 930/930mv]\r\n");
    assert_eq!(wire.c.prompt.stage.open_row(), None);
    // The capture still read it.
    let vars = wire.c.prompt.vars.prompt_vars();
    assert_eq!(vars.get("hp").map(String::as_str), Some("1020"));

    let mut batch = super::ReadBatch::new(false);
    let step = super::line_step(
        &mut wire.p,
        &mut wire.c,
        &mut batch,
        super::Line {
            bytes: PROMPT_LINE.as_bytes().to_vec(),
            painted: None,
        },
        PROMPT_LINE.to_string(),
        tokio::time::Instant::now(),
        Some(7),
    );
    assert_eq!(batch.log.len(), 1, "a prompt that shows is logged");
    assert_eq!(step.len(), 1);
    assert_eq!(step[0].scrollback, [PROMPT_LINE.as_bytes()]);
    // A drawn prompt is neither logged nor kept for scrollback.
    let (mut p, mut c) = capture_profile(HP);
    let mut batch = super::ReadBatch::new(false);
    let step = super::line_step(
        &mut p,
        &mut c,
        &mut batch,
        super::Line {
            bytes: PROMPT_LINE.as_bytes().to_vec(),
            painted: None,
        },
        PROMPT_LINE.to_string(),
        tokio::time::Instant::now(),
        Some(7),
    );
    assert!(batch.log.is_empty());
    assert_eq!(step.len(), 1);
    let leftover = &step[0].scrollback;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(batch.prompt_vars, "the prompt vars follow a prompt");
}

#[test]
fn a_profile_without_a_capture_shows_the_game_prompt() {
    let (mut p, mut c) = Live::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, HP),
    );
    let mut wire = Wire::new((p, c));
    let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]\n\r");
    assert_eq!(out.bytes, b"[1020/1020hp 800/800mn 930/930mv]\r\n");
    assert_eq!(wire.c.prompt.stage.open_row(), None);
}

#[test]
fn a_prompt_split_across_reads_replaces_its_painted_start() {
    let mut wire = Wire::new(capture_profile(HP));
    let first = wire.read(b"You are hungry.\n\r[1020/1020hp 80");
    assert_eq!(
        first.bytes,
        with(&[b"You are hungry.\r\n", &wire.mark(1), b"[1020/1020hp 80"])
    );
    let second = wire.read(b"0/800mn 930/930mv]\n\r");
    assert_eq!(
        second.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), b"<1020>\x1b[0m"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_line_split_across_reads_replaces_its_painted_start() {
    let mut wire = Wire::new(capture_profile(HP));
    let first = wire.read(b"You are hun");
    assert_eq!(first.bytes, with(&[&wire.mark(1), b"You are hun"]));
    // The partial grew, so it paints again whole.
    let second = wire.read(b"gry");
    assert_eq!(
        second.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), b"You are hungry"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let third = wire.read(b".\n\rNext.\n\r");
    assert_eq!(
        third.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 2,
            bytes: b"You are hungry.\r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    assert_eq!(third.bytes, b"Next.\r\n");
}

#[test]
fn a_partial_the_capture_settles_on_draws_in_the_same_read() {
    let (mut p, mut c) = capture_profile(HP);
    let config = vosh_prompt::PromptConfig {
        capture: regex_capture(r"^<(?<hp>\d+)hp (?<mana>\d+)m> $", true),
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    let mut wire = Wire::new((p, c));
    let out = wire.read(b"You are hungry.\n\r<100hp 50m> ");
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &wire.mark(1), b"<100>\x1b[0m"])
    );
    assert_eq!(wire.acc.partial(), None, "a drawn partial is gone");

    // Split before the final space, it paints and waits, then draws
    // over the painted start.
    let first = wire.read(b"\n\r<90hp 50m>");
    assert_eq!(first.bytes, with(&[b"\r\n", &wire.mark(2), b"<90hp 50m>"]));
    let second = wire.read(b" ");
    assert_eq!(
        second.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 2,
            bytes: with(&[&wire.mark(3), b"<90>\x1b[0m"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
}

#[test]
fn an_unanchored_capture_waits_for_the_line_end() {
    let mut wire = Wire::new(capture_profile(HP));
    let out = wire.read(b"[1020/1020hp 800/800mn 930/930mv]");
    assert_eq!(
        out.bytes,
        with(&[&wire.mark(1), b"[1020/1020hp 800/800mn 930/930mv]"])
    );
    assert_eq!(wire.c.prompt.stage.open_row(), None);
}

#[test]
fn a_ga_in_the_same_read_draws_with_no_flash() {
    let mut wire = Wire::new(capture_profile(HP));
    let out = wire.read_ga(b"[1020/1020hp 800/800mn 930/930mv]");
    assert_eq!(out.bytes, with(&[&wire.mark(1), b"<1020>\x1b[0m"]));
    assert_eq!(out.replace, None);
    assert_eq!(wire.acc.partial(), None);
}

#[test]
fn a_ga_in_the_next_read_draws_over_the_painted_prompt() {
    let mut wire = Wire::new(capture_profile(HP));
    let first = wire.read(b"[1020/1020hp 800/800mn 930/930mv]");
    assert_eq!(
        first.bytes,
        with(&[&wire.mark(1), b"[1020/1020hp 800/800mn 930/930mv]"])
    );
    let second = wire.read_ga(b"");
    assert_eq!(
        second.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), b"<1020>\x1b[0m"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_ga_on_a_partial_nothing_reads_ends_its_row() {
    let mut wire = Wire::new(Live::default());
    let out = wire.read_ga(b"<100hp> ");
    assert_eq!(out.bytes, b"<100hp> \r\n");
    let first = wire.read(b"<90hp> ");
    assert_eq!(first.bytes, with(&[&wire.mark(1), b"<90hp> "]));
    let second = wire.read_ga(b"");
    assert_eq!(second.bytes, b"\r\n");
    assert_eq!(second.replace, None);
}

#[test]
fn a_ga_on_a_partial_that_grew_after_its_paint_writes_the_whole_of_it() {
    // The game's prompt, cut inside by TCP, on a profile that reads
    // no prompt.
    let mut wire = Wire::new(Live::default());
    let first = wire.read(b"Huh?\n\r<100hp 50");
    assert_eq!(
        first.bytes,
        with(&[b"Huh?\r\n", &wire.mark(1), b"<100hp 50"])
    );
    let second = wire.read_ga(b"m 30mv> ");
    assert_eq!(
        second.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: b"<100hp 50m 30mv> \r\n".to_vec(),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &second.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_game_prompt_nothing_reads_shows_whole_wherever_the_reads_split() {
    let screen = same_at_every_split(
        &Live::default,
        "Huh?\n\r<1020hp 800m 930mv> *\n\rYou are hungry.\n\r<1020hp 800m 930mv> *",
    );
    assert_eq!(
        screen,
        [
            "Huh?",
            "<1020hp 800m 930mv>",
            "",
            "You are hungry.",
            "<1020hp 800m 930mv>"
        ]
    );
}

/// A profile that draws `<%hp>` from a capture that settles.
fn settling_profile(draw: bool) -> Live {
    let (mut p, mut c) = capture_profile(HP);
    let config = vosh_prompt::PromptConfig {
        draw,
        capture: regex_capture(r"^<(?<hp>\d+)hp (?<mana>\d+)m> $", true),
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    (p, c)
}

#[test]
fn a_prompt_whole_before_its_line_end_keeps_the_line_end() {
    let mut wire = Wire::new(settling_profile(true));
    let out = wire.read(b"<1020hp 800m> \n\rA rat arrives.\n\r");
    assert_eq!(
        out.bytes,
        with(&[&wire.mark(1), b"<1020>\x1b[0m\r\n", b"A rat arrives.\r\n"])
    );
}

#[test]
fn a_prompt_that_settles_draws_the_same_wherever_the_reads_split() {
    let draws = || settling_profile(true);
    let shows = || settling_profile(false);
    // No GA: the line end after a prompt ends its row, as it does
    // when the prompt settled at the end of an earlier read.
    let text = "You are hungry.\n\r<1020hp 800m> \n\rA rat arrives.\n\r<1000hp 800m> ";
    assert_eq!(
        same_at_every_split(&draws, text),
        ["You are hungry.", "<1020>", "A rat arrives.", "<1000>"]
    );
    assert_eq!(
        same_at_every_split(&shows, text),
        [
            "You are hungry.",
            "<1020hp 800m>",
            "A rat arrives.",
            "<1000hp 800m>"
        ]
    );
    // With a GA, drawn or shown as sent, the prompt keeps the cursor
    // after it, as it does when it settled before the GA came.
    let text = "<1020hp 800m> *\n\rA rat arrives.\n\r<1000hp 800m> *";
    assert_eq!(
        same_at_every_split(&draws, text),
        ["<1020>", "A rat arrives.", "<1000>"]
    );
    assert_eq!(
        same_at_every_split(&shows, text),
        ["<1020hp 800m>", "A rat arrives.", "<1000hp 800m>"]
    );
}

#[test]
fn a_prompt_that_waits_for_its_line_end_draws_the_same_wherever_the_reads_split() {
    // The migrated capture on a prompt ending in %c, which the game
    // follows with a space and a GA.
    let text = "You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r *\n\rA rat arrives.\n\r[1000/1020hp 800/800mn 930/930mv]\n\r *";
    assert_eq!(
        same_at_every_split(&|| capture_profile(HP), text),
        ["You are hungry.", "<1020>", "", "A rat arrives.", "<1000>"]
    );
}

#[test]
fn the_open_row_closes_on_a_send_and_on_other_output() {
    let mut wire = Wire::new(capture_profile(HP));
    let _ = wire.read(PROMPT_ROW);
    assert!(wire.c.prompt.stage.open_row().is_some());
    wire.send();
    assert_eq!(wire.c.prompt.stage.open_row(), None);

    let _ = wire.read(PROMPT_ROW);
    assert!(wire.c.prompt.stage.open_row().is_some());
    let _ = wire.read_with(b"", false, true);
    assert_eq!(
        wire.c.prompt.stage.open_row(),
        None,
        "output from elsewhere"
    );

    let _ = wire.read(PROMPT_ROW);
    let _ = wire.read(b"You flee!\n\r");
    assert_eq!(wire.c.prompt.stage.open_row(), None, "a line after it");
}

const PROMPT_ROW: &[u8] = b"[1020/1020hp 800/800mn 930/930mv]\n\r";

#[test]
fn a_capture_trigger_with_no_reader_hides_the_prompt_and_is_named_once() {
    // The trigger older builds wrote for `#prompt`.
    let (mut p, mut c) = Live::default();
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            name: "prompt-capture".into(),
            patterns: vec![vosh_automation::trigger::TriggerPattern::regex(CAPTURE)],
            priority: 100,
            enabled: true,
            actions: vec![
                vosh_automation::trigger::TriggerAction::Gag,
                vosh_automation::trigger::TriggerAction::Script {
                    body: "mud.set_prompt_var(\"hp\", captures[2])".into(),
                },
            ],
            preset: None,
            group: None,
            target: vosh_automation::trigger::TriggerTarget::Line,
            alert: None,
        })
        .expect("the trigger compiles");
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig::from_legacy(true, HP),
    );
    let mut wire = Wire::new((p, c));
    let batch = wire.read_with(PROMPT_ROW, false, false);
    // The trigger hid the prompt, and nothing draws in its place.
    assert!(batch.out.is_empty());
    assert_eq!(batch.gag_without_reader, ["prompt-capture"]);
    let batch = wire.read_with(PROMPT_ROW, false, false);
    assert!(batch.gag_without_reader.is_empty(), "once a session");

    // With a capture in the profile, the capture reads the prompt
    // and the trigger never sees it.
    let config = vosh_prompt::PromptConfig {
        capture: regex_capture(CAPTURE, false),
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let batch = wire.read_with(PROMPT_ROW, false, false);
    assert_eq!(drawn_in(&batch.out.bytes).as_deref(), Some("<1020>\x1b[0m"));
    let leftover = &batch.gag_without_reader;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_prompts_trigger_acts_on_the_recognized_prompt() {
    let (mut p, mut c) = capture_profile(HP);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            target: vosh_automation::trigger::TriggerTarget::Prompt,
            ..vosh_automation::trigger::Trigger::new(
                "mark",
                "hp",
                vosh_automation::trigger::TriggerAction::Replace {
                    template: "HP".into(),
                },
            )
        })
        .expect("the trigger compiles");
    let mut wire = Wire::new((p, c));
    let out = wire.read(PROMPT_ROW);
    assert_eq!(out.bytes, b"[1020/1020HP 800/800mn 930/930mv]\r\n");
}

#[test]
fn line_triggers_that_matched_a_read_prompt_are_noted() {
    let (mut p, c) = capture_profile(HP);
    let highlight = |name: &str, pattern: &str, target| vosh_automation::trigger::Trigger {
        target,
        ..vosh_automation::trigger::Trigger::new(
            name,
            pattern,
            vosh_automation::trigger::TriggerAction::Gag,
        )
    };
    for trigger in [
        highlight(
            "hp-watch",
            r"\d+hp",
            vosh_automation::trigger::TriggerTarget::Line,
        ),
        highlight(
            "prompt-look",
            "hp",
            vosh_automation::trigger::TriggerTarget::Prompt,
        ),
        highlight(
            "hungry",
            "hungry",
            vosh_automation::trigger::TriggerTarget::Line,
        ),
    ] {
        p.triggers.set(trigger).expect("the trigger compiles");
    }
    let mut wire = Wire::new((p, c));
    let _ = wire.read(b"You are hungry.\n\r");
    assert_eq!(wire.c.prompt.stage.line_trigger_notice(), None);
    // The Line trigger does not hide the prompt, since it never sees
    // it, and it is named.
    let out = wire.read(PROMPT_ROW);
    assert_eq!(drawn_in(&out.bytes).as_deref(), Some("<1020>\x1b[0m"));
    assert_eq!(
        wire.c.prompt.stage.line_trigger_notice(),
        Some(vec!["hp-watch".to_string()])
    );
}

#[test]
fn the_open_row_keeps_where_each_piece_of_the_design_landed() {
    let mut wire = Wire::new(capture_profile("<%hp/%{maxhp}> %mana"));
    let batch = wire.read_with(PROMPT_ROW, false, false);
    assert!(batch.prompt, "the read brought a prompt");
    let spans = |wire: &Wire| -> Vec<(usize, usize, usize)> {
        wire.c
            .prompt
            .stage
            .open_row()
            .map(|o| o.spans.iter().map(|s| (s.piece, s.col, s.width)).collect())
            .unwrap_or_default()
    };
    assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 9), (2, 10, 2), (3, 12, 3)]);
    // With the rows they sit in, which the webview wraps at its width.
    let plain = |wire: &Wire| wire.c.prompt.stage.open_row().map(|o| o.plain.clone());
    assert_eq!(plain(&wire).as_deref(), Some("<1020/1020> 800"));

    // Drawing off shows the game's own line, which has no pieces, and
    // drawing on again brings them back with the repaint.
    let now = tokio::time::Instant::now();
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let _ = super::repaint_step(&wire.p, &mut wire.c, false, now);
    let leftover = &spans(&wire);
    assert!(leftover.is_empty(), "{leftover:?}");
    let config = vosh_prompt::PromptConfig {
        draw: true,
        template: "[%hp]".into(),
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let _ = super::repaint_step(&wire.p, &mut wire.c, false, now);
    assert_eq!(spans(&wire), [(0, 0, 1), (1, 1, 4), (2, 5, 1)]);
    assert_eq!(plain(&wire).as_deref(), Some("[1020]"));

    // Other output closes the row, and its pieces go with it. A line
    // that is no prompt leaves the flag down.
    let batch = wire.read_with(b"You are hungry.\n\r", false, false);
    assert_eq!(wire.c.prompt.stage.open_row(), None);
    assert!(!batch.prompt);
}

#[test]
fn draw_off_repaints_the_open_row_as_the_game_sent_it() {
    let mut wire = Wire::new(capture_profile(HP));
    let _ = wire.read(PROMPT_ROW);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let now = tokio::time::Instant::now();
    let off = super::repaint_step(&wire.p, &mut wire.c, false, now);
    assert_eq!(
        off.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), PROMPT_ROW_SHOWN]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
    let leftover = &off.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");

    // Drawing back on paints the design over the same row, and a new
    // design repaints it.
    let config = vosh_prompt::PromptConfig {
        draw: true,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let on = super::repaint_step(&wire.p, &mut wire.c, false, now);
    assert_eq!(
        on.replace.map(|r| (r.gen, r.bytes)),
        Some((wire.gen0 + 2, with(&[&wire.mark(3), b"<1020>\x1b[0m"])))
    );
    assert!(super::repaint_step(&wire.p, &mut wire.c, false, now).is_empty());
    let config = vosh_prompt::PromptConfig {
        template: "[%hp]".into(),
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let new = super::repaint_step(&wire.p, &mut wire.c, false, now);
    assert_eq!(
        new.replace.map(|r| r.bytes),
        Some(with(&[&wire.mark(4), b"[1020]\x1b[0m"]))
    );

    // Nothing repaints once other output closed the row, after a
    // send, or after the webview wrote to the terminal itself.
    assert!(super::repaint_step(&wire.p, &mut wire.c, true, now).is_empty());
    let _ = wire.read(PROMPT_ROW);
    wire.send();
    assert!(super::repaint_step(&wire.p, &mut wire.c, false, now).is_empty());
    let _ = wire.read(PROMPT_ROW);
    assert!(wire.c.prompt.stage.open_row().is_some());
    wire.c.prompt.stage.close();
    assert!(super::repaint_step(&wire.p, &mut wire.c, false, now).is_empty());
}

#[test]
fn only_a_new_width_closes_the_open_row() {
    let mut wire = Wire::new(capture_profile(HP));
    let mut negotiator = vosh_protocol::telnet::Negotiator::new();
    negotiator.set_window_size(94, 41);
    let _ = wire.read(PROMPT_ROW);
    // The webview sends the size the session already holds on every
    // connect. The row stays open, so turning drawing off repaints it.
    super::window_size_step(&mut wire.c, &mut negotiator, 94, 41, false);
    assert!(wire.c.prompt.stage.open_row().is_some());
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let now = tokio::time::Instant::now();
    let off = super::repaint_step(&wire.p, &mut wire.c, false, now);
    assert_eq!(
        off.replace.map(|r| r.bytes),
        Some(with(&[&wire.mark(2), PROMPT_ROW_SHOWN]))
    );

    // A new height wraps nothing again, such as when your prompt
    // leaves the band for the text and the terminal grows, so the
    // row stays open and each change of drawing repaints it.
    super::window_size_step(&mut wire.c, &mut negotiator, 94, 43, false);
    assert_eq!(negotiator.window_size, (94, 43));
    assert!(wire.c.prompt.stage.open_row().is_some());
    for draw in [true, false] {
        let open = wire.c.prompt.stage.open_row().map(|r| r.gen);
        let config = vosh_prompt::PromptConfig {
            draw,
            ..wire.c.prompt.config().clone()
        };
        take_config(&mut wire.p, &mut wire.c, config);
        let repaint = super::repaint_step(&wire.p, &mut wire.c, false, now);
        assert_eq!(repaint.replace.map(|r| r.gen), open, "draw {draw}");
    }

    // A new width wraps the row again, so it closes, and nothing
    // repaints.
    super::window_size_step(&mut wire.c, &mut negotiator, 80, 43, false);
    assert_eq!(negotiator.window_size, (80, 43));
    assert!(wire.c.prompt.stage.open_row().is_none());
    let config = vosh_prompt::PromptConfig {
        draw: true,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    assert!(super::repaint_step(&wire.p, &mut wire.c, false, now).is_empty());
}

/// The prompt row as the game sent it, with its line end.
const PROMPT_ROW_SHOWN: &[u8] = b"[1020/1020hp 800/800mn 930/930mv]\r\n";

#[test]
fn the_ring_records_a_candidate_on_every_send_and_ga() {
    // Drawing on.
    let mut wire = Wire::new(capture_profile(HP));
    let _ = wire.read(PROMPT_ROW);
    wire.send();
    let _ = wire.read_ga(b"You say hi.\n\r[1000/1020hp 800/800mn 930/930mv]\n\r");
    let ring: Vec<(String, bool, bool, bool)> = wire
        .c
        .prompt
        .stage
        .ring()
        .map(|c| (c.plain.clone(), c.recognized, c.draw, c.capture))
        .collect();
    assert_eq!(
        ring,
        [
            (PROMPT_LINE.to_string(), true, true, true),
            (
                "[1000/1020hp 800/800mn 930/930mv]".to_string(),
                true,
                true,
                true
            ),
        ]
    );

    // Drawing off.
    let (mut p, mut c) = capture_profile(HP);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    let mut wire = Wire::new((p, c));
    let _ = wire.read(PROMPT_ROW);
    wire.send();
    let entry = wire.c.prompt.stage.ring().next().expect("an entry");
    assert!(entry.recognized && !entry.draw && entry.capture);

    // No capture: the prompt line, and a partial at a send.
    let mut wire = Wire::new(Live::default());
    let _ = wire.read(b"You are hungry.\n\r<100hp> ");
    wire.send();
    let _ = wire.read_ga(b"<90hp> ");
    let _ = wire.read(PROMPT_ROW);
    wire.send();
    let ring: Vec<(String, bool, bool)> = wire
        .c
        .prompt
        .stage
        .ring()
        .map(|c| (c.plain.clone(), c.recognized, c.capture))
        .collect();
    assert_eq!(
        ring,
        [
            ("<100hp> ".to_string(), false, false),
            ("<90hp> ".to_string(), false, false),
            (PROMPT_LINE.to_string(), false, false),
        ]
    );
}

/// A profile that draws `template` over the capture on The Forsaken
/// Lands, started the way the session starts it.
fn forsaken_profile(template: &str) -> Live {
    let (mut p, mut c) = capture_profile(template);
    super::start_prompt(
        &mut p,
        &mut c,
        crate::profile::worlds::is_forsaken_lands("play.theforsakenlands.com"),
    );
    (p, c)
}

/// Hand a packet from fixtures/gmcp/aabahran to the session the way
/// a socket read does.
fn feed(p: &mut Profile, c: &mut Connection, file: &str) {
    let path = format!(
        "{}/../fixtures/gmcp/aabahran/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let msg = vosh_protocol::gmcp::parse(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
    super::observe_prompt_gmcp(p, c, &msg);
}

fn feed_inline(p: &mut Profile, c: &mut Connection, package: &str, data: serde_json::Value) {
    super::observe_prompt_gmcp(
        p,
        c,
        &vosh_protocol::gmcp::Message {
            package: package.into(),
            data,
        },
    );
}

/// The lamented tears cases each server build sends, shared with the
/// store tests in the webview.
fn lament_cases() -> Vec<serde_json::Value> {
    let text = include_str!("../../../../fixtures/gmcp/aabahran/lament.json");
    let doc: serde_json::Value = serde_json::from_str(text).expect("lament.json reads");
    doc["cases"].as_array().expect("a list of cases").clone()
}

#[test]
fn the_three_lament_cases_draw_hidden_vitals_and_report_them() {
    for case in lament_cases() {
        let name = case["name"].as_str().unwrap_or_default();
        let (mut p, mut c) = forsaken_profile(TEMPLATE);
        for file in case["packets"].as_array().expect("packets") {
            feed(&mut p, &mut c, file.as_str().expect("a file name"));
        }
        // The game prints zeros for every vital under the song.
        let drawn = draw_line(&mut p, &mut c, "[0/0hp 0/0mn 0/0mv]").expect("the prompt draws");
        assert_eq!(plain(&drawn), "[?(?%)h ?(?%)m ?(?%)v] ", "{name}");
        // Each mark in bright black, then the look before it.
        assert!(
            drawn.contains("\x1b[3m\x1b[90m?\x1b[39m("),
            "{name}: {drawn:?}"
        );
        // The report the panes read, once.
        let hidden = c
            .prompt
            .vars
            .take_hidden_change()
            .expect("a change to report");
        assert_eq!(
            serde_json::to_value(hidden).expect("it serializes"),
            case["hidden"],
            "{name}"
        );
        assert!(c.prompt.vars.take_hidden_change().is_none(), "{name}");
        // The vitals store never reads a hidden value from the vars.
        let vars = c.prompt.vars.prompt_vars();
        for key in ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"] {
            assert_eq!(vars.get(key).map(String::as_str), Some("?"), "{name} {key}");
        }
    }
}

#[test]
fn a_prompt_draws_again_once_the_song_ends() {
    let cases = lament_cases();
    let older = &cases[2];
    let (mut p, mut c) = forsaken_profile(TEMPLATE);
    for file in older["packets"].as_array().expect("packets") {
        feed(&mut p, &mut c, file.as_str().expect("a file name"));
    }
    assert!(c.prompt.vars.take_hidden_change().is_some());
    // The song ends. Char.Affects comes at once, the rest at the next
    // prompt.
    feed(&mut p, &mut c, "char-affects.gmcp");
    feed(&mut p, &mut c, "char-vitals.gmcp");
    feed(&mut p, &mut c, "group-info-own-row.gmcp");
    let drawn =
        draw_line(&mut p, &mut c, "[850/900hp 760/820mn 250/250mv]").expect("the prompt draws");
    assert_eq!(plain(&drawn), "[850(94%)h 760(93%)m 250(100%)v] ");
    let hidden = c
        .prompt
        .vars
        .take_hidden_change()
        .expect("a change to report");
    assert_eq!(
        serde_json::to_value(hidden).expect("it serializes"),
        serde_json::json!({"vitals":false,"tank":false,"opponent":false,"affects":false,"group":false})
    );
}

#[test]
fn a_reconnect_in_the_song_hides_the_vitals_from_the_prompt_alone() {
    // The older build after a link dead reconnect sends no
    // Char.Affects until the next tick, and Char.Vitals carries the
    // true values. Only the prompt the capture reads shows the song.
    let (mut p, mut c) = forsaken_profile(TEMPLATE);
    feed(&mut p, &mut c, "char-vitals.gmcp");
    let drawn = draw_line(&mut p, &mut c, "[0/0hp 0/0mn 0/0mv]").expect("the prompt draws");
    assert_eq!(plain(&drawn), "[?(?%)h ?(?%)m ?(?%)v] ");
    let hidden = c
        .prompt
        .vars
        .take_hidden_change()
        .expect("a change to report");
    assert_eq!(
        serde_json::to_value(hidden).expect("it serializes"),
        serde_json::json!({"vitals":true,"tank":false,"opponent":false,"affects":false,"group":false})
    );
    let vars = c.prompt.vars.prompt_vars();
    for key in ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"] {
        assert_eq!(vars.get(key).map(String::as_str), Some("?"), "{key}");
    }
}

#[test]
fn other_hosts_hide_nothing() {
    let (mut p, mut c) = capture_profile("%hp/%maxhp %opponent %{opponent_hp:pct}");
    super::start_prompt(
        &mut p,
        &mut c,
        crate::profile::worlds::is_forsaken_lands("127.0.0.1"),
    );
    for file in [
        "char-affects-lament.gmcp",
        "char-vitals.gmcp",
        "char-combat-lament-older.gmcp",
    ] {
        feed(&mut p, &mut c, file);
    }
    assert_eq!(
        plain(&draw_line(&mut p, &mut c, PROMPT_LINE).expect("the prompt draws")),
        "1020/1020 a Blackwatch guard 41"
    );
    assert!(c.prompt.vars.take_hidden_change().is_none());
}

/// The pieces the phase 1 gate draws from GMCP, with a separator
/// between groups.
const GATE: &str = "%gold %opponent|%{moon1:game} %{moon3:word}|%pos %lang %weather %{temp:unit} %region|%tank %{tank_hp:game}|%exits";

/// The packets the new build sends at login and in a fight, plus
/// Char.Worth and World.Moons.
fn new_build_fight(p: &mut Profile, c: &mut Connection) {
    for file in [
        "char-prompt.gmcp",
        "char-vitals.gmcp",
        "char-combat-tank.gmcp",
        "char-state.gmcp",
        "room-weather.gmcp",
        "room-info.gmcp",
    ] {
        feed(p, c, file);
    }
    feed_inline(
        p,
        c,
        "Char.Worth",
        serde_json::json!({"gold":1250,"bank":5000,"exp":125_000,"tnl":1250,"trains":3,"practices":12,"cps":40,"rps":7,"cabal":"none"}),
    );
    feed_inline(
        p,
        c,
        "World.Moons",
        serde_json::json!({"moons":[
            {"name":"Lysenties","active":true,"phase":4,"phase_name":"full and whole"},
            {"name":"Nercuros","active":false,"phase":2,"phase_name":"half-lit and growing"},
            {"name":"Dyphrities","active":true,"phase":7,"phase_name":"a thin crescent, fading"}
        ],"eclipse":false,"triad":false,"near_alignment":true}),
    );
}

#[test]
fn the_session_draws_the_gate_pieces_from_the_new_build_packets() {
    let (mut p, mut c) = forsaken_profile(GATE);
    new_build_fight(&mut p, &mut c);
    assert_eq!(
        plain(&draw_line(&mut p, &mut c, PROMPT_LINE).expect("the prompt draws")),
        "1250 a Blackwatch guard|FUL waning crescent|sit common rainy 60°F Coastal North|Tester [===|===|===|=--]|S"
    );
    // Nothing is hidden, so nothing is reported.
    assert!(c.prompt.vars.take_hidden_change().is_none());
}

#[test]
fn exits_draw_from_room_info_only_on_the_new_build() {
    let (mut p, mut c) = forsaken_profile("[%exits]");
    feed(&mut p, &mut c, "char-vitals.gmcp");
    feed(&mut p, &mut c, "room-info.gmcp");
    // No Char.Prompt this session, so Room.Info feeds no exits.
    assert_eq!(
        plain(&draw_line(&mut p, &mut c, PROMPT_LINE).expect("it draws")),
        "[]"
    );
    feed(&mut p, &mut c, "char-prompt.gmcp");
    assert_eq!(
        plain(&draw_line(&mut p, &mut c, PROMPT_LINE).expect("it draws")),
        "[S]"
    );
}

#[test]
fn vosh_supplies_the_tick_target_tracked_affects_and_profile() {
    let (mut p, mut c) = forsaken_profile("%tick|%{tick:unit}|%target|%{missing:names}|%profile");
    let now = tokio::time::Instant::now();
    c.tick.enable(&mut p.tick, now);
    c.target.name = Some("guard".into());
    p.name = Some(crate::profile::set::DEFAULT_PROFILE_NAME.into());
    p.ui.tracked_affects = vec![crate::profile::ui::TrackedAffect {
        name: "sanctuary".into(),
        label: None,
    }];
    let supplied = crate::prompt::client_values(&p, &c, now);
    let interval = i64::try_from(p.tick.config.interval_secs).expect("seconds");
    assert_eq!(
        supplied.tick,
        Some(vosh_prompt::values::Tick {
            remaining: interval,
            interval: Some(interval),
            since: Some(0),
        })
    );
    feed(&mut p, &mut c, "char-affects.gmcp");
    let drawn = plain(&draw_line(&mut p, &mut c, PROMPT_LINE).expect("it draws"));
    let parts: Vec<&str> = drawn.split('|').collect();
    assert!(
        parts[0]
            .parse::<i64>()
            .is_ok_and(|s| s > 0 && s <= interval),
        "{drawn}"
    );
    assert!(parts[1].ends_with('s'), "{drawn}");
    assert_eq!(&parts[2..], ["guard", "sanctuary", "Default"]);
}

#[test]
fn a_new_connection_starts_the_prompt_over() {
    let (mut p, mut c) = forsaken_profile(GATE);
    new_build_fight(&mut p, &mut c);
    assert!(c.prompt.vars.new_build());
    let _ = draw_line(&mut p, &mut c, PROMPT_LINE);
    assert!(!c.prompt.vars.prompt_vars().is_empty());

    super::end_prompt(&mut p, &mut c);
    assert!(!c.prompt.vars.new_build());
    assert!(c.prompt.vars.prompt_vars().is_empty());
    assert!(c.prompt.vars.gmcp().get("Char.Worth").is_none());
    // The hidden state that ended with the connection is never
    // reported, since the stores clear on the disconnect.
    assert!(c.prompt.vars.take_hidden_change().is_none());
    // The profile's [prompt] table outlives the connection.
    assert!(c.prompt.config().draw);
    assert_eq!(c.prompt.config().template, GATE);

    super::start_prompt(&mut p, &mut c, false);
    assert!(!c.prompt.forsaken());
    assert_eq!(c.prompt.config().template, GATE);
}

/// The tank line James's PROMPT prints while someone in the group tanks,
/// and the prompt after it in a fight.
const TANK_LINE: &str = "Tester: [===|===|---|---]";
const FIGHT_LINE: &str = "[159/1020hp 310/800mn 489/930mv]";

/// A profile that reads Aabahran's codes `prompt` and draws
/// `template` in its place.
pub(super) fn codes_profile(prompt: &str, template: &str) -> Live {
    let (mut p, mut c) = Live::default();
    take_config(
        &mut p,
        &mut c,
        vosh_prompt::PromptConfig {
            draw: true,
            template: template.to_string(),
            capture: vosh_prompt::CaptureConfig::Aabahran(vosh_prompt::config::AabahranCapture {
                prompt: prompt.to_string(),
                ..vosh_prompt::config::AabahranCapture::default()
            }),
            ..vosh_prompt::PromptConfig::default()
        },
    );
    (p, c)
}

#[test]
fn the_codes_read_and_draw_a_one_line_prompt() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let out = wire.read(b"You are hungry.\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
    assert_eq!(
        out.bytes,
        with(&[b"You are hungry.\r\n", &wire.mark(1), b"<1020>\x1b[0m"])
    );
    assert!(wire.c.prompt.stage.open_row().is_some());
}

#[test]
fn a_tank_line_shows_as_sent_when_the_design_reads_nothing_on_it() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
    assert_eq!(
        out.bytes,
        with(&[
            TANK_LINE.as_bytes(),
            b"\r\n",
            &wire.mark(1),
            b"<159>\x1b[0m"
        ])
    );
    // The capture read the whole block.
    let vars = wire.c.prompt.vars.prompt_vars();
    assert_eq!(vars.get("tank").map(String::as_str), Some("Tester"));
    assert_eq!(vars.get("fight").map(String::as_str), Some("1"));
    // Drawing off brings back only the line the design replaced, so
    // the tank line never shows twice.
    let block = wire.c.prompt.stage.last_raw().expect("the block").clone();
    assert_eq!(block.replaced, [1]);
    assert_eq!(block.shown(), format!("{FIGHT_LINE}\r\n").into_bytes());
}

#[test]
fn a_design_that_reads_the_tank_takes_over_the_whole_block() {
    for template in ["%tank <%hp>", "%{tank_hp:game} <%hp>", "%{raw}"] {
        let mut wire = Wire::new(codes_profile(CODES, template));
        let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
        let text = String::from_utf8_lossy(&out.bytes).into_owned();
        assert!(
            text.starts_with(&String::from_utf8_lossy(&wire.mark(1)).into_owned()),
            "{template}: {text:?}"
        );
        let block = wire.c.prompt.stage.last_raw().expect("the block");
        assert_eq!(block.replaced, [0, 1], "{template}");
    }
}

#[test]
fn a_tank_block_draws_the_same_wherever_the_reads_split() {
    // The game follows a prompt ending in %c with a space and a GA,
    // and starts the next output with a line end.
    let text = format!("You flee.\n\r{TANK_LINE}\n\r{FIGHT_LINE}\n\r *\n\rThe guard arrives.\n\r");
    let screen = same_at_every_split(&|| codes_profile(CODES, HP), &text);
    assert_eq!(
        screen,
        ["You flee.", TANK_LINE, "<159>", "", "The guard arrives."]
    );
    let screen = same_at_every_split(&|| codes_profile(CODES, "%tank <%hp>"), &text);
    assert_eq!(
        screen,
        ["You flee.", "Tester <159>", "", "The guard arrives."]
    );
}

#[test]
fn a_held_tank_line_paints_at_the_end_of_a_read_and_the_prompt_replaces_it() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let out = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
    assert_eq!(
        out.bytes,
        with(&[&wire.mark(1), TANK_LINE.as_bytes(), b"\r\n"])
    );
    let out = wire.read(format!("{FIGHT_LINE}\n\r").as_bytes());
    let leftover = &out.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        out.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[
                TANK_LINE.as_bytes(),
                b"\r\n",
                &wire.mark(2),
                b"<159>\x1b[0m"
            ]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
    // A send forgets a painted tank line, which stays as it shows,
    // and the next prompt reads on its own.
    let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
    wire.send();
    let out = wire.read(format!("{FIGHT_LINE}\n\r").as_bytes());
    assert_eq!(out.replace, None);
    assert_eq!(out.bytes, with(&[&wire.mark(4), b"<159>\x1b[0m"]));
}

#[test]
fn a_held_line_your_send_lets_go_runs_the_line_pass_and_is_logged() {
    let (mut p, c) = codes_profile(CODES, HP);
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "answer",
            "^Bob says: ",
            vosh_automation::trigger::TriggerAction::Send {
                template: "nod".into(),
            },
        ))
        .unwrap();
    let mut wire = Wire::new((p, c));
    // A line that can start a tank block ends the read, so the stage
    // holds it and paints it.
    let out = wire.read(b"You flee.\n\rBob says: \n\r");
    assert_eq!(
        out.bytes,
        with(&[b"You flee.\r\n", &wire.mark(1), b"Bob says: \r\n"])
    );
    assert!(wire.c.prompt.stage.holds());
    // You send before the next read. The line stays as it shows and
    // runs the Line pass, so its trigger answers and it is logged and
    // kept for scrollback.
    let mut batch = super::ReadBatch::new(false);
    let steps = super::let_go_held(
        &mut wire.p,
        &mut wire.c,
        &mut batch,
        tokio::time::Instant::now(),
        Some(3),
    );
    assert!(!wire.c.prompt.stage.holds());
    assert!(batch.out.is_empty(), "it stays as it shows");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].result.sends, ["nod"]);
    assert_eq!(steps[0].scrollback, bytes_of(&["Bob says: "]));
    let logged: Vec<&str> = batch.log.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(logged, ["Bob says: "]);
    // The next read is read on its own.
    wire.send();
    let out = wire.read(b"The Bank of Aabahran\n\r");
    assert_eq!(out.replace, None);
    assert_eq!(out.bytes, b"The Bank of Aabahran\r\n");
}

#[test]
fn a_held_line_a_script_hides_still_shows_and_its_echo_follows() {
    let (mut p, c) = codes_profile(CODES, HP);
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            name: "swap".into(),
            patterns: vec![vosh_automation::trigger::TriggerPattern::regex(
                "^Bob says: ",
            )],
            priority: 0,
            enabled: true,
            actions: vec![
                vosh_automation::trigger::TriggerAction::Gag,
                vosh_automation::trigger::TriggerAction::Script {
                    body: "mud.echo('Bob speaks.')".into(),
                },
            ],
            preset: None,
            group: None,
            target: vosh_automation::trigger::TriggerTarget::Line,
            alert: None,
        })
        .unwrap();
    let mut wire = Wire::new((p, c));
    let _ = wire.read(b"Bob says: \n\r");
    let mut batch = super::ReadBatch::new(false);
    let steps = super::let_go_held(
        &mut wire.p,
        &mut wire.c,
        &mut batch,
        tokio::time::Instant::now(),
        Some(3),
    );
    // The end of its read painted it, so it is logged and kept as it
    // shows, and the echo lands after it.
    assert_eq!(batch.out.bytes, b"Bob speaks.\r\n");
    assert_eq!(steps[0].scrollback, bytes_of(&["Bob says: "]));
    assert_eq!(batch.log.len(), 1);
}

#[test]
fn a_held_line_at_the_end_of_the_session_is_logged_and_kept() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
    let (log, kept) = super::end_held(&mut wire.c, Some(3));
    let logged: Vec<&str> = log.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(logged, [TANK_LINE]);
    assert_eq!(kept, bytes_of(&[TANK_LINE]));
    assert!(!wire.c.prompt.stage.holds());
    // Without a log session it is still kept.
    let _ = wire.read(format!("{TANK_LINE}\n\r").as_bytes());
    let (log, kept) = super::end_held(&mut wire.c, None);
    assert!(log.is_empty());
    assert_eq!(kept.len(), 1);
}

#[test]
fn drawing_off_on_a_tank_block_brings_back_only_the_line_it_replaced() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let _ = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..wire.c.prompt.config().clone()
    };
    take_config(&mut wire.p, &mut wire.c, config);
    let out = super::repaint_step(&wire.p, &mut wire.c, false, tokio::time::Instant::now());
    assert_eq!(
        out.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), FIGHT_LINE.as_bytes(), b"\r\n"]),
            fresh: false,
            above: None,
            tail: Vec::new(),
        })
    );
}

#[test]
fn a_ga_after_prompt_all_draws_in_the_same_and_the_next_read() {
    let profile = || codes_profile("%n%P%C<%hhp %mm %vmv> ", HP);
    let mut wire = Wire::new(profile());
    let out = wire.read_ga(b"<159hp 310m 489mv> ");
    assert_eq!(out.bytes, with(&[&wire.mark(1), b"<159>\x1b[0m"]));
    let screen = same_at_every_split(
        &profile,
        "You flee.\n\rTester: [===|===|===|---]\n\r<159hp 310m 489mv> *\n\rThe guard arrives.\n\r<159hp 310m 489mv> *",
    );
    assert_eq!(
        screen,
        [
            "You flee.",
            "Tester: [===|===|===|---]",
            "<159>",
            "The guard arrives.",
            "<159>"
        ]
    );
}

#[test]
fn a_held_line_the_rest_never_follows_shows_as_any_line() {
    let text = format!("{TANK_LINE}\n\rYou are hungry.\n\r{FIGHT_LINE}\n\r");
    let screen = same_at_every_split(&|| codes_profile(CODES, HP), &text);
    assert_eq!(screen, [TANK_LINE, "You are hungry.", "<159>"]);
    // A GA ends a held line, since the prompt it started never came.
    let screen = same_at_every_split(
        &|| codes_profile(CODES, HP),
        &format!("{TANK_LINE}\n\r*You flee.\n\r"),
    );
    assert_eq!(screen, [TANK_LINE, "You flee."]);
}

#[test]
fn a_released_line_runs_the_line_pass_and_is_logged() {
    let (mut p, mut c) = codes_profile(CODES, HP);
    p.triggers
        .set(vosh_automation::trigger::Trigger::new(
            "hush",
            "^Tester: ",
            vosh_automation::trigger::TriggerAction::Gag,
        ))
        .unwrap();
    let mut batch = super::ReadBatch::new(false);
    let now = tokio::time::Instant::now();
    let mut acc = super::LineAccumulator::new();
    let mut steps = Vec::new();
    for line in acc.feed(format!("{TANK_LINE}\n\rYou are hungry.\n\r").as_bytes()) {
        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
        steps.extend(super::line_step(
            &mut p,
            &mut c,
            &mut batch,
            line,
            plain,
            now,
            Some(3),
        ));
    }
    // The tank line ran the Line pass once it was let go, and its
    // trigger hid it.
    assert_eq!(batch.out.bytes, b"You are hungry.\r\n");
    assert_eq!(steps.len(), 2);
    assert_eq!(batch.log.len(), 1, "only the line that shows is logged");
}

#[test]
fn drawing_off_shows_the_whole_block_as_sent_and_logs_it() {
    let (mut p, mut c) = codes_profile(CODES, HP);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    let mut wire = Wire::new((p, c));
    let out = wire.read(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes());
    assert_eq!(
        out.bytes,
        format!("{TANK_LINE}\r\n{FIGHT_LINE}\r\n").into_bytes()
    );
    let mut batch = super::ReadBatch::new(false);
    let now = tokio::time::Instant::now();
    let mut acc = super::LineAccumulator::new();
    for line in acc.feed(format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r").as_bytes()) {
        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
        let _ = super::line_step(
            &mut wire.p,
            &mut wire.c,
            &mut batch,
            line,
            plain,
            now,
            Some(3),
        );
    }
    let logged: Vec<&str> = batch.log.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(logged, [TANK_LINE, FIGHT_LINE]);
}

/// Run `text` through the Line pass as one read. Returns what it
/// logged, what it kept for scrollback, and what it wrote.
fn logged_and_kept(
    p: &mut Profile,
    c: &mut Connection,
    text: &str,
) -> (Vec<String>, Vec<Vec<u8>>, Vec<u8>) {
    let mut batch = super::ReadBatch::new(false);
    let now = tokio::time::Instant::now();
    let mut acc = super::LineAccumulator::new();
    let mut kept = Vec::new();
    for line in acc.feed(text.as_bytes()) {
        let plain = vosh_protocol::ansi::plain_text(&line.bytes);
        for step in super::line_step(p, c, &mut batch, line, plain, now, Some(3)) {
            kept.extend(step.scrollback);
        }
    }
    let logged = batch.log.iter().map(|e| e.text.clone()).collect();
    (logged, kept, batch.out.bytes)
}

fn bytes_of(lines: &[&str]) -> Vec<Vec<u8>> {
    lines.iter().map(|l| l.as_bytes().to_vec()).collect()
}

#[test]
fn a_tank_line_that_shows_while_vosh_draws_is_logged_and_kept() {
    let fight = format!("You flee.\n\r{TANK_LINE}\n\r{FIGHT_LINE}\n\r");
    let (mut p, mut c) = codes_profile(CODES, HP);
    let (logged, kept, _) = logged_and_kept(&mut p, &mut c, &fight);
    assert_eq!(logged, ["You flee.", TANK_LINE]);
    assert_eq!(kept, bytes_of(&["You flee.", TANK_LINE]));
    // A design that reads the tank draws in place of the line, which
    // then is neither logged nor kept.
    let (mut p, mut c) = codes_profile(CODES, "%tank <%hp>");
    let (logged, kept, _) = logged_and_kept(&mut p, &mut c, &fight);
    assert_eq!(logged, ["You flee."]);
    assert_eq!(kept, bytes_of(&["You flee."]));
}

#[test]
fn with_drawing_off_every_line_that_shows_is_logged_and_kept() {
    let block = format!("{TANK_LINE}\n\r{FIGHT_LINE}\n\r");
    let (mut p, mut c) = codes_profile(CODES, HP);
    let config = vosh_prompt::PromptConfig {
        draw: false,
        ..c.prompt.config().clone()
    };
    take_config(&mut p, &mut c, config);
    let (logged, kept, _) = logged_and_kept(&mut p, &mut c, &block);
    assert_eq!(logged, [TANK_LINE, FIGHT_LINE]);
    assert_eq!(kept, bytes_of(&[TANK_LINE, FIGHT_LINE]));
    // A Prompts trigger hides the final line. The tank line still
    // shows, so it is still logged and kept.
    p.triggers
        .set(vosh_automation::trigger::Trigger {
            target: vosh_automation::trigger::TriggerTarget::Prompt,
            ..vosh_automation::trigger::Trigger::new(
                "hide-prompt",
                "hp ",
                vosh_automation::trigger::TriggerAction::Gag,
            )
        })
        .unwrap();
    let (logged, kept, shown) = logged_and_kept(&mut p, &mut c, &block);
    assert_eq!(shown, format!("{TANK_LINE}\r\n").into_bytes());
    assert_eq!(logged, [TANK_LINE]);
    assert_eq!(kept, bytes_of(&[TANK_LINE]));
}

#[test]
fn prompt_all_settles_with_its_tank_line_in_one_read() {
    let mut wire = Wire::new(codes_profile("%n%P%C<%hhp %mm %vmv> ", HP));
    let out = wire.read(b"Tester: [===|===|===|---]\n\r<159hp 310m 489mv> ");
    assert_eq!(
        out.bytes,
        with(&[
            b"Tester: [===|===|===|---]\r\n",
            &wire.mark(1),
            b"<159>\x1b[0m"
        ])
    );
    let screen = same_at_every_split(
        &|| codes_profile("%n%P%C<%hhp %mm %vmv> ", HP),
        "You flee.\n\rTester: [===|===|===|---]\n\r<159hp 310m 489mv> ",
    );
    assert_eq!(screen, ["You flee.", "Tester: [===|===|===|---]", "<159>"]);
}

#[test]
fn the_away_prompt_shows_as_sent_and_notes_you_are_away() {
    let mut wire = Wire::new(codes_profile("%n%P%C<%hhp %mm %vmv> ", HP));
    let out = wire.read(b"<AFK> ");
    assert_eq!(out.bytes, b"<AFK> ");
    assert_eq!(wire.c.prompt.stage.open_row(), None);
    let vars = wire.c.prompt.vars.prompt_vars();
    assert_eq!(vars.get("afk").map(String::as_str), Some("1"));
}

#[test]
fn a_char_prompt_before_its_text_reads_the_new_codes_at_once() {
    let (mut p, mut c) = codes_profile("<%hhp> ", HP);
    super::start_prompt(&mut p, &mut c, true);
    let mut wire = Wire::new((p, c));
    feed_inline(
        &mut wire.p,
        &mut wire.c,
        "Char.Prompt",
        serde_json::json!({"enabled": true, "prompt": "%n%P%C<%hhp %mm %vmv> ", "fprompt": ""}),
    );
    let seen = wire.c.prompt.take_seen();
    assert!(seen[0].applied);
    let out = wire.read(b"Prompt set to %n%P%C<%hhp %mm %vmv> \n\r<159hp 310m 489mv> ");
    assert_eq!(
        out.bytes,
        with(&[
            b"Prompt set to %n%P%C<%hhp %mm %vmv> \r\n",
            &wire.mark(1),
            b"<159>\x1b[0m"
        ])
    );
}

#[test]
fn the_reply_to_your_prompt_updates_the_capture_before_the_next_prompt() {
    let (mut p, mut c) = codes_profile("<%hhp> ", HP);
    super::start_prompt(&mut p, &mut c, true);
    let mut wire = Wire::new((p, c));
    wire.send_line("prom %n%P%C<%hhp %mm %vmv>");
    let out = wire.read(b"Prompt set to %n%P%C<%hhp %mm %vmv> \n\r<159hp 310m 489mv> ");
    assert_eq!(
        out.bytes,
        with(&[
            b"Prompt set to %n%P%C<%hhp %mm %vmv> \r\n",
            &wire.mark(1),
            b"<159>\x1b[0m"
        ])
    );
    let seen = wire.c.prompt.take_seen();
    assert!(seen[0].applied);

    // prompt off sets nothing, whatever the reply says.
    wire.send_line("prompt off");
    let _ = wire.read(b"You will no longer see prompts.\n\rPrompt set to \x01\x02\n\r");
    assert!(wire.c.prompt.prompts_off());
    let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.c.prompt.config().capture else {
        panic!("an aabahran capture");
    };
    assert_eq!(codes.prompt, "%n%P%C<%hhp %mm %vmv> ");
}

#[test]
fn the_new_build_with_prompts_off_raises_no_not_matching() {
    let (mut p, mut c) = codes_profile("%n%P%C<%hhp %mm %vmv> ", HP);
    super::start_prompt(&mut p, &mut c, true);
    let mut wire = Wire::new((p, c));
    feed(&mut wire.p, &mut wire.c, "char-prompt-off.gmcp");
    // Each pulse brings the prompt time packages and no prompt text.
    for _ in 0..5 {
        feed(&mut wire.p, &mut wire.c, "char-vitals.gmcp");
        feed(&mut wire.p, &mut wire.c, "char-state.gmcp");
        let _ = wire.read(b"");
    }
    let report = wire.c.prompt.take_status_change().expect("a report");
    assert_eq!(report.status, vosh_prompt::Status::PromptsOff);
    // Prompts on again, and the prompt reads.
    feed(&mut wire.p, &mut wire.c, "char-prompt.gmcp");
    feed(&mut wire.p, &mut wire.c, "char-vitals.gmcp");
    let _ = wire.read(b"<159hp 310m 489mv> ");
    assert_eq!(wire.c.prompt.status(), vosh_prompt::Status::Matching);
}

#[test]
fn a_live_partial_waits_for_the_next_read_and_never_flashes() {
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let batch = wire.read_holding(b"You flee.\n\r[1020/1020hp 80");
    assert!(batch.hold);
    assert_eq!(batch.out.bytes, b"You flee.\r\n", "nothing raw yet");
    let out = wire.read(b"0/800mn 930/930mv]\n\r");
    assert_eq!(out.replace, None);
    assert_eq!(out.bytes, with(&[&wire.mark(1), b"<1020>\x1b[0m"]));

    // Held tank lines wait with the partial after them.
    let mut wire = Wire::new(codes_profile(CODES, HP));
    let batch = wire.read_holding(format!("{TANK_LINE}\n\r[159/10").as_bytes());
    assert!(batch.hold);
    let leftover = &batch.out.bytes;
    assert!(leftover.is_empty(), "{leftover:?}");
    let out = wire.read(b"20hp 310/800mn 489/930mv]\n\r");
    assert_eq!(out.replace, None);
    assert_eq!(
        out.bytes,
        with(&[
            TANK_LINE.as_bytes(),
            b"\r\n",
            &wire.mark(1),
            b"<159>\x1b[0m"
        ])
    );
}

#[test]
fn a_partial_no_shape_can_become_paints_at_once() {
    let mut wire = Wire::new(codes_profile("<%hhp %mm %vmv> ", HP));
    let batch = wire.read_holding(b"By what name do you wish to be known? ");
    assert!(!batch.hold);
    assert_eq!(
        batch.out.bytes,
        with(&[&wire.mark(1), b"By what name do you wish to be known? "])
    );
    // Nothing reads a prompt in a profile without a capture.
    let mut wire = Wire::new(Live::default());
    assert!(!wire.read_holding(b"<10hp 2").hold);
}

#[test]
fn a_partial_that_waited_paints_at_the_deadline() {
    let mut wire = Wire::new(codes_profile("<%hhp %mm %vmv> ", HP));
    let batch = wire.read_holding(b"<10hp 2");
    assert!(batch.hold);
    let mut out = vosh_prompt::stage::Output::new(false);
    super::hold_step(&mut wire.c, &mut wire.acc, &mut out);
    assert_eq!(out.bytes, with(&[&wire.mark(1), b"<10hp 2"]));
    // The rest of it replaces what painted.
    let out = wire.read(b"0m 30mv> ");
    assert_eq!(
        out.replace,
        Some(vosh_prompt::stage::Replace {
            gen: wire.gen0 + 1,
            bytes: with(&[&wire.mark(2), b"<10>\x1b[0m"]),
            fresh: true,
            above: None,
            tail: Vec::new(),
        })
    );
}

#[test]
fn an_empty_setting_draws_over_the_fallback() {
    let mut wire = Wire::new(codes_profile("", HP));
    let out = wire.read(b"<20hp 100m 110mv> ");
    assert_eq!(out.bytes, with(&[&wire.mark(1), b"<20>\x1b[0m"]));
}

/// A profile that reads Aabahran's codes `prompt` on a connection to
/// the fake game on a local port, and draws `<%hp>` in its place.
fn fake_profile(prompt: &str) -> Live {
    profile(prompt, HP, true)
}

/// The screen a native grid `columns` wide shows after `reads` of raw
/// wire bytes, rows trimmed, up to the last row that shows anything.
fn wire_screen(wire: &mut Wire, columns: usize, reads: &[&[u8]]) -> Vec<String> {
    let mut grid = crate::native::grid::TermGrid::new(columns, 60);
    for read in reads {
        grid.session_output(&wire.read_wire(read));
    }
    let mut rows: Vec<String> = (0..grid.screen_lines())
        .map(|line| grid.row_string(line).trim_end().to_string())
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// Read `bytes` as one read and as two cut at every place [`cuts`]
/// names, at 40 and 12 wide, and check each screen is the one a single
/// read gives. Returns the 80 wide screen of one read and the wire
/// that read it.
fn wire_same_at_every_split(profile: &dyn Fn() -> Live, bytes: &[u8]) -> (Vec<String>, Wire) {
    for columns in [40, 12] {
        let whole = wire_screen(&mut Wire::new(profile()), columns, &[bytes]);
        for at in cuts(bytes) {
            let reads = vosh_prompt::testkit::reads(bytes, &[at]);
            assert_eq!(
                wire_screen(&mut Wire::new(profile()), columns, &reads),
                whole,
                "{columns} wide, cut after {at}"
            );
        }
    }
    let mut wire = Wire::new(profile());
    let screen = wire_screen(&mut wire, 80, &[bytes]);
    (screen, wire)
}

/// Every value lamented tears hides, as `session://hidden` reports it.
fn all_hidden() -> serde_json::Value {
    serde_json::json!({"vitals": true, "tank": true, "opponent": true, "affects": true, "group": true})
}

const ROOM: [&str; 3] = [
    "The Bank of Aabahran",
    "  Marble counters line the hall, and a clerk nods at you.",
    "[Exits: south]",
];

#[test]
fn the_quiet_wire_draws_its_prompt_at_every_split() {
    let bytes = wire_fixture("quiet");
    let (screen, wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
    assert_eq!(screen, [ROOM[0], ROOM[1], ROOM[2], "", "<1020>"]);
    let vars = wire.c.prompt.vars.prompt_vars();
    assert_eq!(vars.get("maxhp").map(String::as_str), Some("1020"));
    assert_eq!(wire.c.prompt.status(), vosh_prompt::Status::Matching);
}

#[test]
fn the_fight_wire_reads_the_tank_block_at_every_split() {
    let bytes = wire_fixture("fight-tank");
    let (screen, wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
    // The design reads nothing on the tank line, so it shows as sent.
    assert_eq!(
        screen,
        [
            "A Blackwatch guard attacks you!",
            "A Blackwatch guard has quite a few wounds.",
            "",
            "Tester: [===|===|===|---]",
            "<765>"
        ]
    );
    let vars = wire.c.prompt.vars.prompt_vars();
    assert_eq!(vars.get("tank").map(String::as_str), Some("Tester"));
    assert_eq!(vars.get("fight").map(String::as_str), Some("1"));
    // A design that reads the tank takes over the whole block.
    let profile = || {
        let (mut p, mut c) = codes_profile(CODES, "%tank %{tank_hp:pct}%% <%hp>");
        super::start_prompt(&mut p, &mut c, false);
        (p, c)
    };
    let (screen, _) = wire_same_at_every_split(&profile, &bytes);
    assert_eq!(screen[3..], ["Tester 75% <765>"]);
}

#[test]
fn each_lament_wire_hides_what_the_song_hides_at_every_split() {
    for (name, battle) in [
        ("lament-new", false),
        ("lament-243cac5c", false),
        ("lament-older", true),
    ] {
        let bytes = wire_fixture(name);
        let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
        let mut want = vec!["Tears fall as the lament takes you."];
        if battle {
            want.push("A Blackwatch guard has quite a few wounds.");
        }
        want.extend(["", "Tester:", "<?>"]);
        assert_eq!(screen, want, "{name}");
        let hidden = wire
            .c
            .prompt
            .vars
            .take_hidden_change()
            .expect("a change to report");
        assert_eq!(
            serde_json::to_value(hidden).expect("it serializes"),
            all_hidden(),
            "{name}"
        );
    }
    // A new build session that had Char.Prompt at login hides the
    // same values by the flags alone.
    let (mut p, mut c) = fake_profile(CODES);
    feed(&mut p, &mut c, "char-prompt.gmcp");
    let mut wire = Wire::new((p, c));
    let _ = wire.read_wire(&wire_fixture("lament-new"));
    assert!(wire.c.prompt.vars.new_build());
    let hidden = wire.c.prompt.vars.take_hidden_change().expect("a change");
    assert_eq!(
        serde_json::to_value(hidden).expect("it serializes"),
        all_hidden()
    );
}

#[test]
fn prompt_all_that_the_next_pulse_completes_draws_both_prompts() {
    let bytes = wire_fixture("prompt-all-next");
    let (screen, _) = wire_same_at_every_split(&|| fake_profile("%n%P%C<%hhp %mm %vmv> "), &bytes);
    assert_eq!(
        screen,
        [
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "",
            "<1020>",
            "A Blackwatch guard arrives from the south.",
            "",
            "<1020>"
        ]
    );
}

#[test]
fn a_ga_after_prompt_all_draws_with_no_flash_at_every_split() {
    let bytes = wire_fixture("ga");
    let (screen, _) = wire_same_at_every_split(&|| fake_profile("%n%P%C<%hhp %mm %vmv> "), &bytes);
    assert_eq!(screen, [ROOM[0], ROOM[1], ROOM[2], "", "<1020>"]);
    // In one read the game's own prompt never reaches the terminal.
    let mut wire = Wire::new(fake_profile("%n%P%C<%hhp %mm %vmv> "));
    let out = wire.read_wire(&bytes);
    assert!(!plain(&String::from_utf8_lossy(&out.bytes)).contains("mv>"));
}

#[test]
fn an_eor_ends_a_prompt_as_a_ga_does() {
    let ga = wire_fixture("ga");
    let mark = ga.len() - 2;
    assert_eq!(ga[mark..], [255, 249]);
    let mut eor = ga.clone();
    eor[mark + 1] = 239;
    // A pattern that never settles, so only the mark makes the
    // partial your prompt.
    let profile = || {
        let (mut p, mut c) = Live::default();
        take_config(
            &mut p,
            &mut c,
            vosh_prompt::PromptConfig {
                draw: true,
                template: HP.into(),
                capture: vosh_prompt::CaptureConfig::Regex(vosh_prompt::config::RegexCapture {
                    lines: vec![r"^<(?<hp>\d+)hp (?<mana>\d+)m (?<move>\d+)mv> $".into()],
                    settle: false,
                    ..vosh_prompt::config::RegexCapture::default()
                }),
                ..vosh_prompt::PromptConfig::default()
            },
        );
        super::start_prompt(&mut p, &mut c, false);
        (p, c)
    };
    let (at_ga, _) = wire_same_at_every_split(&profile, &ga);
    let (at_eor, _) = wire_same_at_every_split(&profile, &eor);
    assert_eq!(at_eor, at_ga);
    assert_eq!(at_eor.last().map(String::as_str), Some("<1020>"));
    // With no mark the game's own prompt shows.
    let bare = wire_screen(&mut Wire::new(profile()), 80, &[&ga[..mark]]);
    assert_eq!(bare.last().map(String::as_str), Some("<1020hp 800m 930mv>"));
}

#[test]
fn the_login_wire_gives_vosh_the_prompt_with_no_typing() {
    let bytes = wire_fixture("login-new");
    // A capture that follows the game, started on another setting.
    let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile("<%hhp> "), &bytes);
    assert_eq!(
        screen,
        [
            "Welcome to the fake Aabahran, Tester.",
            ROOM[0],
            ROOM[1],
            ROOM[2],
            "",
            "<1020>"
        ]
    );
    let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.c.prompt.config().capture else {
        panic!("an aabahran capture");
    };
    assert_eq!(codes.prompt, CODES);
    assert_eq!(codes.source, Some(vosh_prompt::config::CaptureSource::Gmcp));
    assert!(wire.c.prompt.vars.new_build());
    let seen = wire.c.prompt.take_seen();
    assert_eq!(
        seen,
        [vosh_prompt::GamePromptSeen {
            kind: vosh_prompt::engine::SeenKind::Gmcp,
            text: CODES.into(),
            applied: true,
            lost: Vec::new(),
        }]
    );
    // A profile that reads no prompt keeps the setting for the card.
    let (mut p, mut c) = Live::default();
    super::start_prompt(&mut p, &mut c, false);
    let mut wire = Wire::new((p, c));
    let _ = wire.read_wire(&bytes);
    let packet = wire
        .c
        .prompt
        .vars
        .gmcp()
        .char_prompt()
        .expect("the login Char.Prompt");
    assert_eq!(packet.prompt, CODES);
    assert!(packet.at_login);
    assert!(!wire.c.prompt.take_seen()[0].applied);
}

#[test]
fn the_prompt_x_wire_reads_the_new_codes_right_after_the_reply() {
    let bytes = wire_fixture("prompt-x-new");
    let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
    assert_eq!(screen, ["Prompt set to <%h/%Hhp %m/%Mmn>", "", "<1020>"]);
    let vosh_prompt::CaptureConfig::Aabahran(codes) = &wire.c.prompt.config().capture else {
        panic!("an aabahran capture");
    };
    assert_eq!(codes.prompt, vosh_prompt::testkit::wire::PROMPT_X);
    let seen = wire.c.prompt.take_seen();
    assert_eq!(seen.len(), 1, "one toast, from Char.Prompt: {seen:?}");
    assert!(seen[0].applied);
}

#[test]
fn the_prompts_off_wire_counts_no_miss_while_the_packages_keep_coming() {
    let bytes = wire_fixture("prompts-off-new");
    let (screen, mut wire) = wire_same_at_every_split(&|| fake_profile(CODES), &bytes);
    assert_eq!(
        screen,
        [
            "You will no longer see prompts.",
            "",
            "",
            "Pulse 1 of 3.",
            "",
            "",
            "Pulse 2 of 3.",
            "",
            "",
            "Pulse 3 of 3."
        ]
    );
    assert_eq!(wire.c.prompt.status(), vosh_prompt::Status::PromptsOff);
    let report = wire.c.prompt.take_status_change().expect("a report");
    assert_eq!(report.status, vosh_prompt::Status::PromptsOff);
    let state = crate::app::state::AppState::default();
    let echo = crate::input::run_line(&state, &mut wire.p, &mut wire.c, "#prompt")
        .result
        .echo;
    assert_eq!(
        echo.last().map(String::as_str),
        Some("You turned prompts off in the game. Type prompt in the game to turn them back on.")
    );
}

/// Rubric's terminal ground, a light parchment.
const RUBRIC: vosh_automation::trigger::readable::Rgb = (0xf0, 0xe5, 0xcf);

/// A minimap row as minimap.c prints it, two rooms of desert in yellow
/// 220 and you in pink 213, which both fade on Rubric's parchment.
const DESERT_ROW: &str = "\x1b[38;5;220m. \x1b[38;5;220m. \x1b[38;5;213m@\x1b[0;0m";

/// What the terminal gets for `read`, a GA after it when `ga`, with the
/// game ground at `game`, and whether the read told the webview of a
/// prompt.
fn shown_on_game_ground(
    read: &str,
    ga: bool,
    game: Option<vosh_automation::trigger::readable::Rgb>,
) -> (String, bool) {
    super::highlight_ground::set(None, game);
    let batch = Wire::new(Live::default()).read_with(read.as_bytes(), ga, false);
    super::highlight_ground::set(None, None);
    (
        String::from_utf8(batch.out.bytes).unwrap(),
        batch.prompt_vars,
    )
}

/// `shown` with yellow 220 and pink 213 lifted for Rubric, each as a true
/// color.
fn lifted_on_rubric(shown: &str) -> String {
    let [yellow, pink] = [220, 213].map(|n| {
        let open = format!("\x1b[38;5;{n}m");
        let lift = vosh_automation::trigger::readable::lift_game_sgr(open.as_bytes(), RUBRIC);
        let lift = String::from_utf8(lift.into_owned()).unwrap();
        assert!(lift.starts_with("\x1b[38;2;"), "{lift:?}");
        (open, lift)
    });
    shown
        .replace(&yellow.0, &yellow.1)
        .replace(&pink.0, &pink.1)
}

#[test]
fn the_game_256_colors_lift_on_a_light_ground_while_fit_game_colors_is_on() {
    let line = format!("{DESERT_ROW}\n\r");
    let as_sent = format!("{DESERT_ROW}\r\n");
    let (fit_on, _) = shown_on_game_ground(&line, false, Some(RUBRIC));
    assert_eq!(fit_on, lifted_on_rubric(&as_sent));
    let (fit_off, _) = shown_on_game_ground(&line, false, None);
    assert_eq!(fit_off, as_sent);

    // A partial a GA ends that Vosh does not read as your prompt lifts
    // too, and with no trigger acting on it the webview hears of no
    // prompt.
    let (fit_off, told) = shown_on_game_ground(DESERT_ROW, true, None);
    assert!(fit_off.contains(DESERT_ROW), "{fit_off:?}");
    assert!(!told);
    let (fit_on, told) = shown_on_game_ground(DESERT_ROW, true, Some(RUBRIC));
    assert_eq!(fit_on, lifted_on_rubric(&fit_off));
    assert!(!told);
}
