//! `prompt_compile` tests: what a capture compiles to, the codes and
//! warnings the card shows, and the designs it offers to start from.

use std::collections::BTreeMap;

use vosh_prompt::aabahran::{WarningKind, Which, Who};
use vosh_prompt::report::{line_report, report, CompileRequest};
use vosh_prompt::template::TokenKind;
use vosh_prompt::Template;

const JAMES_PROMPT: &str = "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c";

const SAME_AS_THE_GAME: &str = "%{if:wizi}%c_240(Wizi %wizi)%c_reset %{end}%{if:incog}%c_240(Incog %incog)%c_reset %{end}%{if:tank}%tank: %{tank_hp:game}%nl%{end}[%{c:hp:game}%hp%c_reset/%{maxhp}hp %mana/%{maxmana}mn %move/%{maxmove}mv] ";

fn codes(prompt: &str, fprompt: &str, typed: bool) -> vosh_prompt::report::CompileReport {
    report(
        &CompileRequest::Aabahran {
            prompt: prompt.into(),
            fprompt: fprompt.into(),
            typed,
        },
        Who::default(),
        &|_| true,
    )
}

fn regex(lines: &[&str], supplied: &dyn Fn(&str) -> bool) -> vosh_prompt::report::CompileReport {
    report(
        &CompileRequest::Regex {
            lines: lines.iter().map(|l| (*l).to_string()).collect(),
            names: BTreeMap::new(),
        },
        Who::default(),
        supplied,
    )
}

fn assert_reads_clean(template: &str) {
    let parsed = Template::parse(template);
    assert!(
        parsed.tokens().iter().all(|t| t.kind != TokenKind::Unknown),
        "{template}"
    );
}

fn preset<'a>(report: &'a vosh_prompt::report::CompileReport, id: &str) -> Option<&'a str> {
    report
        .presets
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.template.as_str())
}

#[test]
fn james_prompt_reads_his_vitals_and_the_tank_line() {
    let report = codes(JAMES_PROMPT, "", false);
    assert!(report.ok);
    assert_eq!(report.error, None);
    assert_eq!(report.prompt, JAMES_PROMPT);
    assert_eq!(
        report.vars,
        ["hp", "maxhp", "mana", "maxmana", "move", "maxmove", "tank", "tank_bar"]
    );
    let kinds: Vec<&str> = report.shapes.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds, ["normal", "tank", "afk"]);
    assert_eq!(report.shapes[0].label, "Your prompt");
    assert_eq!(
        report.shapes[1].label,
        "Your prompt while someone in your group tanks"
    );
    assert_eq!(report.shapes[1].lines.len(), 2);
    assert!(report.warnings.is_empty());
    let written: Vec<(&str, &str, bool)> = report
        .codes
        .iter()
        .map(|c| (c.code.as_str(), c.label.as_str(), c.read))
        .collect();
    assert_eq!(
        written,
        [
            ("%n", "Tank", true),
            ("%P", "Tank health", true),
            ("%h", "Health", true),
            ("%H", "Max health", true),
            ("%m", "Mana", true),
            ("%M", "Max mana", true),
            ("%v", "Moves", true),
            ("%V", "Max moves", true),
        ]
    );
    assert_eq!(report.codes[2].span, [7, 9]);
    assert_eq!(report.codes[2].which, Which::Prompt);
}

#[test]
fn same_as_the_game_writes_every_code_in_the_game_look() {
    let report = codes(JAMES_PROMPT, "", false);
    assert_eq!(preset(&report, "game"), Some(SAME_AS_THE_GAME));
    let ids: Vec<&str> = report.presets.iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        ["game", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    let labels: Vec<&str> = report.presets.iter().map(|p| p.label).collect();
    assert_eq!(
        labels,
        [
            "Same as the game",
            "Minimal",
            "Colored by how full",
            "Percent",
            "Bars",
            "Detailed",
            "Start empty"
        ]
    );
    for preset in &report.presets {
        assert_reads_clean(&preset.template);
        if preset.id == "empty" {
            assert_eq!(preset.template, "");
        } else {
            // Presets end in a space, as the game's own prompt does.
            assert!(preset.template.ends_with(' '), "{}", preset.template);
        }
    }
    assert_eq!(
        preset(&report, "minimal"),
        Some("%{hp}h %{mana}m %{move}v > ")
    );
}

#[test]
fn prompt_all_and_a_fight_prompt_with_colors_draw_as_the_game_does() {
    let all = codes("all", "", true);
    assert_eq!(all.prompt, "%n%P%C<%hhp %mm %vmv> ");
    let game = preset(&all, "game").expect("same as the game");
    assert!(
        game.ends_with("%{if:tank}%tank: %{tank_hp:game}%nl%{end}<%{c:hp:game}%hp%{c:reset}hp %{mana}m %{move}mv> "),
        "{game}"
    );
    assert_reads_clean(game);

    // The fight prompt the game sends in the fixture, with its backtick
    // colors, draws only in a fight.
    let fight = codes(
        "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c",
        "`1%h``hp [%p] > ",
        false,
    );
    assert!(fight.ok);
    let game = preset(&fight, "game").expect("same as the game");
    assert!(game.contains("%{ifnot:fight}"), "{game}");
    assert!(
        game.ends_with("%{end}%{if:fight}%c_reset%c_red%{c:hp:game}%hp%c_reset%{c:reset}hp [%{if:tank}[%tank_hp]%{end}] > %{end}"),
        "{game}"
    );
    assert_reads_clean(game);
    assert!(fight.vars.contains(&"tank_pct".to_string()));
}

#[test]
fn a_color_that_runs_into_a_code_does_not_compile() {
    let report = codes("<`%h> ", "", false);
    assert!(!report.ok);
    let error = report.error.clone().expect("an error");
    assert_eq!(
        error.message,
        "A color code runs into %h. Put a space between them in the game."
    );
    assert_eq!(error.which, Some(Which::Prompt));
    assert!(report.shapes.is_empty());
    // Same as the game needs codes Vosh can read, and the rest stay.
    assert_eq!(preset(&report, "game"), None);
    assert!(preset(&report, "minimal").is_some());
}

#[test]
fn warnings_carry_their_span_and_sentence() {
    let report = codes("<%h%m %vmv> ", "", false);
    assert!(report.ok);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.kind == WarningKind::RunTogether)
        .expect("a warning");
    assert_eq!(
        warning.message,
        "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
    );
    assert_eq!(warning.span, [1, 5]);
    assert_eq!(warning.which, Which::Prompt);
    // The two codes it cannot tell apart are not read.
    let read: Vec<(&str, bool)> = report
        .codes
        .iter()
        .map(|c| (c.code.as_str(), c.read))
        .collect();
    assert_eq!(read, [("%h", false), ("%m", false), ("%v", true)]);
}

#[test]
fn a_typed_setting_is_stored_as_the_game_stores_it() {
    let report = codes("[%h/%Hhp]", "", true);
    assert_eq!(report.prompt, "[%h/%Hhp] ");
    let stored = codes("[%h/%Hhp]", "", false);
    assert_eq!(stored.prompt, "[%h/%Hhp]");
}

#[test]
fn a_pattern_reports_what_it_fills_and_presets_from_what_is_supplied() {
    let report = regex(&[r"^<(?<hp>\d+)hp> $"], &|_| false);
    assert!(report.ok);
    assert_eq!(report.vars, ["hp"]);
    assert_eq!(report.shapes.len(), 1);
    assert!(report.shapes[0].settle);
    assert_eq!(report.shapes[0].kind, "line");
    let ids: Vec<&str> = report.presets.iter().map(|p| p.id).collect();
    assert_eq!(ids, ["minimal", "how_full", "detailed", "empty"]);
    assert_eq!(preset(&report, "minimal"), Some("%{hp}h > "));
    assert_eq!(preset(&report, "how_full"), Some("[%{hp}hp] "));

    // With GMCP vitals every pair and its max is supplied.
    let vitals = ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"];
    let full = regex(&[r"^<(?<hp>\d+)hp> $"], &|name| vitals.contains(&name));
    let ids: Vec<&str> = full.presets.iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        ["minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    for preset in &full.presets {
        assert_reads_clean(&preset.template);
    }
    assert_eq!(
        preset(&full, "bars"),
        Some("hp %hp_bar:6 mn %mana_bar:6 mv %move_bar:6 ")
    );

    // Nothing supplied leaves Start empty.
    let none = regex(&["^> $"], &|_| false);
    assert!(none.ok);
    assert_eq!(none.presets.len(), 1);
}

#[test]
fn a_pattern_that_does_not_read_says_so() {
    let bad = regex(&[r"\[(?<hp>\d+"], &|_| false);
    assert!(!bad.ok);
    assert_eq!(
        bad.error.map(|e| e.message),
        Some("Vosh cannot read that pattern.".to_string())
    );
    let two = regex(&["a", "b"], &|_| false);
    assert_eq!(
        two.error.map(|e| e.message),
        Some("Vosh reads your prompt from one line.".to_string())
    );
}

#[test]
fn a_line_another_game_prints_reports_its_numbers_and_the_names_it_reads() {
    let text = "<100hp 50m 30mv> ";
    let line = line_report(text, &[], &|_| false);
    assert!(line.ok);
    assert_eq!(line.shapes.len(), 1);
    assert_eq!(line.shapes[0].kind, "line");
    assert_eq!(
        line.shapes[0].lines,
        [r"^<(?<hp>-?\d+)hp +(?<mana>-?\d+)m +(?<move>-?\d+)mv> +$"]
    );
    assert!(line.shapes[0].settle);
    assert_eq!(line.vars, ["hp", "mana", "move"]);
    assert!(line.names.is_empty());
    let marks: Vec<(&str, &str, [usize; 2])> = line
        .numbers
        .iter()
        .map(|n| (n.name.as_str(), n.label.as_str(), n.span))
        .collect();
    assert_eq!(
        marks,
        [
            ("hp", "Health", [1, 4]),
            ("mana", "Mana", [7, 9]),
            ("move", "Moves", [11, 13])
        ]
    );
    assert!(preset(&line, "how_full").is_some());
    // Names you give, and a number you leave out.
    let named = line_report(text, &["health".into(), String::new()], &|_| false);
    assert_eq!(named.vars, ["health", "move"]);
    assert_eq!(named.numbers[1].name, "");
    assert_eq!(named.numbers[1].suggested, "mana");
    // A name no group can carry goes by the number of its group.
    let odd = line_report("1 2", &["9x".into()], &|_| false);
    assert_eq!(odd.names, BTreeMap::from([("1".into(), "9x".into())]));
    assert_eq!(odd.vars, ["9x", "n2"]);
    // The card reads the report as JSON.
    let json = serde_json::to_value(&line).expect("json");
    assert_eq!(
        json["numbers"][0],
        serde_json::json!({
            "span": [1, 4],
            "text": "100",
            "name": "hp",
            "suggested": "hp",
            "label": "Health",
            "max": false,
        })
    );
    assert_eq!(json["names"], serde_json::json!({}));
    // Codes have neither.
    let codes = codes(JAMES_PROMPT, "", false);
    assert!(codes.numbers.is_empty() && codes.names.is_empty());
    // A pattern reports the names it was handed.
    let pattern = report(
        &CompileRequest::Regex {
            lines: vec![r"^<(\d+)hp> $".into()],
            names: BTreeMap::from([("1".into(), "hp".into())]),
        },
        Who::default(),
        &|_| false,
    );
    assert_eq!(pattern.names, BTreeMap::from([("1".into(), "hp".into())]));
    assert_eq!(pattern.vars, ["hp"]);
}
