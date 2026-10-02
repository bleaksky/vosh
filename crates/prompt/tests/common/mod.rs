//! Shared helpers for the resolver tests: the Aabahran packets in
//! `fixtures/gmcp/aabahran`, fixed clocks and short ways to feed and
//! draw.

// Each test file uses its own share of these.
#![allow(dead_code, unreachable_pub)]

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
use serde_json::Value as Json;
use vosh_prompt::{
    render_str, Capture, FieldRef, RenderOptions, Resolved, Value, Values, Vars, Vosh,
};

macro_rules! fixture {
    ($file:literal) => {
        (
            $file,
            include_str!(concat!("../../../../fixtures/gmcp/aabahran/", $file)),
        )
    };
}

pub const FIXTURES: &[(&str, &str)] = &[
    fixture!("char-vitals.gmcp"),
    fixture!("char-vitals-hidden.gmcp"),
    fixture!("char-vitals-zero.gmcp"),
    fixture!("char-affects.gmcp"),
    fixture!("char-affects-hidden.gmcp"),
    fixture!("char-affects-empty.gmcp"),
    fixture!("char-affects-lament.gmcp"),
    fixture!("char-combat.gmcp"),
    fixture!("char-combat-hidden.gmcp"),
    fixture!("char-combat-withheld.gmcp"),
    fixture!("char-combat-lament-older.gmcp"),
    fixture!("char-combat-tank.gmcp"),
    fixture!("char-combat-tank-hidden.gmcp"),
    fixture!("char-combat-end.gmcp"),
    fixture!("char-prompt.gmcp"),
    fixture!("char-prompt-off.gmcp"),
    fixture!("char-prompt-fight.gmcp"),
    fixture!("char-state.gmcp"),
    fixture!("room-info.gmcp"),
    fixture!("room-info-rhapsody.gmcp"),
    fixture!("room-weather.gmcp"),
    fixture!("room-weather-indoors.gmcp"),
    fixture!("group-info.gmcp"),
    fixture!("group-info-solo.gmcp"),
    fixture!("group-info-hidden.gmcp"),
    fixture!("group-info-empty.gmcp"),
    fixture!("group-info-own-row.gmcp"),
];

pub const JAMES: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

pub const DETAILED: &str = "%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %opponent_cond%nl%{end}%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv %{c:8}tick%c_default %tick%{if:exits} %{c:8}[%c_default%exits%{c:8}]%c_default%{end} %{gold}g%{if:missing} %c_3%missing missing%c_default%{end}";

pub fn at() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-09-29T12:58:02-05:00").expect("a valid time")
}

pub fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 29)
        .and_then(|d| d.and_hms_opt(8, 42, 10))
        .expect("a valid date")
}

pub fn vosh() -> Vosh {
    Vosh {
        now: Some(now()),
        ..Vosh::default()
    }
}

/// Keep one fixture packet.
pub fn feed(vars: &mut Vars, file: &str) {
    let (_, text) = FIXTURES
        .iter()
        .find(|(name, _)| *name == file)
        .unwrap_or_else(|| panic!("no fixture {file}"));
    let msg = vosh_protocol::gmcp::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{file}: {e}"));
    vars.observe(&msg.package, msg.data, at());
}

/// Keep a packet written inline.
pub fn packet(vars: &mut Vars, package: &str, data: Json) {
    vars.observe(package, data, at());
}

pub fn capture(vars: &mut Vars, pairs: &[(&str, &str)]) -> Vec<&'static str> {
    vars.capture(Capture {
        values: pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        raw: None,
    })
}

/// James's PROMPT captured under lamented tears, `[0/0hp 0/0mn 0/0mv]`.
pub fn lament_capture(vars: &mut Vars) {
    capture(
        vars,
        &[
            ("hp", "0"),
            ("maxhp", "0"),
            ("mana", "0"),
            ("maxmana", "0"),
            ("move", "0"),
            ("maxmove", "0"),
        ],
    );
}

pub fn resolve(vars: &Vars, field: &str) -> Resolved {
    let vosh = vosh();
    let f = match field.split_once(':') {
        Some((name, param)) => FieldRef::with_param(name, param),
        None => FieldRef::new(field),
    };
    vars.resolver(&vosh).resolve(&f)
}

pub fn draw_with(vars: &Vars, vosh: &Vosh, template: &str) -> String {
    render_str(template, &vars.resolver(vosh), RenderOptions::default()).plain
}

pub fn draw(vars: &Vars, template: &str) -> String {
    draw_with(vars, &vosh(), template)
}

pub fn text(s: &str) -> Resolved {
    Resolved::Value(Value::Text(s.to_string()))
}

pub fn num(n: i64) -> Resolved {
    Resolved::Value(Value::Num(n))
}
