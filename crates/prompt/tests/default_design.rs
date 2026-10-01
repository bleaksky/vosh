//! Vosh's default design, At a glance, drawn the way a session draws it.
//!
//! Each state is what the game holds when it prints a prompt, with the
//! GMCP packets that reach Vosh before that prompt in the server's wire
//! order: the login packets, then the prompt time packages Char.Vitals,
//! Char.Worth, Char.Combat and Group.Info, and on the new build Char.State
//! and Room.Weather. The test kit's `bust_a_prompt` twin prints the game's
//! own prompt for the PROMPT setting below, the stage reads it line by
//! line as the session offers it, and the resolver draws the design from
//! what it read. Nothing is drawn by hand.
//!
//! The states are the ones the design was chosen on, the fight with a
//! tank, a group, a long mob name, lamented tears, an immortal, the older
//! server build and missing affects among them, then meditating. Prompts
//! that give no max, and other games, draw last.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde_json::{json, Value as Json};
use vosh_prompt::config::{AabahranCapture, RegexCapture};
use vosh_prompt::stage::{End, Offer};
use vosh_prompt::template::{Code, ColorSpec, Format, TokenKind};
use vosh_prompt::testkit::{game, shown, Build};
use vosh_prompt::vars::{self, Capture, FormatId, Tick, Vosh};
use vosh_prompt::{
    render_str, CaptureConfig, PromptConfig, PromptEngine, RenderOptions, Template, DEFAULT_DESIGN,
};

/// The PROMPT the design was chosen against, as the game stores it, with
/// the space `do_prompt` appends since it does not end in `%c`.
const PROMPT: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv (%K hp) %s [%S]> ";

const OPPONENT: &str = "a Blackwatch guard";

/// A mob short description from the area files, 46 characters.
const LONG_NAME: &str = "Davaius, Captain Commander of the Dragon Guard";

/// The affects the profile tracks, for `missing`.
const TRACKED: [&str; 5] = [
    "sanctuary",
    "haste",
    "protection evil",
    "detect invisible",
    "fly",
];

/// Char.State's word for each position, from dead to standing.
const POSITIONS: [&str; 10] = [
    "dead",
    "mortally wounded",
    "incapacitated",
    "stunned",
    "meditate",
    "sleeping",
    "resting",
    "sitting",
    "fighting",
    "standing",
];

const FIGHTING: usize = 8;
const RESTING: usize = 6;
const SLEEPING: usize = 5;
const MEDITATE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Room {
    /// The Bank of Aabahran, one exit south.
    Bank,
    /// The village square, four exits.
    Square,
}

/// What the game holds for you, with the defaults every state starts
/// from: full, out of a fight, solo, in the Bank, every tracked affect on.
#[derive(Debug, Clone)]
struct St {
    name: &'static str,
    build: Build,
    hit: i64,
    max_hit: i64,
    mana: i64,
    max_mana: i64,
    moves: i64,
    max_move: i64,
    position: usize,
    fighting: bool,
    /// Who your opponent fights, with their health and max.
    tank: Option<(&'static str, i64, i64)>,
    opponent: &'static str,
    opp_pct: i64,
    lament: bool,
    wizi: i64,
    incog: i64,
    afk: bool,
    gold: i64,
    /// How many of the tracked affects, from the first, are off.
    missing: usize,
    room: Room,
    /// Others in your group, besides a tank who is not you.
    group: Vec<(&'static str, i64, i64)>,
}

impl St {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            build: Build::New,
            hit: 1020,
            max_hit: 1020,
            mana: 800,
            max_mana: 800,
            moves: 930,
            max_move: 930,
            position: 9,
            fighting: false,
            tank: None,
            opponent: OPPONENT,
            opp_pct: 60,
            lament: false,
            wizi: 0,
            incog: 0,
            afk: false,
            gold: 1250,
            missing: 0,
            room: Room::Bank,
            group: Vec::new(),
        }
    }

    /// In a fight that `tank` takes, standing your ground.
    fn fight(mut self, tank: (&'static str, i64, i64)) -> Self {
        self.fighting = true;
        self.position = FIGHTING;
        self.tank = Some(tank);
        self
    }

    fn immortal(&self) -> bool {
        self.wizi > 0 || self.incog > 0
    }

    fn level(&self) -> i64 {
        if self.immortal() {
            60
        } else {
            50
        }
    }

    /// The game's side, for the test kit's prompt printer.
    fn game(&self) -> game::State {
        game::State {
            hit: self.hit,
            max_hit: self.max_hit,
            mana: self.mana,
            max_mana: self.max_mana,
            moves: self.moves,
            max_move: self.max_move,
            lament: self.lament,
            fighting: self.fighting,
            tank: self.tank.map(|(name, hit, max_hit)| game::Tank {
                name: name.into(),
                hit,
                max_hit,
            }),
            invis: self.wizi,
            incog: self.incog,
            afk: self.afk,
            immortal: self.immortal(),
            gold: self.gold,
            lang: "common".into(),
            position: self.position,
            exits: "[Exits: S]".into(),
            color_256: true,
            fallback_hides: self.build == Build::New,
            ..game::State::default()
        }
    }

    /// The packets that reach Vosh before the prompt, in wire order, for
    /// the PROMPT setting `prompt`.
    fn packets(&self, prompt: &str) -> Vec<(&'static str, Json)> {
        let new = self.build == Build::New;
        let level = self.level();
        let mut out = vec![(
            "Char.Status",
            json!({"name": "Tester", "level": level, "race": "elf", "class": "invoker"}),
        )];
        if new {
            out.push((
                "Char.Prompt",
                json!({"enabled": true, "prompt": prompt, "fprompt": ""}),
            ));
        }
        let affects: Vec<Json> = [
            ("sanctuary", 12, "none", 0),
            ("haste", 8, "dex", 1),
            ("protection evil", 20, "none", 0),
            ("detect invisible", 30, "none", 0),
            ("fly", 2, "none", 0),
            ("armor", 24, "ac", -20),
        ]
        .into_iter()
        .filter(|(name, ..)| !TRACKED[..self.missing].contains(name))
        .map(|(name, duration, location, modifier)| {
            json!({"kind": "spell", "name": name, "duration": duration, "level": 50,
                   "location": location, "modifier": modifier})
        })
        .collect();
        if self.lament {
            out.push(("Char.Affects", json!({"affects": [], "hidden": true})));
        } else {
            out.push(("Char.Affects", json!({ "affects": affects })));
        }
        let worth = json!({"gold": self.gold, "bank": 5000, "exp": 123_456, "tnl": 2345,
                           "trains": 3, "practices": 12, "cps": 12, "rps": 3, "cabal": "none"});
        out.push(("Char.Worth", worth.clone()));
        out.push((
            "World.Time",
            json!({"hour": 20, "day": 3, "month": 5, "year": 1203,
                   "sunlight": "dark", "sky": "cloudless"}),
        ));
        out.push((
            "Room.Info",
            match self.room {
                Room::Bank => json!({"num": 5279, "name": "The Bank of Aabahran",
                    "area": "Fort Blackwatch", "terrain": "inside", "sector": 0, "region": 0,
                    "climate": "Temperate", "exits": {"south": 5233}}),
                Room::Square => json!({"num": 5271, "name": "Blackwatch Village Square",
                    "area": "Fort Blackwatch", "terrain": "city", "sector": 1, "region": 0,
                    "climate": "Temperate",
                    "exits": {"north": 5270, "east": 5272, "south": 5274, "west": 5273}}),
            },
        ));
        // The prompt time packages.
        if self.lament {
            out.push((
                "Char.Vitals",
                json!({"hp": 0, "maxhp": 0, "mana": 0, "maxmana": 0, "move": 0,
                       "maxmove": 0, "hidden": true}),
            ));
        } else {
            out.push((
                "Char.Vitals",
                json!({"hp": self.hit, "maxhp": self.max_hit, "mana": self.mana,
                       "maxmana": self.max_mana, "move": self.moves,
                       "maxmove": self.max_move}),
            ));
        }
        out.push(("Char.Worth", worth));
        out.push(("Char.Combat", self.combat()));
        out.push(("Group.Info", self.group_info()));
        if new {
            out.push((
                "Char.State",
                json!({"position": POSITIONS[self.position], "language": "common"}),
            ));
            out.push((
                "Room.Weather",
                json!({"sky": "indoors", "temp": 68, "unit": "F", "region": "Temperate"}),
            ));
        }
        out
    }

    fn combat(&self) -> Json {
        if !self.fighting {
            return json!({});
        }
        let mut k = json!({ "target": self.opponent });
        if self.build == Build::New {
            if self.lament {
                k["hidden"] = json!(true);
            } else {
                k["condition"] = json!(condition(self.opp_pct));
                k["hp_pct"] = json!(self.opp_pct);
            }
            if let Some((name, hit, max_hit)) = self.tank {
                let mut tank = json!({ "name": name });
                // Only lamented tears drops the tank's health.
                if !self.lament {
                    tank["hp_pct"] = json!(100 * hit / max_hit.max(1));
                }
                k["tank"] = tank;
            }
        } else {
            k["condition"] = json!(condition(self.opp_pct));
            k["hp_pct"] = json!(self.opp_pct);
        }
        k
    }

    /// Solo, the game sends Group.Info `{}` (`gmcp_send_group`, no leader,
    /// no master and no follower). A group lists every member, you
    /// included.
    fn group_info(&self) -> Json {
        if self.lament {
            return json!({"hidden": true});
        }
        let mut others = self.group.clone();
        if let Some(tank) = self.tank {
            if tank.0 != "Tester" && others.iter().all(|o| o.0 != tank.0) {
                others.push(tank);
            }
        }
        if others.is_empty() {
            return json!({});
        }
        let pct = |v: i64, m: i64| if m == 0 { 0 } else { v * 100 / m };
        let mut members = vec![json!({"id": 1, "name": "Tester", "level": self.level(),
            "class": "invoker", "hp_pct": pct(self.hit, self.max_hit),
            "mana_pct": pct(self.mana, self.max_mana),
            "move_pct": pct(self.moves, self.max_move), "tnl": 2345})];
        for (i, (name, hit, max_hit)) in others.iter().enumerate() {
            members.push(
                json!({"id": 2 + i, "name": name, "level": 50, "class": "warrior",
                "hp_pct": pct(*hit, *max_hit), "mana_pct": 100, "move_pct": 90,
                "tnl": 800}),
            );
        }
        let leader = members.last().map(|m| m["name"].clone());
        json!({"leader": leader, "members": members})
    }
}

/// What the game says of your opponent's health.
fn condition(pct: i64) -> &'static str {
    match pct {
        100.. => "excellent",
        90.. => "a few scratches",
        75.. => "small wounds",
        50.. => "quite a few wounds",
        30.. => "big nasty wounds",
        15.. => "pretty hurt",
        0.. => "awful",
        _ => "bleeding to death",
    }
}

/// The states the design was chosen on, each with what it draws.
fn states() -> Vec<(St, &'static str)> {
    let you = |hit| ("Tester", hit, 1020);
    vec![
        (
            St::new("full"),
            "1020/1020hp 800/800mn 930/930mv  std common  [S]  1,250g ",
        ),
        (
            St {
                hit: 180,
                ..St::new("low")
            },
            "180/1020hp 800/800mn 930/930mv  std common  [S]  1,250g ",
        ),
        (
            St {
                hit: 765,
                mana: 640,
                ..St::new("tanking")
            }
            .fight(you(765)),
            "a Blackwatch guard ██████░░░░ 60% quite a few wounds\n\
             765/1020hp 640/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                hit: 1002,
                opp_pct: 95,
                ..St::new("fight-start")
            }
            .fight(you(1002)),
            "a Blackwatch guard ██████████ 95% a few scratches\n\
             1002/1020hp 800/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                mana: 520,
                ..St::new("group")
            }
            .fight(("Ally", 781, 1000)),
            "a Blackwatch guard ██████░░░░ 60% quite a few wounds  tank Ally\n\
             1020/1020hp 520/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                hit: 765,
                mana: 640,
                group: vec![("Ally", 1000, 1000)],
                ..St::new("group-tank")
            }
            .fight(you(765)),
            "a Blackwatch guard ██████░░░░ 60% quite a few wounds  tank Tester\n\
             765/1020hp 640/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                hit: 880,
                opponent: LONG_NAME,
                ..St::new("long-name")
            }
            .fight(you(880)),
            "Davaius, Captain Commander of the Dragon Guard ██████░░░░ 60% quite a few wounds\n\
             880/1020hp 800/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                hit: 150,
                mana: 90,
                moves: 610,
                opp_pct: 22,
                ..St::new("desperate")
            }
            .fight(you(150)),
            "a Blackwatch guard ██░░░░░░░░ 22% pretty hurt\n\
             150/1020hp 90/800mn 610/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                lament: true,
                ..St::new("lament")
            },
            "?/?hp ?/?mn ?/?mv  std common  [S]  1,250g  missing ? ",
        ),
        (
            St {
                lament: true,
                ..St::new("lament-fight")
            }
            .fight(you(765)),
            // The game hides your group too, so the tank shows solo.
            "a Blackwatch guard ·········· ?% ?  tank Tester\n\
             ?/?hp ?/?mn ?/?mv  fgt common  [S]  1,250g  missing ? ",
        ),
        (
            St {
                wizi: 60,
                incog: 60,
                ..St::new("immortal")
            },
            "1020/1020hp 800/800mn 930/930mv  std common  [S]  1,250g  wizi 60 incog 60 ",
        ),
        (
            St {
                wizi: 60,
                incog: 60,
                mana: 520,
                ..St::new("immortal-fight")
            }
            .fight(("Ally", 781, 1000)),
            // Out of a fight only.
            "a Blackwatch guard ██████░░░░ 60% quite a few wounds  tank Ally\n\
             1020/1020hp 520/800mn 930/930mv  fgt common  [S]  1,250g ",
        ),
        (
            St {
                afk: true,
                ..St::new("afk")
            },
            "1020/1020hp 800/800mn 930/930mv  std common  [S]  1,250g ",
        ),
        (
            St {
                position: RESTING,
                mana: 410,
                moves: 388,
                ..St::new("resting")
            },
            "1020/1020hp 410/800mn 388/930mv  rst common  [S]  1,250g ",
        ),
        (
            St {
                position: SLEEPING,
                hit: 640,
                mana: 220,
                ..St::new("sleeping")
            },
            "640/1020hp 220/800mn 930/930mv  slp common  [S]  1,250g ",
        ),
        (
            St {
                position: MEDITATE,
                mana: 260,
                ..St::new("meditate")
            },
            // The game's `%S` prints nothing while you meditate.
            "1020/1020hp 260/800mn 930/930mv  med common  [S]  1,250g ",
        ),
        (
            St {
                build: Build::Older,
                position: MEDITATE,
                ..St::new("older-meditate")
            },
            "1020/1020hp 800/800mn 930/930mv  med common  1,250g ",
        ),
        (
            St {
                room: Room::Square,
                ..St::new("square")
            },
            "1020/1020hp 800/800mn 930/930mv  std common  [N E S W]  1,250g ",
        ),
        (
            St {
                build: Build::Older,
                ..St::new("older")
            },
            // No Room.Info exits, and position and language from %S and %s.
            "1020/1020hp 800/800mn 930/930mv  std common  1,250g ",
        ),
        (
            St {
                missing: 1,
                ..St::new("missing-1")
            },
            "1020/1020hp 800/800mn 930/930mv  std common  [S]  1,250g  missing sanctuary ",
        ),
        (
            St {
                missing: 3,
                ..St::new("missing-3")
            },
            "1020/1020hp 800/800mn 930/930mv  std common  [S]  1,250g  \
             missing sanctuary, haste, protection evil ",
        ),
    ]
}

/// What a session draws for one state.
struct Drawn {
    ansi: String,
    plain: String,
    rows: usize,
    /// Rows the band keeps for the design and the game's prompt.
    zone: usize,
    /// The game's lines, and how many of them the design replaces.
    lines: usize,
    replaced: usize,
    afk: bool,
}

fn at() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-09-30T20:14:00-05:00").expect("a valid time")
}

/// The game's lines for one prompt, cut at its `\n\r` line ends.
fn lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new()];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"\n\r") {
            out.push(Vec::new());
            i += 2;
        } else {
            out.last_mut().expect("a line").push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Draw `template` for `st` the way the session does.
fn draw_as(template: &str, st: &St) -> Drawn {
    draw_with(template, PROMPT, st, |_| true)
}

/// Draw `template` for `st` the way the session does, with `prompt` as
/// the PROMPT setting and only the packets `sent` lets through.
fn draw_with(template: &str, prompt: &str, st: &St, sent: fn(&str) -> bool) -> Drawn {
    let mut engine = PromptEngine::default();
    engine.set_config(PromptConfig {
        draw: true,
        template: template.into(),
        capture: CaptureConfig::Aabahran(AabahranCapture {
            prompt: prompt.into(),
            ..AabahranCapture::default()
        }),
        ..PromptConfig::default()
    });
    engine.connect(true);
    for (package, data) in st.packets(prompt) {
        if sent(package) {
            engine.observe(package, data, at());
        }
    }
    let game_lines = lines(&game::prompt(prompt, "", &st.game()));
    let last = game_lines.len() - 1;
    let mut block = None;
    for (i, raw) in game_lines.iter().enumerate() {
        // The head lines end in `\n\r`, and IAC GA ends the last.
        let end = if i == last { End::Marker } else { End::Line };
        if let Offer::Prompt(b, _) = engine.stage.offer(raw, &shown(raw), None, end).offer {
            block = Some(b);
        }
    }
    let block = block.unwrap_or_else(|| panic!("{}: the stage read no prompt", st.name));
    engine.vars.capture(Capture {
        values: block.values.clone(),
        raw: Some(block.raw_text()),
    });
    let rendered = render_str(
        template,
        &engine.vars.resolver(&vosh()),
        RenderOptions::default(),
    );
    Drawn {
        ansi: rendered.ansi,
        plain: rendered.plain,
        rows: rendered.rows,
        zone: engine.zone(),
        lines: block.lines.len(),
        replaced: block.replaced.len(),
        afk: block.afk,
    }
}

/// What Vosh itself supplies in every state.
fn vosh() -> Vosh {
    Vosh {
        tick: Some(Tick {
            remaining: 14,
            interval: Some(30),
        }),
        target: None,
        profile: Some("Default".into()),
        now: NaiveDate::from_ymd_opt(2026, 9, 30).and_then(|d| d.and_hms_opt(20, 14, 0)),
        tracked: TRACKED.map(String::from).to_vec(),
    }
}

/// Draw the default design on a game other than The Forsaken Lands,
/// whose prompt `line` a pattern reads, after `packets`.
fn draw_elsewhere(pattern: &str, line: &str, packets: &[(&str, Json)]) -> Drawn {
    let mut engine = PromptEngine::default();
    engine.set_config(PromptConfig {
        draw: true,
        template: DEFAULT_DESIGN.into(),
        capture: CaptureConfig::Regex(RegexCapture {
            lines: vec![pattern.into()],
            settle: vosh_prompt::capture::settle(pattern),
            ..RegexCapture::default()
        }),
        ..PromptConfig::default()
    });
    engine.connect(false);
    assert!(!engine.forsaken());
    for (package, data) in packets {
        engine.observe(package, data.clone(), at());
    }
    let raw = line.as_bytes();
    let Offer::Prompt(block, _) = engine
        .stage
        .offer(raw, &shown(raw), None, End::Marker)
        .offer
    else {
        panic!("the stage read no prompt in {line:?}");
    };
    engine.vars.capture(Capture {
        values: block.values.clone(),
        raw: Some(block.raw_text()),
    });
    let rendered = render_str(
        DEFAULT_DESIGN,
        &engine.vars.resolver(&vosh()),
        RenderOptions::default(),
    );
    Drawn {
        ansi: rendered.ansi,
        plain: rendered.plain,
        rows: rendered.rows,
        zone: engine.zone(),
        lines: block.lines.len(),
        replaced: block.replaced.len(),
        afk: block.afk,
    }
}

/// The drawn text holds no code left raw, no bare slash and no SGR
/// sequence the text does not account for, and ends in a reset.
fn assert_clean(name: &str, drawn: &Drawn) {
    let text = strip_sgr(&drawn.ansi).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(text, drawn.plain.replace('\n', "\r\n"), "{name}");
    assert!(drawn.ansi.ends_with("\x1b[0m"), "{name}");
    assert!(
        !drawn.plain.contains(['%', '{', '}']),
        "{name}: {:?}",
        drawn.plain
    );
    // A slash sits between a value and its max, hidden or not.
    let value = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit() || c == '?');
    for (i, _) in drawn.plain.match_indices('/') {
        assert!(
            value(drawn.plain[..i].chars().last()) && value(drawn.plain[i + 1..].chars().next()),
            "{name}: a slash with no value beside it in {:?}",
            drawn.plain
        );
    }
}

fn draw(st: &St) -> Drawn {
    draw_as(DEFAULT_DESIGN, st)
}

/// The bytes with every SGR sequence taken out, or the first escape that
/// is not one.
fn strip_sgr(ansi: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = ansi.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        if chars.next() != Some('[') {
            return Err(format!("an escape that is not CSI in {ansi:?}"));
        }
        let mut params = String::new();
        loop {
            match chars.next() {
                Some('m') => break,
                Some(p) if p.is_ascii_digit() || p == ';' => params.push(p),
                other => return Err(format!("CSI {params}{other:?} is not SGR in {ansi:?}")),
            }
        }
    }
    Ok(out)
}

/// The text of a row with every run of digits, and the commas between
/// them, as one `#`, and every gauge cell as `▪`. Rows that read the
/// same this way put every other cell in the same place.
fn shape(row: &str) -> String {
    let mut out = String::new();
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '0'..='9' => {
                while chars
                    .peek()
                    .is_some_and(|n| n.is_ascii_digit() || *n == ',')
                {
                    chars.next();
                }
                out.push('#');
            }
            '█' | '░' | '·' => out.push('▪'),
            _ => out.push(c),
        }
    }
    out
}

/// How many cells the numbers of a row take, their digits and the commas
/// that group them.
fn number_cells(row: &str) -> usize {
    let mut cells = 0;
    let mut in_number = false;
    for c in row.chars() {
        in_number = c.is_ascii_digit() || (in_number && c == ',');
        if in_number {
            cells += 1;
        }
    }
    cells
}

#[test]
fn the_default_design_parses_into_fields_and_formats_vosh_knows() {
    let template = Template::parse(DEFAULT_DESIGN);
    assert!(!template.is_empty());
    // It ends in a space, as the game's own prompt does, so your echo
    // never touches it.
    assert!(DEFAULT_DESIGN.ends_with("%{end} "), "{DEFAULT_DESIGN:?}");
    for (i, token) in template.tokens().iter().enumerate() {
        let text = template.token_text(i);
        match &token.kind {
            TokenKind::Unknown => panic!("{text} is no code Vosh knows"),
            TokenKind::Raw => panic!("{text} draws the game's own prompt"),
            // Text never holds a `%`, so no code is left to print as
            // written.
            TokenKind::Text(t) => assert!(!t.contains('%'), "{text}"),
            TokenKind::If(field) | TokenKind::IfNot(field) => {
                assert!(vars::entry(&field.name).is_some(), "{text}");
            }
            TokenKind::Code(
                Code::Fg(ColorSpec::ByValue { field, game })
                | Code::Bg(ColorSpec::ByValue { field, game }),
            ) => {
                let entry = vars::entry(&field.name).unwrap_or_else(|| panic!("{text}"));
                assert!(entry.kind.formats().contains(&FormatId::Pct), "{text}");
                assert!(!game, "{text} colors by how full, not by the game's bands");
            }
            TokenKind::Value(value) => {
                let entry = vars::entry(&value.field.name)
                    .unwrap_or_else(|| panic!("{text} reads no field Vosh knows"));
                let id = match value.format {
                    Format::Value => FormatId::Value,
                    Format::Pct => FormatId::Pct,
                    Format::Bar { .. } => FormatId::Bar,
                    Format::Grouped => FormatId::Grouped,
                    Format::Names => FormatId::Names,
                    ref other => panic!("{text} uses {other:?}"),
                };
                // A max spelling reads as part of its gauge.
                assert!(
                    entry.kind.formats().contains(&id) || (id == FormatId::Value && !entry.listed),
                    "{} does not offer {id:?} in {text}",
                    entry.name
                );
            }
            TokenKind::Percent | TokenKind::Nl | TokenKind::End | TokenKind::Code(_) => {}
        }
    }
    // Each condition closes, and the fight row is the only line break.
    let count = |f: fn(&TokenKind) -> bool| template.tokens().iter().filter(|t| f(&t.kind)).count();
    assert_eq!(
        count(|k| matches!(k, TokenKind::If(_) | TokenKind::IfNot(_))),
        count(|k| matches!(k, TokenKind::End))
    );
    assert_eq!(count(|k| matches!(k, TokenKind::Nl)), 1);
}

#[test]
fn every_state_draws_its_text_with_no_code_left_raw() {
    for (st, want) in states() {
        let drawn = draw(&st);
        assert_eq!(drawn.plain, want, "{}", st.name);
        let text = strip_sgr(&drawn.ansi).unwrap_or_else(|e| panic!("{}: {e}", st.name));
        assert_eq!(text, drawn.plain.replace('\n', "\r\n"), "{}", st.name);
        assert!(drawn.ansi.ends_with("\x1b[0m"), "{}", st.name);
        let mut rest = drawn.plain.as_str();
        while let Some(at) = rest.find('%') {
            // A percent sign only ever follows the opponent's health.
            let before = rest[..at].chars().last();
            assert!(
                before.is_some_and(|c| c.is_ascii_digit() || c == '?'),
                "{}: {:?}",
                st.name,
                drawn.plain
            );
            rest = &rest[at + 1..];
        }
        assert!(!drawn.plain.contains(['{', '}']), "{}", st.name);
        assert_eq!(drawn.rows, if st.fighting { 2 } else { 1 }, "{}", st.name);
        // The game's tank line folds into the fight row, so the band
        // that holds your prompt keeps two rows in every state.
        assert_eq!(drawn.zone, 2, "{}", st.name);
        assert_eq!(drawn.replaced, drawn.lines, "{}", st.name);
        // The game's away prompt shows as the game sends it.
        assert_eq!(drawn.afk, st.afk, "{}", st.name);
    }
}

#[test]
fn health_and_missing_affects_carry_the_only_color() {
    let (st, _) = states()
        .into_iter()
        .find(|(st, _)| st.name == "desperate")
        .expect("the desperate state");
    let ansi = draw(&st).ansi;
    // Your three numbers by how full, the gauge, and the quiet gray of
    // every label, max and tag.
    assert!(ansi.contains("\x1b[31m150"), "{ansi:?}");
    assert!(ansi.contains("\x1b[31m90"), "{ansi:?}");
    assert!(ansi.contains("\x1b[33m610"), "{ansi:?}");
    assert!(ansi.contains("\x1b[38;5;245m/1020hp"), "{ansi:?}");
    let missing = states()
        .into_iter()
        .find(|(st, _)| st.name == "missing-1")
        .expect("one affect missing")
        .0;
    let ansi = draw(&missing).ansi;
    assert!(
        ansi.contains("\x1b[38;5;245mmissing \x1b[33msanctuary"),
        "{ansi:?}"
    );
}

#[test]
fn the_fight_row_holds_still_as_the_fight_goes() {
    let mut rows = Vec::new();
    for opp_pct in [100, 95, 60, 22, 9, 0] {
        for hit in [1020, 765, 150, 9] {
            let st = St {
                hit,
                opp_pct,
                ..St::new("fight")
            }
            .fight(("Tester", hit, 1020));
            rows.push(draw(&st).plain);
        }
    }
    let lament = St {
        lament: true,
        ..St::new("lament-fight")
    }
    .fight(("Tester", 765, 1020));
    rows.push(draw(&lament).plain);
    let name = OPPONENT.chars().count();
    for row in &rows {
        let fight: Vec<char> = row.lines().next().expect("a fight row").chars().collect();
        // The name, then a ten cell gauge one cell after it, then the
        // percent one cell after that, in every round.
        let opponent: String = fight[..name].iter().collect();
        assert_eq!(opponent, OPPONENT, "{row}");
        assert_eq!(fight[name], ' ', "{row}");
        assert!(
            fight[name + 1..name + 11]
                .iter()
                .all(|c| matches!(c, '█' | '░' | '·')),
            "{row}"
        );
        assert_eq!(fight[name + 11], ' ', "{row}");
        assert!(
            fight[name + 12].is_ascii_digit() || fight[name + 12] == '?',
            "{row}"
        );
    }
}

#[test]
fn the_vitals_row_moves_only_by_how_many_digits_change() {
    let base = draw(&St::new("full")).plain;
    let mut seen = 0;
    for hit in [1020, 999, 765, 180, 50, 7, 0] {
        for (mana, moves) in [(800, 930), (90, 610), (5, 12)] {
            for gold in [1250, 999_999, 0] {
                let st = St {
                    hit,
                    mana,
                    moves,
                    gold,
                    ..St::new("vitals")
                };
                let row = draw(&st).plain;
                // Every cell but the digits keeps its place.
                assert_eq!(shape(&row), shape(&base), "{row}");
                assert_eq!(
                    row.chars().count() - number_cells(&row),
                    base.chars().count() - number_cells(&base),
                    "{row}"
                );
                seen += 1;
            }
        }
    }
    assert_eq!(seen, 63);
}

#[test]
fn a_prompt_with_no_max_draws_each_value_alone() {
    // A PROMPT with no max codes, on a build that sends no Char.Vitals,
    // so Vosh reads your health, mana and moves but no max.
    let st = St {
        build: Build::Older,
        ..St::new("no-max")
    };
    let no_vitals = |package: &str| package != "Char.Vitals";
    let drawn = draw_with(DEFAULT_DESIGN, "<%hhp %mm %vmv> ", &st, no_vitals);
    assert_eq!(drawn.plain, "1020hp 800mn 930mv  1,250g ");
    assert_clean("no-max", &drawn);
    // With no share to color by, the numbers keep the terminal's color
    // and the labels their gray.
    assert!(
        drawn.ansi.starts_with("1020\x1b[38;5;245mhp\x1b[39m 800"),
        "{:?}",
        drawn.ansi
    );
    // Health alone.
    let drawn = draw_with(DEFAULT_DESIGN, "%hhp> ", &st, no_vitals);
    assert_eq!(drawn.plain, "1020hp  1,250g ");
    assert_clean("health alone", &drawn);
}

#[test]
fn on_another_game_each_vital_draws_with_what_it_has() {
    // A max of 0 there means the pair does not apply, as for a class
    // with no mana, so neither the value nor its slash shows.
    let vitals = json!({"hp": 500, "maxhp": 500, "mana": 0, "maxmana": 0,
                        "move": 100, "maxmove": 100});
    let drawn = draw_elsewhere(
        r"^<(?<hp>\d+)hp (?<move>\d+)mv> $",
        "<480hp 100mv> ",
        &[("Char.Vitals", vitals)],
    );
    assert_eq!(drawn.plain, "480/500hp 100/100mv ");
    assert_clean("no mana", &drawn);
    assert!(drawn.ansi.starts_with("\x1b[32m480"), "{:?}", drawn.ansi);

    // A pattern that reads no max, on a game that sends no Char.Vitals.
    let drawn = draw_elsewhere(
        r"^<(?<hp>\d+)hp (?<mana>\d+)m (?<move>\d+)mv> $",
        "<1020hp 800m 930mv> ",
        &[],
    );
    assert_eq!(drawn.plain, "1020hp 800mn 930mv ");
    assert_clean("no max", &drawn);
    // Health alone.
    let drawn = draw_elsewhere(r"^<(?<hp>\d+)hp> $", "<1020hp> ", &[]);
    assert_eq!(drawn.plain, "1020hp ");
    assert_clean("health alone", &drawn);
}
