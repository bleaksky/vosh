//! `prompt_edit` tests. Every op keeps the text
//! and the look of every piece it does not change, and the writer never
//! writes a token that reads back as something else.

use vosh_prompt::card::edit::{
    apply, ColorChoice, EditError, EditOp, FormatChoice, FormatName, StyleChoice, When,
};
use vosh_prompt::design::{PieceKind, TokenKind};
use vosh_prompt::render::SgrState;
use vosh_prompt::testkit::designs::{DETAILED, JAMES};
use vosh_prompt::testkit::mud::PROMPT;
use vosh_prompt::testkit::now;
use vosh_prompt::values::Samples;
use vosh_prompt::{render_str, FieldRef, RenderOptions, Resolved, SpanColor, Template, Values};

/// The catalog's samples, in a fight or out of one.
struct Sampled {
    fight: bool,
}

impl Values for Sampled {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        if !self.fight && field.name == "fight" {
            return Resolved::Absent;
        }
        Samples { now: now() }.resolve(field)
    }
}

fn known(field: &FieldRef) -> bool {
    !matches!(Samples { now: now() }.resolve(field), Resolved::Unknown)
}

fn edit(template: &str, op: &EditOp) -> String {
    apply(template, op, &known).unwrap_or_else(|e| panic!("{op:?} on {template}: {e}"))
}

/// Each drawn piece's text and look, in the order the pieces draw. The
/// last is the whole look, dim, inverse, strike and the underline's kind
/// and color with the rest.
type Part = (String, SpanColor, SpanColor, bool, bool, bool, SgrState);

fn parts(template: &str, fight: bool) -> Vec<(usize, Part)> {
    let rendered = render_str(template, &Sampled { fight }, RenderOptions::default());
    // Each character with the cell it starts at, since spans count cells.
    let rows: Vec<Vec<(usize, char)>> = rendered
        .plain
        .split('\n')
        .map(|l| {
            let mut cell = 0;
            l.chars()
                .map(|c| {
                    let at = cell;
                    cell += vosh_prompt::wrap::cell_width(c);
                    (at, c)
                })
                .collect()
        })
        .collect();
    rendered
        .spans
        .iter()
        .map(|s| {
            let text: String = rows[s.row]
                .iter()
                .filter(|(at, _)| (s.col..s.col + s.width).contains(at))
                .map(|(_, c)| c)
                .collect();
            (
                s.piece,
                (text, s.fg, s.bg, s.bold, s.italic, s.underline, s.look),
            )
        })
        .collect()
}

/// The parts of every piece but `skip`, by the piece's position among the
/// pieces that draw. Parts next to each other with one look read as one,
/// since two runs of text that meet become one piece.
fn others(template: &str, fight: bool, skip: &[usize]) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for (piece, part) in parts(template, fight) {
        if skip.contains(&piece) {
            continue;
        }
        match out.last_mut() {
            Some(last)
                if (last.1, last.2, last.3, last.4, last.5, last.6)
                    == (part.1, part.2, part.3, part.4, part.5, part.6) =>
            {
                last.0.push_str(&part.0);
            }
            _ => out.push(part),
        }
    }
    out
}

fn assert_reads_clean(template: &str) {
    let parsed = Template::parse(template);
    assert!(
        parsed.tokens().iter().all(|t| t.kind != TokenKind::Unknown),
        "{template}"
    );
}

#[test]
fn removing_italic_from_james_health_leaves_every_other_piece_as_it_was() {
    let op = EditOp::SetStyle {
        piece: 1,
        style: StyleChoice::Italic,
        on: false,
    };
    let edited = edit(JAMES, &op);
    assert_eq!(
        edited,
        "%{c:100,100,100}[%c_reset%hp%s_italic(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset"
    );
    for fight in [true, false] {
        assert_eq!(
            others(&edited, fight, &[1]),
            others(JAMES, fight, &[1]),
            "fight {fight}"
        );
    }
    // Health itself lost its italic and nothing else.
    let hp = |t: &str| parts(t, true).into_iter().find(|(p, _)| *p == 1).unwrap().1;
    let (before, after) = (hp(JAMES), hp(&edited));
    assert!(before.4 && !after.4);
    assert_eq!((before.0, before.1, before.2), (after.0, after.1, after.2));
    // Every other piece keeps its SGR byte for byte: the render differs
    // only where the italic code moved from before the health to after it.
    let render = |t: &str| render_str(t, &Sampled { fight: true }, RenderOptions::default()).ansi;
    assert_eq!(
        render(&edited),
        render(JAMES).replacen("\x1b[3m1020(", "1020\x1b[3m(", 1)
    );
}

#[test]
fn a_style_turned_on_is_turned_back_off_for_the_next_piece() {
    let edited = edit(
        "hp %hp mn %mana",
        &EditOp::SetStyle {
            piece: 1,
            style: StyleChoice::Bold,
            on: true,
        },
    );
    assert_eq!(edited, "hp %s_bold%hp%s_off mn %mana");
    assert_eq!(
        others(&edited, true, &[1]),
        others("hp %hp mn %mana", true, &[1])
    );
}

fn style(piece: usize, style: StyleChoice, on: bool) -> EditOp {
    EditOp::SetStyle { piece, style, on }
}

#[test]
fn the_styles_more_offers_turn_on_and_back_off_for_the_next_piece() {
    for (choice, code) in [
        (StyleChoice::Strike, "%s_strike"),
        (StyleChoice::Dim, "%s_dim"),
        (StyleChoice::Inverse, "%s_inverse"),
        (StyleChoice::Blink, "%s_blink"),
    ] {
        let edited = edit("hp %hp mn %mana", &style(1, choice, true));
        assert_eq!(edited, format!("hp {code}%hp%s_off mn %mana"));
        // Off again, every piece looks as it did before.
        let off = edit(&edited, &style(1, choice, false));
        assert_eq!(off, "hp %hp%s_off mn %mana");
        assert_eq!(
            others(&off, true, &[]),
            others("hp %hp mn %mana", true, &[])
        );
    }
}

#[test]
fn an_underline_kind_replaces_the_kind_the_piece_had() {
    let edited = edit("hp %hp mn %mana", &style(1, StyleChoice::Curly, true));
    assert_eq!(edited, "hp %s_curly%hp%s_off mn %mana");
    // Another kind takes the place of the piece's own code.
    let edited = edit(&edited, &style(1, StyleChoice::Dashed, true));
    assert_eq!(edited, "hp %s_dashed%hp%s_off mn %mana");
    // A kind it inherits stays on the pieces after it.
    let edited = edit("%s_underline%hp x", &style(0, StyleChoice::Double, true));
    assert_eq!(edited, "%s_double%hp%s_underline x");
    assert_eq!(
        others(&edited, true, &[0]),
        others("%s_underline%hp x", true, &[0])
    );
    // Off is off for any kind, the one it inherits too, and the piece
    // after it keeps the line.
    let edited = edit("%s_curly[%hp]", &style(1, StyleChoice::Underline, false));
    assert_eq!(edited, "%s_curly[%s_off%hp%s_curly]");
    let edited = edit("x %s_dotted%hp", &style(1, StyleChoice::Underline, false));
    assert_eq!(edited, "x %hp");
}

#[test]
fn more_styles_turns_an_underline_kind_on_and_the_checked_kind_off() {
    // More styles sends a kind on to pick it and the checked kind off.
    let edited = edit("x %hp", &style(1, StyleChoice::Dotted, true));
    assert_eq!(edited, "x %s_dotted%hp");
    let edited = edit(&edited, &style(1, StyleChoice::Dotted, false));
    assert_eq!(edited, "x %hp");
    // Another kind's code off on a dotted piece ends the line too.
    let edited = edit("x %s_dotted%hp", &style(1, StyleChoice::Curly, false));
    assert_eq!(edited, "x %hp");
}

#[test]
fn an_underline_takes_a_color_of_its_own() {
    let rgb = ColorChoice::Rgb {
        r: 191,
        g: 97,
        b: 106,
    };
    let underline = |piece: usize, color: ColorChoice| EditOp::SetColor {
        piece,
        color,
        background: false,
        underline: true,
    };
    let template = "x %s_curly%hp y";
    let edited = edit(template, &underline(1, rgb.clone()));
    assert_eq!(edited, "x %s_curly%{ul:#bf616a}%hp%{ul:default} y");
    assert_eq!(others(&edited, true, &[1]), others(template, true, &[1]));
    // The text's own color takes it back out, and every piece looks as it
    // did before.
    let back = edit(&edited, &underline(1, ColorChoice::Default));
    assert_eq!(back, "x %s_curly%hp%{ul:default} y");
    assert_eq!(others(&back, true, &[]), others(template, true, &[]));

    // A theme color goes braced too, since no short form reads it.
    assert_eq!(
        edit(
            "%s_curly%hp",
            &underline(0, ColorChoice::Named { index: 1 })
        ),
        "%s_curly%{ul:red}%hp"
    );
    // A color goes on one place at a time.
    let both = EditOp::SetColor {
        piece: 0,
        color: rgb,
        background: true,
        underline: true,
    };
    assert!(apply("%hp", &both, &known).is_err());
}

#[test]
fn a_color_keeps_the_pieces_after_it_in_theirs() {
    let template = "%c_red[%hp] %mana";
    let edited = edit(
        template,
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Named { index: 2 },
            background: false,
            underline: false,
        },
    );
    assert_eq!(edited, "%c_red[%c_green%hp%c_red] %mana");
    assert_eq!(others(&edited, true, &[1]), others(template, true, &[1]));

    // Terminal text writes the default color, and By value colors the
    // value by how full it is.
    let edited = edit(
        "%c_red%hp",
        &EditOp::SetColor {
            piece: 0,
            color: ColorChoice::Default,
            background: false,
            underline: false,
        },
    );
    assert_eq!(edited, "%hp");
    let edited = edit(
        "x %c_red%hp",
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::ByValue {
                field: None,
                game: false,
                steps: false,
            },
            background: false,
            underline: false,
        },
    );
    assert_eq!(edited, "x %c_hp%hp");
    let edited = edit(
        "x %hp y",
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Rgb {
                r: 128,
                g: 200,
                b: 255,
            },
            background: true,
            underline: false,
        },
    );
    assert_eq!(edited, "x %{bg:#80c8ff}%hp%bg_default y");
}

#[test]
fn text_takes_no_color_by_value() {
    let err = apply(
        "hp %hp",
        &EditOp::SetColor {
            piece: 0,
            color: ColorChoice::ByValue {
                field: None,
                game: false,
                steps: false,
            },
            background: false,
            underline: false,
        },
        &known,
    );
    assert_eq!(
        err,
        Err(EditError(
            "Only a value can take its color from how full it is.".into()
        ))
    );
}

#[test]
fn a_color_by_steps_writes_and_keeps_its_scale() {
    let steps = |field: Option<&str>| ColorChoice::ByValue {
        field: field.map(str::to_string),
        game: false,
        steps: true,
    };
    let color = |template: &str, piece: usize, color: ColorChoice| {
        edit(
            template,
            &EditOp::SetColor {
                piece,
                color,
                background: false,
                underline: false,
            },
        )
    };
    assert_eq!(
        color("x %hp y", 1, steps(None)),
        "x %{c:hp:steps}%hp%c_default y"
    );
    // The sign after a percent takes the steps of the value before it.
    assert_eq!(
        color("%pct_hp%c_hp%%", 1, steps(Some("hp"))),
        "%pct_hp%{c:hp:steps}%%"
    );
    // Other edits keep it as it is.
    let kept = edit(
        "%pct_hp%{c:hp:steps}%%x",
        &EditOp::SetStyle {
            piece: 0,
            style: StyleChoice::Bold,
            on: true,
        },
    );
    assert_eq!(kept, "%s_bold%pct_hp%s_off%{c:hp:steps}%%x");
    assert_reads_clean("%pct_hp%{c:hp:steps}%%x");
    // A bar takes one color for its cells, so it takes no steps.
    assert_eq!(
        apply(
            "x %{hp:bar:6} y",
            &EditOp::SetColor {
                piece: 1,
                color: steps(None),
                background: false,
                underline: false,
            },
            &known,
        ),
        Err(EditError(
            "A bar cannot take the steps from red to green.".into()
        ))
    );
    let both = ColorChoice::ByValue {
        field: None,
        game: true,
        steps: true,
    };
    assert!(apply(
        "x %hp y",
        &EditOp::SetColor {
            piece: 1,
            color: both,
            background: false,
            underline: false,
        },
        &known,
    )
    .is_err());
}

#[test]
fn show_as_writes_the_forms_the_card_names() {
    let set = |template: &str, format: FormatChoice| {
        edit(template, &EditOp::SetFormat { piece: 1, format })
    };
    let template = "[%c_hp%hp/%{maxhp}hp]";
    assert_eq!(
        set(template, FormatChoice::of(FormatName::Value)),
        "[%c_hp%{hp}hp]"
    );
    assert_eq!(
        set(template, FormatChoice::of(FormatName::Percent)),
        "[%c_hp%pct_hp%%hp]"
    );
    assert_eq!(
        set("[%c_hp%hp hp]", FormatChoice::of(FormatName::CurMax)),
        "[%c_hp%hp/%{maxhp} hp]"
    );
    let bar = FormatChoice {
        width: Some(6),
        ..FormatChoice::of(FormatName::Bar)
    };
    assert_eq!(set(template, bar), "[%c_hp%{hp:bar:6}hp]");
    // A bar keeps its width when only its color changes.
    let red = FormatChoice {
        color: Some(ColorChoice::Named { index: 1 }),
        ..FormatChoice::of(FormatName::Bar)
    };
    assert_eq!(set("x %hp_bar:6 y", red), "x %{hp:bar:6:red} y");
    assert_eq!(
        set("x %{pos} y", FormatChoice::of(FormatName::Word)),
        "x %{pos:word} y"
    );
    assert_eq!(
        set("x %gold y", FormatChoice::of(FormatName::Thousands)),
        "x %{gold:thousands} y"
    );
    assert_eq!(
        set("x %{hour:word} y", FormatChoice::of(FormatName::Ampm)),
        "x %{hour:ampm} y"
    );
    assert_eq!(
        set("x %tick y", FormatChoice::of(FormatName::Since)),
        "x %{tick:since} y"
    );
    // The percent as the game cuts it takes no sign, as %pct_hp does.
    assert_eq!(
        set(template, FormatChoice::of(FormatName::PctGame)),
        "[%c_hp%{hp:pct:game}hp]"
    );
    assert_eq!(
        set("x %pct_mana%% y", FormatChoice::of(FormatName::PctGame)),
        "x %{mana:pct:game} y"
    );
    for template in [
        "[%c_hp%{hp}hp]",
        "[%c_hp%pct_hp%%hp]",
        "[%c_hp%{hp:bar:6}hp]",
        "x %{pos:word} y",
        "x %{gold:thousands} y",
        "x %{hour:ampm} y",
        "x %{tick:since} y",
        "[%c_hp%{hp:pct:game}hp]",
    ] {
        assert_reads_clean(template);
    }
    // A width past 80 and a format the value lacks change nothing.
    let wide = FormatChoice {
        width: Some(81),
        ..FormatChoice::of(FormatName::Bar)
    };
    assert_eq!(
        apply(
            template,
            &EditOp::SetFormat {
                piece: 1,
                format: wide
            },
            &known
        ),
        Err(EditError("A bar is 1 to 80 cells wide.".into()))
    );
    assert_eq!(
        apply(
            "x %gold",
            &EditOp::SetFormat {
                piece: 1,
                format: FormatChoice::of(FormatName::Hm)
            },
            &known
        ),
        Err(EditError("Vosh cannot show that value that way.".into()))
    );
    // Gold has no max, so it has no percent of any kind.
    assert_eq!(
        apply(
            "x %gold",
            &EditOp::SetFormat {
                piece: 1,
                format: FormatChoice::of(FormatName::PctGame)
            },
            &known
        ),
        Err(EditError("Vosh cannot show that value that way.".into()))
    );
}

#[test]
fn a_bar_takes_its_color_as_its_own() {
    let edited = edit(
        "x %{hp:bar:6} y",
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Named { index: 4 },
            background: false,
            underline: false,
        },
    );
    assert_eq!(edited, "x %{hp:bar:6:blue} y");
    let edited = edit(
        &edited,
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::ByValue {
                field: None,
                game: false,
                steps: false,
            },
            background: false,
            underline: false,
        },
    );
    // A bar colored by how full has a short form, which a token the
    // writer writes anew takes.
    assert_eq!(edited, "x %hp_bar:6 y");
}

#[test]
fn a_bar_by_how_full_writes_its_short_form_and_braces_before_a_name() {
    let n = Template::parse("[%hp]").pieces().len();
    let bar = |at: usize, template: &str| {
        edit(
            template,
            &EditOp::InsertField {
                at,
                field: "hp".into(),
                format: Some(FormatChoice {
                    format: FormatName::Bar,
                    width: Some(10),
                    color: None,
                    chars: None,
                }),
            },
        )
    };
    assert_eq!(bar(n, "[%hp]"), "[%hp]%hp_bar:10");
    // Before a digit or a colon the short form would read on, so it takes
    // braces.
    assert_eq!(bar(0, "5"), "%{hp:bar:10}5");
    assert_eq!(bar(0, ":"), "%{hp:bar:10}:");
}

#[test]
fn inserts_use_braces_when_the_next_character_would_extend_a_name() {
    assert_eq!(
        edit(
            "%hp",
            &EditOp::InsertText {
                at: 1,
                text: "hp and 100%".into()
            }
        ),
        "%{hp}hp and 100%%"
    );
    assert_eq!(
        edit(
            "hp",
            &EditOp::InsertField {
                at: 0,
                field: "gold".into(),
                format: None
            }
        ),
        "%{gold}hp"
    );
    assert_eq!(
        edit(
            "$%c_red",
            &EditOp::InsertField {
                at: 1,
                field: "gold".into(),
                format: Some(FormatChoice::of(FormatName::Thousands)),
            }
        ),
        "$%{gold:thousands}%c_red"
    );
    assert_eq!(
        edit(
            "(%c_red)",
            &EditOp::InsertField {
                at: 1,
                field: "move".into(),
                format: Some(FormatChoice::of(FormatName::PctGame)),
            }
        ),
        "(%{move:pct:game}%c_red)"
    );
    assert_eq!(
        edit("%c_red%hp", &EditOp::InsertNl { at: 0 }),
        "%nl%c_red%hp"
    );
    assert_eq!(edit("%hp", &EditOp::InsertNl { at: 1 }), "%hp%nl");
    assert_eq!(edit("%{hp}x", &EditOp::InsertNl { at: 1 }), "%{hp}%{nl}x");
    // A literal percent at the end of text stays literal before a code.
    assert_eq!(
        edit(
            "50%",
            &EditOp::InsertField {
                at: 1,
                field: "hp".into(),
                format: None
            }
        ),
        "50%%%hp"
    );
    assert_eq!(
        edit(
            "x %hp_bar:6",
            &EditOp::InsertText {
                at: 2,
                text: ":2".into()
            }
        ),
        "x %{hp:bar:6}:2"
    );
    assert_eq!(
        edit(
            "x",
            &EditOp::InsertField {
                at: 1,
                field: "aff:Giant_Strength".into(),
                format: Some(FormatChoice::of(FormatName::On))
            }
        ),
        "x%{aff:Giant_Strength:on}"
    );
}

#[test]
fn the_writer_never_writes_an_unknown_name() {
    for field in ["nope", "hp:x", "aff", "c_red", "a b"] {
        assert_eq!(
            apply(
                "x",
                &EditOp::InsertField {
                    at: 1,
                    field: field.into(),
                    format: None
                },
                &known
            ),
            Err(EditError("Vosh does not know that value.".into())),
            "{field}"
        );
    }
}

#[test]
fn the_card_reads_a_field_as_the_grammar_reads_a_braced_one() {
    // Every name counts as known here, so only the reading can refuse.
    let insert = |field: &str| {
        apply(
            "x",
            &EditOp::InsertField {
                at: 1,
                field: field.into(),
                format: None,
            },
            &|_| true,
        )
    };
    // Spaces around the field go, the name reads in lowercase, and only
    // a queue's parameter does too.
    for (field, written) in [
        (" HP ", "x%hp"),
        ("AFF:Sanctuary", "x%{aff:Sanctuary}"),
        ("Queue:Bash", "x%{queue:bash}"),
        ("gmcp:Char.Vitals.ep", "x%{gmcp:Char.Vitals.ep}"),
        ("member_hp:Rook", "x%{member_hp:Rook}"),
    ] {
        assert_eq!(insert(field), Ok(written.to_string()), "{field}");
    }
    // A parameter goes only where one belongs, holds no colon or space,
    // and never stands empty.
    for field in [
        "", "aff:", "aff:a:b", "hp:", ":hp", "aff: x", "aff:a b", "h.p", "aff:x;y",
    ] {
        assert_eq!(
            insert(field),
            Err(EditError("Vosh does not know that value.".into())),
            "{field}"
        );
    }
}

#[test]
fn removing_a_piece_keeps_the_look_it_handed_on() {
    let template = "%s_italic(%hp) %mana";
    let edited = edit(template, &EditOp::Remove { piece: 0 });
    assert_eq!(edited, "%s_italic%hp) %mana");
    assert_eq!(others(&edited, true, &[]), others(template, true, &[0]));

    // A condition goes with its end, and the pieces inside stay.
    let edited = edit("a%{if:fight}%hp%{end}b", &EditOp::Remove { piece: 1 });
    assert_eq!(edited, "a%{hp}b");
    let edited = edit("a%{if:fight}%hp%{end}b", &EditOp::Remove { piece: 3 });
    assert_eq!(edited, "a%{hp}b");
    // Empty text removes the piece.
    let edited = edit(
        "a %hp b",
        &EditOp::SetText {
            piece: 0,
            text: String::new(),
        },
    );
    assert_eq!(edited, "%hp b");
}

#[test]
fn moving_a_piece_keeps_its_look_and_every_other_one() {
    let template = "%c_red%hp %c_blue%mana %move";
    let moved = edit(template, &EditOp::Move { piece: 0, to: 4 });
    // The moved piece takes its own color with it, and the move after it
    // keeps the blue it had.
    let every = |t: &str| {
        let mut all: Vec<Part> = parts(t, true).into_iter().map(|(_, p)| p).collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    };
    assert_eq!(every(&moved), every(template), "{moved}");
    assert_eq!(moved, "%c_red %c_blue%mana %c_red%hp%c_blue%move");
    assert_reads_clean(&moved);
    // Moving a piece to where it already is changes nothing.
    assert_eq!(edit(template, &EditOp::Move { piece: 1, to: 2 }), template);
}

#[test]
fn when_wraps_a_piece_in_a_fight_condition_that_ends_as_it_started() {
    let template = "%c_red%hp %mana";
    let edited = edit(
        template,
        &EditOp::SetWhen {
            piece: 0,
            when: When::Fight,
        },
    );
    assert_eq!(edited, "%{if:fight}%c_red%hp%c_default%{end}%c_red %mana");
    // In a fight every piece looks as it did. Out of one the health goes
    // and every other piece still looks as it did.
    assert_eq!(others(&edited, true, &[]), others(template, true, &[]));
    assert_eq!(others(&edited, false, &[]), others(template, false, &[0]));

    // A piece next to a fight condition shares it.
    let shared = edit(
        &edited,
        &EditOp::SetWhen {
            piece: 4,
            when: When::Fight,
        },
    );
    assert_eq!(others(&shared, true, &[]), others(template, true, &[]));
    assert_eq!(
        Template::parse(&shared)
            .pieces()
            .iter()
            .filter(|p| p.kind == PieceKind::If)
            .count(),
        1,
        "{shared}"
    );

    // Always takes it out again.
    let back = edit(
        &edited,
        &EditOp::SetWhen {
            piece: 1,
            when: When::Always,
        },
    );
    assert_eq!(others(&back, true, &[]), others(template, true, &[]));
    assert_eq!(others(&back, false, &[]), others(template, false, &[]));
    assert!(!back.contains("%{if:fight}"), "{back}");

    // Out of a fight is the other condition.
    let out_of = edit(
        template,
        &EditOp::SetWhen {
            piece: 2,
            when: When::NotFight,
        },
    );
    assert!(out_of.contains("%{ifnot:fight}"), "{out_of}");
    assert_reads_clean(&out_of);
}

#[test]
fn when_splits_a_condition_to_take_out_a_piece_in_its_middle() {
    let template = "%{if:fight}a%hp b%{end}";
    let edited = edit(
        template,
        &EditOp::SetWhen {
            piece: 2,
            when: When::Always,
        },
    );
    assert_eq!(edited, "%{if:fight}a%{end}%hp%{if:fight} b%{end}");
    assert_eq!(others(&edited, true, &[]), others(template, true, &[]));
}

#[test]
fn detailed_keeps_its_looks_through_every_op_on_every_piece() {
    keeps_its_looks_through_every_op_on_every_piece(DETAILED);
}

/// At a glance as it first shipped, Vosh's first default design, whose
/// conditions nest.
const AT_A_GLANCE_FIRST: &str = vosh_prompt::config::RETIRED_DEFAULTS[1];

#[test]
fn every_preset_keeps_its_looks_through_every_op_on_every_piece() {
    let game = vosh_prompt::card::presets::same_as_the_game(
        PROMPT,
        "",
        vosh_prompt::aabahran::Who::default(),
    );
    assert!(game.is_some());
    for preset in vosh_prompt::card::presets::aabahran(game) {
        keeps_its_looks_through_every_op_on_every_piece(&preset.template);
    }
    keeps_its_looks_through_every_op_on_every_piece(JAMES);
    keeps_its_looks_through_every_op_on_every_piece(AT_A_GLANCE_FIRST);
    keeps_its_looks_through_every_op_on_every_piece(vosh_prompt::DEFAULT_DESIGN);
}

/// Every style, color and removal on every piece of `design` keeps the
/// look of every other piece, in a fight and out of one.
fn keeps_its_looks_through_every_op_on_every_piece(design: &str) {
    let count = Template::parse(design).pieces().len();
    let mut ops = Vec::new();
    for piece in 0..count {
        ops.push(EditOp::SetStyle {
            piece,
            style: StyleChoice::Underline,
            on: true,
        });
        ops.push(EditOp::SetColor {
            piece,
            color: ColorChoice::Index { index: 208 },
            background: false,
            underline: false,
        });
        ops.push(EditOp::SetStyle {
            piece,
            style: StyleChoice::Curly,
            on: true,
        });
        ops.push(EditOp::SetStyle {
            piece,
            style: StyleChoice::Strike,
            on: true,
        });
        ops.push(EditOp::SetStyle {
            piece,
            style: StyleChoice::Blink,
            on: true,
        });
        ops.push(EditOp::SetColor {
            piece,
            color: ColorChoice::Rgb {
                r: 191,
                g: 97,
                b: 106,
            },
            background: false,
            underline: true,
        });
        ops.push(EditOp::SetColor {
            piece,
            color: ColorChoice::Named { index: 4 },
            background: true,
            underline: false,
        });
        ops.push(EditOp::Remove { piece });
        ops.push(EditOp::InsertText {
            at: piece,
            text: "x".into(),
        });
        ops.push(EditOp::InsertRight { at: piece });
        ops.push(EditOp::Move {
            piece,
            to: count.min(piece + 3),
        });
    }
    for op in &ops {
        let Ok(edited) = apply(design, op, &known) else {
            continue;
        };
        assert_reads_clean(&edited);
        // A removed condition takes its end with it, and runs of text it
        // held apart read as one piece.
        let reparsed = Template::parse(&edited);
        assert!(
            reparsed.pieces().len() + 4 >= count,
            "{op:?} lost pieces: {edited}"
        );
        // A style or a color changes its own piece alone, in a fight and
        // out of one.
        if let EditOp::SetStyle { piece, .. } | EditOp::SetColor { piece, .. } = op {
            for fight in [true, false] {
                assert_eq!(
                    others(&edited, fight, &[*piece]),
                    others(design, fight, &[*piece]),
                    "{op:?} fight {fight}: {edited}"
                );
            }
        }
        // Taking out a piece that shows leaves the rest as they were.
        if let EditOp::Remove { piece } = op {
            let kind = Template::parse(design).pieces()[*piece].kind;
            if !matches!(
                kind,
                PieceKind::If | PieceKind::IfNot | PieceKind::End | PieceKind::Nl
            ) {
                for fight in [true, false] {
                    assert_eq!(
                        others(&edited, fight, &[]),
                        others(design, fight, &[*piece]),
                        "{op:?} fight {fight}: {edited}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_look_set_inside_a_condition_comes_back_inside_it() {
    // In a fight the opponent's red carries on to your health, and out of
    // one your health draws in the terminal's color.
    let design = "%{if:fight}%c_red%opponent%{end} %hp";
    let blue = EditOp::SetColor {
        piece: 1,
        color: ColorChoice::Named { index: 4 },
        background: false,
        underline: false,
    };
    let edited = edit(design, &blue);
    assert_eq!(edited, "%{if:fight}%c_blue%opponent%c_red%{end} %hp");
    for fight in [true, false] {
        assert_eq!(
            others(&edited, fight, &[1]),
            others(design, fight, &[1]),
            "fight {fight}"
        );
    }
    // Taking the opponent out keeps your health's look in a fight and out
    // of one.
    let removed = edit(design, &EditOp::Remove { piece: 1 });
    for fight in [true, false] {
        assert_eq!(
            others(&removed, fight, &[]),
            others(design, fight, &[1]),
            "fight {fight}: {removed}"
        );
    }
    // A color before a condition comes back before it, so the text after
    // the condition keeps its look out of a fight.
    let design = "%c_red%hp%{if:fight} %opponent%{end} X";
    let blue = EditOp::SetColor {
        piece: 0,
        color: ColorChoice::Named { index: 4 },
        background: false,
        underline: false,
    };
    let edited = edit(design, &blue);
    assert_eq!(edited, "%c_blue%hp%c_red%{if:fight} %opponent%{end} X");
    for fight in [true, false] {
        assert_eq!(
            others(&edited, fight, &[0]),
            others(design, fight, &[0]),
            "fight {fight}"
        );
    }
}

/// The edit's text and where the piece it acted on now sits.
fn edit_at(template: &str, op: &EditOp) -> (String, Option<usize>) {
    vosh_prompt::card::edit::apply_at(template, op, &known)
        .unwrap_or_else(|e| panic!("{op:?} on {template}: {e}"))
}

/// The template text of piece `index`.
fn piece_text(template: &str, index: usize) -> String {
    Template::parse(template).piece_text(index).to_string()
}

#[test]
fn the_card_follows_the_piece_an_edit_acted_on() {
    // A style keeps the piece where it was.
    let hp = Template::parse(JAMES)
        .pieces()
        .iter()
        .position(|p| p.kind == PieceKind::Value)
        .expect("the hp value");
    let (text, at) = edit_at(
        JAMES,
        &EditOp::SetStyle {
            piece: hp,
            style: StyleChoice::Bold,
            on: true,
        },
    );
    assert_eq!(at, Some(hp));
    assert!(piece_text(&text, hp).ends_with("%hp"));
    // In a fight puts a condition before it, so it moves on by one.
    let (text, at) = edit_at(
        JAMES,
        &EditOp::SetWhen {
            piece: hp,
            when: When::Fight,
        },
    );
    assert_eq!(at, Some(hp + 1));
    assert!(piece_text(&text, hp + 1).ends_with("%hp"));
    // A value added at the end is the last piece that shows.
    let n = Template::parse("[%hp]").pieces().len();
    let (text, at) = edit_at(
        "[%hp]",
        &EditOp::InsertField {
            at: n,
            field: "gold".into(),
            format: None,
        },
    );
    assert_eq!(text, "[%hp]%gold");
    assert_eq!(at.map(|i| piece_text(&text, i)), Some("%gold".into()));
    // Text added next to text joins it, and the card picks the whole.
    let (text, at) = edit_at(
        "[%hp]",
        &EditOp::InsertText {
            at: 3,
            text: " ok".into(),
        },
    );
    assert_eq!(text, "[%hp] ok");
    assert_eq!(at.map(|i| piece_text(&text, i)), Some("] ok".into()));
    // A line break added is picked.
    let (text, at) = edit_at("[%hp]", &EditOp::InsertNl { at: 2 });
    assert_eq!(at.map(|i| piece_text(&text, i)), Some("%nl".into()));
    // A moved piece is where it landed.
    let (text, at) = edit_at("%hp %mana", &EditOp::Move { piece: 2, to: 0 });
    assert_eq!(text, "%mana%hp ");
    assert_eq!(at, Some(0));
    // A removed one is gone.
    assert_eq!(edit_at("%hp %mana", &EditOp::Remove { piece: 0 }).1, None);
    // Empty text removes, and nothing is added.
    assert_eq!(
        edit_at(
            "[%hp]",
            &EditOp::InsertText {
                at: 0,
                text: String::new()
            }
        )
        .1,
        None
    );
}

#[test]
fn a_push_to_the_right_goes_in_moves_and_takes_when_as_a_line_break_does() {
    // The picker's Push to the right edge adds it at the caret, and the
    // card picks it.
    let (text, at) = edit_at("<%hp> %c_cyan%mana", &EditOp::InsertRight { at: 3 });
    assert_eq!(text, "<%hp> %{right}%c_cyan%mana");
    assert_eq!(at.map(|i| piece_text(&text, i)), Some("%{right}".into()));
    assert_reads_clean(&text);
    // It shows only in a fight, and always again.
    let fight = edit(
        &text,
        &EditOp::SetWhen {
            piece: 3,
            when: When::Fight,
        },
    );
    assert_eq!(fight, "<%hp> %{if:fight}%{right}%{end}%c_cyan%mana");
    let always = edit(
        &fight,
        &EditOp::SetWhen {
            piece: 4,
            when: When::Always,
        },
    );
    assert_eq!(always, text);
    // It moves, and goes, as any part does.
    assert_eq!(
        edit(&text, &EditOp::Move { piece: 3, to: 0 }),
        "%{right}<%hp> %c_cyan%mana"
    );
    assert_eq!(
        edit(&text, &EditOp::Remove { piece: 3 }),
        "<%hp> %c_cyan%mana"
    );
    // It takes no color or style, since its cells are spaces.
    assert_eq!(
        apply(
            &text,
            &EditOp::SetColor {
                piece: 3,
                color: ColorChoice::Named { index: 1 },
                background: false,
                underline: false,
            },
            &known,
        ),
        Err(EditError("Only a part that shows can take a color.".into()))
    );
    // A look set before it keeps reaching past it, in its spaces too.
    let colored = edit("%c_red<%hp> %mana", &EditOp::InsertRight { at: 2 });
    assert_eq!(colored, "%c_red<%hp%{right}> %mana");
    assert_reads_clean(&colored);
}

#[test]
fn when_puts_a_line_break_in_a_fight_and_takes_it_out_again() {
    // The card offers When on a line break, and the picker's Line
    // break in a fight sets it on the break it adds.
    let template = "%hp%nl%mana";
    let fight = edit(
        template,
        &EditOp::SetWhen {
            piece: 1,
            when: When::Fight,
        },
    );
    assert_eq!(fight, "%hp%{if:fight}%nl%{end}%mana");
    assert_eq!(
        render_str(&fight, &Sampled { fight: false }, RenderOptions::default()).plain,
        "1020800"
    );
    let always = edit(
        &fight,
        &EditOp::SetWhen {
            piece: 2,
            when: When::Always,
        },
    );
    assert_eq!(always, template);

    // Detailed's fight line break can show always.
    let nl = Template::parse(DETAILED)
        .pieces()
        .iter()
        .position(|p| p.kind == PieceKind::Nl)
        .expect("Detailed breaks its fight line");
    let out = edit(
        DETAILED,
        &EditOp::SetWhen {
            piece: nl,
            when: When::Always,
        },
    );
    assert!(out.contains("%opponent_cond%{end}%nl%c_hp"), "{out}");
    assert_eq!(others(&out, true, &[]), others(DETAILED, true, &[]));
    assert_reads_clean(&out);

    // A line break the picker adds, then In a fight on it, as the card
    // sends them.
    let (added, at) = edit_at("%hp %mana", &EditOp::InsertNl { at: 2 });
    let at = at.expect("the break is picked");
    let fight = edit(
        &added,
        &EditOp::SetWhen {
            piece: at,
            when: When::Fight,
        },
    );
    assert_eq!(fight, "%hp %{if:fight}%nl%{end}%mana");
}

#[test]
fn show_as_reads_a_max_as_a_form_of_its_gauge() {
    // `%{maxhp}` after a color code stands alone, and the card shows it
    // as Health in the form Max with every form of Health to choose.
    let template = "[%c_hp%hp%c_default/%{maxhp}hp]";
    let max = Template::parse(template)
        .pieces()
        .iter()
        .position(|p| &template[p.start..p.end] == "%{maxhp}")
        .expect("the max is its own piece");
    let set = |format: FormatName| {
        edit(
            template,
            &EditOp::SetFormat {
                piece: max,
                format: FormatChoice::of(format),
            },
        )
    };
    assert_eq!(set(FormatName::Value), "[%c_hp%hp%c_default/%{hp}hp]");
    assert_eq!(
        set(FormatName::CurMax),
        "[%c_hp%hp%c_default/%hp/%{maxhp}hp]"
    );
    assert_eq!(set(FormatName::Max), template);
    assert_eq!(set(FormatName::Percent), "[%c_hp%hp%c_default/%pct_hp%%hp]");
    assert!(
        set(FormatName::Bar).contains("/%{hp:bar:10}hp]"),
        "{}",
        set(FormatName::Bar)
    );
    for format in [
        FormatName::Value,
        FormatName::CurMax,
        FormatName::Percent,
        FormatName::Bar,
    ] {
        assert_reads_clean(&set(format));
    }
}

#[test]
fn the_games_prompt_goes_in_as_the_raw_token() {
    let (text, at) = edit_at(
        "%hp ",
        &EditOp::InsertField {
            at: 2,
            field: "raw".into(),
            format: None,
        },
    );
    assert_eq!(text, "%hp %{raw}");
    assert_eq!(at.map(|i| piece_text(&text, i)), Some("%{raw}".into()));
    let (text, _) = edit_at(
        "",
        &EditOp::InsertField {
            at: 0,
            field: "raw".into(),
            format: Some(FormatChoice::of(FormatName::Value)),
        },
    );
    assert_eq!(text, "%{raw}");
}

#[test]
fn a_change_added_after_a_dim_part_draws_its_sign_color_until_you_color_it() {
    let draw = |template: &str| {
        render_str(
            template,
            &Sampled { fight: false },
            RenderOptions::default(),
        )
        .ansi
    };
    let added = edit(
        "%s_dim%c_gray[",
        &EditOp::InsertField {
            at: 1,
            field: "hp_change".into(),
            format: None,
        },
    );
    assert_eq!(added, "%s_dim%c_gray[%hp_change");
    assert_eq!(
        draw(&added),
        "\x1b[2m\x1b[90m[\x1b[22;32m+34\x1b[2;90m\x1b[0m"
    );
    // A color you give the value itself wins.
    let colored = edit(
        &added,
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Named { index: 6 },
            background: false,
            underline: false,
        },
    );
    assert_eq!(colored, "%s_dim%c_gray[%c_cyan%hp_change");
    assert_eq!(draw(&colored), "\x1b[2m\x1b[90m[\x1b[36m+34\x1b[0m");
}
