//! The game's side of a prompt, as the server writes it at 54f14ef6.
//!
//! [`prompt`] follows `bust_a_prompt` (`comm.c:1709-1954`), and
//! [`send_to_char`] follows `send_to_char`, `process_color` and
//! `process_color_256`, so a PROMPT and a [`State`] give the bytes the
//! game sends. `fixtures/prompt/aabahran/lines.json` holds it to the
//! prompt lines the design notes quote.

use crate::aabahran::colors;
use serde::Deserialize;

/// Someone in your group your opponent fights, you included.
#[derive(Debug, Clone, Deserialize)]
pub struct Tank {
    pub name: String,
    pub hit: i64,
    pub max_hit: i64,
}

/// What the game holds for you when it prints the prompt.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct State {
    pub hit: i64,
    pub max_hit: i64,
    pub mana: i64,
    pub max_mana: i64,
    pub moves: i64,
    pub max_move: i64,
    pub lament: bool,
    pub fighting: bool,
    pub tank: Option<Tank>,
    pub invis: i64,
    pub incog: i64,
    pub afk: bool,
    pub immortal: bool,
    pub npc: bool,
    pub color_256: bool,
    pub cp: i64,
    pub rp: i64,
    pub gold: i64,
    pub exp: i64,
    pub tnl: i64,
    pub hour: i64,
    pub temp: i64,
    pub sky: String,
    pub region: String,
    /// As `lang_table` stores it.
    pub lang: String,
    /// The game's position number, 4 for meditate.
    pub position: usize,
    pub stallion: bool,
    /// What `do_promptexit` writes.
    pub exits: String,
    /// What `append_custom_aff_timer` writes for slots 1 to 10.
    pub slots: Vec<String>,
    /// Each moon's phase, None when it is not up.
    pub moons: Vec<Option<usize>>,
    pub room: String,
    pub room_vnum: i64,
    pub area: String,
    pub area_vnum: i64,
    pub olc: String,
    pub olc_vnum: String,
    pub pacified: bool,
    /// Your `prefix` setting.
    pub prefix: String,
    /// The fallback prompt of an empty PROMPT prints zeros under
    /// lamented tears, as the new build does. The older builds print the
    /// true values there.
    pub fallback_hides: bool,
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
            fallback_hides: true,
        }
    }
}

/// What `%S` prints for each position, from dead to standing.
pub const POS_ABBREV: [&str; 10] = [
    "dea", "mor", "inc", "stn", "", "slp", "rst", "sit", "fgt", "std",
];
/// What `%j` prints for each moon phase, from new to waning.
pub const PHASES: [&str; 8] = ["new", "wax", "Hwx", "Gwx", "FUL", "Gwn", "Hwn", "wan"];

/// `bust_a_prompt`, `comm.c:1709-1954`, from the fight prompt choice
/// on.
pub fn prompt(prompt: &str, fprompt: &str, st: &State) -> Vec<u8> {
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
        let shown = |v: i64| if st.fallback_hides { lam(v) } else { v };
        let text = format!(
            "<{}hp {}m {}mv>{} ",
            shown(st.hit),
            shown(st.mana),
            shown(st.moves),
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
pub fn health_prompt(cur: i64, max: i64, digital: bool) -> Option<String> {
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
pub fn send_to_char(out: &mut Vec<u8>, txt: &str, st: &State) {
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
