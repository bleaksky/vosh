//! The Aabahran PROMPT compiler against the game's own output: James's
//! PROMPT compiling to the patterns the game prints, one hand case per
//! code, the shapes and their settle flags, the warnings with their copy,
//! and the compile error.
//!
//! Character names in these lines are placeholders, so `Ilsabet` reads
//! `Tester`.

use std::collections::BTreeMap;

use vosh_prompt::aabahran::lex::PROMPT_ALL;
use vosh_prompt::aabahran::{
    compile, Compiled, Origin, Shape, ShapeKind, Warning, WarningKind, Which, Who,
};
use vosh_prompt::testkit::mud::PROMPT;
use vosh_prompt::{Capture, Vars};

/// The Normal shape.
const NORMAL: &str = r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?\[(?<hp>-?\d+)/(?<maxhp>\d+)hp (?<mana>-?\d+)/(?<maxmana>\d+)mn (?<move>-?\d+)/(?<maxmove>\d+)mv\] *$";

/// The Tank shape, with `%P` carrying its brackets.
const TANK: [&str; 2] = [
    r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?(?<tank>.+?): (?:\[(?<tank_bar>[=-]{3}(?:\|[=-]{3}){3})\])? *$",
    r"^\[(?<hp>-?\d+)/(?<maxhp>\d+)hp (?<mana>-?\d+)/(?<maxmana>\d+)mn (?<move>-?\d+)/(?<maxmove>\d+)mv\] *$",
];

const IMMORTAL: Who = Who {
    immortal: true,
    mobile: false,
    keeps_backticks: false,
};

fn stored(prompt: &str, fprompt: &str) -> Compiled {
    compile(prompt, fprompt, Origin::Stored, Who::default())
        .unwrap_or_else(|e| panic!("{prompt:?} {fprompt:?}: {e}"))
}

fn shape(compiled: &Compiled, which: Which, kind: ShapeKind) -> &Shape {
    compiled
        .shapes
        .iter()
        .find(|s| s.which == which && s.kind == kind)
        .unwrap_or_else(|| panic!("no {which:?} {kind:?} shape"))
}

fn patterns(shape: &Shape) -> Vec<&str> {
    shape.lines.iter().map(|l| l.line.as_str()).collect()
}

/// The values a read gives, with the empty ones left out.
fn values(read: Option<vosh_prompt::capture::Recognized>) -> BTreeMap<String, String> {
    read.expect("the shape reads the lines")
        .values
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn james_prompt_compiles_to_the_worked_patterns() {
    let compiled = stored(PROMPT, "");
    assert_eq!(compiled.prompt, PROMPT);
    assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
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
    let normal = shape(&compiled, Which::Prompt, ShapeKind::Normal);
    assert_eq!(patterns(normal), [NORMAL]);
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    assert_eq!(patterns(tank), TANK);
    // Every line ends in %c, so nothing is left open and nothing
    // settles.
    for shape in [normal, tank] {
        assert!(shape.lines.iter().all(|l| l.partial.is_none()));
        assert!(!shape.settle);
    }
    assert_eq!(
        compiled.reads(Which::Prompt),
        ["hp", "maxhp", "mana", "maxmana", "move", "maxmove", "tank", "tank_bar"]
    );
}

#[test]
fn james_prompt_reads_his_lines() {
    let compiled = stored(PROMPT, "");
    let normal = shape(&compiled, Which::Prompt, ShapeKind::Normal);
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    // Out of a fight, with the immortal prefix.
    assert_eq!(
        values(normal.read(&["(Wizi 52) (Incog 52) [329/329hp 9999/9999mn 9999/9999mv]"])),
        map(&[
            ("wizi", "52"),
            ("incog", "52"),
            ("hp", "329"),
            ("maxhp", "329"),
            ("mana", "9999"),
            ("maxmana", "9999"),
            ("move", "9999"),
            ("maxmove", "9999"),
        ])
    );
    // Tanking, the bar red below 25 percent.
    let block = [
        "(Wizi 60) (Incog 60) Tester: [===|---|---|---]",
        "[1020/1020hp 800/800mn 930/930mv]",
    ];
    assert_eq!(
        values(tank.read(&block)),
        map(&[
            ("wizi", "60"),
            ("incog", "60"),
            ("tank", "Tester"),
            ("tank_bar", "===|---|---|---"),
            ("hp", "1020"),
            ("maxhp", "1020"),
            ("mana", "800"),
            ("maxmana", "800"),
            ("move", "930"),
            ("maxmove", "930"),
        ])
    );
    // The head line alone starts the Tank shape and is no Normal prompt.
    assert!(tank.lines[0].line.is_match(block[0]));
    assert!(normal.read(&block[..1]).is_none());
    // The last line alone reads as Normal.
    assert!(normal.read(&block[1..]).is_some());
}

#[test]
fn the_lament_pair_reads_and_hides_everything_it_zeroed() {
    let compiled = stored(PROMPT, "");
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    let read = tank
        .read(&["Tester: ", "[0/0hp 0/0mn 0/0mv]"])
        .expect("the lament pair");
    assert_eq!(read.values["tank"], "Tester");
    assert_eq!(read.values["tank_bar"], "");
    assert_eq!(read.values["maxhp"], "0");
    let mut vars = Vars::new(true);
    vars.capture(Capture {
        values: read.values,
        raw: None,
    });
    let hidden = vars.hidden();
    assert!(hidden.hp && hidden.mana && hidden.moves, "H1: {hidden:?}");
    assert!(hidden.tank, "H2: {hidden:?}");
}

#[test]
fn prompt_all_settles_and_reads_as_a_partial_or_a_line() {
    let compiled = compile("all", "", Origin::Typed, Who::default()).unwrap();
    assert_eq!(compiled.prompt, PROMPT_ALL);
    let normal = shape(&compiled, Which::Prompt, ShapeKind::Normal);
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    assert!(normal.settle && tank.settle);
    let partial = normal.lines[0].partial.as_ref().expect("a partial");
    assert_eq!(
        partial.as_str(),
        r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?<(?<hp>-?\d+)hp (?<mana>-?\d+)m (?<move>-?\d+)mv> +$"
    );
    let want = map(&[("hp", "159"), ("mana", "310"), ("move", "489")]);
    assert_eq!(values(normal.read_partial(&["<159hp 310m 489mv> "])), want);
    // A read split before the final space waits.
    assert!(normal.read_partial(&["<159hp 310m 489mv>"]).is_none());
    assert!(normal.read_partial(&["<159hp 310"]).is_none());
    // The next pulse's line end completes it in the same read.
    assert_eq!(values(normal.read(&["<159hp 310m 489mv> "])), want);
    assert_eq!(values(normal.read(&["<159hp 310m 489mv>"])), want);
    // A reply that lands on the prompt row is no prompt.
    assert!(normal
        .read(&["<32hp 310m 489mv> You flee east from combat!"])
        .is_none());
    assert!(normal
        .read_partial(&["<32hp 310m 489mv> You flee east from combat!"])
        .is_none());
    // Tanking, the vitals follow the tank line as a partial.
    assert_eq!(
        values(tank.read_partial(&["Tester: [===|===|===|---]", "<159hp 310m 489mv> "])),
        map(&[
            ("tank", "Tester"),
            ("tank_bar", "===|===|===|---"),
            ("hp", "159"),
            ("mana", "310"),
            ("move", "489"),
        ])
    );
}

#[test]
fn a_typed_prompt_with_spaces_around_it_reads_what_the_game_prints() {
    // You type `prompt  <%hhp %mm %vmv> `. The game stores the setting
    // without the spaces before it and prints `<1020hp 800m 930mv> `.
    let compiled = compile(" <%hhp %mm %vmv> ", "", Origin::Typed, Who::default()).unwrap();
    assert_eq!(compiled.prompt, "<%hhp %mm %vmv> ");
    let either = shape(&compiled, Which::Prompt, ShapeKind::Either);
    assert_eq!(
        values(either.read_partial(&["<1020hp 800m 930mv> "])),
        map(&[("hp", "1020"), ("mana", "800"), ("move", "930")])
    );
    let compiled = compile(" all ", " off ", Origin::Typed, Who::default()).unwrap();
    assert_eq!(
        (compiled.prompt.as_str(), compiled.fprompt.as_str()),
        (PROMPT_ALL, "")
    );
}

#[test]
fn a_mortal_typing_backticks_reads_what_the_game_prints() {
    // A mortal types `prompt `(240)[%h/%Hhp]`. The game drops `( on the
    // line, stores `240)[%h/%Hhp] ` and prints `240)[1020/1020hp] `.
    let compiled = compile("`(240)[%h/%Hhp]", "", Origin::Typed, Who::default()).unwrap();
    assert_eq!(compiled.prompt, "240)[%h/%Hhp] ");
    let either = shape(&compiled, Which::Prompt, ShapeKind::Either);
    assert_eq!(
        values(either.read_partial(&["240)[1020/1020hp] "])),
        map(&[("hp", "1020"), ("maxhp", "1020")])
    );
    // Trust 55 keeps the color, which prints nothing.
    let trusted = Who {
        keeps_backticks: true,
        ..Who::default()
    };
    let compiled = compile("`(240)[%h/%Hhp]", "", Origin::Typed, trusted).unwrap();
    assert_eq!(compiled.prompt, "`(240)[%h/%Hhp] ");
    let either = shape(&compiled, Which::Prompt, ShapeKind::Either);
    assert!(either.read_partial(&["[1020/1020hp] "]).is_some());
}

#[test]
fn an_empty_prompt_reads_the_fallback() {
    for compiled in [
        stored("", ""),
        compile("   ", "", Origin::Typed, Who::default()).unwrap(),
    ] {
        assert_eq!(compiled.prompt, "");
        let kinds: Vec<ShapeKind> = compiled.shapes.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [ShapeKind::Fallback, ShapeKind::Afk]);
        let fallback = &compiled.shapes[0];
        assert!(fallback.settle);
        assert_eq!(
            fallback.lines[0].partial.as_ref().unwrap().as_str(),
            r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?<(?<hp>-?\d+)hp (?<mana>-?\d+)m (?<move>-?\d+)mv>.* $"
        );
        let want = map(&[("hp", "1020"), ("mana", "800"), ("move", "930")]);
        assert_eq!(
            values(fallback.read_partial(&["<1020hp 800m 930mv> "])),
            want
        );
        // Your prefix setting follows it.
        assert_eq!(
            values(fallback.read_partial(&["<1020hp 800m 930mv>tell bob "])),
            want
        );
        // The new build prints zeros under lamented tears.
        assert!(fallback.read_partial(&["<0hp 0m 0mv> "]).is_some());
    }
}

#[test]
fn away_reads_as_its_own_shape() {
    let compiled = stored(PROMPT, "");
    let afk = shape(&compiled, Which::Prompt, ShapeKind::Afk);
    assert!(afk.settle);
    assert_eq!(
        afk.lines[0].partial.as_ref().unwrap().as_str(),
        r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?<AFK> $"
    );
    assert_eq!(
        values(afk.read_partial(&["(Wizi 60) (Incog 60) <AFK> "])),
        map(&[("wizi", "60"), ("incog", "60"), ("afk", "1")])
    );
    assert_eq!(values(afk.read(&["<AFK>"])), map(&[("afk", "1")]));
    assert!(afk.read_partial(&["<AFK>"]).is_none());
}

#[test]
fn the_game_keeps_what_fits_on_the_line_and_so_does_vosh() {
    // The game reads 253 characters of `prompt ` and the setting.
    let typed = format!("{}<%h>", "x".repeat(300));
    let compiled = compile(&typed, "", Origin::Typed, Who::default()).unwrap();
    assert_eq!(compiled.prompt, format!("{} ", "x".repeat(246)));
    assert_eq!(
        compiled.warnings,
        [Warning {
            kind: WarningKind::Cut,
            which: Which::Prompt,
            span: 246..246,
            text:
                "The game keeps the first 246 characters of your prompt. Vosh reads the same 246."
                    .into(),
        }]
    );
    // What is left reads nothing, and the code past the cut is gone.
    let leftover = &compiled.reads(Which::Prompt);
    assert!(leftover.is_empty(), "{leftover:?}");
    // A 250 character setting loses its last 4, as "Line too long." does
    // in the game, so Vosh reads what the game prints.
    let typed = format!("<%hhp>{}", "z".repeat(244));
    let compiled = compile(&typed, "", Origin::Typed, Who::default()).unwrap();
    let either = shape(&compiled, Which::Prompt, ShapeKind::Either);
    let printed = format!("<1020hp>{} ", "z".repeat(240));
    assert_eq!(
        values(either.read_partial(&[printed.as_str()])),
        map(&[("hp", "1020")])
    );
}

/// One hand case per code: the setting, a line the game prints for it,
/// and the value Vosh reads. The line ends in `%c`, so it reads whole.
#[test]
fn every_code_reads_what_the_game_prints() {
    let cases: &[(&str, &str, &str, &str)] = &[
        ("<%h>%c", "<-12>", "hp", "-12"),
        ("<%H>%c", "<1020>", "maxhp", "1020"),
        ("<%m>%c", "<800>", "mana", "800"),
        ("<%M>%c", "<800>", "maxmana", "800"),
        ("<%v>%c", "<930>", "move", "930"),
        ("<%V>%c", "<930>", "maxmove", "930"),
        ("<%K>%c", "<75>", "hp_pct", "75"),
        ("<%k>%c", "<-3>", "mana_pct", "-3"),
        ("<%E>%c", "<100>", "move_pct", "100"),
        ("<%a>%c", "<-5>", "cp", "-5"),
        ("<%A>%c", "<12>", "rp", "12"),
        ("<%g>%c", "<1250>", "gold", "1250"),
        ("<%x>%c", "<-40>", "exp", "-40"),
        ("<%X>%c", "<-2>", "tnl", "-2"),
        ("<%t>%c", "<23>", "hour", "23"),
        ("<%w>%c", "<-3>", "temp", "-3"),
        ("<%W>%c", "<error!>", "weather", "error!"),
        ("<%G>%c", "<Coastal South>", "region", "Coastal South"),
        ("<%s>%c", "<thsu'ul>", "lang", "thsu'ul"),
        ("<%S>%c", "<fgt>", "pos", "fgt"),
        ("<%i>%c", "<D>", "stallion", "D"),
        ("<%e>%c", "<[Exits: N (E) S]>", "exits", " N (E) S"),
        ("<%f1>%c", "<~>", "slot1", "~"),
        ("<%f0>%c", "<12>", "slot10", "12"),
        ("<%j1>%c", "<FUL>", "moon1", "FUL"),
        ("<%j3>%c", "<->", "moon3", "-"),
        ("<%n>%c", "<Tester: >", "tank", "Tester"),
        ("<%p>%c", "<[45]>", "tank_pct", "45"),
        (
            "<%P>%c",
            "<[===|===|---|---]>",
            "tank_bar",
            "===|===|---|---",
        ),
        (
            "<%r>%c",
            "<The Bank of Aabahran>",
            "room",
            "The Bank of Aabahran",
        ),
        ("<%R>%c", "<3001>", "room_num", "3001"),
        ("<%z>%c", "<Aabahran>", "area", "Aabahran"),
        ("<%b>%c", "<12>", "area_num", "12"),
        ("<%o>%c", "<MPEdit>", "olc", "MPEdit"),
        ("<%O>%c", "<1200>", "olc_vnum", "1200"),
        ("<%u>%c", "<not pacified>", "pacify", "not pacified"),
    ];
    for (setting, line, name, value) in cases {
        let compiled = compile(setting, "", Origin::Stored, IMMORTAL).unwrap();
        assert!(
            compiled.warnings.is_empty(),
            "{setting}: {:?}",
            compiled.warnings
        );
        let read = compiled.shapes[0]
            .read(&[line])
            .unwrap_or_else(|| panic!("{setting} reads {line}"));
        assert_eq!(read.values[*name], *value, "{setting} on {line}");
    }
}

#[test]
fn codes_that_print_nothing_read_empty() {
    let immortal = |setting: &str| compile(setting, "", Origin::Stored, IMMORTAL).unwrap();
    // Meditating, out of a tank fight, a mortal's room codes, and no
    // editor open.
    for (setting, name) in [
        ("<%S>%c", "pos"),
        ("<%n>%c", "tank"),
        ("<%p>%c", "tank_pct"),
        ("<%P>%c", "tank_bar"),
        ("<%r>%c", "room"),
        ("<%R>%c", "room_num"),
        ("<%o>%c", "olc"),
        ("<%O>%c", "olc_vnum"),
    ] {
        let read = immortal(setting).shapes[0].read(&["<>"]);
        assert_eq!(
            read.map(|r| r.values[name].clone()).as_deref(),
            Some(""),
            "{setting}"
        );
    }
    // A moon digit that names no moon always prints - and reads nothing.
    let compiled = immortal("<%j5>%c");
    assert!(compiled.shapes[0].read(&["<->"]).is_some());
    let leftover = &compiled.reads(Which::Prompt);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_fight_prompt_compiles_its_own_shapes() {
    // The fight prompt in the Char.Prompt fixture, color codes kept.
    let compiled = stored(PROMPT, "`1%h``hp [%p] > ");
    let kinds: Vec<(Which, ShapeKind)> =
        compiled.shapes.iter().map(|s| (s.which, s.kind)).collect();
    assert_eq!(
        kinds,
        [
            (Which::Prompt, ShapeKind::Normal),
            (Which::Prompt, ShapeKind::Tank),
            (Which::Fight, ShapeKind::Either),
            (Which::Prompt, ShapeKind::Afk),
        ]
    );
    let fight = shape(&compiled, Which::Fight, ShapeKind::Either);
    assert!(fight.settle);
    // %p prints its own brackets inside the ones you wrote, and nothing
    // when no one in your group tanks.
    assert_eq!(
        values(fight.read_partial(&["159hp [[45]] > "])),
        map(&[("hp", "159"), ("tank_pct", "45")])
    );
    assert_eq!(
        values(fight.read_partial(&["159hp [] > "])),
        map(&[("hp", "159")])
    );
    assert_eq!(compiled.reads(Which::Fight), ["hp", "tank_pct"]);

    // A fight prompt with %C compiles the pair.
    let compiled = stored("<%hhp> ", "%n%p%C<%hhp %mm> ");
    let fight: Vec<ShapeKind> = compiled
        .shapes
        .iter()
        .filter(|s| s.which == Which::Fight)
        .map(|s| s.kind)
        .collect();
    assert_eq!(fight, [ShapeKind::Normal, ShapeKind::Tank]);
    let tank = shape(&compiled, Which::Fight, ShapeKind::Tank);
    assert_eq!(
        values(tank.read_partial(&["Tester: [45]", "<10hp 20m> "])),
        map(&[
            ("tank", "Tester"),
            ("tank_pct", "45"),
            ("hp", "10"),
            ("mana", "20")
        ])
    );
    // A fight prompt the game has not set adds nothing.
    assert_eq!(stored("<%hhp> ", "").shapes.len(), 2);
}

#[test]
fn tank_codes_that_change_no_line_merge_into_one_shape() {
    let compiled = stored("%n%p<%hhp> ", "");
    let prompt: Vec<&Shape> = compiled
        .shapes
        .iter()
        .filter(|s| s.kind != ShapeKind::Afk)
        .collect();
    assert_eq!(prompt.len(), 1);
    let either = prompt[0];
    assert_eq!(either.kind, ShapeKind::Either);
    assert_eq!(
        values(either.read_partial(&["<10hp> "])),
        map(&[("hp", "10")])
    );
    assert_eq!(
        values(either.read_partial(&["Tester: [50]<10hp> "])),
        map(&[("tank", "Tester"), ("tank_pct", "50"), ("hp", "10")])
    );
    // Lamented tears blanks %p, and the name stays.
    assert_eq!(
        values(either.read_partial(&["Tester: <0hp> "])),
        map(&[("tank", "Tester"), ("hp", "0")])
    );

    // %C changes the lines, so the two stay apart.
    let compiled = stored("[%h]%C<%m> ", "");
    assert_eq!(
        patterns(shape(&compiled, Which::Prompt, ShapeKind::Normal)).len(),
        1
    );
    assert_eq!(
        patterns(shape(&compiled, Which::Prompt, ShapeKind::Tank)).len(),
        2
    );
}

#[test]
fn each_shape_settles_only_when_it_ends_in_a_character_you_wrote() {
    // Ends in %c: nothing left open.
    let compiled = stored(PROMPT, "");
    assert!(compiled
        .shapes
        .iter()
        .filter(|s| s.kind != ShapeKind::Afk)
        .all(|s| !s.settle));
    // Ends in the space the game adds.
    assert!(stored("<%hhp> ", "").shapes[0].settle);
    // %C at the end: the Normal line is open and ends in ], the Tank one
    // ends in a line end.
    let compiled = stored("[%h]%C", "");
    let normal = shape(&compiled, Which::Prompt, ShapeKind::Normal);
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    assert!(normal.settle);
    assert_eq!(
        normal.lines[0].partial.as_ref().unwrap().as_str(),
        r"^(?:\(Wizi (?<wizi>\d+)\) )?(?:\(Incog (?<incog>\d+)\) )?\[(?<hp>-?\d+)\]$"
    );
    assert!(!tank.settle);
    assert!(tank.lines[0].partial.is_none());
    // A stored setting that ends in a code waits for a line end.
    let compiled = stored("[%h] %m", "");
    assert!(!compiled.shapes[0].settle);
    assert!(compiled.shapes[0].read_partial(&["[1] 2"]).is_none());
    assert!(compiled.shapes[0].read(&["[1] 2"]).is_some());
    // Trailing spaces in a partial read as one or more, and colors among
    // them take no cell.
    let compiled = stored("<%h>`1  ``", "");
    let shape = &compiled.shapes[0];
    assert!(shape.settle);
    assert!(shape.read_partial(&["<5> "]).is_some());
    assert!(shape.read_partial(&["<5>   "]).is_some());
    assert!(shape.read_partial(&["<5>"]).is_none());
}

#[test]
fn lines_split_at_each_break_and_only_the_first_has_the_prefix() {
    let compiled = stored("%t (%j1 %j2 %j3) %c<%hhp> ", "");
    let shape = &compiled.shapes[0];
    assert_eq!(shape.kind, ShapeKind::Either);
    assert_eq!(shape.lines.len(), 2);
    assert!(shape.lines[0].line.as_str().contains("wizi"));
    assert!(!shape.lines[1].line.as_str().contains("wizi"));
    assert_eq!(
        values(shape.read_partial(&["(Wizi 60) 14 (FUL - wan) ", "<10hp> "])),
        map(&[
            ("wizi", "60"),
            ("hour", "14"),
            ("moon1", "FUL"),
            ("moon2", "-"),
            ("moon3", "wan"),
            ("hp", "10"),
        ])
    );
}

#[test]
fn colors_take_no_cell() {
    // `%l(101)blah%L` and `%l2%L` from James's log, around codes.
    let compiled = stored("%l(101)blah%L <%l2%h%L> ", "");
    let leftover = &compiled.warnings;
    assert!(leftover.is_empty(), "{leftover:?}");
    assert_eq!(
        values(compiled.shapes[0].read_partial(&["blah <10> "])),
        map(&[("hp", "10")])
    );
    // `- and `= print a tilde and a backtick.
    let compiled = stored("`-%h`= ", "");
    assert!(compiled.shapes[0].read_partial(&["~10` "]).is_some());
}

fn warnings(setting: &str, who: Who) -> Vec<(WarningKind, std::ops::Range<usize>, String)> {
    compile(setting, "", Origin::Stored, who)
        .unwrap()
        .warnings
        .into_iter()
        .map(|w| (w.kind, w.span, w.text))
        .collect()
}

#[test]
fn codes_run_together_and_neither_reads() {
    // A healer's prompt.
    let compiled = stored("<%h%m %vmv> ", "");
    assert_eq!(
        compiled
            .warnings
            .iter()
            .map(|w| (w.kind, w.span.clone(), w.text.as_str()))
            .collect::<Vec<_>>(),
        [(
            WarningKind::RunTogether,
            1..5,
            "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
        )]
    );
    assert_eq!(compiled.reads(Which::Prompt), ["move"]);
    // It still recognizes the prompt.
    assert_eq!(
        values(compiled.shapes[0].read_partial(&["<1020800 930mv> "])),
        map(&[("move", "930")])
    );
    assert_eq!(
        warnings("Room %r%z here%c", IMMORTAL),
        [(
            WarningKind::RunTogether,
            5..9,
            "Vosh cannot tell where Room ends and Area begins. Put a space between them in the game."
                .into()
        )]
    );
    // A color between them takes no cell, so they still run together.
    assert_eq!(warnings("%h%l1%m ", Who::default())[0].1, 0..7);
    // A code that can print nothing lets the ones around it meet.
    assert_eq!(
        warnings("%h%S%m ", Who::default())[0].2,
        "Vosh cannot tell where Health ends and Mana begins. Put a space between them in the game."
    );
    // Codes whose edges tell them apart do not warn.
    for setting in [
        "%h%S ",
        "%n%P%C%h%c",
        "%i%s ",
        "%j1%j2%j3 ",
        "%t%j1 ",
        "%e%h ",
    ] {
        assert!(warnings(setting, Who::default()).is_empty(), "{setting}");
    }
}

#[test]
fn a_short_prompt_warns() {
    let short = (
        WarningKind::Short,
        0..2,
        "This prompt is short enough to match other lines. Vosh can draw over them by mistake."
            .to_string(),
    );
    assert_eq!(warnings("> ", Who::default()), std::slice::from_ref(&short));
    // The Normal shape of a tank prompt around it is as short.
    assert_eq!(
        warnings("%n%C> ", Who::default()),
        [(WarningKind::Short, 0..6, short.2.clone())]
    );
    let leftover = &warnings("none ", Who::default());
    assert!(leftover.is_empty(), "{leftover:?}");
    let leftover = &warnings("%h ", Who::default());
    assert!(leftover.is_empty(), "{leftover:?}");
    // Codes Vosh cannot read give it nothing to tell the prompt by.
    let kinds: Vec<WarningKind> = warnings("%h%m%c", Who::default())
        .into_iter()
        .map(|w| w.0)
        .collect();
    assert_eq!(kinds, [WarningKind::RunTogether, WarningKind::Short]);
}

#[test]
fn stale_codes_warn_and_read_nothing() {
    let mortal = compile("<%u> %h ", "", Origin::Stored, Who::default()).unwrap();
    assert_eq!(
        mortal.warnings.iter().map(|w| (w.kind, w.span.clone(), w.text.as_str())).collect::<Vec<_>>(),
        [(
            WarningKind::PacifyMortal,
            1..3,
            "Only immortals get a value for %u. For anyone else the game repeats the text of the code before it, so Vosh cannot read this part."
        )]
    );
    assert_eq!(mortal.reads(Which::Prompt), ["hp"]);
    // Whatever the game repeats there, the prompt still reads.
    assert!(mortal.shapes[0].read_partial(&["<159> 159 "]).is_some());

    let mobile = Who {
        mobile: true,
        ..Who::default()
    };
    let compiled = compile("<lang %s> ", "", Origin::Stored, mobile).unwrap();
    assert_eq!(
        compiled
            .warnings
            .iter()
            .map(|w| (w.kind, w.text.as_str()))
            .collect::<Vec<_>>(),
        [(
            WarningKind::LangMobile,
            "While you control a mobile, %s repeats the text of the code before it."
        )]
    );
    let leftover = &compiled.reads(Which::Prompt);
    assert!(leftover.is_empty(), "{leftover:?}");
}

#[test]
fn a_code_twice_reads_the_first() {
    let compiled = stored("%h %h ", "");
    assert_eq!(
        compiled
            .warnings
            .iter()
            .map(|w| (w.kind, w.span.clone(), w.text.as_str()))
            .collect::<Vec<_>>(),
        [(
            WarningKind::Twice,
            3..5,
            "Your prompt shows Health twice. Vosh reads the first one."
        )]
    );
    assert_eq!(
        values(compiled.shapes[0].read_partial(&["1 2 "])),
        map(&[("hp", "1")])
    );
    // Across lines too.
    let compiled = stored("%f1%c%f1 ", "");
    assert_eq!(
        compiled.warnings[0].text,
        "Your prompt shows Affect slot 1 twice. Vosh reads the first one."
    );
}

#[test]
fn a_lone_percent_at_the_end_warns() {
    let compiled = compile("<%h>%", "", Origin::Typed, Who::default()).unwrap();
    assert_eq!(compiled.prompt, "<%h>% ");
    assert_eq!(
        compiled.warnings.iter().map(|w| (w.kind, w.span.clone(), w.text.as_str())).collect::<Vec<_>>(),
        [(
            WarningKind::LonePercent,
            4..6,
            "Your prompt ends in a lone %, which swallows the space the game adds. Remove it in the game."
        )]
    );
    // The game prints <5> with no space after it, which settles on >.
    let shape = &compiled.shapes[0];
    assert!(shape.settle);
    assert!(shape.read_partial(&["<5>"]).is_some());
}

#[test]
fn a_color_that_runs_into_a_code_does_not_compile() {
    let err = compile("<`%h> ", "", Origin::Stored, Who::default()).unwrap_err();
    assert_eq!(err.which, Which::Prompt);
    assert_eq!(err.code, "%h");
    assert_eq!(err.span, 1..4);
    assert_eq!(
        err.to_string(),
        "A color code runs into %h. Put a space between them in the game."
    );
    let err = compile("<%h> ", "`(2%f1 ", Origin::Stored, Who::default()).unwrap_err();
    assert_eq!(err.which, Which::Fight);
    assert_eq!(
        err.text,
        "A color code runs into %f1. Put a space between them in the game."
    );
    // In a shape where %C prints nothing, the runs on each side of it
    // join, and a backtick before it runs into the code after it.
    assert!(compile("<`%C%h> ", "", Origin::Stored, Who::default()).is_err());
}

#[test]
fn a_backtick_that_takes_a_bracket_prints_nothing() {
    // An immortal keeps the backtick in `<`%e> `. The game prints
    // `<[Exits: N (E) S]> ` in every state, as with no backtick.
    let compiled = stored("<`%e> ", "");
    let either = shape(&compiled, Which::Prompt, ShapeKind::Either);
    assert_eq!(
        values(either.read_partial(&["<[Exits: N (E) S]> "])),
        map(&[("exits", " N (E) S")])
    );
    // %P prints nothing without a tank and under lament, and then the
    // backtick takes the ] after it, which prints as it is.
    let compiled = stored("%n%C[`%P] <%hhp> ", "");
    let tank = shape(&compiled, Which::Prompt, ShapeKind::Tank);
    assert_eq!(
        values(tank.read_partial(&["Tester: ", "[[===|===|---|---]] <159hp> "]))["tank_bar"],
        "===|===|---|---"
    );
    assert!(tank.read_partial(&["Tester: ", "[] <159hp> "]).is_some());
    let normal = shape(&compiled, Which::Prompt, ShapeKind::Normal);
    assert!(normal.read_partial(&["[] <1020hp> "]).is_some());
    // Under lament the backtick would take the 1 after %P as a color.
    let err = compile("%n%C`%P1 <%hhp> ", "", Origin::Stored, Who::default()).unwrap_err();
    assert_eq!(err.code, "%P");
}

#[test]
fn other_lines_are_no_prompt() {
    let compiled = stored(PROMPT, "");
    let all = stored(PROMPT_ALL, "");
    let lines = [
        // Chat quoting a prompt.
        "Tester says '[1020/1020hp 800/800mn 930/930mv]'",
        "You tell Tester '<159hp 310m 489mv> '",
        // A score block.
        "Health: 1020/1020  Mana: 800/800  Moves: 930/930",
        // The pager and the editor.
        "[Hit Return to continue]",
        "> ",
        "",
    ];
    for compiled in [&compiled, &all] {
        for shape in &compiled.shapes {
            for line in lines {
                let one = [line];
                assert!(shape.read(&one).is_none(), "{:?} read {line:?}", shape.kind);
                assert!(
                    shape.read_partial(&one).is_none(),
                    "{:?} read {line:?}",
                    shape.kind
                );
            }
        }
    }
}

/// The Char.Prompt fixtures, settings as the game stores them.
const CHAR_PROMPT: [&str; 3] = [
    include_str!("../../../fixtures/gmcp/aabahran/char-prompt.gmcp"),
    include_str!("../../../fixtures/gmcp/aabahran/char-prompt-off.gmcp"),
    include_str!("../../../fixtures/gmcp/aabahran/char-prompt-fight.gmcp"),
];

#[test]
fn every_char_prompt_fixture_compiles_as_sent() {
    for file in CHAR_PROMPT {
        let msg = vosh_protocol::gmcp::parse(file.trim_end().as_bytes()).expect("a packet");
        let prompt = msg.data["prompt"].as_str().expect("prompt");
        let fprompt = msg.data["fprompt"].as_str().expect("fprompt");
        let compiled = compile(prompt, fprompt, Origin::Stored, Who::default())
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(compiled.prompt, prompt);
        assert_eq!(compiled.fprompt, fprompt);
        assert!(
            compiled.warnings.is_empty(),
            "{file}: {:?}",
            compiled.warnings
        );
        // Stored as sent, a typed copy stores the same. The fight
        // prompt's backticks mean someone the game keeps them for set it.
        let trusted = Who {
            keeps_backticks: true,
            ..Who::default()
        };
        let typed = compile(prompt, fprompt, Origin::Typed, trusted).unwrap();
        assert_eq!(
            (typed.prompt, typed.fprompt),
            (compiled.prompt, compiled.fprompt)
        );
    }
}
