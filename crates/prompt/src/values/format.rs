//! The values a template draws and the plain text of each format.
//!
//! A resolver hands the renderer a [`Resolved`] per field. The renderer
//! draws the colored forms itself (bars, the game's tank bar, hidden marks)
//! and asks `Value::text` for everything else.

// Game numbers and bar widths sit far below 2^52, so the float math for
// shares and rounded percents is exact.
#![allow(clippy::cast_precision_loss)]

use chrono::{Datelike, NaiveDateTime, Timelike};

use crate::aabahran::codes::{Position, PHASES};
use crate::design::Format;

/// What a resolver knows about a field right now.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// Vosh has a current value.
    Value(Value),
    /// The game hides it right now. Drawn as a dim `?`.
    Hidden,
    /// It does not apply right now, such as no tank out of a fight. A false
    /// flag and a zero count are Absent. Drawn as nothing.
    Absent,
    /// Vosh has no source for it yet. Drawn as nothing.
    Missing,
    /// The name is in no catalog and no script set it. The token prints as
    /// written so a typo stays visible.
    Unknown,
}

/// The phase as a word, for the `word` format.
pub(crate) const MOON_WORDS: [&str; 8] = [
    "new",
    "waxing crescent",
    "first quarter",
    "waxing gibbous",
    "full",
    "waning gibbous",
    "last quarter",
    "waning crescent",
];

/// A value a field holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A whole number, such as gold, level or a room number.
    Num(i64),
    /// A value with a max, such as health. `pct` is the game's own percent
    /// (`%K`), used when no max is known.
    Gauge {
        cur: i64,
        max: Option<i64>,
        pct: Option<i64>,
    },
    /// A number with a decimal point, as some games and scripts write
    /// health (`1.5`), with its max when one is known. It prints as
    /// written and draws a percent, a bar and a color by how full as a
    /// gauge does.
    Decimal {
        text: String,
        value: f64,
        max: Option<f64>,
    },
    /// A percent, such as the opponent's health or a member's mana.
    Pct(i64),
    /// The tank's health percent. Its game format is the `%P` bar.
    TankHp(i64),
    /// Text, such as a name, a word or a script value.
    Text(String),
    /// A flag that holds. A false flag is [`Resolved::Absent`].
    Flag,
    /// Names with their count, such as tracked affects missing. An empty
    /// list is [`Resolved::Absent`].
    List(Vec<String>),
    /// A moon. `phase` runs 0 (new) to 7, `name` is the packet's phase name.
    Moon {
        phase: u8,
        active: bool,
        name: Option<String>,
    },
    Position(Position),
    /// A language as Char.State spells it.
    Lang(String),
    /// Exits. `letters` is what the value format prints (`N E (S) W`),
    /// `game` the game's own text (`[Exits: N E (S) W]`).
    Exits {
        letters: String,
        game: String,
    },
    /// An immortal level with the game's word for it, `Wizi` or `Incog`.
    Level {
        word: String,
        level: i64,
    },
    /// An affect slot as `%f` prints it, ticks, `~` or `-`.
    Slot(String),
    /// The game hour, 0 to 23.
    Hour(u8),
    /// A temperature, with its unit when the game names one.
    Temp {
        degrees: i64,
        unit: Option<char>,
    },
    /// Seconds with the interval as the max, the tick. `since` is the
    /// whole seconds since it last turned, which keep counting past the
    /// interval when a tick comes late, None when nothing says.
    Seconds {
        secs: i64,
        max: Option<i64>,
        since: Option<i64>,
    },
    /// The local clock. `date` marks the date field, which prints a date.
    Clock {
        at: NaiveDateTime,
        date: bool,
    },
    /// Ticks left on an affect, -1 for permanent.
    Ticks(i64),
    /// A group member and their health percent.
    Member {
        name: String,
        pct: i64,
    },
    /// Text with the game's colors, the raw prompt.
    Styled(String),
}

/// A percent rounded to the nearest whole number. None when `max` is not
/// above 0.
pub(crate) fn percent_rounded(cur: i64, max: i64) -> Option<i64> {
    (max > 0).then(|| ((cur as f64 / max as f64) * 100.0).round() as i64)
}

/// A percent by integer division, as the server works it out. None when
/// `max` is below 1, where the game prints nothing.
pub(crate) fn percent_game(cur: i64, max: i64) -> Option<i64> {
    (max >= 1).then(|| cur.saturating_mul(100) / max)
}

/// `1,250`.
pub(crate) fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `1.2k`, `812k`, `3.4m`. Tenths are cut, not rounded, so 1250 reads
/// `1.2k` next to its grouped `1,250`.
pub(crate) fn short(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let a = n.unsigned_abs();
    let (unit, suffix) = match a {
        0..=999 => return n.to_string(),
        1_000..=999_999 => (1_000, "k"),
        1_000_000..=999_999_999 => (1_000_000, "m"),
        _ => (1_000_000_000, "b"),
    };
    let whole = a / unit;
    let tenth = a % unit * 10 / unit;
    if whole < 10 && tenth > 0 {
        format!("{sign}{whole}.{tenth}{suffix}")
    } else {
        format!("{sign}{whole}{suffix}")
    }
}

/// `12.3K`, the number over a thousand with one decimal and a capital K,
/// as the old tt++ prompt printed gold. The tenth rounds as printf
/// rounds a double, to the nearest with a tie going to the even digit,
/// so 1350 reads `1.4K` and 1250, a tie, `1.2K`. It stays in thousands
/// past a million, `1234.6K`.
pub(crate) fn thousands(n: i64) -> String {
    format!("{:.1}K", n as f64 / 1000.0)
}

/// The game hour as `2 pm`.
pub(crate) fn hour_word(hour: u8) -> String {
    let h = hour % 24;
    let half = if h < 12 { "am" } else { "pm" };
    let twelve = match h % 12 {
        0 => 12,
        n => n,
    };
    format!("{twelve} {half}")
}

/// The game hour as the old tt++ prompt printed it, `3PM`, with `12AM`
/// for midnight and `12PM` for noon.
pub(crate) fn hour_ampm(hour: u8) -> String {
    let h = hour % 24;
    let half = if h < 12 { "AM" } else { "PM" };
    let twelve = match h % 12 {
        0 => 12,
        n => n,
    };
    format!("{twelve}{half}")
}

/// A language as `%s` prints it, first letter lowercased.
pub(crate) fn lang_game(lang: &str) -> String {
    let mut chars = lang.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A color band the game draws a value in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Band {
    Plain,
    Yellow,
    BoldYellow,
    Red,
    BoldRed,
}

impl Band {
    /// The foreground SGR the band writes. Plain is the terminal's color.
    pub(crate) fn fg(self) -> &'static str {
        match self {
            Band::Plain => "39",
            Band::Yellow => "33",
            Band::BoldYellow => "1;33",
            Band::Red => "31",
            Band::BoldRed => "1;31",
        }
    }

    /// The background SGR the band writes. Bold has no background.
    pub(crate) fn bg(self) -> &'static str {
        match self {
            Band::Plain => "49",
            Band::Yellow | Band::BoldYellow => "43",
            Band::Red | Band::BoldRed => "41",
        }
    }
}

/// The `%h` bands. Plain above 40 percent, bold yellow at 40 and below,
/// red at 20 and below.
pub(crate) fn h_band(pct: i64) -> Band {
    if pct <= 20 {
        Band::Red
    } else if pct <= 40 {
        Band::BoldYellow
    } else {
        Band::Plain
    }
}

/// The `%P` bands (`health_prompt`). Yellow under 75 percent, red under 25,
/// bold red under 5.
pub(crate) fn p_band(pct: i64) -> Band {
    if pct < 5 {
        Band::BoldRed
    } else if pct < 25 {
        Band::Red
    } else if pct < 75 {
        Band::Yellow
    } else {
        Band::Plain
    }
}

/// The ANSI color for how full a share is. Green from two thirds, yellow
/// from one third, red below.
pub(crate) fn how_full(fraction: f64) -> u8 {
    if fraction >= 0.66 {
        2
    } else if fraction >= 0.33 {
        3
    } else {
        1
    }
}

/// The 256 colors the old tt++ prompt drew a percent in, from red at 0
/// to green at 100, one for each tenth: 196 202 208 214 220 226 190 154
/// 118 82 46.
pub(crate) const STEPS: [u8; 11] = [196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46];

/// The step a percent falls in, as the old prompt took it: the percent by
/// integer division, then its tenth, so 39 is the step of 30 and 100 the
/// last. A percent below 0 is red and one past 100 green.
pub(crate) fn step_color(pct: i64) -> u8 {
    STEPS[usize::try_from((pct / 10).clamp(0, 10)).unwrap_or(0)]
}

/// Which of the twelve `%P` cells are full, as `health_prompt` fills them.
pub fn tank_bar_cells(pct: i64) -> [bool; 12] {
    let mut cells = [false; 12];
    for (i, cell) in cells.iter_mut().enumerate() {
        *cell = (i as i64 * 25 / 3) < pct;
    }
    cells
}

impl Value {
    /// A number as a prompt value writes it, whole or with a decimal
    /// point. None when the text is no number.
    pub(crate) fn parse_number(text: &str) -> Option<Value> {
        let t = text.trim();
        if let Ok(n) = t.parse::<i64>() {
            return Some(Value::Num(n));
        }
        let value = t.parse::<f64>().ok().filter(|n| n.is_finite())?;
        Some(Value::Decimal {
            text: t.to_string(),
            value,
            max: None,
        })
    }

    /// The number a whole or decimal value holds.
    pub(crate) fn number(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n as f64),
            Value::Decimal { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// True for a whole or decimal 0, a max that means no such pool.
    pub(crate) fn is_zero(&self) -> bool {
        match self {
            Value::Num(n) => *n == 0,
            Value::Decimal { value, .. } => value.abs() < f64::EPSILON,
            _ => false,
        }
    }

    /// This number over `max`, with the game's own percent when it gave
    /// one. Two whole numbers make a gauge, and a decimal point on either
    /// makes a decimal with its max. None when either is no number.
    pub(crate) fn over(&self, max: &Value, pct: Option<i64>) -> Option<Value> {
        match (self, max) {
            (Value::Num(cur), Value::Num(max)) => Some(Value::Gauge {
                cur: *cur,
                max: Some(*max),
                pct,
            }),
            _ => Some(Value::Decimal {
                text: self.value_text(""),
                value: self.number()?,
                max: Some(max.number()?),
            }),
        }
    }

    /// How full the value is, 0 to 1, for bars and color by how full.
    /// None when nothing gives a share.
    pub(crate) fn fraction(&self) -> Option<f64> {
        let share =
            |cur: i64, max: i64| (max > 0).then(|| (cur as f64 / max as f64).clamp(0.0, 1.0));
        match self {
            Value::Decimal {
                value,
                max: Some(max),
                ..
            } if *max > 0.0 => Some((value / max).clamp(0.0, 1.0)),
            Value::Gauge { cur, max, pct } => match (max, pct) {
                (Some(max), _) => share(*cur, *max),
                (None, Some(pct)) => share(*pct, 100),
                (None, None) => None,
            },
            Value::Pct(pct) | Value::TankHp(pct) | Value::Member { pct, .. } => share(*pct, 100),
            Value::Seconds {
                secs,
                max: Some(max),
                ..
            } => share(*secs, *max),
            _ => None,
        }
    }

    /// True for the values a bar can draw, even when no share is known yet.
    pub(crate) fn has_bar(&self) -> bool {
        matches!(
            self,
            Value::Gauge { .. }
                | Value::Decimal { .. }
                | Value::Pct(_)
                | Value::TankHp(_)
                | Value::Member { .. }
                | Value::Seconds { .. }
        )
    }

    /// The rounded percent the `pct` format prints.
    pub(crate) fn percent(&self) -> Option<i64> {
        match self {
            Value::Decimal {
                value,
                max: Some(max),
                ..
            } if *max > 0.0 => Some((value / max * 100.0).round() as i64),
            Value::Gauge { cur, max, pct } => match (max, pct) {
                (Some(max), _) if *max > 0 => percent_rounded(*cur, *max),
                (_, Some(pct)) => Some(*pct),
                _ => None,
            },
            Value::Pct(pct) | Value::TankHp(pct) | Value::Member { pct, .. } => Some(*pct),
            Value::Seconds {
                secs,
                max: Some(max),
                ..
            } => percent_rounded(*secs, *max),
            _ => None,
        }
    }

    /// The percent by the server's integer division, for the game's bands,
    /// the steps and the `pct:game` format. A decimal cuts its fraction off
    /// the same way.
    pub(crate) fn game_percent(&self) -> Option<i64> {
        match self {
            Value::Decimal {
                value,
                max: Some(max),
                ..
            } if *max > 0.0 => Some((value * 100.0 / max).trunc() as i64),
            Value::Gauge { cur, max, pct } => match (max, pct) {
                (Some(max), _) if *max >= 1 => percent_game(*cur, *max),
                (_, Some(pct)) => Some(*pct),
                _ => None,
            },
            Value::Pct(pct) | Value::TankHp(pct) | Value::Member { pct, .. } => Some(*pct),
            Value::Seconds {
                secs,
                max: Some(max),
                ..
            } => percent_game(*secs, *max),
            _ => None,
        }
    }

    /// The text of the value format.
    fn value_text(&self, label: &str) -> String {
        match self {
            Value::Num(n) | Value::Level { level: n, .. } | Value::Temp { degrees: n, .. } => {
                n.to_string()
            }
            Value::Gauge { cur, .. } => cur.to_string(),
            Value::Pct(pct) | Value::TankHp(pct) => pct.to_string(),
            Value::Text(s)
            | Value::Lang(s)
            | Value::Slot(s)
            | Value::Styled(s)
            | Value::Decimal { text: s, .. } => s.clone(),
            Value::Flag => label.to_string(),
            Value::List(names) => names.len().to_string(),
            Value::Moon { phase, active, .. } => moon_code(*phase, *active).to_string(),
            Value::Position(p) => p.short().to_string(),
            Value::Exits { letters, .. } => letters.clone(),
            Value::Hour(h) => h.to_string(),
            Value::Seconds { secs, .. } => secs.to_string(),
            Value::Clock { at, date: false } => at.format("%H:%M:%S").to_string(),
            Value::Clock { at, date: true } => at.format("%Y-%m-%d").to_string(),
            Value::Ticks(-1) => "perm".to_string(),
            Value::Ticks(n) => n.to_string(),
            Value::Member { name, pct } => format!("{name} {pct}%"),
        }
    }

    /// The plain text of a format, or None when the format does not apply
    /// to this value. The renderer draws bars, the tank's game bar, an
    /// immortal level's game look and styled text itself.
    pub(crate) fn text(&self, format: &Format, label: &str) -> Option<String> {
        match format {
            Format::Value => Some(self.value_text(label)),
            Format::Max => match self {
                Value::Gauge { max, .. } | Value::Seconds { max, .. } => {
                    Some(max.map(|m| m.to_string()).unwrap_or_default())
                }
                Value::Decimal { max, .. } => Some(max.map(|m| m.to_string()).unwrap_or_default()),
                _ => None,
            },
            Format::Pct => self.percent().map(|p| p.to_string()),
            Format::PctGame => self.game_percent().map(|p| p.to_string()),
            Format::Game => Some(match self {
                Value::Moon { phase, active, .. } => moon_code(*phase, *active).to_string(),
                Value::Exits { game, .. } => game.clone(),
                Value::Lang(lang) => lang_game(lang),
                Value::Level { word, level } => format!("({word} {level})"),
                Value::Position(p) => p.abbrev().to_string(),
                _ => self.value_text(label),
            }),
            Format::Word => match self {
                Value::Position(p) => Some(p.word().to_string()),
                Value::Moon { phase, active, .. } => Some(if *active {
                    MOON_WORDS
                        .get(usize::from(*phase))
                        .copied()
                        .unwrap_or_default()
                        .to_string()
                } else {
                    String::new()
                }),
                Value::Hour(h) => Some(hour_word(*h)),
                Value::Text(s) => Some(s.clone()),
                _ => None,
            },
            Format::Ampm => match self {
                Value::Hour(h) => Some(hour_ampm(*h)),
                _ => None,
            },
            Format::Name => match self {
                Value::Moon { name, active, .. } => Some(if *active {
                    name.clone().unwrap_or_default()
                } else {
                    String::new()
                }),
                Value::Member { name, .. } | Value::Text(name) => Some(name.clone()),
                _ => None,
            },
            Format::Grouped => self.whole().map(grouped),
            Format::Short => self.whole().map(short),
            Format::Thousands => self.whole().map(thousands),
            Format::Unit => match self {
                Value::Seconds { secs, .. } => Some(format!("{secs}s")),
                Value::Temp {
                    degrees,
                    unit: Some(unit),
                } => Some(format!("{degrees}°{unit}")),
                Value::Temp {
                    degrees,
                    unit: None,
                } => Some(format!("{degrees}°")),
                _ => None,
            },
            // Nothing when Vosh does not know when the tick last turned,
            // as a max it does not know draws nothing.
            Format::Since => match self {
                Value::Seconds { since, .. } => {
                    Some(since.map(|s| format!("{s}s")).unwrap_or_default())
                }
                _ => None,
            },
            Format::Trunc(chars) => match self {
                Value::Styled(_) => None,
                _ => Some(self.value_text(label).chars().take(*chars).collect()),
            },
            Format::Hm | Format::Hms | Format::Md => match self {
                Value::Clock { at, .. } => Some(clock(at, format)),
                _ => None,
            },
            Format::Count => match self {
                Value::List(names) => Some(names.len().to_string()),
                _ => None,
            },
            Format::Names => match self {
                Value::List(names) => Some(names.join(", ")),
                _ => None,
            },
            Format::On => Some(label.to_string()),
            Format::Off => Some(String::new()),
            Format::Bar { .. } => None,
        }
    }

    /// The whole number the grouped and short formats print.
    fn whole(&self) -> Option<i64> {
        match self {
            Value::Num(n) | Value::Gauge { cur: n, .. } => Some(*n),
            _ => None,
        }
    }
}

/// What `%j` prints for a moon, `-` when it is not up.
pub(crate) fn moon_code(phase: u8, active: bool) -> &'static str {
    if !active {
        return "-";
    }
    PHASES.get(usize::from(phase)).copied().unwrap_or("-")
}

fn clock(at: &NaiveDateTime, format: &Format) -> String {
    match format {
        Format::Hm => format!("{:02}:{:02}", at.hour(), at.minute()),
        Format::Hms => format!("{:02}:{:02}:{:02}", at.hour(), at.minute(), at.second()),
        _ => {
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("{} {}", MONTHS[at.month0() as usize], at.day())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::now;

    fn text(value: &Value, format: Format) -> Option<String> {
        value.text(&format, "Label")
    }

    #[test]
    fn numbers_group_and_shorten() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1250), "1,250");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(-1250), "-1,250");
        assert_eq!(short(999), "999");
        assert_eq!(short(1000), "1k");
        assert_eq!(short(1250), "1.2k");
        assert_eq!(short(1299), "1.2k");
        assert_eq!(short(12_500), "12k");
        assert_eq!(short(812_345), "812k");
        assert_eq!(short(1_200_000), "1.2m");
        assert_eq!(short(3_400_000_000), "3.4b");
        assert_eq!(short(-1250), "-1.2k");
    }

    #[test]
    fn thousands_print_as_the_old_tintin_prompt_did() {
        // Each pair is what tt++ 2.02.61 printed for @gold{n} in the old
        // prompt, with the K it wrote after it.
        for (n, old) in [
            (0, "0.0"),
            (49, "0.0"),
            (50, "0.1"),
            (99, "0.1"),
            (500, "0.5"),
            (950, "0.9"),
            (999, "1.0"),
            (1050, "1.1"),
            (1150, "1.1"),
            (1250, "1.2"),
            (1350, "1.4"),
            (1450, "1.4"),
            (1999, "2.0"),
            (2650, "2.6"),
            (12_345, "12.3"),
            (12_399, "12.4"),
            (99_999, "100.0"),
            (812_345, "812.3"),
            (1_234_567, "1234.6"),
        ] {
            assert_eq!(thousands(n), format!("{old}K"), "{n}");
        }
        assert_eq!(thousands(-1250), "-1.2K");
        let gold = Value::Num(12_345);
        assert_eq!(text(&gold, Format::Thousands).as_deref(), Some("12.3K"));
        let gauge = Value::Gauge {
            cur: 500,
            max: Some(1000),
            pct: None,
        };
        assert_eq!(text(&gauge, Format::Thousands).as_deref(), Some("0.5K"));
        assert_eq!(text(&Value::Text("lots".into()), Format::Thousands), None);
    }

    #[test]
    fn the_hour_reads_as_the_old_tintin_prompt_did() {
        // What tt++ 2.02.61 printed for @time{n} in the old prompt.
        for (hour, old) in [
            (0, "12AM"),
            (1, "1AM"),
            (3, "3AM"),
            (9, "9AM"),
            (11, "11AM"),
            (12, "12PM"),
            (13, "1PM"),
            (15, "3PM"),
            (23, "11PM"),
        ] {
            assert_eq!(hour_ampm(hour), old, "{hour}");
            let value = Value::Hour(hour);
            assert_eq!(text(&value, Format::Ampm).as_deref(), Some(old));
        }
        assert_eq!(text(&Value::Num(15), Format::Ampm), None);
    }

    #[test]
    fn percents_round_or_divide_as_the_server_does() {
        assert_eq!(percent_rounded(300, 1020), Some(29));
        assert_eq!(percent_rounded(1, 3), Some(33));
        assert_eq!(percent_rounded(2, 3), Some(67));
        assert_eq!(percent_rounded(5, 0), None);
        assert_eq!(percent_game(2, 3), Some(66));
        assert_eq!(percent_game(408, 1020), Some(40));
        assert_eq!(percent_game(5, 0), None);
    }

    #[test]
    fn game_bands_follow_the_server() {
        assert_eq!(h_band(41), Band::Plain);
        assert_eq!(h_band(40), Band::BoldYellow);
        assert_eq!(h_band(21), Band::BoldYellow);
        assert_eq!(h_band(20), Band::Red);
        assert_eq!(p_band(75), Band::Plain);
        assert_eq!(p_band(74), Band::Yellow);
        assert_eq!(p_band(24), Band::Red);
        assert_eq!(p_band(4), Band::BoldRed);
        assert_eq!(how_full(1.0), 2);
        assert_eq!(how_full(0.66), 2);
        assert_eq!(how_full(0.5), 3);
        assert_eq!(how_full(0.33), 3);
        assert_eq!(how_full(0.2), 1);
    }

    #[test]
    fn steps_color_each_tenth_as_the_old_prompt_did() {
        // What tt++ 2.02.61 drew the percent sign in for @percent.
        for (pct, color) in [
            (0, 196),
            (9, 196),
            (10, 202),
            (20, 208),
            (37, 214),
            (40, 220),
            (59, 226),
            (65, 190),
            (79, 154),
            (80, 118),
            (90, 82),
            (99, 82),
            (100, 46),
        ] {
            assert_eq!(step_color(pct), color, "{pct}");
        }
        // Past the ends it stays at the ends.
        assert_eq!(step_color(-15), 196);
        assert_eq!(step_color(110), 46);
    }

    #[test]
    fn tank_bar_cells_fill_as_health_prompt_does() {
        let full = |pct| tank_bar_cells(pct).iter().filter(|c| **c).count();
        assert_eq!(full(100), 12);
        assert_eq!(full(0), 0);
        assert_eq!(full(1), 1);
        // 17 to 24 percent fills three cells, the logged `[===|---|---|---]`.
        assert_eq!(full(17), 3);
        assert_eq!(full(24), 3);
        assert_eq!(full(25), 3);
        assert_eq!(full(26), 4);
        assert_eq!(full(60), 8);
    }

    #[test]
    fn positions_map_between_words_and_abbreviations() {
        assert_eq!(Position::from_word("fighting"), Some(Position::Fighting));
        assert_eq!(Position::from_abbrev("fgt"), Some(Position::Fighting));
        assert_eq!(Position::from_abbrev(""), Some(Position::Meditate));
        assert_eq!(
            Position::from_word("meditate").map(Position::abbrev),
            Some("")
        );
        assert_eq!(Position::from_word("hovering"), None);
        for p in Position::ALL {
            assert_eq!(Position::from_word(p.word()), Some(p));
            assert_eq!(Position::from_abbrev(p.abbrev()), Some(p));
        }
    }

    #[test]
    fn vitals_print_value_max_and_percent() {
        let hp = Value::Gauge {
            cur: 300,
            max: Some(1020),
            pct: None,
        };
        assert_eq!(text(&hp, Format::Value).as_deref(), Some("300"));
        assert_eq!(text(&hp, Format::Max).as_deref(), Some("1020"));
        assert_eq!(text(&hp, Format::Pct).as_deref(), Some("29"));
        assert_eq!(text(&hp, Format::Grouped).as_deref(), Some("300"));
        assert_eq!(hp.game_percent(), Some(29));
        // %K alone feeds the percent when no max is known.
        let pct_only = Value::Gauge {
            cur: 300,
            max: None,
            pct: Some(29),
        };
        assert_eq!(text(&pct_only, Format::Pct).as_deref(), Some("29"));
        assert_eq!(text(&pct_only, Format::Max).as_deref(), Some(""));
        assert_eq!(pct_only.fraction(), Some(0.29));
        // A max of 0 gives no percent, so the format does not apply.
        let zero = Value::Gauge {
            cur: 0,
            max: Some(0),
            pct: None,
        };
        assert_eq!(text(&zero, Format::Pct), None);
        assert_eq!(zero.fraction(), None);
        assert!(zero.has_bar());
    }

    #[test]
    fn the_game_percent_cuts_as_the_old_tintin_prompt_did() {
        // Each row is what tt++ 2.02.61 printed for @percent{cur;max} in
        // the old prompt, next to the rounded percent.
        for (cur, max, old, rounded) in [
            (300, 800, 37, 38),
            (610, 930, 65, 66),
            (1019, 1020, 99, 100),
            (408, 1020, 40, 40),
            (918, 1020, 90, 90),
            (930, 930, 100, 100),
            (204, 1020, 20, 20),
            (0, 800, 0, 0),
        ] {
            let gauge = Value::Gauge {
                cur,
                max: Some(max),
                pct: None,
            };
            let game = text(&gauge, Format::PctGame);
            assert_eq!(game, Some(old.to_string()), "{cur} of {max}");
            assert_eq!(game, gauge.game_percent().map(|p| p.to_string()));
            assert_eq!(text(&gauge, Format::Pct), Some(rounded.to_string()));
        }
        // %K alone feeds it when no max is known, as it feeds pct.
        let pct_only = Value::Gauge {
            cur: 300,
            max: None,
            pct: Some(37),
        };
        assert_eq!(text(&pct_only, Format::PctGame).as_deref(), Some("37"));
        let zero = Value::Gauge {
            cur: 0,
            max: Some(0),
            pct: None,
        };
        assert_eq!(text(&zero, Format::PctGame), None);
        // A decimal cuts its fraction off the same way.
        let half = Value::Decimal {
            text: "1.5".to_string(),
            value: 1.5,
            max: Some(4.0),
        };
        assert_eq!(text(&half, Format::PctGame).as_deref(), Some("37"));
        assert_eq!(text(&half, Format::Pct).as_deref(), Some("38"));
        let tick = Value::Seconds {
            secs: 14,
            max: Some(60),
            since: Some(46),
        };
        assert_eq!(text(&tick, Format::PctGame).as_deref(), Some("23"));
        assert_eq!(text(&Value::Num(5), Format::PctGame), None);
        assert_eq!(text(&Value::Text("full".into()), Format::PctGame), None);
    }

    #[test]
    fn numbers_read_whole_or_with_a_decimal_point() {
        assert_eq!(Value::parse_number(" 42 "), Some(Value::Num(42)));
        let half = Value::parse_number("1.5").expect("a number");
        assert_eq!(
            half,
            Value::Decimal {
                text: "1.5".to_string(),
                value: 1.5,
                max: None,
            }
        );
        assert_eq!(Value::parse_number("inf"), None);
        assert_eq!(Value::parse_number("full"), None);
        assert!(Value::Num(0).is_zero());
        assert!(Value::parse_number("0.0").is_some_and(|v| v.is_zero()));
        assert!(!half.is_zero());
        // Two whole numbers make a gauge, and a decimal point on either
        // makes a decimal with its max.
        assert_eq!(
            Value::Num(300).over(&Value::Num(1020), Some(29)),
            Some(Value::Gauge {
                cur: 300,
                max: Some(1020),
                pct: Some(29),
            })
        );
        let gauge = half.over(&Value::Num(3), None).expect("a gauge");
        assert_eq!(text(&gauge, Format::Value).as_deref(), Some("1.5"));
        assert_eq!(text(&gauge, Format::Max).as_deref(), Some("3"));
        assert_eq!(text(&gauge, Format::Pct).as_deref(), Some("50"));
        assert_eq!(text(&gauge, Format::Grouped), None);
        assert_eq!(gauge.fraction(), Some(0.5));
        assert_eq!(gauge.game_percent(), Some(50));
        assert!(gauge.has_bar());
        assert_eq!(Value::Text("full".into()).over(&Value::Num(3), None), None);
    }

    #[test]
    fn game_formats_print_as_the_game_does() {
        let moon = Value::Moon {
            phase: 4,
            active: true,
            name: Some("full and whole".to_string()),
        };
        assert_eq!(text(&moon, Format::Value).as_deref(), Some("FUL"));
        assert_eq!(text(&moon, Format::Game).as_deref(), Some("FUL"));
        assert_eq!(text(&moon, Format::Word).as_deref(), Some("full"));
        assert_eq!(text(&moon, Format::Name).as_deref(), Some("full and whole"));
        let down = Value::Moon {
            phase: 2,
            active: false,
            name: Some("half-lit and growing".to_string()),
        };
        assert_eq!(text(&down, Format::Game).as_deref(), Some("-"));
        assert_eq!(text(&down, Format::Word).as_deref(), Some(""));
        assert_eq!(text(&down, Format::Name).as_deref(), Some(""));

        let pos = Value::Position(Position::Fighting);
        assert_eq!(text(&pos, Format::Value).as_deref(), Some("fgt"));
        assert_eq!(text(&pos, Format::Game).as_deref(), Some("fgt"));
        assert_eq!(text(&pos, Format::Word).as_deref(), Some("fighting"));
        let meditate = Value::Position(Position::Meditate);
        // The game's `%S` prints nothing while you meditate, and Vosh's
        // own three letters say so.
        assert_eq!(text(&meditate, Format::Value).as_deref(), Some("med"));
        assert_eq!(text(&meditate, Format::Game).as_deref(), Some(""));
        assert_eq!(text(&meditate, Format::Word).as_deref(), Some("meditate"));

        let lang = Value::Lang("Thsu'ul".to_string());
        assert_eq!(text(&lang, Format::Value).as_deref(), Some("Thsu'ul"));
        assert_eq!(text(&lang, Format::Game).as_deref(), Some("thsu'ul"));

        let exits = Value::Exits {
            letters: "N E (S) W".to_string(),
            game: "[Exits: N E (S) W]".to_string(),
        };
        assert_eq!(text(&exits, Format::Value).as_deref(), Some("N E (S) W"));
        assert_eq!(
            text(&exits, Format::Game).as_deref(),
            Some("[Exits: N E (S) W]")
        );

        let wizi = Value::Level {
            word: "Wizi".to_string(),
            level: 60,
        };
        assert_eq!(text(&wizi, Format::Value).as_deref(), Some("60"));
        assert_eq!(text(&wizi, Format::Game).as_deref(), Some("(Wizi 60)"));

        let slot = Value::Slot("~".to_string());
        assert_eq!(text(&slot, Format::Game).as_deref(), Some("~"));
        // A field with no look of its own prints its value in game format.
        assert_eq!(
            text(&Value::Num(1250), Format::Game).as_deref(),
            Some("1250")
        );
    }

    #[test]
    fn words_units_and_clocks() {
        assert_eq!(hour_word(0), "12 am");
        assert_eq!(hour_word(11), "11 am");
        assert_eq!(hour_word(12), "12 pm");
        assert_eq!(hour_word(14), "2 pm");
        assert_eq!(
            text(&Value::Hour(14), Format::Word).as_deref(),
            Some("2 pm")
        );
        let tick = Value::Seconds {
            secs: 14,
            max: Some(60),
            since: Some(46),
        };
        assert_eq!(text(&tick, Format::Unit).as_deref(), Some("14s"));
        assert_eq!(text(&tick, Format::Pct).as_deref(), Some("23"));
        // The old TinTin prompt counted up from the tick, and so does
        // since, past the interval when the tick comes late.
        assert_eq!(text(&tick, Format::Since).as_deref(), Some("46s"));
        let late = Value::Seconds {
            secs: 0,
            max: Some(30),
            since: Some(33),
        };
        assert_eq!(text(&late, Format::Since).as_deref(), Some("33s"));
        let unknown = Value::Seconds {
            secs: 14,
            max: None,
            since: None,
        };
        assert_eq!(text(&unknown, Format::Since).as_deref(), Some(""));
        assert_eq!(text(&Value::Num(5), Format::Since), None);
        let temp = |unit| Value::Temp { degrees: 61, unit };
        assert_eq!(
            text(&temp(Some('F')), Format::Unit).as_deref(),
            Some("61°F")
        );
        assert_eq!(text(&temp(None), Format::Unit).as_deref(), Some("61°"));
        let time = Value::Clock {
            at: now(),
            date: false,
        };
        let date = Value::Clock {
            at: now(),
            date: true,
        };
        assert_eq!(text(&time, Format::Value).as_deref(), Some("08:42:10"));
        assert_eq!(text(&time, Format::Hm).as_deref(), Some("08:42"));
        assert_eq!(text(&time, Format::Hms).as_deref(), Some("08:42:10"));
        assert_eq!(text(&date, Format::Value).as_deref(), Some("2026-09-29"));
        assert_eq!(text(&date, Format::Md).as_deref(), Some("Sep 29"));
    }

    #[test]
    fn lists_flags_and_affects() {
        let missing = Value::List(vec!["sanctuary".to_string(), "haste".to_string()]);
        assert_eq!(text(&missing, Format::Value).as_deref(), Some("2"));
        assert_eq!(text(&missing, Format::Count).as_deref(), Some("2"));
        assert_eq!(
            text(&missing, Format::Names).as_deref(),
            Some("sanctuary, haste")
        );
        assert_eq!(text(&Value::Flag, Format::Value).as_deref(), Some("Label"));
        assert_eq!(text(&Value::Flag, Format::On).as_deref(), Some("Label"));
        assert_eq!(text(&Value::Flag, Format::Off).as_deref(), Some(""));
        assert_eq!(
            text(&Value::Ticks(12), Format::Value).as_deref(),
            Some("12")
        );
        assert_eq!(
            text(&Value::Ticks(-1), Format::Value).as_deref(),
            Some("perm")
        );
        let low = Value::Member {
            name: "Iskra".to_string(),
            pct: 45,
        };
        assert_eq!(text(&low, Format::Value).as_deref(), Some("Iskra 45%"));
        assert_eq!(text(&low, Format::Name).as_deref(), Some("Iskra"));
        assert_eq!(text(&low, Format::Pct).as_deref(), Some("45"));
    }

    #[test]
    fn formats_that_do_not_apply_give_none() {
        let name = Value::Text("the Bank of Aabahran".to_string());
        assert_eq!(text(&name, Format::Trunc(8)).as_deref(), Some("the Bank"));
        assert_eq!(text(&name, Format::Pct), None);
        assert_eq!(text(&name, Format::Max), None);
        assert_eq!(text(&name, Format::Grouped), None);
        assert_eq!(text(&name, Format::Unit), None);
        assert_eq!(text(&name, Format::Hm), None);
        assert_eq!(text(&Value::Num(5), Format::Pct), None);
        assert_eq!(text(&Value::Num(5), Format::Names), None);
    }
}
