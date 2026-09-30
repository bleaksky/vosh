//! The Aabahran compiler against what the game prints, through
//! `fixtures/prompt/aabahran`.
//!
//! [`game`] is a test expander that follows `bust_a_prompt`,
//! `send_to_char`, `process_color` and `health_prompt` as the server
//! writes them, so it turns a PROMPT and a game state into the bytes the
//! game sends. `lines.json` holds it to the prompt lines the design notes
//! quote, and each of those lines reads back as its shape. `prompts.json`
//! lists settings, and each one is expanded in several game states, read
//! back, and checked value by value against the state.

use std::collections::BTreeMap;

use serde::Deserialize;
use vosh_prompt::aabahran::{compile, Compiled, Origin, ShapeKind, Which, Who};

const LINES: &str = include_str!("../../../fixtures/prompt/aabahran/lines.json");
const PROMPTS: &str = include_str!("../../../fixtures/prompt/aabahran/prompts.json");

/// The game's side, as `comm.c` and `misc.c` write it.
mod game {
    use serde::Deserialize;
    use vosh_prompt::aabahran::colors;

    /// Someone in your group your opponent fights, you included.
    #[derive(Debug, Clone, Deserialize)]
    pub(crate) struct Tank {
        pub(crate) name: String,
        pub(crate) hit: i64,
        pub(crate) max_hit: i64,
    }

    /// What the game holds for you when it prints the prompt.
    #[derive(Debug, Clone, Deserialize)]
    #[serde(default)]
    pub(crate) struct State {
        pub(crate) hit: i64,
        pub(crate) max_hit: i64,
        pub(crate) mana: i64,
        pub(crate) max_mana: i64,
        pub(crate) moves: i64,
        pub(crate) max_move: i64,
        pub(crate) lament: bool,
        pub(crate) fighting: bool,
        pub(crate) tank: Option<Tank>,
        pub(crate) invis: i64,
        pub(crate) incog: i64,
        pub(crate) afk: bool,
        pub(crate) immortal: bool,
        pub(crate) npc: bool,
        pub(crate) color_256: bool,
        pub(crate) cp: i64,
        pub(crate) rp: i64,
        pub(crate) gold: i64,
        pub(crate) exp: i64,
        pub(crate) tnl: i64,
        pub(crate) hour: i64,
        pub(crate) temp: i64,
        pub(crate) sky: String,
        pub(crate) region: String,
        /// As `lang_table` stores it.
        pub(crate) lang: String,
        /// The game's position number, 4 for meditate.
        pub(crate) position: usize,
        pub(crate) stallion: bool,
        /// What `do_promptexit` writes.
        pub(crate) exits: String,
        /// What `append_custom_aff_timer` writes for slots 1 to 10.
        pub(crate) slots: Vec<String>,
        /// Each moon's phase, None when it is not up.
        pub(crate) moons: Vec<Option<usize>>,
        pub(crate) room: String,
        pub(crate) room_vnum: i64,
        pub(crate) area: String,
        pub(crate) area_vnum: i64,
        pub(crate) olc: String,
        pub(crate) olc_vnum: String,
        pub(crate) pacified: bool,
        /// Your `prefix` setting.
        pub(crate) prefix: String,
    }

    impl Default for State {
        fn default() -> Self {
            Self {
                hit: 1020,
                max_hit: 1020,
                mana: 800,
                max_mana: 800,
                moves: 930,
                max_move: 930,
                lament: false,
                fighting: false,
                tank: None,
                invis: 0,
                incog: 0,
                afk: false,
                immortal: false,
                npc: false,
                color_256: true,
                cp: 12,
                rp: 3,
                gold: 1250,
                exp: 123_456,
                tnl: 2345,
                hour: 14,
                temp: 61,
                sky: "cloudy".into(),
                region: "Coastal North".into(),
                lang: "Thsu'ul".into(),
                position: 9,
                stallion: false,
                exits: "[Exits: N (E) S]".into(),
                slots: ["14", "~", "-", "-", "-", "-", "-", "-", "-", "3"]
                    .map(String::from)
                    .to_vec(),
                moons: vec![Some(4), Some(0), None],
                room: "The Bank of Aabahran".into(),
                room_vnum: 3001,
                area: "Aabahran".into(),
                area_vnum: 12,
                olc: String::new(),
                olc_vnum: String::new(),
                pacified: false,
                prefix: String::new(),
            }
        }
    }

    pub(crate) const POS_ABBREV: [&str; 10] = [
        "dea", "mor", "inc", "stn", "", "slp", "rst", "sit", "fgt", "std",
    ];
    pub(crate) const PHASES: [&str; 8] = ["new", "wax", "Hwx", "Gwx", "FUL", "Gwn", "Hwn", "wan"];

    /// `bust_a_prompt`, `comm.c:1709-1954`, from the fight prompt choice
    /// on.
    pub(crate) fn prompt(prompt: &str, fprompt: &str, st: &State) -> Vec<u8> {
        let mut out = Vec::new();
        let setting = if st.fighting && !fprompt.is_empty() {
            fprompt
        } else {
            prompt
        };
        if st.invis > 1 {
            send_to_char(&mut out, &format!("`(240)(Wizi {})`` ", st.invis), st);
        }
        if st.incog > 1 {
            send_to_char(&mut out, &format!("`(240)(Incog {})`` ", st.incog), st);
        }
        if st.afk {
            send_to_char(&mut out, "<AFK> ", st);
            return out;
        }
        let lam = |v: i64| if st.lament { 0 } else { v };
        if setting.is_empty() {
            let text = format!(
                "<{}hp {}m {}mv>{} ",
                lam(st.hit),
                lam(st.mana),
                lam(st.moves),
                st.prefix
            );
            send_to_char(&mut out, &text, st);
            return out;
        }
        let chars: Vec<char> = setting.chars().collect();
        let mut buf = String::new();
        let mut buf2 = String::new();
        let tank = st.tank.as_ref();
        let mut k = 0;
        while k < chars.len() {
            if chars[k] != '%' {
                buf.push(chars[k]);
                k += 1;
                continue;
            }
            k += 1;
            let Some(&c) = chars.get(k) else {
                // The game reads on past the end.
                break;
            };
            let digit = chars.get(k + 1).and_then(|d| d.to_digit(10));
            let mut set = |text: String| {
                buf2 = text;
                buf2.clone()
            };
            let i: String = match c {
                'b' => set(if st.immortal && st.area_vnum != 0 {
                    st.area_vnum.to_string()
                } else {
                    String::new()
                }),
                'a' => set(st.cp.to_string()),
                'A' => set(st.rp.to_string()),
                'c' => set("\n\r".into()),
                'C' => set(if tank.is_some() { "\n\r" } else { "" }.into()),
                'h' => {
                    let pct = (st.max_hit != 0).then(|| st.hit * 100 / st.max_hit);
                    let v = lam(st.hit);
                    set(match pct {
                        Some(p) if p <= 20 => format!("`1{v}``"),
                        Some(p) if p <= 40 => format!("`#{v}``"),
                        _ => v.to_string(),
                    })
                }
                'e' => set(st.exits.clone()),
                'f' => match digit {
                    Some(slot) => {
                        k += 1;
                        let pos = if slot == 0 { 9 } else { slot as usize - 1 };
                        set(st.slots[pos].clone())
                    }
                    None => String::new(),
                },
                'g' => set(st.gold.to_string()),
                'j' => match digit {
                    Some(slot) => {
                        k += 1;
                        let moon = slot as usize;
                        match (1..=3)
                            .contains(&moon)
                            .then(|| st.moons[moon - 1])
                            .flatten()
                        {
                            Some(p) => PHASES[p].to_string(),
                            None => "-".into(),
                        }
                    }
                    None => String::new(),
                },
                'H' => set(lam(st.max_hit).to_string()),
                'i' => if st.stallion { "M" } else { "D" }.into(),
                'l' => {
                    k += 1;
                    let x = chars.get(k).map(char::to_string).unwrap_or_default();
                    set(format!("`{x}"))
                }
                'L' => "``".into(),
                'm' => set(lam(st.mana).to_string()),
                'M' => set(lam(st.max_mana).to_string()),
                'n' => set(tank.map(|t| format!("{}: ", t.name)).unwrap_or_default()),
                'p' | 'P' => {
                    if let (Some(t), false) = (tank, st.lament) {
                        // Below a max of 1 the game writes nothing, so the
                        // buffer keeps what the code before it wrote.
                        if let Some(text) = health_prompt(t.hit, t.max_hit, c == 'p') {
                            buf2 = text;
                        }
                    } else {
                        buf2.clear();
                    }
                    buf2.clone()
                }
                'r' => set(if st.immortal {
                    st.room.clone()
                } else {
                    String::new()
                }),
                'R' => set(if st.immortal {
                    st.room_vnum.to_string()
                } else {
                    String::new()
                }),
                't' => set(st.hour.to_string()),
                's' => {
                    if !st.npc {
                        let mut name = st.lang.clone();
                        if let Some(first) = name.get(..1) {
                            name.replace_range(..1, &first.to_ascii_lowercase());
                        }
                        buf2 = name;
                    }
                    buf2.clone()
                }
                'S' => POS_ABBREV.get(st.position).copied().unwrap_or("").into(),
                'u' => {
                    if st.immortal {
                        buf2 = if st.pacified {
                            "pacified"
                        } else {
                            "not pacified"
                        }
                        .into();
                    }
                    buf2.clone()
                }
                'v' => set(lam(st.moves).to_string()),
                'V' => set(lam(st.max_move).to_string()),
                'w' => set(st.temp.to_string()),
                'W' => set(st.sky.clone()),
                'G' => set(st.region.clone()),
                'x' => set(st.exp.to_string()),
                'X' => set(st.tnl.to_string()),
                'z' => set(if st.immortal {
                    st.area.clone()
                } else {
                    String::new()
                }),
                '%' => set("%".into()),
                'o' => set(st.olc.clone()),
                'O' => set(st.olc_vnum.clone()),
                'K' => set(lam(st.hit * 100 / st.max_hit).to_string()),
                'k' => set(lam(st.mana * 100 / st.max_mana).to_string()),
                'E' => set(lam(st.moves * 100 / st.max_move).to_string()),
                _ => String::new(),
            };
            k += 1;
            buf.push_str(&i);
        }
        send_to_char(&mut out, &buf, st);
        if !st.prefix.is_empty() {
            out.extend_from_slice(st.prefix.as_bytes());
            out.push(b' ');
        }
        out
    }

    /// `health_prompt`, `misc.c:2072-2130`. None below a max of 1, where
    /// the game writes nothing.
    pub(crate) fn health_prompt(cur: i64, max: i64, digital: bool) -> Option<String> {
        if max < 1 {
            return None;
        }
        let percent = 100 * cur / max;
        if digital {
            return Some(format!("[{percent}]"));
        }
        let color = if percent < 5 {
            '!'
        } else if percent < 25 {
            '1'
        } else {
            '3'
        };
        let mut s = String::from("[");
        if percent < 75 {
            s.push('`');
            s.push(color);
        }
        for i in 0..12 {
            if i != 0 && i % 3 == 0 {
                s.push_str("``|");
                if percent < 75 {
                    s.push('`');
                    s.push(color);
                }
            }
            s.push(if i * 25 / 3 < percent { '=' } else { '-' });
        }
        s.push_str("``]");
        Some(s)
    }

    /// `send_to_char`, `comm.c:6583-6649`.
    pub(crate) fn send_to_char(out: &mut Vec<u8>, txt: &str, st: &State) {
        let a: Vec<char> = txt.chars().collect();
        let length = a.len();
        let mut i = 0;
        while i < length {
            if a[i] != '`' {
                let mut bytes = [0; 4];
                out.extend_from_slice(a[i].encode_utf8(&mut bytes).as_bytes());
                i += 1;
                continue;
            }
            i += 1;
            if i >= length {
                continue;
            }
            let c = a[i];
            if (c == '(' || c == ')') && i + 4 <= length {
                let close = if c == '(' { ')' } else { '(' };
                let digits: String = a[i + 1..i + 4].iter().collect();
                if digits.chars().all(|d| d.is_ascii_digit()) && a.get(i + 4) == Some(&close) {
                    let n: i64 = digits.parse().expect("digits");
                    process_color_256(out, n, c == ')', st);
                    i += 5;
                    continue;
                }
            }
            process_color(out, c);
            i += 1;
        }
    }

    /// `process_color`, `comm.c:1957-2049`, with color on.
    fn process_color(out: &mut Vec<u8>, a: char) {
        match a {
            '-' => out.push(b'~'),
            '=' => out.push(b'`'),
            _ => match colors::index(a).and_then(colors::sgr) {
                Some(sgr) => out.extend_from_slice(sgr.as_bytes()),
                None => {
                    let mut bytes = [0; 4];
                    out.extend_from_slice(a.encode_utf8(&mut bytes).as_bytes());
                }
            },
        }
    }

    /// `process_color_256`, `comm.c:2056-2091`.
    fn process_color_256(out: &mut Vec<u8>, n: i64, background: bool, st: &State) {
        if !(0..=255).contains(&n) {
            return;
        }
        if st.color_256 {
            let field = if background { 48 } else { 38 };
            out.extend_from_slice(format!("\x1b[{field};5;{n}m").as_bytes());
            return;
        }
        let ansi = if n < 16 {
            n
        } else if n >= 232 {
            if n < 240 {
                8
            } else if n < 250 {
                7
            } else {
                15
            }
        } else {
            let idx = n - 16;
            let (r, g, b) = (idx / 36, (idx % 36) / 6, idx % 6);
            let bit = |x: i64| i64::from(x >= 3);
            let mut ansi = (bit(b) << 2) | (bit(g) << 1) | bit(r);
            if r >= 4 || g >= 4 || b >= 4 {
                ansi += 8;
            }
            ansi
        };
        let base = match (background, ansi < 8) {
            (false, true) => 30 + ansi,
            (false, false) => 90 + ansi - 8,
            (true, true) => 40 + ansi,
            (true, false) => 100 + ansi - 8,
        };
        out.extend_from_slice(format!("\x1b[{base}m").as_bytes());
    }
}

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
            mobile: false,
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
            mobile: false,
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
