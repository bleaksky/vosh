//! The fake Aabahran in the test kit plays the game as the server writes
//! it: the wire order, `do_prompt` and
//! `do_fprompt` on each build, Char.Prompt on the new build alone, the
//! packages that keep coming with prompts off, the hidden and lament
//! packets, and prompts that read back through the compiler.

use serde_json::Value as Json;
use vosh_prompt::aabahran::{compile, Origin, Who};
use vosh_prompt::testkit::mud::{self, telnet, PROMPT, PROMPT_ALL, UNFILLED};
use vosh_prompt::testkit::{reads, shown, Build, Mud, Options};

const BUILDS: [Build; 3] = [Build::New, Build::Unflagged, Build::Older];

/// One thing on the wire: a packet or a run of text.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Packet(String, Json),
    Text(Vec<u8>),
}

/// The packets and text runs of `raw`, in order. GA and other commands
/// end a text run.
fn parts(raw: &[u8]) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut text = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != telnet::IAC {
            text.push(raw[i]);
            i += 1;
            continue;
        }
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut text)));
        }
        if raw[i + 1] == telnet::SB {
            assert_eq!(raw[i + 2], telnet::GMCP);
            let end = raw[i..]
                .windows(2)
                .position(|w| w == [telnet::IAC, telnet::SE])
                .expect("a packet ends")
                + i;
            let msg = vosh_protocol::gmcp::parse(&raw[i + 3..end]).expect("every packet parses");
            parts.push(Part::Packet(msg.package, msg.data));
            i = end + 2;
        } else {
            i += 2;
        }
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

fn packets(raw: &[u8]) -> Vec<(String, Json)> {
    parts(raw)
        .into_iter()
        .filter_map(|p| match p {
            Part::Packet(name, data) => Some((name, data)),
            Part::Text(_) => None,
        })
        .collect()
}

fn names(raw: &[u8]) -> Vec<String> {
    packets(raw).into_iter().map(|(name, _)| name).collect()
}

fn packet(raw: &[u8], package: &str) -> Option<Json> {
    packets(raw)
        .into_iter()
        .find(|(name, _)| name == package)
        .map(|(_, data)| data)
}

fn run(mud: &mut Mud, line: &str) -> Vec<u8> {
    mud.command(line)
        .into_iter()
        .flat_map(|w| w.bytes)
        .collect()
}

fn playing(build: Build) -> Mud {
    Mud::playing(Options::new(build))
}

/// The prompt lines at the end of `raw`, as the terminal shows them: the
/// lines after the last blank line.
fn prompt_lines(raw: &[u8]) -> Vec<String> {
    let text = shown(raw);
    let block = text.rsplit("\n\r\n\r").next().unwrap_or(&text);
    let mut lines: Vec<String> = block.split("\n\r").map(str::to_string).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// Read the prompt at the end of `raw` through the shapes `prompt`
/// compiles to. Returns the shape's values.
fn read_back(prompt: &str, raw: &[u8]) -> std::collections::BTreeMap<String, String> {
    let compiled = compile(prompt, "", Origin::Stored, Who::default()).expect("it compiles");
    let lines = prompt_lines(raw);
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    compiled
        .shapes
        .iter()
        .find_map(|shape| shape.read(&lines).or_else(|| shape.read_partial(&lines)))
        .unwrap_or_else(|| panic!("no shape of {prompt} reads {lines:?}"))
        .values
}

#[test]
fn every_packet_comes_before_the_text_of_its_pulse() {
    for build in BUILDS {
        let mut mud = playing(build);
        for line in ["look", "fight", "lament", "prompt all", "blind", ""] {
            let raw = run(&mut mud, line);
            let first_text = parts(&raw)
                .iter()
                .position(|p| matches!(p, Part::Text(_)))
                .expect("some text");
            assert!(
                parts(&raw)[first_text..]
                    .iter()
                    .all(|p| matches!(p, Part::Text(_))),
                "{build:?} {line}"
            );
        }
    }
}

#[test]
fn the_prompt_time_packages_come_in_the_game_order() {
    let mut mud = playing(Build::New);
    assert_eq!(
        names(&run(&mut mud, "")),
        [
            "Char.Vitals",
            "Char.Worth",
            "Char.Combat",
            "Group.Info",
            "Char.State",
            "Room.Weather"
        ]
    );
    for build in [Build::Unflagged, Build::Older] {
        let mut mud = playing(build);
        assert_eq!(
            names(&run(&mut mud, "")),
            ["Char.Vitals", "Char.Worth", "Char.Combat", "Group.Info"],
            "{build:?}"
        );
    }
}

#[test]
fn login_sends_char_prompt_before_the_first_room_text_on_the_new_build() {
    let mut mud = Mud::new(Options::new(Build::New));
    let greeting = mud.greeting();
    assert!(greeting.starts_with(&[telnet::IAC, telnet::WILL, telnet::GMCP]));
    let writes = mud.receive(&[telnet::IAC, telnet::DO, telnet::GMCP]);
    assert_eq!(writes.len(), 1);
    let login = &writes[0].bytes;
    assert_eq!(
        names(login)[..7],
        [
            "Char.Status",
            "Char.Prompt",
            "Char.Affects",
            "Char.Worth",
            "World.Time",
            "World.Moons",
            "Room.Info"
        ]
    );
    let prompt = packet(login, "Char.Prompt").expect("Char.Prompt");
    assert_eq!(
        prompt,
        serde_json::json!({"enabled": true, "prompt": PROMPT, "fprompt": ""})
    );
    assert_eq!(
        packet(login, "Char.Status").expect("Char.Status")["name"],
        "Tester"
    );
    assert!(shown(login).contains("The Bank of Aabahran"));

    for build in [Build::Unflagged, Build::Older] {
        let mut mud = Mud::new(Options::new(build));
        let login: Vec<u8> = mud
            .receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
            .into_iter()
            .flat_map(|w| w.bytes)
            .collect();
        assert!(names(&login).contains(&"Char.Status".to_string()));
        assert!(
            !names(&login).contains(&"Char.Prompt".to_string()),
            "{build:?}"
        );
    }
}

#[test]
fn a_reconnect_sends_no_status_and_no_prompt_settings() {
    let mut mud = Mud::new(Options {
        reconnect: true,
        ..Options::new(Build::New)
    });
    let login: Vec<u8> = mud
        .receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
        .into_iter()
        .flat_map(|w| w.bytes)
        .collect();
    let names = names(&login);
    assert!(!names.contains(&"Char.Status".to_string()));
    assert!(!names.contains(&"Char.Prompt".to_string()));
    assert!(names.contains(&"Char.Vitals".to_string()));
    assert!(shown(&login).starts_with("Reconnecting."));
    // The next prompt command sends Char.Prompt again.
    assert!(packet(&run(&mut mud, "prompt"), "Char.Prompt").is_some());
}

#[test]
fn without_gmcp_the_first_line_logs_you_in() {
    let mut mud = Mud::new(Options::new(Build::New));
    let writes = mud.receive(b"\r\n");
    assert_eq!(writes.len(), 1);
    let leftover = &packets(&writes[0].bytes);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(shown(&writes[0].bytes).contains("Welcome to the fake Aabahran"));
    // A command can come in pieces.
    let leftover = &mud.receive(b"lo");
    assert!(leftover.is_empty(), "{leftover:?}");
    let writes = mud.receive(b"ok\r\n");
    assert!(shown(&writes[0].bytes).starts_with("The Bank of Aabahran"));
    let leftover = &packets(&writes[0].bytes);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn prompt_stores_a_setting_as_do_prompt_does() {
    assert_eq!(mud::stored("%h"), "%h ");
    assert_eq!(mud::stored("%h   "), "%h ");
    assert_eq!(mud::stored("[%h]%c"), "[%h]%c");
    assert_eq!(mud::stored("[%h]%C"), "[%h]%C");
    assert_eq!(mud::stored("a~b"), "a-b ");
    let long = "x".repeat(300);
    assert_eq!(mud::stored(&long), format!("{} ", "x".repeat(mud::KEEP)));

    for build in BUILDS {
        let mut mud = playing(build);
        let raw = run(&mut mud, "prom <%h/%Hhp %m/%Mmn>");
        assert_eq!(mud.prompt, "<%h/%Hhp %m/%Mmn> ");
        assert!(
            shown(&raw).starts_with("Prompt set to <%h/%Hhp %m/%Mmn> \n\r"),
            "{build:?}"
        );
        assert_eq!(
            read_back(&mud.prompt, &raw)["hp"],
            "1020",
            "{build:?} reads the new prompt"
        );
        let raw = run(&mut mud, "prompt all");
        assert_eq!(mud.prompt, PROMPT_ALL);
        assert!(shown(&raw).ends_with("<1020hp 800m 930mv> "), "{build:?}");
        let raw = run(&mut mud, "prompt");
        assert!(shown(&raw).starts_with(&format!("Current prompt: {PROMPT_ALL}\n\r")));
    }
}

#[test]
fn char_prompt_follows_every_change_on_the_new_build_alone() {
    let mut mud = playing(Build::New);
    for (line, prompt, fprompt) in [
        ("prompt <%hhp>", "<%hhp> ", ""),
        ("prompt", "<%hhp> ", ""),
        ("fprompt `1%h``hp [%p] >", "<%hhp> ", "`1%h``hp [%p] > "),
        ("fprompt off", "<%hhp> ", ""),
    ] {
        let raw = run(&mut mud, line);
        assert_eq!(
            packet(&raw, "Char.Prompt"),
            Some(serde_json::json!({"enabled": true, "prompt": prompt, "fprompt": fprompt})),
            "{line}"
        );
    }
    // The fight prompt's reply keeps its colors as the game prints them.
    let raw = run(&mut mud, "fprompt `1%h``hp");
    assert!(raw
        .windows(b"Fight prompt set to \x1b[0;31m%h".len())
        .any(|w| w == b"Fight prompt set to \x1b[0;31m%h"));
    // fprompt alone shows it and sends nothing.
    let raw = run(&mut mud, "fprompt");
    assert_eq!(packet(&raw, "Char.Prompt"), None);
    assert!(shown(&raw).starts_with("Current fight prompt: "));

    for build in [Build::Unflagged, Build::Older] {
        let mut mud = playing(build);
        for line in ["prompt <%hhp>", "prompt", "fprompt %h", "prompt off"] {
            assert_eq!(packet(&run(&mut mud, line), "Char.Prompt"), None, "{line}");
        }
    }
}

#[test]
fn prompt_off_returns_on_the_new_build_and_falls_through_before_it() {
    let mut mud = playing(Build::New);
    let raw = run(&mut mud, "prompt off");
    assert_eq!(
        packet(&raw, "Char.Prompt").expect("Char.Prompt")["enabled"],
        false
    );
    assert_eq!(shown(&raw), "You will no longer see prompts.\n\r\n\r");
    assert!(!mud.prompt_on);
    assert_eq!(mud.prompt, PROMPT);
    // The packages keep coming with no prompt text.
    let raw = mud.pulse_later("Pulse 1 of 1.");
    assert!(names(&raw).contains(&"Char.Vitals".to_string()));
    assert_eq!(shown(&raw), "\n\rPulse 1 of 1.\n\r\n\r");
    // prompt alone turns prompts back on.
    let raw = run(&mut mud, "prompt");
    assert_eq!(
        packet(&raw, "Char.Prompt").expect("Char.Prompt")["enabled"],
        true
    );
    assert!(mud.prompt_on);

    for build in [Build::Unflagged, Build::Older] {
        let mut mud = playing(build);
        let raw = run(&mut mud, "prompt OFF");
        assert_eq!(
            shown(&raw),
            format!("You will no longer see prompts.\n\rPrompt set to {UNFILLED}\n\r\n\r"),
            "{build:?}"
        );
        assert_eq!(mud.prompt, UNFILLED);
        assert!(!mud.prompt_on);
        // No packages come while prompts are off.
        let leftover = &packets(&mud.pulse_later("Pulse 1 of 1."));
        assert!(leftover.is_empty(), "{leftover:?}");
    }
}

#[test]
fn away_prints_afk_and_only_the_new_build_keeps_sending_packages() {
    let mut mud = playing(Build::New);
    let raw = run(&mut mud, "afk");
    assert!(shown(&raw).ends_with("<AFK> "));
    assert!(!packets(&raw).is_empty(), "expected entries");
    let mut mud = playing(Build::Older);
    let raw = run(&mut mud, "afk");
    assert!(shown(&raw).ends_with("<AFK> "));
    let leftover = &packets(&raw);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_fight_prints_the_tank_line_and_reads_back() {
    for build in BUILDS {
        let mut mud = playing(build);
        let raw = run(&mut mud, "fight");
        assert_eq!(
            prompt_lines(&raw),
            [
                "Tester: [===|===|===|---]",
                "[765/1020hp 800/800mn 930/930mv]"
            ],
            "{build:?}"
        );
        let values = read_back(PROMPT, &raw);
        assert_eq!(values["tank"], "Tester");
        assert_eq!(values["tank_bar"], "===|===|===|---");
        assert_eq!(values["hp"], "765");
        assert!(shown(&raw).contains("A Blackwatch guard has quite a few wounds. \n\r"));
    }
    let mut mud = playing(Build::New);
    let combat = packet(&run(&mut mud, "fight"), "Char.Combat").expect("Char.Combat");
    assert_eq!(
        combat,
        serde_json::json!({"target": "a Blackwatch guard", "condition": "quite a few wounds", "hp_pct": 54, "tank": {"name": "Tester", "hp_pct": 75}})
    );
    // The fight ends.
    let raw = run(&mut mud, "fight");
    assert_eq!(packet(&raw, "Char.Combat"), Some(serde_json::json!({})));
    assert_eq!(prompt_lines(&raw), ["[1020/1020hp 800/800mn 930/930mv]"]);
}

#[test]
fn the_round_that_ends_a_fight_comes_after_its_empty_char_combat() {
    let mut mud = playing(Build::New);
    let _ = run(&mut mud, "fight");
    // The guard's death, as `act` prints `$n is DEAD!!` to the room
    // (fight.c:6295).
    let raw = mud.fight_ends_later("A Blackwatch guard is DEAD!!");
    // stop_fighting sends Char.Combat in the middle of the round, before
    // the prompt time packages, which send it again.
    assert_eq!(names(&raw)[..2], ["Char.Combat", "Char.Vitals"]);
    let combats: Vec<Json> = packets(&raw)
        .into_iter()
        .filter(|(name, _)| name == "Char.Combat")
        .map(|(_, data)| data)
        .collect();
    assert_eq!(combats, [serde_json::json!({}), serde_json::json!({})]);
    let first_text = parts(&raw)
        .iter()
        .position(|p| matches!(p, Part::Text(_)))
        .expect("some text");
    assert!(parts(&raw)[first_text..]
        .iter()
        .all(|p| matches!(p, Part::Text(_))));
    assert!(shown(&raw).starts_with("\n\rA Blackwatch guard is DEAD!!\n\r"));
    assert_eq!(prompt_lines(&raw), ["[1020/1020hp 800/800mn 930/930mv]"]);
}

#[test]
fn lament_hides_what_each_build_hides() {
    let expect = [
        (
            Build::New,
            serde_json::json!({"affects": [], "hidden": true}),
            serde_json::json!({"hp": 0, "maxhp": 0, "mana": 0, "maxmana": 0, "move": 0, "maxmove": 0, "hidden": true}),
            serde_json::json!({"target": "a Blackwatch guard", "hidden": true, "tank": {"name": "Tester"}}),
            serde_json::json!({"hidden": true}),
        ),
        (
            Build::Unflagged,
            serde_json::json!({"affects": []}),
            serde_json::json!({"hp": 0, "maxhp": 0, "mana": 0, "maxmana": 0, "move": 0, "maxmove": 0}),
            serde_json::json!({"target": "a Blackwatch guard"}),
            serde_json::json!({}),
        ),
    ];
    for (build, affects, vitals, combat, group) in expect {
        let mut mud = playing(build);
        let _ = run(&mut mud, "fight");
        let raw = run(&mut mud, "lament");
        assert_eq!(names(&raw)[0], "Char.Affects", "the song lands at once");
        assert_eq!(packet(&raw, "Char.Affects"), Some(affects), "{build:?}");
        assert_eq!(packet(&raw, "Char.Vitals"), Some(vitals), "{build:?}");
        assert_eq!(packet(&raw, "Char.Combat"), Some(combat), "{build:?}");
        assert_eq!(packet(&raw, "Group.Info"), Some(group), "{build:?}");
        assert!(!shown(&raw).contains("has quite a few wounds"), "{build:?}");
        assert_eq!(
            prompt_lines(&raw),
            ["Tester: ", "[0/0hp 0/0mn 0/0mv]"],
            "{build:?}"
        );
    }
    // The older build sends true values and names the song.
    let mut mud = playing(Build::Older);
    let _ = run(&mut mud, "fight");
    let raw = run(&mut mud, "lament");
    let affects = packet(&raw, "Char.Affects").expect("Char.Affects");
    assert!(affects["affects"]
        .as_array()
        .expect("a list")
        .iter()
        .any(|a| a["name"] == "lamented tears"));
    assert_eq!(packet(&raw, "Char.Vitals").expect("vitals")["hp"], 765);
    assert_eq!(
        packet(&raw, "Char.Combat").expect("combat")["condition"],
        "quite a few wounds"
    );
    let group = packet(&raw, "Group.Info").expect("group");
    assert_eq!(group["members"][0]["name"], "Tester");
    assert!(shown(&raw).contains("has quite a few wounds"));
    assert_eq!(prompt_lines(&raw), ["Tester: ", "[0/0hp 0/0mn 0/0mv]"]);
    // The song ends and Char.Affects goes out at once.
    let raw = run(&mut mud, "lament");
    assert_eq!(names(&raw)[0], "Char.Affects");
    assert_eq!(packet(&raw, "Char.Vitals").expect("vitals")["hp"], 765);
}

#[test]
fn the_fallback_prints_zeros_under_lament_on_the_new_build_alone() {
    for (build, want) in [
        (Build::New, "<0hp 0m 0mv> "),
        (Build::Unflagged, "<1020hp 800m 930mv> "),
        (Build::Older, "<1020hp 800m 930mv> "),
    ] {
        let mut mud = Mud::playing(Options {
            prompt: String::new(),
            ..Options::new(build)
        });
        let raw = run(&mut mud, "lament");
        assert!(shown(&raw).ends_with(want), "{build:?}: {}", shown(&raw));
    }
}

#[test]
fn blind_withholds_the_opponent_by_each_build() {
    let mut mud = playing(Build::New);
    let _ = run(&mut mud, "fight");
    let raw = run(&mut mud, "blind");
    assert_eq!(
        packet(&raw, "Char.Combat"),
        Some(
            serde_json::json!({"target": "a Blackwatch guard", "hidden": true, "tank": {"name": "Tester", "hp_pct": 75}})
        )
    );
    assert!(!shown(&raw).contains("has quite a few wounds"));
    assert_eq!(packet(&raw, "Char.Vitals").expect("vitals")["hp"], 765);
    let raw = run(&mut mud, "look");
    assert!(packet(&raw, "Room.Info").is_none());

    let mut mud = playing(Build::Unflagged);
    let _ = run(&mut mud, "fight");
    let raw = run(&mut mud, "blind");
    assert_eq!(
        packet(&raw, "Char.Combat"),
        Some(serde_json::json!({"target": "a Blackwatch guard"}))
    );
    let mut mud = playing(Build::Older);
    let _ = run(&mut mud, "fight");
    let raw = run(&mut mud, "blind");
    assert_eq!(packet(&raw, "Char.Combat").expect("combat")["hp_pct"], 54);
}

#[test]
fn the_new_build_sends_state_and_weather_that_match_the_prompt() {
    let mut mud = Mud::playing(Options {
        prompt: "%S %s %W %w %G %e ".into(),
        ..Options::new(Build::New)
    });
    let raw = run(&mut mud, "look");
    assert_eq!(
        packet(&raw, "Char.State"),
        Some(serde_json::json!({"position": "standing", "language": "common"}))
    );
    assert_eq!(
        packet(&raw, "Room.Weather"),
        Some(serde_json::json!({"sky": "indoors", "temp": 68, "unit": "F", "region": "Temperate"}))
    );
    assert!(shown(&raw).ends_with("std common indoors 68 Temperate [Exits: S] "));
}

#[test]
fn split_cuts_the_next_prompt_into_two_writes() {
    let mut mud = playing(Build::New);
    let writes = mud.command("split");
    assert_eq!(writes.len(), 1);
    let writes = mud.command("look");
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].after_ms, 0);
    assert_eq!(writes[1].after_ms, mud::SPLIT_MS);
    let first = shown(&writes[0].bytes);
    assert!(first.ends_with("[1020/10"), "{first}");
    let whole: Vec<u8> = writes.into_iter().flat_map(|w| w.bytes).collect();
    assert!(shown(&whole).ends_with("[1020/1020hp 800/800mn 930/930mv]\n\r"));
    // Only the next prompt.
    assert_eq!(mud.command("look").len(), 1);
}

#[test]
fn pulses_come_apart_and_start_on_a_new_line() {
    let mut mud = playing(Build::New);
    let writes = mud.command("pulses 3");
    assert_eq!(writes.len(), 3);
    for (n, write) in writes.iter().enumerate() {
        assert_eq!(write.after_ms, mud::PULSE_MS);
        assert_eq!(names(&write.bytes)[0], "Char.Vitals");
        assert!(shown(&write.bytes).starts_with(&format!("\n\rPulse {} of 3.\n\r", n + 1)));
    }
    let writes = mud.command("spam 2");
    assert!(shown(&writes[0].bytes).starts_with("Line 1 of 2 of the spam.\n\rLine 2 of 2"));
}

#[test]
fn a_bash_lags_you_and_the_game_holds_what_you_type_ahead() {
    let mut mud = playing(Build::New);
    let writes = mud.receive(b"bash tolliver\r\nkick\r\nlook\r\n");
    assert_eq!(writes.len(), 3);
    assert_eq!(writes[0].after_ms, 0);
    assert!(shown(&writes[0].bytes).starts_with("You slam into Tolliver, and send him flying!\n\r"));
    // kick waits out the lag, and look comes a pulse after it.
    assert_eq!(writes[1].after_ms, mud::BASH_MS);
    assert!(shown(&writes[1].bytes).starts_with("Huh?\n\r"));
    assert_eq!(writes[2].after_ms, mud::GAME_PULSE_MS);
    assert!(shown(&writes[2].bytes).starts_with("The Bank of Aabahran"));

    // A line typed once the game answered the held ones comes at once.
    assert_eq!(mud.receive(b"look\r\n")[0].after_ms, 0);

    // A line that comes later still waits out the lag.
    mud.receive(b"bash maren\r\n");
    let writes = mud.receive(b"look\r\n");
    assert_eq!(writes[0].after_ms, mud::BASH_MS);
}

#[test]
fn a_bash_that_finds_nobody_puts_no_lag_on_you() {
    let mut mud = playing(Build::New);
    let writes = mud.receive(b"bash\r\nbash Vex\r\nlook\r\n");
    assert!(shown(&writes[0].bytes).starts_with("But you aren't fighting anyone!\n\r"));
    assert!(shown(&writes[1].bytes).starts_with("They aren't here.\n\r"));
    assert!(writes.iter().all(|w| w.after_ms == 0));
}

#[test]
fn compact_and_telnetga_change_how_a_prompt_ends() {
    let mut mud = playing(Build::New);
    let raw = run(&mut mud, "");
    assert!(raw.ends_with(&[telnet::IAC, telnet::GA]));
    assert_eq!(shown(&raw), "\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
    let _ = run(&mut mud, "compact");
    let _ = run(&mut mud, "telnetga");
    let raw = run(&mut mud, "");
    assert!(!raw.ends_with(&[telnet::IAC, telnet::GA]));
    assert_eq!(shown(&raw), "[1020/1020hp 800/800mn 930/930mv]\n\r");
}

#[test]
fn a_game_that_plays_eor_answers_each_ask_and_ends_prompts_with_eor() {
    let ask = [telnet::IAC, telnet::DO, telnet::TELOPT_EOR];
    let will = [telnet::IAC, telnet::WILL, telnet::TELOPT_EOR];
    // A game without it ignores the ask and keeps GA.
    let mut mud = playing(Build::New);
    let leftover = &mud.receive(&ask);
    assert!(leftover.is_empty(), "{leftover:?}");
    assert!(run(&mut mud, "").ends_with(&[telnet::IAC, telnet::GA]));

    let mut mud = Mud::playing(Options {
        eor: true,
        ..Options::new(Build::New)
    });
    assert!(run(&mut mud, "").ends_with(&[telnet::IAC, telnet::GA]));
    // Each ask gets its own answer, as a game that keeps no state gives.
    for _ in 0..2 {
        let writes = mud.receive(&ask);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].bytes, will);
    }
    let raw = run(&mut mud, "");
    assert!(raw.ends_with(&[telnet::IAC, telnet::EOR]));
    assert!(!raw.ends_with(&[telnet::IAC, telnet::GA, telnet::IAC, telnet::EOR]));
    assert_eq!(shown(&raw), "\n\r[1020/1020hp 800/800mn 930/930mv]\n\r");
}

#[test]
fn quit_closes_the_connection() {
    let mut mud = playing(Build::New);
    let writes = mud.command("quit");
    assert_eq!(writes.len(), 1);
    assert!(writes[0].close);
    assert!(mud.command("nonsense")[0]
        .bytes
        .ends_with(&[telnet::IAC, telnet::GA]));
}

#[test]
fn reads_split_a_pulse_anywhere() {
    let mut mud = playing(Build::New);
    let raw = run(&mut mud, "look");
    for at in 0..=raw.len() {
        let parts = reads(&raw, &[at]);
        assert_eq!(parts.concat(), raw);
    }
}

#[test]
fn an_immortal_with_wizi_and_incog_sees_the_prefix_before_each_prompt() {
    let mut mud = Mud::new(Options {
        wizi: 60,
        incog: 60,
        ..Options::new(Build::New)
    });
    let login: Vec<u8> = mud
        .receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
        .into_iter()
        .flat_map(|w| w.bytes)
        .collect();
    assert_eq!(
        packet(&login, "Char.Status").expect("status")["level"],
        mud::IMMORTAL_LEVEL
    );
    assert_eq!(
        prompt_lines(&login),
        ["(Wizi 60) (Incog 60) [1020/1020hp 800/800mn 930/930mv]"]
    );
    // In the game's 256 color 240, as row 410133 shows it.
    let wizi = b"\x1b[38;5;240m(Wizi 60)";
    assert!(login.windows(wizi.len()).any(|w| w == wizi));
    let values = read_back(PROMPT, &login);
    assert_eq!(values.get("wizi").map(String::as_str), Some("60"));
    assert_eq!(values.get("incog").map(String::as_str), Some("60"));

    // Every prompt after it carries the prefix too.
    let raw = run(&mut mud, "prompt [%h/%Hhp (%K hp) %s [%S]>");
    assert_eq!(
        prompt_lines(&raw),
        ["(Wizi 60) (Incog 60) [1020/1020hp (100 hp) common [std]> "]
    );

    // A mortal logs in with neither.
    let mut mud = Mud::new(Options::new(Build::New));
    let login: Vec<u8> = mud
        .receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
        .into_iter()
        .flat_map(|w| w.bytes)
        .collect();
    assert_eq!(
        packet(&login, "Char.Status").expect("status")["level"],
        mud::MORTAL_LEVEL
    );
    assert_eq!(prompt_lines(&login), ["[1020/1020hp 800/800mn 930/930mv]"]);
}

/// Each affect in a Char.Affects packet as `name hours`.
fn affect_hours(raw: &[u8]) -> Vec<String> {
    packet(raw, "Char.Affects").expect("Char.Affects")["affects"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|a| format!("{} {}", a["name"].as_str().unwrap_or(""), a["duration"]))
        .collect()
}

#[test]
fn a_cast_lands_at_once_and_each_tick_takes_an_hour_off() {
    let mut mud = playing(Build::New);
    let raw = run(&mut mud, "cast 10 sanctuary");
    assert_eq!(names(&raw)[0], "Char.Affects", "the cast lands at once");
    assert_eq!(affect_hours(&raw), ["bless 6", "armor 44", "sanctuary 10"]);
    assert!(shown(&raw).contains("You cast sanctuary."));
    let raw = run(&mut mud, "tick");
    assert_eq!(affect_hours(&raw), ["bless 5", "armor 43", "sanctuary 9"]);
    // A recast starts the hours over.
    let raw = run(&mut mud, "cast 48 armor");
    assert_eq!(affect_hours(&raw), ["bless 5", "armor 48", "sanctuary 9"]);
    // An affect at 0 wears off on the next tick, and a permanent one stays.
    let _ = run(&mut mud, "cast 0 bless");
    let _ = run(&mut mud, "cast -1 mounted");
    let raw = run(&mut mud, "tick");
    assert_eq!(
        affect_hours(&raw),
        ["armor 47", "sanctuary 8", "mounted -1"]
    );
    // The login list is what the pfile holds.
    let mut mud = Mud::new(Options {
        affects: vec![mud::Affect::spell("fly", 12)],
        ..Options::new(Build::New)
    });
    let login: Vec<u8> = mud
        .receive(&[telnet::IAC, telnet::DO, telnet::GMCP])
        .into_iter()
        .flat_map(|w| w.bytes)
        .collect();
    assert_eq!(affect_hours(&login), ["fly 12"]);
}

/// Every Char.Affects list in `raw`, in order, each as `name hours`.
fn affect_lists(raw: &[u8]) -> Vec<Vec<String>> {
    packets(raw)
        .into_iter()
        .filter(|(name, _)| name == "Char.Affects")
        .map(|(_, data)| {
            data["affects"]
                .as_array()
                .expect("a list")
                .iter()
                .map(|a| format!("{} {}", a["name"].as_str().unwrap_or(""), a["duration"]))
                .collect()
        })
        .collect()
}

#[test]
fn quitting_takes_each_affect_off_in_turn_before_the_goodbye() {
    let mut mud = playing(Build::New);
    let _ = run(&mut mud, "cast 10 sanctuary");
    let _ = run(&mut mud, "tick");
    let writes = mud.command("quit");
    assert_eq!(writes.len(), 1);
    assert!(writes[0].close);
    // free_char calls affect_remove on each affect, and each removal
    // writes the list that is left straight to the socket.
    assert_eq!(
        affect_lists(&writes[0].bytes),
        [vec!["armor 43", "sanctuary 9"], vec!["sanctuary 9"], vec![]]
    );
    // The goodbye waits in the output buffer until the socket closes.
    let parts = parts(&writes[0].bytes);
    assert!(matches!(parts.last(), Some(Part::Text(_))));
    assert!(parts[..parts.len() - 1]
        .iter()
        .all(|p| matches!(p, Part::Packet(..))));
    // Under lamented tears the game hides every one of those lists.
    let mut mud = playing(Build::New);
    let _ = run(&mut mud, "lament");
    let hidden: Vec<Json> = packets(&mud.command("quit")[0].bytes)
        .into_iter()
        .filter(|(name, _)| name == "Char.Affects")
        .map(|(_, data)| data)
        .collect();
    assert_eq!(hidden.len(), 2);
    assert!(hidden.iter().all(|d| d["hidden"] == true));
}

#[test]
fn quit_menu_takes_your_affects_off_and_the_next_line_plays_you_again() {
    for arg in ["menu", "m", "switch", "character", "char"] {
        let mut mud = playing(Build::New);
        let _ = run(&mut mud, "cast 10 sanctuary");
        let _ = run(&mut mud, "tick");
        let writes = mud.command(&format!("quit {arg}"));
        assert!(writes.iter().all(|w| !w.close), "quit {arg} keeps the link");
        let raw: Vec<u8> = writes.into_iter().flat_map(|w| w.bytes).collect();
        assert_eq!(
            affect_lists(&raw),
            [vec!["armor 43", "sanctuary 9"], vec!["sanctuary 9"], vec![]],
            "quit {arg}"
        );
        assert!(shown(&raw)
            .contains("You step away from the Forsaken Lands and return to your account menu."));
        // Your pfile kept the affects as they were when you quit.
        let again: Vec<u8> = mud
            .receive(b"\r\n")
            .into_iter()
            .flat_map(|w| w.bytes)
            .collect();
        assert_eq!(
            affect_hours(&again),
            ["bless 5", "armor 43", "sanctuary 9"],
            "quit {arg}"
        );
        assert!(shown(&again).contains("Welcome to the fake Aabahran, Tester."));
    }
    // Any other word is a plain quit.
    let mut mud = playing(Build::New);
    assert!(mud.command("quit now")[0].close);
}
