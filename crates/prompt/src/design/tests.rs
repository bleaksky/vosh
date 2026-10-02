use super::tokens::value;
use super::*;
use crate::testkit::designs::JAMES;

fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source).into_iter().map(|t| t.kind).collect()
}

fn val(name: &str) -> TokenKind {
    value(FieldRef::new(name), Format::Value)
}

fn fmt(name: &str, format: Format) -> TokenKind {
    value(FieldRef::new(name), format)
}

fn text(s: &str) -> TokenKind {
    TokenKind::Text(s.to_string())
}

fn fg(spec: ColorSpec) -> TokenKind {
    TokenKind::Code(Code::Fg(spec))
}

fn bg(spec: ColorSpec) -> TokenKind {
    TokenKind::Code(Code::Bg(spec))
}

fn by_value(name: &str) -> ColorSpec {
    ColorSpec::ByValue {
        field: FieldRef::new(name),
        scale: Scale::Thirds,
    }
}

fn by_scale(name: &str, scale: Scale) -> ColorSpec {
    ColorSpec::ByValue {
        field: FieldRef::new(name),
        scale,
    }
}

fn bar(width: u8, color: BarColor) -> Format {
    Format::Bar { width, color }
}

#[test]
fn legacy_values_parse_as_before() {
    assert_eq!(kinds("%hp"), vec![val("hp")]);
    assert_eq!(kinds("%HP"), vec![val("hp")]);
    assert_eq!(kinds("%{hp}"), vec![val("hp")]);
    assert_eq!(kinds("%{MaxHp}"), vec![val("maxhp")]);
    assert_eq!(kinds("%pct_hp"), vec![fmt("hp", Format::Pct)]);
    assert_eq!(kinds("%{pct_hp}"), vec![fmt("hp", Format::Pct)]);
    assert_eq!(
        kinds("%time %date"),
        vec![val("time"), text(" "), val("date")]
    );
    assert_eq!(kinds("%nope"), vec![val("nope")]);
    assert_eq!(kinds("%hp/%mhp"), vec![val("hp"), text("/"), val("mhp")]);
}

#[test]
fn legacy_bars_take_their_parameters_from_the_text_after_them() {
    let auto = BarColor::Auto;
    let named = |i| BarColor::Color(ColorSpec::Named(i));
    assert_eq!(kinds("%hp_bar"), vec![fmt("hp", bar(10, auto.clone()))]);
    assert_eq!(
        kinds("%hp_bar:4:green after"),
        vec![fmt("hp", bar(4, named(2))), text(" after")]
    );
    assert_eq!(
        kinds("%bar_mana:6"),
        vec![fmt("mana", bar(6, auto.clone()))]
    );
    // Width 0 keeps the default, and anything past 80 is 80.
    assert_eq!(
        kinds("%move_bar:0:yellow"),
        vec![fmt("move", bar(10, named(3)))]
    );
    assert_eq!(kinds("%hp_bar:200"), vec![fmt("hp", bar(80, auto.clone()))]);
    // A color with no width still reads, and a lone colon is taken.
    assert_eq!(kinds("%hp_bar::red"), vec![fmt("hp", bar(10, named(1)))]);
    assert_eq!(
        kinds("%hp_bar:x"),
        vec![fmt("hp", bar(10, auto.clone())), text("x")]
    );
    assert_eq!(
        kinds("%hp_bar:10:"),
        vec![fmt("hp", bar(10, auto.clone())), text(":")]
    );
    assert_eq!(kinds("%{hp_bar}:3"), vec![fmt("hp", bar(3, auto.clone()))]);
    assert_eq!(
        kinds("%hp_bar%c_red"),
        vec![fmt("hp", bar(10, auto)), fg(ColorSpec::Named(1))]
    );
}

#[test]
fn legacy_colors_and_styles_parse_as_before() {
    assert_eq!(kinds("%c_red"), vec![fg(ColorSpec::Named(1))]);
    assert_eq!(kinds("%{c:196}"), vec![fg(ColorSpec::Index(196))]);
    assert_eq!(kinds("%c_042"), vec![fg(ColorSpec::Index(42))]);
    assert_eq!(kinds("%{c:#FF8800}"), vec![fg(ColorSpec::Rgb(255, 136, 0))]);
    assert_eq!(kinds("%c_ff8800"), vec![fg(ColorSpec::Rgb(255, 136, 0))]);
    assert_eq!(
        kinds("%{c:255,128,0}"),
        vec![fg(ColorSpec::Rgb(255, 128, 0))]
    );
    assert_eq!(
        kinds("%{c:300,1,99999999999}"),
        vec![fg(ColorSpec::Rgb(255, 1, 0))]
    );
    assert_eq!(kinds("%c_hp"), vec![fg(by_value("hp"))]);
    assert_eq!(kinds("%{c:hp}"), vec![fg(by_value("hp"))]);
    assert_eq!(kinds("%c_300"), vec![fg(by_value("300"))]);
    assert_eq!(kinds("%c_reset"), vec![TokenKind::Code(Code::Reset)]);
    assert_eq!(kinds("%{c:reset}"), vec![TokenKind::Code(Code::Reset)]);
    assert_eq!(kinds("%bg_reset"), vec![TokenKind::Code(Code::Reset)]);
    assert_eq!(kinds("%s_reset"), vec![TokenKind::Code(Code::Reset)]);
    assert_eq!(kinds("%bg_green"), vec![bg(ColorSpec::Named(2))]);
    assert_eq!(kinds("%{bg:#330033}"), vec![bg(ColorSpec::Rgb(51, 0, 51))]);
    assert_eq!(kinds("%bg_hp"), vec![bg(by_value("hp"))]);
    assert_eq!(kinds("%{c_red}"), vec![fg(ColorSpec::Named(1))]);
    assert_eq!(kinds("%{bg_blue}"), vec![bg(ColorSpec::Named(4))]);
    for (name, style) in [
        ("bold", Style::Bold),
        ("dim", Style::Dim),
        ("italic", Style::Italic),
        ("underline", Style::Underline(UnderlineStyle::Single)),
        ("under", Style::Underline(UnderlineStyle::Single)),
        ("inverse", Style::Inverse),
        ("inv", Style::Inverse),
        ("strike", Style::Strike),
        ("blink", Style::Blink),
    ] {
        assert_eq!(
            kinds(&format!("%s_{name}")),
            vec![TokenKind::Code(Code::Style(style))]
        );
        assert_eq!(
            kinds(&format!("%{{s:{name}}}")),
            vec![TokenKind::Code(Code::Style(style))]
        );
    }
}

fn style(style: Style) -> TokenKind {
    TokenKind::Code(Code::Style(style))
}

fn ul(spec: ColorSpec) -> TokenKind {
    TokenKind::Code(Code::UnderlineColor(spec))
}

#[test]
fn underline_kinds_parse_as_styles() {
    for (name, line) in [
        ("double", UnderlineStyle::Double),
        ("curly", UnderlineStyle::Curly),
        ("dotted", UnderlineStyle::Dotted),
        ("dashed", UnderlineStyle::Dashed),
    ] {
        assert_eq!(
            kinds(&format!("%s_{name}")),
            vec![style(Style::Underline(line))]
        );
        assert_eq!(
            kinds(&format!("%{{S:{name}}}")),
            vec![style(Style::Underline(line))]
        );
    }
    assert_eq!(
        kinds("%s_underline"),
        vec![style(Style::Underline(UnderlineStyle::Single))]
    );
}

#[test]
fn an_underline_color_takes_the_forms_of_a_text_color() {
    assert_eq!(
        kinds("%{ul:#BF616A}"),
        vec![ul(ColorSpec::Rgb(191, 97, 106))]
    );
    assert_eq!(
        kinds("%{ul:191,97,106}"),
        vec![ul(ColorSpec::Rgb(191, 97, 106))]
    );
    assert_eq!(
        kinds("%{ul:bf616a}"),
        vec![ul(ColorSpec::Rgb(191, 97, 106))]
    );
    assert_eq!(kinds("%{ul:red}"), vec![ul(ColorSpec::Named(1))]);
    assert_eq!(kinds("%{ul:208}"), vec![ul(ColorSpec::Index(208))]);
    assert_eq!(kinds("%{ul:default}"), vec![ul(ColorSpec::Default)]);
    assert_eq!(kinds("%{ul:hp}"), vec![ul(by_value("hp"))]);
    assert_eq!(
        kinds("%{ul:hp:game}"),
        vec![ul(by_scale("hp", Scale::Game))]
    );
    assert_eq!(kinds("%{ul:reset}"), vec![TokenKind::Code(Code::Reset)]);
    assert_eq!(kinds("%{ul:}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%{ul:1,2}"), vec![TokenKind::Unknown]);
}

#[test]
fn a_name_that_starts_with_ul_stays_a_value() {
    // A script names its values freely, so the underline color has no
    // short form to take `ul_` from them.
    assert_eq!(kinds("%ul_kills"), vec![val("ul_kills")]);
    assert_eq!(kinds("%ul_red"), vec![val("ul_red")]);
    assert_eq!(kinds("%{ul_red}"), vec![val("ul_red")]);
    assert_eq!(
        kinds("%{ul_red:pct}"),
        vec![TokenKind::Value(ValueRef {
            field: FieldRef::new("ul_red"),
            format: Format::Pct,
        })]
    );
    assert_eq!(kinds("%ul"), vec![val("ul")]);
}

#[test]
fn each_style_writes_its_own_sgr() {
    let sgr = |s: Style| s.sgr();
    assert_eq!(sgr(Style::Bold), "1");
    assert_eq!(sgr(Style::Dim), "2");
    assert_eq!(sgr(Style::Italic), "3");
    assert_eq!(sgr(Style::Underline(UnderlineStyle::Single)), "4");
    assert_eq!(sgr(Style::Underline(UnderlineStyle::Double)), "4:2");
    assert_eq!(sgr(Style::Underline(UnderlineStyle::Curly)), "4:3");
    assert_eq!(sgr(Style::Underline(UnderlineStyle::Dotted)), "4:4");
    assert_eq!(sgr(Style::Underline(UnderlineStyle::Dashed)), "4:5");
    assert_eq!(sgr(Style::Inverse), "7");
    assert_eq!(sgr(Style::Strike), "9");
    assert_eq!(sgr(Style::Blink), "5");
    assert_eq!(sgr(Style::Off), "22;23;24;25;27;29");
}

#[test]
fn new_style_codes_write_back_as_they_read() {
    for source in [
        "%s_double",
        "%s_curly",
        "%s_dotted",
        "%s_dashed",
        "%s_strike",
        "%s_dim",
        "%s_inverse",
        "%s_blink",
        "%{ul:red}",
        "%{ul:default}",
        "%{ul:#bf616a}",
        "%{ul:hp:game}",
        "%{c:hp:steps}",
        "%{bg:mana:steps}",
        "%{ul:move:steps}",
    ] {
        let tokens = kinds(source);
        assert_eq!(write_tokens(&tokens), source, "{source}");
    }
    assert_eq!(
        write_token(&ul(ColorSpec::Rgb(191, 97, 106)), false),
        "%{ul:#bf616a}"
    );
    assert_eq!(write_token(&ul(ColorSpec::Named(1)), false), "%{ul:red}");
    assert_eq!(write_token(&ul(ColorSpec::Default), false), "%{ul:default}");
    assert_eq!(write_token(&ul(by_value("hp")), false), "%{ul:hp}");
    assert_eq!(write_token(&ul(ColorSpec::Named(1)), true), "%{ul:red}");
    assert_eq!(
        write_token(&style(Style::Underline(UnderlineStyle::Curly)), true),
        "%{s:curly}"
    );
}

#[test]
fn every_code_the_help_lists_reads_as_a_code() {
    let help = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../HELP.md"))
        .expect("HELP.md at the repo root");
    let topic = help
        .find(" Prompt design codes\n")
        .expect("the prompt design codes topic");
    let rows: Vec<&str> = help[topic..]
        .lines()
        .skip_while(|l| !l.starts_with("| Code"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .collect();
    assert!(rows.len() >= 18, "{rows:?}");
    for row in rows {
        let cell = row.split('|').nth(1).expect("a code cell");
        for code in cell.split('`').skip(1).step_by(2) {
            let template = Template::parse(code);
            let tokens = template.tokens();
            assert!(
                tokens
                    .iter()
                    .all(|t| t.kind != TokenKind::Unknown && !matches!(t.kind, TokenKind::Text(_))),
                "{code} reads as {tokens:?}"
            );
            // A color or style code reads as one, never as a value
            // by a name nothing has.
            let painted = [
                "%c_", "%bg_", "%ul_", "%s_", "%{c:", "%{bg:", "%{ul:", "%{s:",
            ];
            if painted.iter().any(|p| code.starts_with(p)) {
                assert!(
                    matches!(
                        tokens,
                        [Token {
                            kind: TokenKind::Code(_),
                            ..
                        }]
                    ),
                    "{code} reads as {tokens:?}"
                );
            }
        }
    }
}

#[test]
fn reads_the_field_an_underline_color_follows() {
    let template = Template::parse("%{ul:hp}%s_curly%{ul:mana:game}x");
    let names: Vec<String> = template.reads().iter().map(ToString::to_string).collect();
    assert_eq!(names, vec!["hp", "mana"]);
}

#[test]
fn anything_else_after_a_percent_stays_literal() {
    assert_eq!(kinds("%%"), vec![TokenKind::Percent]);
    assert_eq!(kinds("%)h"), vec![text("%)h")]);
    assert_eq!(kinds("% "), vec![text("% ")]);
    assert_eq!(kinds("end %"), vec![text("end %")]);
    assert_eq!(kinds("%{"), vec![text("%{")]);
    assert_eq!(kinds("%{}"), vec![text("%{}")]);
    assert_eq!(kinds("%{a b}"), vec![text("%{a b}")]);
    // A colon ends an unbraced name, as it always did.
    assert_eq!(kinds("%c:red"), vec![val("c"), text(":red")]);
}

#[test]
fn codes_the_grammar_does_not_know_print_as_written() {
    for source in [
        "%s_nope",
        "%c_",
        "%{c:}",
        "%{c:1,2}",
        "%{a.b}",
        "%{hp:bogus}",
    ] {
        let tokens = tokenize(source);
        assert_eq!(tokens.len(), 1, "{source}");
        assert_eq!(tokens[0].kind, TokenKind::Unknown, "{source}");
        assert_eq!(&source[tokens[0].start..tokens[0].end], source);
    }
}

#[test]
fn tokens_keep_their_byte_ranges() {
    let source = "é%hp_bar:4 %{c:hp:game}%%x";
    let tokens = tokenize(source);
    let spans: Vec<&str> = tokens.iter().map(|t| &source[t.start..t.end]).collect();
    assert_eq!(
        spans,
        vec!["é", "%hp_bar:4", " ", "%{c:hp:game}", "%%", "x"]
    );
}

#[test]
fn james_template_parses_to_the_same_tokens_as_before() {
    let tokens = tokenize(JAMES);
    let written: Vec<&str> = tokens.iter().map(|t| &JAMES[t.start..t.end]).collect();
    assert_eq!(
        written,
        vec![
            "%{c:100,100,100}",
            "[",
            "%c_reset",
            "%s_italic",
            "%hp",
            "(",
            "%c_hp",
            "%pct_hp",
            "%c_reset",
            "%s_italic",
            "%)h ",
            "%mana",
            "(",
            "%{c:128,200,255}",
            "%pct_mana",
            "%c_reset",
            "%s_italic",
            "%)m ",
            "%move",
            "(",
            "%{c:200,255,23}",
            "%pct_move",
            "%c_reset",
            "%s_italic",
            "%)v",
            "%c_reset",
            "%{c:100,100,100}",
            "] ",
            "%c_reset",
        ]
    );
    assert!(tokens.iter().all(|t| t.kind != TokenKind::Unknown));
}

#[test]
fn new_forms_parse() {
    assert_eq!(kinds("%c_default"), vec![fg(ColorSpec::Default)]);
    assert_eq!(kinds("%bg_default"), vec![bg(ColorSpec::Default)]);
    assert_eq!(
        kinds("%s_off"),
        vec![TokenKind::Code(Code::Style(Style::Off))]
    );
    assert_eq!(kinds("%c_gray"), vec![fg(ColorSpec::Named(8))]);
    assert_eq!(kinds("%c_black"), vec![fg(ColorSpec::Named(0))]);
    assert_eq!(kinds("%c_bright_white"), vec![fg(ColorSpec::Named(15))]);
    assert_eq!(kinds("%{c:hp:game}"), vec![fg(by_scale("hp", Scale::Game))]);
    assert_eq!(
        kinds("%{c:hp:steps}"),
        vec![fg(by_scale("hp", Scale::Steps))]
    );
    assert_eq!(
        kinds("%{bg:Mana:STEPS}"),
        vec![bg(by_scale("mana", Scale::Steps))]
    );
    assert_eq!(
        kinds("%{ul:move:steps}"),
        vec![ul(by_scale("move", Scale::Steps))]
    );
    assert_eq!(kinds("%{c:hp:tenths}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%nl%{nl}"), vec![TokenKind::Nl, TokenKind::Nl]);
    assert_eq!(kinds("%{right}%{RIGHT}"), vec![TokenKind::Right; 2]);
    // Unbraced, it is a value a script may set, as it always was.
    assert_eq!(kinds("%right"), vec![val("right")]);
    assert_eq!(kinds("%{right:x}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%{raw}"), vec![TokenKind::Raw]);
    assert_eq!(kinds("%{end}"), vec![TokenKind::End]);
    assert_eq!(
        kinds("%{if:fight}%{ifnot:Fight}"),
        vec![
            TokenKind::If(FieldRef::new("fight")),
            TokenKind::IfNot(FieldRef::new("fight"))
        ]
    );
    assert_eq!(kinds("%{if:}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%maxhp"), vec![val("maxhp")]);
    assert_eq!(kinds("%{hp:max}"), vec![fmt("hp", Format::Max)]);
    assert_eq!(kinds("%{hp:pct}"), vec![fmt("hp", Format::Pct)]);
    assert_eq!(kinds("%{hp:pct:game}"), vec![fmt("hp", Format::PctGame)]);
    assert_eq!(
        kinds("%{Mana:PCT:Game}"),
        vec![fmt("mana", Format::PctGame)]
    );
    assert_eq!(kinds("%{hp:pct:steps}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%{hp:pct:game:1}"), vec![TokenKind::Unknown]);
    assert_eq!(
        kinds("%{hp:bar:10:auto}"),
        vec![fmt("hp", bar(10, BarColor::Auto))]
    );
    assert_eq!(
        kinds("%{opponent_hp:bar}"),
        vec![fmt("opponent_hp", bar(10, BarColor::Auto))]
    );
    assert_eq!(
        kinds("%{hp:bar:6:game}"),
        vec![fmt("hp", bar(6, BarColor::Game))]
    );
    assert_eq!(
        kinds("%{hp:bar:6:#FF0000}"),
        vec![fmt(
            "hp",
            bar(6, BarColor::Color(ColorSpec::Rgb(255, 0, 0)))
        )]
    );
    assert_eq!(
        kinds("%{hp:bar:red}"),
        vec![fmt("hp", bar(10, BarColor::Color(ColorSpec::Named(1))))]
    );
    assert_eq!(kinds("%{tank_hp:game}"), vec![fmt("tank_hp", Format::Game)]);
    assert_eq!(kinds("%{pos:word}"), vec![fmt("pos", Format::Word)]);
    assert_eq!(kinds("%{hour:ampm}"), vec![fmt("hour", Format::Ampm)]);
    assert_eq!(kinds("%{Hour:AMPM}"), vec![fmt("hour", Format::Ampm)]);
    assert_eq!(kinds("%{moon1:name}"), vec![fmt("moon1", Format::Name)]);
    assert_eq!(kinds("%{gold:grouped}"), vec![fmt("gold", Format::Grouped)]);
    assert_eq!(kinds("%{gold:short}"), vec![fmt("gold", Format::Short)]);
    assert_eq!(
        kinds("%{gold:thousands}"),
        vec![fmt("gold", Format::Thousands)]
    );
    assert_eq!(kinds("%{gold:thousands:1}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%{temp:unit}"), vec![fmt("temp", Format::Unit)]);
    assert_eq!(kinds("%{tick:since}"), vec![fmt("tick", Format::Since)]);
    assert_eq!(
        kinds("%{room:trunc:20}"),
        vec![fmt("room", Format::Trunc(20))]
    );
    assert_eq!(kinds("%{room:trunc}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%{time:hm}"), vec![fmt("time", Format::Hm)]);
    assert_eq!(kinds("%{time:hms}"), vec![fmt("time", Format::Hms)]);
    assert_eq!(kinds("%{date:md}"), vec![fmt("date", Format::Md)]);
    assert_eq!(
        kinds("%{missing:names}"),
        vec![fmt("missing", Format::Names)]
    );
    assert_eq!(
        kinds("%{missing:count}"),
        vec![fmt("missing", Format::Count)]
    );
    assert_eq!(kinds("%{hp:max:1}"), vec![TokenKind::Unknown]);
}

#[test]
fn the_forms_of_the_old_prompt_write_back_as_they_read() {
    for source in [
        "%{gold:thousands}",
        "%{exp:thousands}",
        "%{hour:ampm}",
        "%{tick:since}",
        "%hp%{right}%mana",
        "%{hp:pct:game}",
        "%{c:#d0d0d0}%{mana:pct:game}%{c:mana:steps}%%",
    ] {
        let tokens = kinds(source);
        assert!(tokens.iter().all(|t| *t != TokenKind::Unknown), "{source}");
        assert_eq!(write_tokens(&tokens), source, "{source}");
    }
}

#[test]
fn param_fields_keep_the_case_of_their_parameter() {
    let param = |name: &str, p: &str, format: Format| value(FieldRef::with_param(name, p), format);
    assert_eq!(
        kinds("%{aff:Giant_Strength}"),
        vec![param("aff", "Giant_Strength", Format::Value)]
    );
    assert_eq!(
        kinds("%{aff:sanctuary:on}"),
        vec![param("aff", "sanctuary", Format::On)]
    );
    assert_eq!(
        kinds("%{member_hp:Quenby:bar:6}"),
        vec![param("member_hp", "Quenby", bar(6, BarColor::Auto))]
    );
    assert_eq!(
        kinds("%{member_hp:id=3}"),
        vec![param("member_hp", "id=3", Format::Value)]
    );
    assert_eq!(
        kinds("%{QUEUE:Bugs}"),
        vec![param("queue", "bugs", Format::Value)]
    );
    assert_eq!(
        kinds("%{gmcp:Char.Vitals.ep}"),
        vec![param("gmcp", "Char.Vitals.ep", Format::Value)]
    );
    assert_eq!(
        kinds("%{gmcp:Char.Affects.affects[name=sanctuary].level}"),
        vec![param(
            "gmcp",
            "Char.Affects.affects[name=sanctuary].level",
            Format::Value
        )]
    );
    assert_eq!(
        kinds("%{if:aff:Sanctuary}"),
        vec![TokenKind::If(FieldRef::with_param("aff", "Sanctuary"))]
    );
    // A parameter field with no parameter reads nothing.
    assert_eq!(kinds("%{aff}"), vec![TokenKind::Unknown]);
    assert_eq!(kinds("%aff"), vec![TokenKind::Unknown]);
}

fn piece_kinds(source: &str) -> Vec<(PieceKind, String)> {
    let template = Template::parse(source);
    (0..template.pieces().len())
        .map(|i| {
            (
                template.pieces()[i].kind,
                template.piece_text(i).to_string(),
            )
        })
        .collect()
}

#[test]
fn pieces_are_codes_then_one_value_or_one_run_of_text() {
    use PieceKind::{Codes, Percent, Text, Value};
    let got = piece_kinds(JAMES);
    let expect: Vec<(PieceKind, &str)> = vec![
        (Text, "%{c:100,100,100}["),
        (Value, "%c_reset%s_italic%hp"),
        (Text, "("),
        (Value, "%c_hp%pct_hp"),
        (Text, "%c_reset%s_italic%)h "),
        (Value, "%mana"),
        (Text, "("),
        (Value, "%{c:128,200,255}%pct_mana"),
        (Text, "%c_reset%s_italic%)m "),
        (Value, "%move"),
        (Text, "("),
        (Value, "%{c:200,255,23}%pct_move"),
        (Text, "%c_reset%s_italic%)v"),
        (Text, "%c_reset%{c:100,100,100}] "),
        (Codes, "%c_reset"),
    ];
    let expect: Vec<(PieceKind, String)> = expect
        .into_iter()
        .map(|(k, s)| (k, s.to_string()))
        .collect();
    assert_eq!(got, expect);
    assert!(!got.iter().any(|(k, _)| *k == Percent));
}

#[test]
fn a_push_to_the_right_stands_alone_as_a_line_break_does() {
    assert_eq!(
        piece_kinds("%hp%c_red%{right}x %mana"),
        vec![
            (PieceKind::Value, "%hp".to_string()),
            (PieceKind::Codes, "%c_red".to_string()),
            (PieceKind::Right, "%{right}".to_string()),
            (PieceKind::Text, "x ".to_string()),
            (PieceKind::Value, "%mana".to_string()),
        ]
    );
}

#[test]
fn current_and_max_fold_into_one_piece() {
    for source in [
        "%hp/%{maxhp}",
        "%hp/%maxhp",
        "%hp/%mhp",
        "%hp/%hp_max",
        "%hp/%max_hp",
        "%{hp}/%{hp:max}",
    ] {
        let got = piece_kinds(source);
        assert_eq!(
            got,
            vec![(PieceKind::CurMax, source.to_string())],
            "{source}"
        );
    }
    // Anything between the two, or another field's max, does not fold.
    assert_eq!(piece_kinds("%hp/%maxmana").len(), 3);
    assert_eq!(piece_kinds("%hp /%maxhp").len(), 3);
    assert_eq!(piece_kinds("%hp%c_red/%maxhp").len(), 3);
    assert_eq!(piece_kinds("%pct_hp/%maxhp").len(), 3);
}

#[test]
fn a_percent_and_its_sign_fold_into_one_piece() {
    assert_eq!(
        piece_kinds("hp %c_hp%pct_hp%%%c_default mn"),
        vec![
            (PieceKind::Text, "hp ".to_string()),
            (PieceKind::Percent, "%c_hp%pct_hp%%".to_string()),
            (PieceKind::Text, "%c_default mn".to_string()),
        ]
    );
    assert_eq!(
        piece_kinds("%{hp:pct}%%)"),
        vec![
            (PieceKind::Percent, "%{hp:pct}%%".to_string()),
            (PieceKind::Text, ")".to_string()),
        ]
    );
    // The game's percent stays a value, its sign text of its own.
    assert_eq!(
        piece_kinds("%{hp:pct:game}%%)"),
        vec![
            (PieceKind::Value, "%{hp:pct:game}".to_string()),
            (PieceKind::Text, "%%)".to_string()),
        ]
    );
    // Text right after the sign stays its own piece.
    assert_eq!(
        piece_kinds("[%c_hp%pct_hp%%hp%c_default/"),
        vec![
            (PieceKind::Text, "[".to_string()),
            (PieceKind::Percent, "%c_hp%pct_hp%%".to_string()),
            (PieceKind::Text, "hp".to_string()),
            (PieceKind::Text, "%c_default/".to_string()),
        ]
    );
    // A percent sign after anything but a percent value is text.
    assert_eq!(
        piece_kinds("%hp%%"),
        vec![
            (PieceKind::Value, "%hp".to_string()),
            (PieceKind::Text, "%%".to_string()),
        ]
    );
}

#[test]
fn codes_before_a_line_break_or_a_condition_stand_alone() {
    use PieceKind::{Codes, End, If, Nl, Text, Value};
    let got = piece_kinds("%c_red%{if:fight}%opponent%c_reset%nl%{end}x");
    let expect = vec![
        (Codes, "%c_red"),
        (If, "%{if:fight}"),
        (Value, "%opponent"),
        (Codes, "%c_reset"),
        (Nl, "%nl"),
        (End, "%{end}"),
        (Text, "x"),
    ];
    let expect: Vec<(PieceKind, String)> = expect
        .into_iter()
        .map(|(k, s)| (k, s.to_string()))
        .collect();
    assert_eq!(got, expect);
}

#[test]
fn reads_lists_every_field_the_template_uses() {
    let template = Template::parse(
        "%{if:fight}%{c:hp:game}%hp%{end}%{mana:bar:6:move}%bg_tick%{raw}%{aff:Haste}",
    );
    let names: Vec<String> = template.reads().iter().map(ToString::to_string).collect();
    assert_eq!(
        names,
        vec!["aff:Haste", "fight", "hp", "mana", "move", "raw", "tick"]
    );
}
