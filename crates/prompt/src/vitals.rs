//! What the session pushes to a footer or the status line that draws
//! your vitals text: the text at
//! the live values, the same text at full values, which a narrow panel
//! lays out from so a fight never moves a line, and for each row
//! whether it reads a fight, which keeps the row under Hide vitals while
//! your prompt is pinned.

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use serde::Serialize;
use serde_json::Value as Json;

use crate::design::{FieldRef, PieceKind, Template};
use crate::engine::Clock;
use crate::render::{render, render_reading, RenderOptions, Rendered};
use crate::values::format::{Resolved, Value};
use crate::values::overrides::{Overridden, Overrides};
use crate::values::{entry, Pair, Values};

/// The fields that name a fight. A row that reads one stays under Hide
/// vitals while your prompt is pinned.
pub const FIGHT_FIELDS: [&str; 6] = [
    "fight",
    "opponent",
    "opponent_hp",
    "opponent_cond",
    "tank",
    "tank_hp",
];

/// A vitals text drawn for a footer or the status line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VitalsText {
    /// The text at the live values.
    pub live: Rendered,
    /// The same text with each vital at its max and your opponent at
    /// 100, through the same conditions, so its rows match the live ones.
    pub full: Rendered,
    /// For each live row, whether it reads one of [`FIGHT_FIELDS`].
    pub fight: Vec<bool>,
    /// The pieces that are a `%{right}`, so the footer finds in the spans
    /// where a row pushes and keeps what follows whole.
    pub right: Vec<usize>,
}

/// Draw `template` with `live` for a place `cols` cells wide.
pub fn draw(
    template: &Template,
    live: &dyn Values,
    cols: Option<usize>,
    now: NaiveDateTime,
) -> VitalsText {
    let options = RenderOptions {
        placeholders: false,
        cols,
    };
    let (rendered, fight) = render_reading(template, live, options, &reads_fight);
    let full = at_full(live);
    VitalsText {
        live: rendered,
        full: render(template, &Overridden::new(live, &full, now), options),
        fight,
        right: template
            .pieces()
            .iter()
            .enumerate()
            .filter(|(_, piece)| piece.kind == PieceKind::Right)
            .map(|(index, _)| index)
            .collect(),
    }
}

/// True for a field that names a fight, by its name or an alias.
fn reads_fight(field: &FieldRef) -> bool {
    field.param.is_none() && entry(&field.name).is_some_and(|e| FIGHT_FIELDS.contains(&e.name))
}

/// Each vital at its max, where `live` knows the max, and your
/// opponent's health at 100, while `live` has it. A value `live` lacks
/// stays absent, so every condition holds as it does live.
fn at_full(live: &dyn Values) -> Overrides {
    let mut values = BTreeMap::new();
    for pair in Pair::ALL {
        if let Resolved::Value(Value::Gauge { max: Some(max), .. }) =
            live.resolve(&FieldRef::new(pair.cur()))
        {
            values.insert(pair.cur().to_string(), Json::from(max));
        }
    }
    if let Resolved::Value(_) = live.resolve(&FieldRef::new("opponent_hp")) {
        values.insert("opponent_hp".to_string(), Json::from(100));
    }
    Overrides {
        values,
        lament: false,
    }
}

/// The clock pieces `template` reads, which draw it again each second
/// for the tick and each minute for the time or the date.
pub fn clock(template: &Template) -> Clock {
    Clock::of(&template.reads())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_VITALS_TEXT;
    use crate::values::overrides::Preview;
    use crate::values::Samples;

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(21, 40, 0)
            .unwrap()
    }

    /// Vosh's text drawn over the samples with `values` on top.
    fn vosh_text(values: &[(&str, Json)]) -> VitalsText {
        let samples = Samples { now: now() };
        let mut over = Preview::Fight.overrides(&samples);
        over.values
            .extend(values.iter().map(|(k, v)| ((*k).to_string(), v.clone())));
        let live = Overridden::new(&samples, &over, now());
        draw(
            &Template::parse(DEFAULT_VITALS_TEXT),
            &live,
            Some(40),
            now(),
        )
    }

    #[test]
    fn in_vosh_text_the_opponent_row_reads_a_fight_and_your_vitals_row_does_not() {
        let fight = vosh_text(&[]);
        assert_eq!(fight.live.rows, 2);
        assert_eq!(fight.fight, [true, false]);

        // Out of a fight the opponent row draws nothing, so the one row
        // left is your vitals.
        let calm = vosh_text(&[("fight", Json::Bool(false))]);
        assert_eq!(calm.live.plain, "1020/1020hp 800/800mn 930/930mv");
        assert_eq!(calm.fight, [false]);
    }

    #[test]
    fn a_condition_names_a_fight_on_its_row_only_while_it_holds() {
        let samples = Samples { now: now() };
        let text = Template::parse("%{ifnot:fight}calm%{end} %hp%nl%c_tank_hp%mana");
        let drawn = draw(&text, &samples, None, now());
        assert_eq!(drawn.fight, [false, true]);
    }

    #[test]
    fn the_full_text_draws_each_vital_at_its_max_and_the_opponent_at_100() {
        let drawn = vosh_text(&[
            ("hp", Json::from(180)),
            ("mana", Json::from(75)),
            ("opponent_hp", Json::from(4)),
        ]);
        assert!(drawn.live.plain.starts_with("Blackwatch Guard"));
        assert!(drawn
            .live
            .plain
            .ends_with("4%\n180/1020hp 75/800mn 930/930mv"));
        assert!(drawn
            .full
            .plain
            .ends_with("100%\n1020/1020hp 800/800mn 930/930mv"));
        assert_eq!(drawn.full.rows, drawn.live.rows);
    }

    #[test]
    fn it_names_the_push_so_the_footer_finds_it_in_the_spans() {
        let drawn = vosh_text(&[]);
        let push = drawn
            .live
            .spans
            .iter()
            .find(|span| drawn.right.contains(&span.piece))
            .expect("the opponent row pushes");
        assert_eq!(push.row, 0);
        assert_eq!(push.col, "Blackwatch Guard".len());
        assert_eq!(push.col + push.width + "54%".len(), 40);
    }

    #[test]
    fn the_clock_follows_the_tick_and_the_time() {
        assert_eq!(
            clock(&Template::parse(DEFAULT_VITALS_TEXT)),
            Clock::default()
        );
        assert!(clock(&Template::parse("%hp (%tick)")).tick);
        assert!(clock(&Template::parse("%{hp} %time")).wall);
    }
}
