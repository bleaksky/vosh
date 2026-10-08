//! Renderer tests. The legacy cases hold the bytes the first renderer
//! wrote for the same template and vars, next to what the new renderer
//! writes, so every change from today is written down.

use std::collections::BTreeMap;

use vosh_prompt::aabahran::codes::Position;
use vosh_prompt::testkit::designs::{DETAILED, JAMES};
use vosh_prompt::testkit::now;
use vosh_prompt::{
    render_str, FieldRef, MapValues, RenderOptions, Rendered, Resolved, SpanColor, Value, Values,
};

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn full() -> BTreeMap<String, String> {
    vars(&[
        ("hp", "1020"),
        ("maxhp", "1020"),
        ("mana", "800"),
        ("maxmana", "800"),
        ("move", "930"),
        ("maxmove", "930"),
    ])
}

fn mixed() -> BTreeMap<String, String> {
    vars(&[
        ("hp", "300"),
        ("maxhp", "1020"),
        ("mana", "400"),
        ("maxmana", "800"),
        ("move", "100"),
        ("maxmove", "930"),
    ])
}

fn legacy() -> BTreeMap<String, String> {
    vars(&[
        ("hp", "329"),
        ("mhp", "400"),
        ("gold", "1250"),
        ("name", "Tester"),
    ])
}

fn zero() -> BTreeMap<String, String> {
    vars(&[
        ("hp", "0"),
        ("maxhp", "0"),
        ("mana", "0"),
        ("maxmana", "0"),
        ("move", "0"),
        ("maxmove", "0"),
    ])
}

fn draw_map(template: &str, map: &BTreeMap<String, String>) -> String {
    render_str(
        template,
        &MapValues::new(map, now()),
        RenderOptions::default(),
    )
    .ansi
}

/// Values fixed per field, written as `name` or `name:param`.
#[derive(Default)]
struct Fixed {
    map: BTreeMap<String, Resolved>,
    labels: BTreeMap<String, String>,
}

impl Fixed {
    fn with(mut self, field: &str, resolved: Resolved) -> Self {
        self.map.insert(field.to_string(), resolved);
        self
    }

    fn value(self, field: &str, value: Value) -> Self {
        self.with(field, Resolved::Value(value))
    }

    fn label(mut self, field: &str, label: &str) -> Self {
        self.labels.insert(field.to_string(), label.to_string());
        self
    }
}

impl Values for Fixed {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        self.map
            .get(&field.to_string())
            .cloned()
            .unwrap_or(Resolved::Unknown)
    }

    fn label(&self, field: &FieldRef) -> String {
        self.labels
            .get(&field.to_string())
            .cloned()
            .unwrap_or_else(|| field.to_string())
    }
}

fn draw(template: &str, values: &Fixed) -> Rendered {
    render_str(template, values, RenderOptions::default())
}

fn gauge(cur: i64, max: i64) -> Value {
    Value::Gauge {
        cur,
        max: Some(max),
        pct: None,
    }
}

fn vitals(hp: i64, mana: i64, mv: i64) -> Fixed {
    Fixed::default()
        .value("hp", gauge(hp, 1020))
        .value("maxhp", Value::Num(1020))
        .value("mana", gauge(mana, 800))
        .value("maxmana", Value::Num(800))
        .value("move", gauge(mv, 930))
        .value("maxmove", Value::Num(930))
}

// James's template, the 207 characters he saves in his default profile.

#[test]
fn james_template_renders_byte_for_byte_except_color_by_how_full() {
    assert_eq!(JAMES.len(), 207);
    let today_full = "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m1020(\x1b[38;5;42m100\x1b[0m\x1b[3m%)h 800(\x1b[38;2;128;200;255m100\x1b[0m\x1b[3m%)m 930(\x1b[38;2;200;255;23m100\x1b[0m\x1b[3m%)v\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m";
    assert_eq!(
        draw_map(JAMES, &full()),
        today_full.replace("\x1b[38;5;42m", "\x1b[32m")
    );
    let today_mixed = "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m300(\x1b[38;5;196m29\x1b[0m\x1b[3m%)h 400(\x1b[38;2;128;200;255m50\x1b[0m\x1b[3m%)m 100(\x1b[38;2;200;255;23m11\x1b[0m\x1b[3m%)v\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m";
    assert_eq!(
        draw_map(JAMES, &mixed()),
        today_mixed.replace("\x1b[38;5;196m", "\x1b[31m")
    );
    // Through a resolver with typed gauges the bytes are the same.
    assert_eq!(
        draw(JAMES, &vitals(1020, 800, 930)).ansi,
        today_full.replace("\x1b[38;5;42m", "\x1b[32m")
    );
    // A max of 0 in a plain var map still prints the raw codes, as before.
    let today_zero = "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m0(%c_hp%pct_hp\x1b[0m\x1b[3m%)h 0(\x1b[38;2;128;200;255m%pct_mana\x1b[0m\x1b[3m%)m 0(\x1b[38;2;200;255;23m%pct_move\x1b[0m\x1b[3m%)v\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m";
    assert_eq!(draw_map(JAMES, &zero()), today_zero);
}

#[test]
fn james_template_under_lament_draws_dim_marks_in_his_colors() {
    let lament = Fixed::default()
        .with("hp", Resolved::Hidden)
        .with("mana", Resolved::Hidden)
        .with("move", Resolved::Hidden);
    let out = draw(JAMES, &lament);
    assert_eq!(out.plain, "[?(?%)h ?(?%)m ?(?%)v] ");
    let expected = concat!(
        "\x1b[38;2;100;100;100m[\x1b[0m\x1b[3m",
        "\x1b[90m?\x1b[39m(\x1b[39m\x1b[90m?\x1b[39m\x1b[0m\x1b[3m%)h ",
        "\x1b[90m?\x1b[39m(\x1b[38;2;128;200;255m\x1b[90m?\x1b[38;2;128;200;255m\x1b[0m\x1b[3m%)m ",
        "\x1b[90m?\x1b[39m(\x1b[38;2;200;255;23m\x1b[90m?\x1b[38;2;200;255;23m\x1b[0m\x1b[3m%)v",
        "\x1b[0m\x1b[38;2;100;100;100m] \x1b[0m\x1b[0m",
    );
    assert_eq!(out.ansi, expected);
}

#[test]
fn legacy_forms_render_as_before_with_theme_colors() {
    // (vars, template, what the first renderer wrote, what Vosh writes now)
    let cases: Vec<(BTreeMap<String, String>, &str, &str, &str)> = vec![
        (
            full(),
            "%hp/%mhp %nope",
            "1020/%mhp %nope\x1b[0m",
            "1020/%mhp %nope\x1b[0m",
        ),
        (full(), "%pct_hp %pct_mana", "100 100\x1b[0m", "100 100\x1b[0m"),
        (
            legacy(),
            "%hp/%mhp %nope",
            "329/400 %nope\x1b[0m",
            "329/400 %nope\x1b[0m",
        ),
        (
            full(),
            "%c_red%hp%c_reset",
            "\x1b[38;5;196m1020\x1b[0m\x1b[0m",
            "\x1b[31m1020\x1b[0m\x1b[0m",
        ),
        (
            full(),
            "%{c:196}x%{c:#ff8800}x%{c:255,128,0}x%c_hp.%{bg:#330033}%s_bold%s_italic%s_dim%s_under%s_inv%s_strike%s_reset",
            "\x1b[38;5;196mx\x1b[38;2;255;136;0mx\x1b[38;2;255;128;0mx\x1b[38;5;42m.\x1b[48;2;51;0;51m\x1b[1m\x1b[3m\x1b[2m\x1b[4m\x1b[7m\x1b[9m\x1b[0m\x1b[0m",
            "\x1b[38;5;196mx\x1b[38;2;255;136;0mx\x1b[38;2;255;128;0mx\x1b[32m.\x1b[48;2;51;0;51m\x1b[1m\x1b[3m\x1b[2m\x1b[4m\x1b[7m\x1b[9m\x1b[0m\x1b[0m",
        ),
        // Bars: empty cells in SGR 90, then the color before the bar in
        // place of a full reset, and no code for an empty run.
        (
            full(),
            "%hp_bar:4:green after",
            "\x1b[38;5;42m████\x1b[38;5;240m\x1b[0m after\x1b[0m",
            "\x1b[32m████\x1b[39m after\x1b[0m",
        ),
        (
            mixed(),
            "%hp_bar:4:green after",
            "\x1b[38;5;42m█\x1b[38;5;240m░░░\x1b[0m after\x1b[0m",
            "\x1b[32m█\x1b[90m░░░\x1b[39m after\x1b[0m",
        ),
        (
            mixed(),
            "%hp_bar|%bar_mana:6|%hp_bar::red|%move_bar:0:yellow|%hp_bar:x",
            "\x1b[38;5;196m███\x1b[38;5;240m░░░░░░░\x1b[0m|\x1b[38;5;220m███\x1b[38;5;240m░░░\x1b[0m|\x1b[38;5;196m███\x1b[38;5;240m░░░░░░░\x1b[0m|\x1b[38;5;220m█\x1b[38;5;240m░░░░░░░░░\x1b[0m|\x1b[38;5;196m███\x1b[38;5;240m░░░░░░░\x1b[0mx\x1b[0m",
            "\x1b[31m███\x1b[90m░░░░░░░\x1b[39m|\x1b[33m███\x1b[90m░░░\x1b[39m|\x1b[31m███\x1b[90m░░░░░░░\x1b[39m|\x1b[33m█\x1b[90m░░░░░░░░░\x1b[39m|\x1b[31m███\x1b[90m░░░░░░░\x1b[39mx\x1b[0m",
        ),
        (
            zero(),
            "%hp_bar|%bar_mana:6|%hp_bar::red|%move_bar:0:yellow|%hp_bar:x",
            "\x1b[38;5;240m░░░░░░░░░░\x1b[0m|\x1b[38;5;240m░░░░░░\x1b[0m|\x1b[38;5;240m░░░░░░░░░░\x1b[0m|\x1b[38;5;240m░░░░░░░░░░\x1b[0m|\x1b[38;5;240m░░░░░░░░░░\x1b[0mx\x1b[0m",
            "\x1b[90m░░░░░░░░░░\x1b[39m|\x1b[90m░░░░░░\x1b[39m|\x1b[90m░░░░░░░░░░\x1b[39m|\x1b[90m░░░░░░░░░░\x1b[39m|\x1b[90m░░░░░░░░░░\x1b[39mx\x1b[0m",
        ),
        // A bar of an unknown name prints its whole token now, the
        // parameters it was written with included.
        (
            legacy(),
            "%hp_bar|%bar_mana:6|%hp_bar::red|%move_bar:0:yellow|%hp_bar:x",
            "\x1b[38;5;42m████████\x1b[38;5;240m░░\x1b[0m|%bar_mana|\x1b[38;5;196m████████\x1b[38;5;240m░░\x1b[0m|%move_bar|\x1b[38;5;42m████████\x1b[38;5;240m░░\x1b[0mx\x1b[0m",
            "\x1b[32m████████\x1b[90m░░\x1b[39m|%bar_mana:6|\x1b[31m████████\x1b[90m░░\x1b[39m|%move_bar:0:yellow|\x1b[32m████████\x1b[90m░░\x1b[39mx\x1b[0m",
        ),
        (
            full(),
            "%%|%)h|%{c:100,100,100}[|% %{ %{} %{a b} %c:red|%",
            "%|%)h|\x1b[38;2;100;100;100m[|% %{ %{} %{a b} %c:red|%\x1b[0m",
            "%|%)h|\x1b[38;2;100;100;100m[|% %{ %{} %{a b} %c:red|%\x1b[0m",
        ),
        (
            mixed(),
            "%{HP}/%{MaxHp} %HP %{pct_hp} %{hp_bar}:3",
            "300/1020 300 29 \x1b[38;5;196m█\x1b[38;5;240m░░\x1b[0m\x1b[0m",
            "300/1020 300 29 \x1b[31m█\x1b[90m░░\x1b[39m\x1b[0m",
        ),
        (
            legacy(),
            "%{HP}/%{MaxHp} %HP %{pct_hp} %{hp_bar}:3",
            "329/%{MaxHp} 329 82 \x1b[38;5;42m██\x1b[38;5;240m░\x1b[0m\x1b[0m",
            "329/%{MaxHp} 329 82 \x1b[32m██\x1b[90m░\x1b[39m\x1b[0m",
        ),
        (
            zero(),
            "%{HP}/%{MaxHp} %HP %{pct_hp} %{hp_bar}:3",
            "0/0 0 %{pct_hp} \x1b[38;5;240m░░░\x1b[0m\x1b[0m",
            "0/0 0 %{pct_hp} \x1b[90m░░░\x1b[39m\x1b[0m",
        ),
        (
            mixed(),
            "%bg_green x%bg_reset %bg_hp y",
            "\x1b[48;5;42m x\x1b[0m \x1b[48;5;196m y\x1b[0m",
            "\x1b[42m x\x1b[0m \x1b[41m y\x1b[0m",
        ),
        (
            zero(),
            "%bg_green x%bg_reset %bg_hp y",
            "\x1b[48;5;42m x\x1b[0m %bg_hp y\x1b[0m",
            "\x1b[42m x\x1b[0m %bg_hp y\x1b[0m",
        ),
        (
            full(),
            "%c_gray%c_white%c_blue%c_cyan%c_magenta%c_yellow%c_green.",
            "\x1b[38;5;240m\x1b[38;5;255m\x1b[38;5;39m\x1b[38;5;51m\x1b[38;5;201m\x1b[38;5;220m\x1b[38;5;42m.\x1b[0m",
            "\x1b[90m\x1b[37m\x1b[34m\x1b[36m\x1b[35m\x1b[33m\x1b[32m.\x1b[0m",
        ),
        (
            legacy(),
            "%name has %gold gold, %c_gold%pct_gold%hp_bar%c_nope%s_nope%c_%{c:}",
            "Tester has 1250 gold, %c_gold%pct_gold\x1b[38;5;42m████████\x1b[38;5;240m░░\x1b[0m%c_nope%s_nope%c_%{c:}\x1b[0m",
            "Tester has 1250 gold, %c_gold%pct_gold\x1b[32m████████\x1b[90m░░\x1b[39m%c_nope%s_nope%c_%{c:}\x1b[0m",
        ),
        (
            full(),
            "%name has %gold gold",
            "%name has %gold gold\x1b[0m",
            "%name has %gold gold\x1b[0m",
        ),
        (
            mixed(),
            "%{c:hp}%{bg:mana}%{s:bold}%{c_red}%{bg_blue}%c_042%c_300%c_ff8800%{c:1,2}",
            "\x1b[38;5;196m\x1b[48;5;220m\x1b[1m\x1b[38;5;196m\x1b[48;5;39m\x1b[38;5;42m%c_300\x1b[38;2;255;136;0m%{c:1,2}\x1b[0m",
            "\x1b[31m\x1b[43m\x1b[1m\x1b[31m\x1b[44m\x1b[38;5;42m%c_300\x1b[38;2;255;136;0m%{c:1,2}\x1b[0m",
        ),
        (
            legacy(),
            "%{c:hp}%{bg:mana}%{s:bold}%{c_red}%{bg_blue}%c_042%c_300%c_ff8800%{c:1,2}",
            "\x1b[38;5;42m%{bg:mana}\x1b[1m\x1b[38;5;196m\x1b[48;5;39m\x1b[38;5;42m%c_300\x1b[38;2;255;136;0m%{c:1,2}\x1b[0m",
            "\x1b[32m%{bg:mana}\x1b[1m\x1b[31m\x1b[44m\x1b[38;5;42m%c_300\x1b[38;2;255;136;0m%{c:1,2}\x1b[0m",
        ),
    ];
    for (map, template, _today, now) in &cases {
        assert_eq!(draw_map(template, map), *now, "{template} with {map:?}");
    }
}

#[test]
fn the_clock_tokens_print_as_before() {
    let map = BTreeMap::new();
    assert_eq!(draw_map("%time %date", &map), "08:42:10 2026-09-29\x1b[0m");
}

#[test]
fn empty_and_invisible_templates_draw_nothing() {
    let none = Fixed::default();
    let out = draw("", &none);
    assert_eq!(out.ansi, "");
    assert_eq!(out.rows, 0);
    let out = draw("%{if:fight}fighting%{end}", &none);
    assert_eq!(out.ansi, "");
    assert_eq!(out.rows, 0);
    // An unclosed color is reset at the end.
    assert!(draw("%{c:100,100,100}[", &none).ansi.ends_with("[\x1b[0m"));
}

// Colors that follow the theme.

#[test]
fn named_colors_use_the_theme_palette() {
    let none = Fixed::default();
    let cases = [
        ("%c_black", "30"),
        ("%c_red", "31"),
        ("%c_green", "32"),
        ("%c_yellow", "33"),
        ("%c_blue", "34"),
        ("%c_magenta", "35"),
        ("%c_cyan", "36"),
        ("%c_white", "37"),
        ("%c_gray", "90"),
        ("%c_bright_red", "91"),
        ("%c_bright_green", "92"),
        ("%c_bright_yellow", "93"),
        ("%c_bright_blue", "94"),
        ("%c_bright_magenta", "95"),
        ("%c_bright_cyan", "96"),
        ("%c_bright_white", "97"),
        ("%bg_black", "40"),
        ("%bg_green", "42"),
        ("%bg_white", "47"),
        ("%bg_gray", "100"),
        ("%bg_bright_white", "107"),
        ("%c_default", "39"),
        ("%bg_default", "49"),
        ("%s_off", "22;23;24;25;27;29"),
        ("%c_8", "38;5;8"),
        ("%c_240", "38;5;240"),
        ("%{c:#80c8ff}", "38;2;128;200;255"),
        ("%c_reset", "0"),
    ];
    for (template, sgr) in cases {
        assert_eq!(
            draw(template, &none).ansi,
            format!("\x1b[{sgr}m\x1b[0m"),
            "{template}"
        );
    }
}

#[test]
fn color_by_how_full_steps_at_two_thirds_and_one_third() {
    let at = |hp| draw("%c_hp", &vitals(hp, 800, 930)).ansi;
    assert_eq!(at(1020), "\x1b[32m\x1b[0m");
    assert_eq!(at(674), "\x1b[32m\x1b[0m");
    assert_eq!(at(673), "\x1b[33m\x1b[0m");
    assert_eq!(at(337), "\x1b[33m\x1b[0m");
    assert_eq!(at(336), "\x1b[31m\x1b[0m");
    // With no value the color is the terminal's own.
    for state in [Resolved::Hidden, Resolved::Absent, Resolved::Missing] {
        let values = Fixed::default().with("hp", state);
        assert_eq!(draw("%c_hp", &values).ansi, "\x1b[39m\x1b[0m");
        assert_eq!(draw("%bg_hp", &values).ansi, "\x1b[49m\x1b[0m");
    }
}

#[test]
fn color_by_the_game_bands_uses_integer_division() {
    let at = |hp| draw("%{c:hp:game}", &vitals(hp, 800, 930)).ansi;
    // 419 * 100 / 1020 is 41.07, plain.
    assert_eq!(at(419), "\x1b[39m\x1b[0m");
    // 418 is 40.98, which the server divides to 40, bold yellow.
    assert_eq!(at(418), "\x1b[1;33m\x1b[0m");
    assert_eq!(at(215), "\x1b[1;33m\x1b[0m");
    // 214 is 20.98, which divides to 20, red.
    assert_eq!(at(214), "\x1b[31m\x1b[0m");
}

#[test]
fn color_by_steps_takes_the_old_prompts_color_for_each_tenth() {
    // The values the old prompt drew: 918, 300 and 930 out of a fight,
    // 408 and 610 in one, and the red branch at 204.
    let values = vitals(918, 300, 930);
    let at = |template: &str| draw(template, &values).ansi;
    assert_eq!(at("%{c:hp:steps}"), "\x1b[38;5;82m\x1b[0m");
    // 300 of 800 is 37.5, which the old prompt cut to 37, step 30.
    assert_eq!(at("%{c:mana:steps}"), "\x1b[38;5;214m\x1b[0m");
    assert_eq!(at("%{c:move:steps}"), "\x1b[38;5;46m\x1b[0m");
    let fight = vitals(408, 300, 610);
    assert_eq!(draw("%{c:hp:steps}", &fight).ansi, "\x1b[38;5;220m\x1b[0m");
    // 610 of 930 is 65.6, cut to 65, the step of 60 and not 70.
    assert_eq!(
        draw("%{c:move:steps}", &fight).ansi,
        "\x1b[38;5;190m\x1b[0m"
    );
    assert_eq!(
        draw("%{c:hp:steps}", &vitals(204, 300, 610)).ansi,
        "\x1b[38;5;208m\x1b[0m"
    );
    // Each step from empty to full, and the ground and the underline
    // take them too.
    let steps: Vec<String> = (0..=10)
        .map(|tenth| draw("%{c:hp:steps}", &vitals(102 * tenth, 0, 0)).ansi)
        .collect();
    let want: Vec<String> = [196, 202, 208, 214, 220, 226, 190, 154, 118, 82, 46]
        .iter()
        .map(|n| format!("\x1b[38;5;{n}m\x1b[0m"))
        .collect();
    assert_eq!(steps, want);
    assert_eq!(at("%{bg:hp:steps}"), "\x1b[48;5;82m\x1b[0m");
    assert_eq!(at("%{ul:hp:steps}"), "\x1b[58:5:82m\x1b[0m");
    // With no value the color is the terminal's own, as by thirds.
    let hidden = Fixed::default().with("hp", Resolved::Hidden);
    assert_eq!(draw("%{c:hp:steps}", &hidden).ansi, "\x1b[39m\x1b[0m");
}

#[test]
fn the_game_percent_prints_the_percent_its_step_colors_by() {
    // In the fight the old prompt drew, it printed 37 for 300 of 800 and
    // 65 for 610 of 930, where the rounded percent reads 38 and 66.
    let fight = vitals(408, 300, 610);
    assert_eq!(
        draw(
            "%{hp:pct:game}%% %{mana:pct:game}%% %{move:pct:game}%%",
            &fight
        )
        .plain,
        "40% 37% 65%"
    );
    assert_eq!(
        draw("%pct_hp%% %pct_mana%% %pct_move%%", &fight).plain,
        "40% 38% 66%"
    );
    // The sign after it takes the step of the same number.
    assert_eq!(
        draw("%{move:pct:game}%{c:move:steps}%%", &fight).ansi,
        "65\x1b[38;5;190m%\x1b[0m"
    );
    // With no value it draws as the rounded percent does.
    let hidden = Fixed::default().with("hp", Resolved::Hidden);
    assert_eq!(
        draw("%{hp:pct:game}", &hidden).ansi,
        draw("%pct_hp", &hidden).ansi
    );
}

// Looks, spans and restoring the color before a mark.

#[test]
fn c_default_keeps_italic_and_bold() {
    let values = Fixed::default().value("hp", gauge(1020, 1020));
    let out = draw("%s_italic%s_bold%c_red%hp%c_default X", &values);
    assert_eq!(out.ansi, "\x1b[3m\x1b[1m\x1b[31m1020\x1b[39m X\x1b[0m");
    let last = out.spans.last().expect("a span for the text");
    assert_eq!(last.fg, SpanColor::Default);
    assert!(last.italic);
    assert!(last.bold);
}

#[test]
fn spans_carry_each_piece_look_with_codes_at_zero_width() {
    let values = vitals(1020, 800, 930);
    let out = draw(JAMES, &values);
    let cols: Vec<(usize, usize, usize)> = out
        .spans
        .iter()
        .map(|s| (s.piece, s.col, s.width))
        .collect();
    // `[` `1020` `(` `100` `%)h ` `800` `(` `100` `%)m ` `930` `(` `100`
    // `%)v` `] `, and the trailing reset draws no span.
    assert_eq!(
        cols,
        vec![
            (0, 0, 1),
            (1, 1, 4),
            (2, 5, 1),
            (3, 6, 3),
            (4, 9, 4),
            (5, 13, 3),
            (6, 16, 1),
            (7, 17, 3),
            (8, 20, 4),
            (9, 24, 3),
            (10, 27, 1),
            (11, 28, 3),
            (12, 31, 3),
            (13, 34, 2),
        ]
    );
    let gray = SpanColor::Rgb {
        r: 100,
        g: 100,
        b: 100,
    };
    assert_eq!(out.spans[0].fg, gray);
    assert!(!out.spans[0].italic);
    // `(` and `%)h` inherit italic from the earlier %s_italic.
    assert!(out.spans[1].italic);
    assert!(out.spans[2].italic);
    assert!(out.spans[4].italic);
    assert_eq!(out.spans[3].fg, SpanColor::Index { index: 2 });
    assert!(out.spans[3].italic);
    assert_eq!(out.spans[4].fg, SpanColor::Default);
    assert_eq!(
        out.spans[7].fg,
        SpanColor::Rgb {
            r: 128,
            g: 200,
            b: 255
        }
    );
    assert_eq!(out.spans[13].fg, gray);
    assert!(!out.spans[13].italic);
    assert_eq!(out.plain, "[1020(100%)h 800(100%)m 930(100%)v] ");
    assert_eq!(out.rows, 1);
}

#[test]
fn hidden_marks_and_placeholders_restore_the_color_before_them() {
    let values = Fixed::default()
        .with("hp", Resolved::Hidden)
        .with("gold", Resolved::Missing)
        .with("tank", Resolved::Absent)
        .label("gold", "Gold")
        .label("tank", "Tank");
    assert_eq!(
        draw("%c_red%hp x", &values).ansi,
        "\x1b[31m\x1b[90m?\x1b[31m x\x1b[0m"
    );
    assert_eq!(
        draw("%{c:#80c8ff}%s_bold%hp_bar:4", &values).ansi,
        "\x1b[38;2;128;200;255m\x1b[1m\x1b[90m····\x1b[38;2;128;200;255m\x1b[0m"
    );
    // Missing and Absent draw nothing, or the label while the editor is open.
    assert_eq!(draw("%c_blue%gold|%tank", &values).plain, "|");
    let open = render_str(
        "%c_blue%gold|%tank",
        &values,
        RenderOptions {
            placeholders: true,
            ..RenderOptions::default()
        },
    );
    assert_eq!(
        open.ansi,
        "\x1b[34m\x1b[90mGold\x1b[34m|\x1b[90mTank\x1b[34m\x1b[0m"
    );
    assert_eq!(open.plain, "Gold|Tank");
}

// Every format in every state.

fn one(template: &str, field: &str, state: Resolved) -> String {
    draw(
        template,
        &Fixed::default().with(field, state).label(field, "Label"),
    )
    .plain
}

#[test]
fn every_format_in_every_state() {
    use Resolved::{Absent, Hidden, Missing};
    let v = |value: Value| Resolved::Value(value);
    let clock = |date| Value::Clock { at: now(), date };
    // (template, field, value, what the value draws, what Hidden draws)
    let cases: Vec<(&str, &str, Resolved, &str, &str)> = vec![
        ("%hp", "hp", v(gauge(300, 1020)), "300", "?"),
        ("%{hp}", "hp", v(gauge(300, 1020)), "300", "?"),
        ("%{hp:max}", "hp", v(gauge(300, 1020)), "1020", "?"),
        ("%pct_hp", "hp", v(gauge(300, 1020)), "29", "?"),
        ("%{hp:pct}", "hp", v(gauge(300, 1020)), "29", "?"),
        ("%pct_hp%%", "hp", v(gauge(300, 1020)), "29%", "?%"),
        (
            "%hp/%{hp:max}",
            "hp",
            v(gauge(300, 1020)),
            "300/1020",
            "?/?",
        ),
        ("%hp_bar:4", "hp", v(gauge(510, 1020)), "██░░", "····"),
        (
            "%{hp:bar:4:auto}",
            "hp",
            v(gauge(510, 1020)),
            "██░░",
            "····",
        ),
        ("%{tank_hp}", "tank_hp", v(Value::TankHp(20)), "20", "?"),
        (
            "%{tank_hp:game}",
            "tank_hp",
            v(Value::TankHp(20)),
            "[===|---|---|---]",
            "[···|···|···|···]",
        ),
        (
            "%{moon1:game}",
            "moon1",
            v(Value::Moon {
                phase: 4,
                active: true,
                name: None,
            }),
            "FUL",
            "?",
        ),
        (
            "%{moon1:word}",
            "moon1",
            v(Value::Moon {
                phase: 4,
                active: true,
                name: None,
            }),
            "full",
            "?",
        ),
        (
            "%{moon1:name}",
            "moon1",
            v(Value::Moon {
                phase: 2,
                active: true,
                name: Some("half-lit and growing".into()),
            }),
            "half-lit and growing",
            "?",
        ),
        (
            "%{exits:game}",
            "exits",
            v(Value::Exits {
                letters: "S".into(),
                game: "[Exits: S]".into(),
            }),
            "[Exits: S]",
            "?",
        ),
        (
            "%{wizi:game}",
            "wizi",
            v(Value::Level {
                word: "Wizi".into(),
                level: 60,
            }),
            "(Wizi 60)",
            "?",
        ),
        (
            "%pos",
            "pos",
            v(Value::Position(Position::Fighting)),
            "fgt",
            "?",
        ),
        (
            "%{pos:word}",
            "pos",
            v(Value::Position(Position::Fighting)),
            "fighting",
            "?",
        ),
        (
            "%{lang:game}",
            "lang",
            v(Value::Lang("Orcish".into())),
            "orcish",
            "?",
        ),
        (
            "%{slot1:game}",
            "slot1",
            v(Value::Slot("~".into())),
            "~",
            "?",
        ),
        ("%{hour:word}", "hour", v(Value::Hour(14)), "2 pm", "?"),
        ("%{gold:grouped}", "gold", v(Value::Num(1250)), "1,250", "?"),
        ("%{gold:short}", "gold", v(Value::Num(1250)), "1.2k", "?"),
        (
            "%{tick:unit}",
            "tick",
            v(Value::Seconds {
                secs: 14,
                max: Some(60),
                since: Some(46),
            }),
            "14s",
            "?",
        ),
        (
            "%{temp:unit}",
            "temp",
            v(Value::Temp {
                degrees: 61,
                unit: Some('F'),
            }),
            "61°F",
            "?",
        ),
        (
            "%{room:trunc:8}",
            "room",
            v(Value::Text("the Bank of Aabahran".into())),
            "the Bank",
            "?",
        ),
        ("%{time:hm}", "time", v(clock(false)), "08:42", "?"),
        ("%{time:hms}", "time", v(clock(false)), "08:42:10", "?"),
        ("%{date:md}", "date", v(clock(true)), "Sep 29", "?"),
        (
            "%{missing:names}",
            "missing",
            v(Value::List(vec!["sanctuary".into(), "haste".into()])),
            "sanctuary, haste",
            "?",
        ),
        (
            "%{missing:count}",
            "missing",
            v(Value::List(vec!["sanctuary".into(), "haste".into()])),
            "2",
            "?",
        ),
        (
            "%{aff:sanctuary}",
            "aff:sanctuary",
            v(Value::Ticks(12)),
            "12",
            "?",
        ),
        (
            "%{aff:sanctuary:on}",
            "aff:sanctuary",
            v(Value::Ticks(12)),
            "Label",
            "?",
        ),
        (
            "%{aff:sanctuary:off}",
            "aff:sanctuary",
            v(Value::Ticks(12)),
            "",
            "?",
        ),
        ("%{eclipse}", "eclipse", v(Value::Flag), "Label", "?"),
        (
            "%{group_low}",
            "group_low",
            v(Value::Member {
                name: "Iskra".into(),
                pct: 45,
            }),
            "Iskra 45%",
            "?",
        ),
        (
            "%{gmcp:Char.Vitals.ep}",
            "gmcp:Char.Vitals.ep",
            v(Value::Num(40)),
            "40",
            "?",
        ),
    ];
    for (template, field, value, drawn, hidden) in cases {
        assert_eq!(one(template, field, value), drawn, "{template} Value");
        assert_eq!(one(template, field, Hidden), hidden, "{template} Hidden");
        let off = if template.ends_with(":off}") {
            "Label"
        } else {
            ""
        };
        assert_eq!(one(template, field, Absent), off, "{template} Absent");
        assert_eq!(one(template, field, Missing), "", "{template} Missing");
    }
}

#[test]
fn the_game_formats_draw_the_game_colors() {
    let tank = |pct| {
        draw(
            "%{tank_hp:game}",
            &Fixed::default().value("tank_hp", Value::TankHp(pct)),
        )
        .ansi
    };
    // Red under 25 percent, restored at each divider, as health_prompt does.
    assert_eq!(
        tank(20),
        "[\x1b[31m===\x1b[39m|\x1b[31m---\x1b[39m|\x1b[31m---\x1b[39m|\x1b[31m---\x1b[39m]\x1b[0m"
    );
    assert_eq!(
        tank(4),
        "[\x1b[1;31m=--\x1b[22;39m|\x1b[1;31m---\x1b[22;39m|\x1b[1;31m---\x1b[22;39m|\x1b[1;31m---\x1b[22;39m]\x1b[0m"
    );
    assert_eq!(tank(100), "[===|===|===|===]\x1b[0m");
    let wizi = draw(
        "%c_red%{wizi:game} ",
        &Fixed::default().value(
            "wizi",
            Value::Level {
                word: "Wizi".into(),
                level: 60,
            },
        ),
    );
    assert_eq!(wizi.ansi, "\x1b[31m\x1b[38;5;240m(Wizi 60)\x1b[31m \x1b[0m");
    let bar = |pct| {
        draw(
            "%{opponent_hp:bar:4:game}",
            &Fixed::default().value("opponent_hp", Value::Pct(pct)),
        )
        .ansi
    };
    assert_eq!(bar(60), "\x1b[33m██\x1b[90m░░\x1b[39m\x1b[0m");
    assert_eq!(bar(100), "\x1b[39m████\x1b[0m");
}

#[test]
fn formats_that_do_not_apply_print_the_token_as_written() {
    let values = Fixed::default()
        .value("room", Value::Text("the Bank".into()))
        .value("gold", Value::Num(1250));
    assert_eq!(
        draw("%{room:pct}|%{gold:bar}|%{room:hm}", &values).plain,
        "%{room:pct}|%{gold:bar}|%{room:hm}"
    );
    // Names nothing knows print as written, and read as false in a condition.
    assert_eq!(draw("%nope %{if:nope}x%{end}", &values).plain, "%nope ");
}

// Conditions, line breaks and the raw prompt.

#[test]
fn detailed_out_of_a_fight_with_nothing_missing_draws_one_line() {
    let values = vitals(1020, 800, 930)
        .with("fight", Resolved::Absent)
        .with("opponent", Resolved::Absent)
        .with("opponent_hp", Resolved::Absent)
        .with("opponent_cond", Resolved::Absent)
        .with("missing", Resolved::Absent)
        .value(
            "exits",
            Value::Exits {
                letters: "S".into(),
                game: "[Exits: S]".into(),
            },
        )
        .value(
            "tick",
            Value::Seconds {
                secs: 14,
                max: Some(60),
                since: Some(46),
            },
        )
        .value("gold", Value::Num(1250));
    let out = draw(DETAILED, &values);
    assert_eq!(
        out.plain,
        "1020/1020hp 800/800mn 930/930mv tick 14 [S] 1250g"
    );
    assert_eq!(out.rows, 1);
    assert!(!out.plain.contains("missing"));

    // In a fight with two affects missing it draws two lines.
    let fighting = values
        .with("fight", Resolved::Value(Value::Flag))
        .value("opponent", Value::Text("Blackwatch Guard".into()))
        .value("opponent_hp", Value::Pct(60))
        .value("opponent_cond", Value::Text("quite a few wounds".into()))
        .value(
            "missing",
            Value::List(vec!["sanctuary".into(), "haste".into()]),
        );
    let out = draw(DETAILED, &fighting);
    assert_eq!(
        out.plain,
        "Blackwatch Guard ██████░░░░ 60% quite a few wounds\n1020/1020hp 800/800mn 930/930mv tick 14 [S] 1250g 2 missing"
    );
    assert_eq!(out.rows, 2);
    assert!(out.ansi.contains("quite a few wounds\r\n"));
}

#[test]
fn conditions_test_value_or_hidden_and_nest() {
    let values = Fixed::default()
        .with("fight", Resolved::Absent)
        .with("missing", Resolved::Hidden)
        .value("afk", Value::Flag);
    assert_eq!(draw("%{if:fight}in a fight%{end}", &values).ansi, "");
    assert_eq!(draw("%{ifnot:fight}calm%{end}", &values).plain, "calm");
    // Hidden counts as there, so its dim mark shows.
    assert_eq!(draw("%{if:missing}%missing%{end}", &values).plain, "?");
    assert_eq!(
        draw("a%{if:afk}b%{if:fight}c%{end}d%{end}e", &values).plain,
        "abde"
    );
    assert_eq!(
        draw("a%{if:fight}b%{if:afk}c%{end}d%{end}e", &values).plain,
        "ae"
    );
    // A stray end is ignored, and an open condition runs to the end.
    assert_eq!(draw("a%{end}b%{if:afk}c", &values).plain, "abc");
}

fn draw_at(template: &str, values: &Fixed, cols: Option<usize>) -> Rendered {
    render_str(
        template,
        values,
        RenderOptions {
            cols,
            ..RenderOptions::default()
        },
    )
}

#[test]
fn a_push_ends_the_rest_of_its_row_on_the_last_column() {
    let values = vitals(1020, 800, 930);
    let out = draw_at("<%hp>%{right}%mana!", &values, Some(20));
    assert_eq!(out.plain, format!("<1020>{}800!", " ".repeat(10)));
    assert_eq!(out.ansi, format!("<1020>{}800!\x1b[0m", " ".repeat(10)));
    assert_eq!(out.rows, 1);
    // The push takes the spaces as its cells, and the pieces after it
    // move right with them.
    let spans: Vec<(usize, usize, usize, usize)> = out
        .spans
        .iter()
        .map(|s| (s.piece, s.row, s.col, s.width))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 0, 0, 1),
            (1, 0, 1, 4),
            (2, 0, 5, 1),
            (3, 0, 6, 10),
            (4, 0, 16, 3),
            (5, 0, 19, 1)
        ]
    );
    // A row that fits exactly keeps one space, and one that does not fit,
    // or a render no terminal shows, gets one space too.
    for cols in [Some(11), Some(10), Some(4), None] {
        let out = draw_at("<%hp>%{right}%mana!", &values, cols);
        assert_eq!(out.plain, "<1020> 800!", "{cols:?}");
    }
    assert_eq!(
        draw_at("<%hp>%{right}%mana!", &values, Some(12)).plain,
        "<1020>  800!"
    );
}

#[test]
fn the_spaces_of_a_push_take_the_look_where_it_sits() {
    let values = vitals(1020, 800, 930);
    // The ground runs through the spaces, and the text color after the
    // push starts after them.
    let out = draw_at(
        "%{bg:#3b4252}%hp%{right}%c_red%mana%c_reset",
        &values,
        Some(12),
    );
    assert_eq!(
        out.ansi,
        "\x1b[48;2;59;66;82m1020     \x1b[31m800\x1b[0m\x1b[0m"
    );
    let push = &out.spans[1];
    assert_eq!((push.piece, push.col, push.width), (1, 4, 5));
    assert_eq!(
        push.bg,
        SpanColor::Rgb {
            r: 59,
            g: 66,
            b: 82
        }
    );
}

#[test]
fn each_row_pushes_on_its_own_and_only_its_first_push_counts() {
    let values = vitals(1020, 800, 930);
    let out = draw_at("a%{right}b%{nl}c%{right}d%{right}e", &values, Some(10));
    assert_eq!(
        out.plain,
        format!("a{}b\nc{}de", " ".repeat(8), " ".repeat(7))
    );
    assert_eq!(
        out.ansi,
        format!("a{}b\r\nc{}de\x1b[0m", " ".repeat(8), " ".repeat(7))
    );
    // The second push on a row takes no cells.
    let pushes: Vec<(usize, usize, usize, usize)> = out
        .spans
        .iter()
        .filter(|s| [1, 5, 7].contains(&s.piece))
        .map(|s| (s.piece, s.row, s.col, s.width))
        .collect();
    assert_eq!(pushes, [(1, 0, 1, 8), (5, 1, 1, 7), (7, 1, 9, 0)]);
    // Only the push that took the spaces says so, for the band to find
    // the gap it closes when the row is too wide for it.
    let marked: Vec<usize> = out
        .spans
        .iter()
        .filter(|s| s.push)
        .map(|s| s.piece)
        .collect();
    assert_eq!(marked, [1, 5]);
    // A push at the end of a row fills it to the edge, and a push in a
    // condition that does not hold pushes nothing.
    assert_eq!(draw_at("ab%{right}", &values, Some(5)).plain, "ab   ");
    assert_eq!(
        draw_at("%{if:fight}x%{right}%{end}y", &values, Some(5)).plain,
        "y"
    );
    // A wide character takes two of the columns.
    assert_eq!(draw_at("界%{right}x", &values, Some(6)).plain, "界   x");
}

#[test]
fn line_breaks_start_new_rows_and_spans_follow_them() {
    let values = Fixed::default().value("hp", gauge(1020, 1020));
    let out = draw("ab%nl%hp%{nl}c", &values);
    assert_eq!(out.ansi, "ab\r\n1020\r\nc\x1b[0m");
    assert_eq!(out.plain, "ab\n1020\nc");
    assert_eq!(out.rows, 3);
    let spans: Vec<(usize, usize, usize, usize)> = out
        .spans
        .iter()
        .map(|s| (s.piece, s.row, s.col, s.width))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0, 0, 0, 2),
            (1, 0, 2, 0),
            (2, 1, 0, 4),
            (3, 1, 4, 0),
            (4, 2, 0, 1)
        ]
    );
}

#[test]
fn raw_draws_the_game_prompt_and_restores_the_look_after_it() {
    let values = Fixed::default().value(
        "raw",
        Value::Styled("Tester: [\x1b[0;31m===\x1b[0;0m|---|---|---]\n\r[20/1020hp]".to_string()),
    );
    let out = draw("%s_italic%{raw}!", &values);
    assert_eq!(
        out.ansi,
        "\x1b[3mTester: [\x1b[0;31m===\x1b[0;0m|---|---|---]\r\n[20/1020hp]\x1b[0m\x1b[3m!\x1b[0m"
    );
    assert_eq!(out.plain, "Tester: [===|---|---|---]\n[20/1020hp]!");
    assert_eq!(out.rows, 2);
    let raw: Vec<(usize, usize, usize)> = out
        .spans
        .iter()
        .filter(|s| s.piece == 0)
        .map(|s| (s.row, s.col, s.width))
        .collect();
    assert_eq!(raw, vec![(0, 0, 25), (1, 0, 11)]);
    let bang = out.spans.last().expect("a span for the text");
    assert!(bang.italic);
}

#[test]
fn spans_count_cells_as_the_webview_lays_them_out() {
    // A wide character takes two cells and a combining mark none, as
    // sgrCells.ts counts them, so a span's width times the cell width is
    // the piece on screen.
    let values = vitals(1020, 800, 930);
    let out = draw("%hp \u{65e5}\u{672c} e\u{301} %mana", &values);
    assert_eq!(out.plain, "1020 \u{65e5}\u{672c} e\u{301} 800");
    let cols: Vec<(usize, usize, usize)> = out
        .spans
        .iter()
        .map(|s| (s.piece, s.col, s.width))
        .collect();
    assert_eq!(cols, vec![(0, 0, 4), (1, 4, 8), (2, 12, 3)]);
    assert_eq!(vosh_prompt::wrap::cell_width('a'), 1);
    assert_eq!(vosh_prompt::wrap::cell_width('\u{65e5}'), 2);
    assert_eq!(vosh_prompt::wrap::cell_width('\u{301}'), 0);
    assert_eq!(vosh_prompt::wrap::cell_width('\u{2588}'), 1);
    assert_eq!(vosh_prompt::wrap::cell_width('\u{1f600}'), 2);
}

// The underline kinds, the underline color and the styles the card adds.

#[test]
fn the_sgr_state_reads_sub_parameters_as_one_code() {
    use vosh_prompt::design::UnderlineStyle;
    use vosh_prompt::render::{Color, SgrState};
    let mut s = SgrState::default();
    s.apply("4:3;58:2::191:97:106");
    assert_eq!(s.underline, Some(UnderlineStyle::Curly));
    assert!(!s.italic && !s.dim && !s.bold);
    assert_eq!(s.underline_color, Color::Rgb(191, 97, 106));
    s.apply("58;5;208");
    assert_eq!(s.underline_color, Color::Index(208));
    s.apply("58:5:9");
    assert_eq!(s.underline_color, Color::Index(9));
    s.apply("59");
    assert_eq!(s.underline_color, Color::Default);
    s.apply("38:2:1:2:3");
    assert_eq!(s.fg, Color::Rgb(1, 2, 3));
    s.apply("38:2::4:5:6;48:5:7");
    assert_eq!((s.fg, s.bg), (Color::Rgb(4, 5, 6), Color::Index(7)));
    s.apply("21");
    assert_eq!(s.underline, Some(UnderlineStyle::Double));
    s.apply("4:0");
    assert_eq!(s.underline, None);
    s.apply("4");
    assert_eq!(s.underline, Some(UnderlineStyle::Single));
    s.apply("4:5");
    assert_eq!(s.underline, Some(UnderlineStyle::Dashed));
    s.apply("24");
    assert_eq!(s.underline, None);
    assert!(!s.italic && !s.dim && !s.bold);
    s.apply("4:4;58:5:1;0");
    assert_eq!(s, SgrState::default());
}

#[test]
fn underline_kinds_and_their_color_write_their_own_sgr() {
    let values = vitals(1020, 800, 930);
    assert_eq!(
        draw("%s_curly%{ul:#bf616a}%hp", &values).ansi,
        "\x1b[4:3m\x1b[58:2::191:97:106m1020\x1b[0m"
    );
    assert_eq!(
        draw("%s_double.%s_dotted.%s_dashed.%s_underline.", &values).ansi,
        "\x1b[4:2m.\x1b[4:4m.\x1b[4:5m.\x1b[4m.\x1b[0m"
    );
    assert_eq!(
        draw(
            "%{ul:red}%s_curly.%{ul:208}.%{ul:1,2,3}.%{ul:default}.",
            &values
        )
        .ansi,
        "\x1b[58:5:1m\x1b[4:3m.\x1b[58:5:208m.\x1b[58:2::1:2:3m.\x1b[59m.\x1b[0m"
    );
    // By how full, and by the game's own bands.
    assert_eq!(
        draw("%{ul:hp}", &vitals(300, 800, 930)).ansi,
        "\x1b[58:5:1m\x1b[0m"
    );
    assert_eq!(
        draw("%{ul:hp:game}", &vitals(214, 800, 930)).ansi,
        "\x1b[58:5:1m\x1b[0m"
    );
    assert_eq!(
        draw("%{ul:hp:game}", &vitals(418, 800, 930)).ansi,
        "\x1b[58:5:3m\x1b[0m"
    );
    assert_eq!(draw("%{ul:hp:game}", &values).ansi, "\x1b[59m\x1b[0m");
    // A field nothing knows prints as written, as a text color does.
    assert_eq!(draw("%{ul:nope}", &values).ansi, "%{ul:nope}\x1b[0m");
}

#[test]
fn a_value_whose_name_starts_with_ul_prints_as_before() {
    // A script can name a value anything, so an underline color takes
    // only the braced form and leaves every name to the values.
    let map = vars(&[("ul_kills", "12"), ("ul_red", "3")]);
    assert_eq!(
        draw_map(
            "kills %ul_kills %{ul_kills} %ul_red %{ul_red:trunc:1}",
            &map
        ),
        "kills 12 12 3 3\x1b[0m"
    );
}

#[test]
fn style_off_ends_any_underline_and_keeps_its_color() {
    let out = draw(
        "%s_curly%{ul:#bf616a}%s_strike%s_dim%s_inverse.%s_off.",
        &Fixed::default(),
    );
    assert_eq!(
        out.ansi,
        "\x1b[4:3m\x1b[58:2::191:97:106m\x1b[9m\x1b[2m\x1b[7m.\x1b[22;23;24;25;27;29m.\x1b[0m"
    );
    assert!(out.spans[0].underline);
    assert!(!out.spans[1].underline);
}

#[test]
fn blink_writes_sgr_5_and_style_off_ends_it() {
    use vosh_prompt::render::SgrState;
    let out = draw("%s_blink.%s_off.", &Fixed::default());
    assert_eq!(out.ansi, "\x1b[5m.\x1b[22;23;24;25;27;29m.\x1b[0m");
    assert!(out.spans[0].look.blink);
    assert!(!out.spans[1].look.blink);
    // The game's prompt resets the look, so the blink comes back after
    // it. A 25 from the game ends it, and the rapid 6 draws steady, as
    // xterm draws it.
    let values = Fixed::default().value("raw", Value::Styled("x".to_string()));
    assert_eq!(
        draw("%s_blink%{raw}!", &values).ansi,
        "\x1b[5mx\x1b[0m\x1b[5m!\x1b[0m"
    );
    let values = Fixed::default().value("raw", Value::Styled("\x1b[25mx".to_string()));
    assert_eq!(
        draw("%s_blink%{raw}!", &values).ansi,
        "\x1b[5m\x1b[25mx\x1b[0m\x1b[5m!\x1b[0m"
    );
    let mut s = SgrState::default();
    s.apply("6");
    assert!(!s.blink);
    s.apply("5");
    assert!(s.blink);
    s.apply("25");
    assert!(!s.blink);
    s.apply("1;5");
    assert_eq!(SgrState::default().transition(&s), "1;5");
    s.apply("22");
    assert_eq!(s.transition(&SgrState::default()), "25");
}

#[test]
fn a_restore_writes_the_underline_kind_and_color_back_whole() {
    // The game's prompt resets the look, so what comes after it gets the
    // curly line and its color back, and no style the colons would name.
    let values = Fixed::default().value("raw", Value::Styled("x".to_string()));
    let out = draw("%s_curly%{ul:#bf616a}%{raw}!", &values);
    assert_eq!(
        out.ansi,
        "\x1b[4:3m\x1b[58:2::191:97:106mx\x1b[0m\x1b[4:3;58:2::191:97:106m!\x1b[0m"
    );
    // A semicolon color and a 4:0 from the game read as they are meant,
    // so the italic after it comes back alone.
    let values = Fixed::default().value(
        "raw",
        Value::Styled("\x1b[4:0;58;2;1;2;3mx\x1b[38:2::4:5:6;21my".to_string()),
    );
    let out = draw("%s_italic%{raw}!", &values);
    assert_eq!(
        out.ansi,
        "\x1b[3m\x1b[4:0;58;2;1;2;3mx\x1b[38:2::4:5:6;21my\x1b[0m\x1b[3m!\x1b[0m"
    );
    // A hidden mark puts the text color back and leaves the line alone.
    let values = Fixed::default().with("hp", Resolved::Hidden);
    assert_eq!(
        draw("%s_dashed%{ul:cyan}%hp", &values).ansi,
        "\x1b[4:5m\x1b[58:5:6m\x1b[90m?\x1b[39m\x1b[0m"
    );
}
