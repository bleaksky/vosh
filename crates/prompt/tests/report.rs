//! `prompt_compile` tests: what a capture compiles to, the codes and
//! warnings the card shows, and the designs it offers to start from.

use std::collections::BTreeMap;

use vosh_prompt::aabahran::{compile, Compiled, Origin, ShapeKind, WarningKind, Which, Who};
use vosh_prompt::capture::fills;
use vosh_prompt::config::RegexCapture;
use vosh_prompt::report::{line_report, report, CompileRequest};
use vosh_prompt::template::TokenKind;
use vosh_prompt::testkit::mud::PROMPT;
use vosh_prompt::Template;

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

/// The two settings compiled as the report compiles them, for the names
/// they read and the codes each shape prints, which the report leaves out
/// since the card never shows them.
fn stored(prompt: &str, fprompt: &str) -> Compiled {
    compile(prompt, fprompt, Origin::Stored, Who::default()).expect("the settings compile")
}

/// The names the numbers of a line read into, without the ones you left
/// out.
fn read_into(report: &vosh_prompt::report::CompileReport) -> Vec<String> {
    report
        .numbers
        .iter()
        .filter(|n| !n.name.is_empty())
        .map(|n| n.name.clone())
        .collect()
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
    let report = codes(PROMPT, "", false);
    assert!(report.ok);
    assert_eq!(report.error, None);
    assert_eq!(report.prompt, PROMPT);
    let compiled = stored(&report.prompt, &report.fprompt);
    assert_eq!(
        compiled.reads(Which::Prompt),
        ["hp", "maxhp", "mana", "maxmana", "move", "maxmove", "tank", "tank_bar"]
    );
    let kinds: Vec<(Which, ShapeKind)> =
        compiled.shapes.iter().map(|s| (s.which, s.kind)).collect();
    assert_eq!(
        kinds,
        [
            (Which::Prompt, ShapeKind::Normal),
            (Which::Prompt, ShapeKind::Tank),
            (Which::Prompt, ShapeKind::Afk),
        ]
    );
    assert_eq!(report.shapes.len(), 3);
    assert_eq!(report.shapes[1].lines.len(), 2);
    let leftover = &report.warnings;
    assert!(leftover.is_empty(), "{leftover:?}");
    // The tank's shape prints every code, and Vosh reads each one.
    let printed: Vec<(String, String, bool)> = compiled.shapes[1]
        .codes
        .iter()
        .map(|c| (c.code.written(), c.code.label(), c.read))
        .collect();
    let written: Vec<(&str, &str, bool)> = printed
        .iter()
        .map(|(code, label, read)| (code.as_str(), label.as_str(), *read))
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
    let health = report.legend.iter().find(|r| r.code == "%h").expect("%h");
    assert_eq!(health.span, [7, 9]);
    assert_eq!(health.which, Which::Prompt);
}

#[test]
fn same_as_the_game_writes_every_code_in_the_game_look() {
    let report = codes(PROMPT, "", false);
    assert_eq!(preset(&report, "game"), Some(SAME_AS_THE_GAME));
    let ids: Vec<&str> = report.presets.iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        ["default", "game", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    let labels: Vec<&str> = report.presets.iter().map(|p| p.label).collect();
    assert_eq!(
        labels,
        [
            "Vosh's default",
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
    let compiled = stored(&fight.prompt, &fight.fprompt);
    assert!(compiled.reads(Which::Fight).contains(&"tank_pct"));
}

#[test]
fn a_blinking_backtick_color_blinks_in_same_as_the_game() {
    // `q is the game's blinking red, ESC[0;5;31m.
    let report = codes("`qHP`` %h ", "", false);
    assert!(report.ok);
    let game = preset(&report, "game").expect("same as the game");
    assert!(game.contains("%c_reset%s_blink%{c:red}HP"), "{game}");
    assert_reads_clean(game);
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
    let leftover = &report.shapes;
    assert!(leftover.is_empty(), "{leftover:?}");
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
    let printed: Vec<(String, bool)> = stored(&report.prompt, &report.fprompt).shapes[0]
        .codes
        .iter()
        .map(|c| (c.code.written(), c.read))
        .collect();
    let read: Vec<(&str, bool)> = printed
        .iter()
        .map(|(code, read)| (code.as_str(), *read))
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
    let capture = RegexCapture {
        lines: vec![r"^<(?<hp>\d+)hp> $".into()],
        ..RegexCapture::default()
    };
    assert_eq!(fills(&capture), ["hp"]);
    assert_eq!(report.shapes.len(), 1);
    assert!(report.shapes[0].settle);
    assert_eq!(report.shapes[0].lines, [r"^<(?<hp>\d+)hp> $"]);
    let ids: Vec<&str> = report.presets.iter().map(|p| p.id).collect();
    assert_eq!(ids, ["default", "minimal", "how_full", "detailed", "empty"]);
    assert_eq!(
        preset(&report, "default"),
        Some(vosh_prompt::DEFAULT_DESIGN)
    );
    assert_eq!(preset(&report, "minimal"), Some("%{hp}h > "));
    assert_eq!(preset(&report, "how_full"), Some("[%{hp}hp] "));

    // With GMCP vitals every pair and its max is supplied.
    let vitals = ["hp", "maxhp", "mana", "maxmana", "move", "maxmove"];
    let full = regex(&[r"^<(?<hp>\d+)hp> $"], &|name| vitals.contains(&name));
    let ids: Vec<&str> = full.presets.iter().map(|p| p.id).collect();
    assert_eq!(
        ids,
        ["default", "minimal", "how_full", "percent", "bars", "detailed", "empty"]
    );
    for preset in &full.presets {
        assert_reads_clean(&preset.template);
    }
    assert_eq!(
        preset(&full, "bars"),
        Some("hp %hp_bar:6 mn %mana_bar:6 mv %move_bar:6 ")
    );

    // Nothing supplied leaves Vosh's default and Start empty.
    let none = regex(&["^> $"], &|_| false);
    assert!(none.ok);
    let ids: Vec<&str> = none.presets.iter().map(|p| p.id).collect();
    assert_eq!(ids, ["default", "empty"]);
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
    assert_eq!(
        line.shapes[0].lines,
        [r"^<(?<hp>-?\d+)hp +(?<mana>-?\d+)m +(?<move>-?\d+)mv> +$"]
    );
    assert!(line.shapes[0].settle);
    assert_eq!(read_into(&line), ["hp", "mana", "move"]);
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
    assert_eq!(read_into(&named), ["health", "move"]);
    assert_eq!(named.numbers[1].name, "");
    assert_eq!(named.numbers[1].suggested, "mana");
    // A name no group can carry goes by the number of its group.
    let odd = line_report("1 2", &["9x".into()], &|_| false);
    assert_eq!(odd.names, BTreeMap::from([("1".into(), "9x".into())]));
    assert_eq!(read_into(&odd), ["9x", "n2"]);
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
    let codes = codes(PROMPT, "", false);
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
    let capture = RegexCapture {
        lines: vec![r"^<(\d+)hp> $".into()],
        names: pattern.names.clone(),
        ..RegexCapture::default()
    };
    assert_eq!(fills(&capture), ["hp"]);
}

/// Each legend row as code, label, tag and whether it carries the warn
/// ring, for reading a test at a glance.
fn legend(
    report: &vosh_prompt::report::CompileReport,
) -> Vec<(String, String, Option<String>, bool)> {
    report
        .legend
        .iter()
        .map(|row| {
            (
                row.code.clone(),
                row.label.clone(),
                row.tag.clone(),
                row.warn,
            )
        })
        .collect()
}

fn row(code: &str, label: &str) -> (String, String, Option<String>, bool) {
    (code.to_string(), label.to_string(), None, false)
}

#[test]
fn the_legend_lists_every_code_and_line_end_in_the_order_the_game_prints_them() {
    let report = codes(PROMPT, "", false);
    assert_eq!(
        legend(&report),
        [
            row("%n", "Tank"),
            row("%P", "Tank health bar"),
            row("%C", "New line"),
            row("%h", "Health"),
            row("%H", "Max health"),
            row("%m", "Mana"),
            row("%M", "Max mana"),
            row("%v", "Moves"),
            row("%V", "Max moves"),
            row("%c", "New line"),
        ]
    );
    // The tank's codes print only in a fight, which the card tags.
    let fight: Vec<bool> = report.legend.iter().map(|r| r.fight).collect();
    assert_eq!(
        fight,
        [true, true, true, false, false, false, false, false, false, false]
    );
    assert!(report.legend.iter().all(|r| r.warning.is_none()));
    assert_eq!(report.legend[3].span, [7, 9]);
    assert_eq!(report.legend[3].which, Which::Prompt);
    assert_eq!(
        report.shows.as_deref(),
        Some("It shows Health, Mana, and Moves with their maxes, and your tank and its health in a fight.")
    );
    assert_eq!(report.fix_note, None);
    let leftover = &report.fixes;
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn codes_that_run_together_take_one_legend_row_and_a_command_that_fixes_them() {
    let report = codes("<%h%m %vmv> ", "", false);
    assert_eq!(
        legend(&report),
        [
            (
                "%h%m".to_string(),
                "Health and Mana".to_string(),
                Some("run together".to_string()),
                true
            ),
            row("%v", "Moves"),
        ]
    );
    assert_eq!(report.legend[0].span, [1, 5]);
    assert_eq!(report.shows, None);
    assert_eq!(
        report.fix_note.as_deref(),
        Some("Vosh reads Moves. Health and Mana come from the game until you fix the prompt.")
    );
    assert_eq!(report.fixes, ["prompt <%h %m %vmv>"]);
    // A fight prompt gets its own command.
    let fight = codes("<%hhp> ", "<%h%v> ", false);
    assert_eq!(fight.fixes, ["fprompt <%h %v>"]);
    assert_eq!(
        fight.fix_note.as_deref(),
        Some("Vosh reads Health. Moves comes from the game until you fix the prompt.")
    );
}

#[test]
fn other_warnings_take_a_legend_row_with_their_sentence() {
    let twice = codes("<%hhp %hhp> ", "", false);
    let second = twice
        .legend
        .iter()
        .find(|r| r.warn)
        .expect("the second use");
    assert_eq!(second.code, "%h");
    assert_eq!(second.tag.as_deref(), Some("second use"));
    assert_eq!(
        second.warning.as_deref(),
        Some("Your prompt shows Health twice. Vosh reads the first one.")
    );
    assert_eq!(second.span, [6, 8]);

    let pacify = codes("<%hhp %u> ", "", false);
    let row = pacify.legend.iter().find(|r| r.code == "%u").expect("%u");
    assert_eq!((row.tag.as_deref(), row.warn), (Some("immortal"), true));
    assert!(row
        .warning
        .as_deref()
        .unwrap()
        .starts_with("Only immortals get a value for %u."));

    let short = codes("> ", "", false);
    assert_eq!(
        legend(&short),
        [(
            "> ".to_string(),
            "A prompt with no values".to_string(),
            None,
            true
        )]
    );
    assert_eq!(
        short.legend[0].warning.as_deref(),
        Some(
            "This prompt is short enough to match other lines. Vosh can draw over them by mistake."
        )
    );
    assert_eq!(short.shows, None);

    let lone = codes("<%hhp>%", "", true);
    let row = lone
        .legend
        .iter()
        .find(|r| r.code == "%")
        .expect("the lone %");
    assert_eq!(row.label, "A lone %");
    assert_eq!((row.tag.as_deref(), row.warn), (Some("at the end"), true));
}

#[test]
fn the_card_says_what_a_prompt_shows_in_one_sentence() {
    let shows = |prompt: &str| codes(prompt, "", false).shows;
    assert_eq!(
        shows("<%hhp %mm %vmv> ").as_deref(),
        Some("It shows Health, Mana, and Moves.")
    );
    assert_eq!(
        shows("<%h/%Hhp> ").as_deref(),
        Some("It shows Health with its max.")
    );
    assert_eq!(
        shows("<%h/%Hhp %ggold %Xtnl> ").as_deref(),
        Some("It shows Health with its max, Gold, and To next level.")
    );
    assert_eq!(
        shows("%n%C<%hhp> ").as_deref(),
        Some("It shows Health and your tank in a fight.")
    );
    assert_eq!(
        shows("%p%C<%hhp> ").as_deref(),
        Some("It shows Health and your tank's health in a fight.")
    );
    assert_eq!(shows("<%ggold> ").as_deref(), Some("It shows Gold."));
}

/// The game cuts its percent codes (%K, %k and %E), so Same as the game
/// writes the form that rounds down the same way.
#[test]
fn same_as_the_game_rounds_percents_down_as_the_game_does() {
    let game = vosh_prompt::presets::same_as_the_game(
        "%h (%K) %m (%k) %v (%E)",
        "",
        vosh_prompt::aabahran::Who::default(),
    )
    .expect("the setting reads");
    for name in ["hp", "mana", "move"] {
        assert!(game.contains(&format!("%{{{name}:pct:game}}")), "{game}");
    }
}
