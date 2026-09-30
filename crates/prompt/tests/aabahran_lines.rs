//! The Aabahran compiler against what the game prints, through
//! `fixtures/prompt/aabahran`.
//!
//! The test kit's [`game`] follows `bust_a_prompt`, `send_to_char`,
//! `process_color` and `health_prompt` as the server writes them, so it
//! turns a PROMPT and a game state into the bytes the game sends.
//! `lines.json` holds it to the prompt lines the design notes quote, and
//! each of those lines reads back as its shape. `prompts.json` lists
//! settings, and each one is expanded in several game states, read back,
//! and checked value by value against the state.

use std::collections::BTreeMap;

use serde::Deserialize;
use vosh_prompt::aabahran::{compile, Compiled, Origin, ShapeKind, Which, Who};
use vosh_prompt::testkit::game;

const LINES: &str = include_str!("../../../fixtures/prompt/aabahran/lines.json");
const PROMPTS: &str = include_str!("../../../fixtures/prompt/aabahran/prompts.json");

/// The text a terminal shows for the bytes, with every CSI taken out.
fn plain(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for p in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&p) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The lines of one prompt's bytes, and whether the last one is open.
fn lines(raw: &[u8]) -> (Vec<Vec<u8>>, bool) {
    let mut lines: Vec<Vec<u8>> = vec![Vec::new()];
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(b"\n\r") {
            lines.push(Vec::new());
            i += 2;
        } else {
            lines.last_mut().expect("a line").push(raw[i]);
            i += 1;
        }
    }
    let open = !raw.ends_with(b"\n\r");
    if !open {
        lines.pop();
    }
    (lines, open)
}

fn kind(name: &str) -> ShapeKind {
    match name {
        "normal" => ShapeKind::Normal,
        "tank" => ShapeKind::Tank,
        "either" => ShapeKind::Either,
        "afk" => ShapeKind::Afk,
        "fallback" => ShapeKind::Fallback,
        other => panic!("no shape {other}"),
    }
}

#[derive(Debug, Deserialize)]
struct LineCase {
    row: Option<u64>,
    what: String,
    prompt: String,
    #[serde(default)]
    fprompt: String,
    state: Option<game::State>,
    lines: Vec<Line>,
    whole: bool,
    shape: Option<String>,
    values: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Line {
    hex: Option<String>,
    plain: String,
}

fn line_cases() -> Vec<LineCase> {
    #[derive(Deserialize)]
    struct File {
        cases: Vec<LineCase>,
    }
    serde_json::from_str::<File>(LINES)
        .expect("lines.json")
        .cases
}

#[test]
fn the_expander_prints_every_quoted_line() {
    for case in line_cases() {
        let label = format!("{:?} {}", case.row, case.what);
        let Some(state) = &case.state else {
            assert!(case.lines.iter().all(|l| l.hex.is_none()), "{label}");
            continue;
        };
        let raw = game::prompt(&case.prompt, &case.fprompt, state);
        let (printed, _) = lines(&raw);
        assert!(printed.len() >= case.lines.len(), "{label}");
        if case.whole {
            assert_eq!(printed.len(), case.lines.len(), "{label}");
        }
        for (line, got) in case.lines.iter().zip(&printed) {
            let want = hex(line.hex.as_deref().expect("hex"));
            assert_eq!(
                String::from_utf8_lossy(got),
                String::from_utf8_lossy(&want),
                "{label}"
            );
            assert_eq!(plain(&want), line.plain, "{label}");
        }
    }
}

#[test]
fn every_quoted_line_reads_as_its_shape() {
    for case in line_cases() {
        let label = format!("{:?} {}", case.row, case.what);
        let immortal = case.state.as_ref().is_some_and(|s| s.immortal);
        let who = Who {
            immortal,
            ..Who::default()
        };
        let compiled = compile(&case.prompt, &case.fprompt, Origin::Stored, who).expect(&label);
        let plains: Vec<&str> = case.lines.iter().map(|l| l.plain.as_str()).collect();
        let Some(shape_name) = &case.shape else {
            for shape in &compiled.shapes {
                assert!(shape.read(&plains).is_none(), "{label}: {:?}", shape.kind);
                assert!(
                    shape.read_partial(&plains).is_none(),
                    "{label}: {:?}",
                    shape.kind
                );
            }
            continue;
        };
        let shape = compiled
            .shapes
            .iter()
            .find(|s| s.kind == kind(shape_name))
            .expect(&label);
        let values = if case.whole {
            let state = case.state.as_ref().expect("a state");
            let (_, open) = lines(&game::prompt(&case.prompt, &case.fprompt, state));
            let read = if open {
                shape.read_partial(&plains)
            } else {
                shape.read(&plains)
            };
            read.expect(&label).values
        } else {
            let re = &shape.lines[0].line;
            let found = re.captures(plains[0]).expect(&label);
            re.capture_names()
                .flatten()
                .map(|n| {
                    let v = found.name(n).map_or("", |m| m.as_str());
                    (n.to_string(), v.to_string())
                })
                .collect()
        };
        for (name, value) in &values {
            let want = case.values.get(name).map_or("", String::as_str);
            assert_eq!(value, want, "{label}: {name}");
        }
        for name in case.values.keys() {
            assert!(values.contains_key(name), "{label}: reads no {name}");
        }
    }
}

#[derive(Debug, Deserialize)]
struct PromptCase {
    row: Option<u64>,
    #[serde(default)]
    what: String,
    origin: String,
    #[serde(default)]
    immortal: bool,
    prompt: String,
    #[serde(default)]
    fprompt: String,
    shapes: Vec<String>,
    warnings: Vec<String>,
}

fn prompt_cases() -> Vec<PromptCase> {
    #[derive(Deserialize)]
    struct File {
        prompts: Vec<PromptCase>,
    }
    serde_json::from_str::<File>(PROMPTS)
        .expect("prompts.json")
        .prompts
}

fn compiled(case: &PromptCase, who: Who) -> Compiled {
    let origin = match case.origin.as_str() {
        "stored" => Origin::Stored,
        "typed" => Origin::Typed,
        other => panic!("no origin {other}"),
    };
    compile(&case.prompt, &case.fprompt, origin, who)
        .unwrap_or_else(|e| panic!("{:?} {}: {e}", case.row, case.prompt))
}

#[test]
fn every_prompt_compiles_to_its_shapes_and_warnings() {
    for case in prompt_cases() {
        let label = format!("{:?} {} {}", case.row, case.what, case.prompt);
        let who = Who {
            immortal: case.immortal,
            ..Who::default()
        };
        let compiled = compiled(&case, who);
        let shapes: Vec<String> = compiled
            .shapes
            .iter()
            .map(|s| {
                let which = match s.which {
                    Which::Prompt => "prompt",
                    Which::Fight => "fprompt",
                };
                let kind = serde_json::to_value(s.kind).expect("a kind");
                format!("{which} {}", kind.as_str().expect("a name"))
            })
            .collect();
        assert_eq!(shapes, case.shapes, "{label}");
        let warnings: Vec<String> = compiled
            .warnings
            .iter()
            .map(|w| {
                let kind = serde_json::to_value(w.kind).expect("a kind");
                kind.as_str().expect("a name").to_string()
            })
            .collect();
        assert_eq!(warnings, case.warnings, "{label}");
    }
}

/// The game states each setting is printed in.
fn states() -> Vec<(&'static str, game::State)> {
    use game::{State, Tank};
    let base = State::default;
    vec![
        ("idle", base()),
        (
            "immortal",
            State {
                immortal: true,
                invis: 60,
                incog: 60,
                pacified: true,
                olc: "MEdit".into(),
                olc_vnum: "1200".into(),
                ..base()
            },
        ),
        (
            "someone else tanks",
            State {
                fighting: true,
                position: 8,
                tank: Some(Tank {
                    name: "Brother Tuck".into(),
                    hit: 50,
                    max_hit: 100,
                }),
                ..base()
            },
        ),
        (
            "you tank at 3 percent",
            State {
                fighting: true,
                position: 8,
                hit: 31,
                tank: Some(Tank {
                    name: "Tester".into(),
                    hit: 31,
                    max_hit: 1020,
                }),
                ..base()
            },
        ),
        (
            "you tank under lamented tears",
            State {
                fighting: true,
                lament: true,
                hit: 300,
                tank: Some(Tank {
                    name: "Tester".into(),
                    hit: 300,
                    max_hit: 1020,
                }),
                ..base()
            },
        ),
        (
            "a fight with no one in your group tanking",
            State {
                fighting: true,
                position: 8,
                ..base()
            },
        ),
        (
            "away",
            State {
                afk: true,
                invis: 60,
                ..base()
            },
        ),
        (
            "meditating under a new moon",
            State {
                position: 4,
                moons: vec![Some(0), None, Some(7)],
                slots: ["-", "5", "~", "-", "-", "-", "-", "-", "-", "-"]
                    .map(String::from)
                    .to_vec(),
                exits: "[Exits: --- ]".into(),
                sky: "error!".into(),
                region: "Mountain East".into(),
                temp: -3,
                ..base()
            },
        ),
        (
            "256 color off",
            State {
                color_256: false,
                invis: 60,
                immortal: true,
                ..base()
            },
        ),
    ]
}

/// What a field should read in a state.
fn expected(name: &str, st: &game::State) -> String {
    let lam = |v: i64| if st.lament { 0 } else { v };
    let tank = st.tank.as_ref().filter(|_| !st.lament);
    let level = |v: i64| if v > 1 { v.to_string() } else { String::new() };
    let immortal = |text: String| if st.immortal { text } else { String::new() };
    match name {
        "wizi" => level(st.invis),
        "incog" => level(st.incog),
        "hp" => lam(st.hit).to_string(),
        "maxhp" => lam(st.max_hit).to_string(),
        "mana" => lam(st.mana).to_string(),
        "maxmana" => lam(st.max_mana).to_string(),
        "move" => lam(st.moves).to_string(),
        "maxmove" => lam(st.max_move).to_string(),
        "hp_pct" => lam(st.hit * 100 / st.max_hit).to_string(),
        "mana_pct" => lam(st.mana * 100 / st.max_mana).to_string(),
        "move_pct" => lam(st.moves * 100 / st.max_move).to_string(),
        "cp" => st.cp.to_string(),
        "rp" => st.rp.to_string(),
        "gold" => st.gold.to_string(),
        "exp" => st.exp.to_string(),
        "tnl" => st.tnl.to_string(),
        "hour" => st.hour.to_string(),
        "temp" => st.temp.to_string(),
        "weather" => st.sky.clone(),
        "region" => st.region.clone(),
        "lang" => {
            let mut lang = st.lang.clone();
            lang.replace_range(..1, &lang[..1].to_ascii_lowercase());
            lang
        }
        "pos" => game::POS_ABBREV[st.position].to_string(),
        "stallion" => if st.stallion { "M" } else { "D" }.into(),
        "exits" => st.exits["[Exits:".len()..st.exits.len() - 1].to_string(),
        "tank" => st.tank.as_ref().map(|t| t.name.clone()).unwrap_or_default(),
        "tank_pct" => tank
            .map(|t| (t.hit * 100 / t.max_hit).to_string())
            .unwrap_or_default(),
        "tank_bar" => tank
            .map(|t| {
                let percent = 100 * t.hit / t.max_hit;
                let cells: Vec<String> = (0..4)
                    .map(|group| {
                        (0..3)
                            .map(|j| {
                                let i = group * 3 + j;
                                if i * 25 / 3 < percent {
                                    '='
                                } else {
                                    '-'
                                }
                            })
                            .collect()
                    })
                    .collect();
                cells.join("|")
            })
            .unwrap_or_default(),
        "room" => immortal(st.room.clone()),
        "room_num" => immortal(st.room_vnum.to_string()),
        "area" => immortal(st.area.clone()),
        "area_num" => immortal(st.area_vnum.to_string()),
        "olc" => st.olc.clone(),
        "olc_vnum" => st.olc_vnum.clone(),
        "pacify" => if st.pacified {
            "pacified"
        } else {
            "not pacified"
        }
        .into(),
        "afk" => "1".into(),
        slot if slot.starts_with("slot") => {
            let n: usize = slot["slot".len()..].parse().expect("a slot");
            st.slots[n - 1].clone()
        }
        moon if moon.starts_with("moon") => {
            let n: usize = moon["moon".len()..].parse().expect("a moon");
            st.moons[n - 1].map_or("-".into(), |p| game::PHASES[p].to_string())
        }
        other => panic!("no expected value for {other}"),
    }
}

#[test]
fn every_prompt_reads_back_what_the_game_printed() {
    let mut checked = std::collections::BTreeSet::new();
    for case in prompt_cases() {
        for (state_name, state) in states() {
            // Vosh compiles for who you are, as the game prints for you.
            let who = Who {
                immortal: state.immortal,
                mobile: state.npc,
                ..Who::default()
            };
            let compiled = compiled(&case, who);
            let label = format!("{:?} {} in state {state_name}", case.row, case.prompt);
            let raw = game::prompt(&compiled.prompt, &compiled.fprompt, &state);
            let (printed, open) = lines(&raw);
            let plains: Vec<String> = printed.iter().map(|l| plain(l)).collect();
            let plains: Vec<&str> = plains.iter().map(String::as_str).collect();
            let found = compiled.shapes.iter().find_map(|shape| {
                let read = if open {
                    shape.read_partial(&plains)
                } else {
                    shape.read(&plains)
                };
                read.map(|r| (shape, r))
            });
            let (shape, read) =
                found.unwrap_or_else(|| panic!("{label}: no shape reads {plains:?}"));
            // The shape is the one the game printed.
            let fight = state.fighting && !compiled.fprompt.is_empty() && !state.afk;
            let want_which = if fight { Which::Fight } else { Which::Prompt };
            assert_eq!(shape.which, want_which, "{label}");
            let want_kinds: &[ShapeKind] = if state.afk {
                &[ShapeKind::Afk]
            } else if compiled.prompt.is_empty() && !fight {
                &[ShapeKind::Fallback]
            } else if state.tank.is_some() {
                &[ShapeKind::Tank, ShapeKind::Either]
            } else {
                &[ShapeKind::Normal, ShapeKind::Either]
            };
            assert!(
                want_kinds.contains(&shape.kind),
                "{label}: {:?}",
                shape.kind
            );
            for (name, value) in &read.values {
                assert_eq!(value, &expected(name, &state), "{label}: {name}");
                if !value.is_empty() {
                    checked.insert(name.clone());
                }
            }
            // An open prompt also reads whole once a line end completes it.
            if open {
                assert!(shape.read(&plains).is_some(), "{label}");
            }
        }
    }
    // Every field a code fills was read with a value somewhere.
    let every = [
        "hp", "maxhp", "mana", "maxmana", "move", "maxmove", "hp_pct", "mana_pct", "move_pct",
        "cp", "rp", "gold", "exp", "tnl", "hour", "temp", "weather", "region", "lang", "pos",
        "stallion", "exits", "slot1", "slot10", "moon1", "moon2", "moon3", "tank", "tank_pct",
        "tank_bar", "room", "room_num", "area", "area_num", "olc", "olc_vnum", "pacify", "wizi",
        "incog", "afk",
    ];
    let missed: Vec<&str> = every
        .into_iter()
        .filter(|name| !checked.contains(*name))
        .collect();
    assert!(missed.is_empty(), "never read with a value: {missed:?}");
}
