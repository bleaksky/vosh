//! `prompt_edit` tests (section 9, piece looks). Every op keeps the text
//! and the look of every piece it does not change, and the writer never
//! writes a token that reads back as something else.

use chrono::{NaiveDate, NaiveDateTime};
use vosh_prompt::edit::{
    apply, ColorChoice, EditError, EditOp, FormatChoice, FormatName, StyleChoice, When,
};
use vosh_prompt::template::{PieceKind, TokenKind};
use vosh_prompt::vars::Samples;
use vosh_prompt::{render_str, FieldRef, RenderOptions, Resolved, SpanColor, Template, Values};

const JAMES: &str = "%{c:100,100,100}[%c_reset%s_italic%hp(%c_hp%pct_hp%c_reset%s_italic%)h %mana(%{c:128,200,255}%pct_mana%c_reset%s_italic%)m %move(%{c:200,255,23}%pct_move%c_reset%s_italic%)v%c_reset%{c:100,100,100}] %c_reset";

const DETAILED: &str = "%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %opponent_cond%nl%{end}%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv %{c:8}tick%c_default %tick%{if:exits} %{c:8}[%c_default%exits%{c:8}]%c_default%{end} %{gold}g%{if:missing} %c_3%missing missing%c_default%{end}";

fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 29)
        .and_then(|d| d.and_hms_opt(8, 42, 10))
        .expect("a valid date")
}

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

/// Each drawn piece's text and look, in the order the pieces draw.
type Part = (String, SpanColor, SpanColor, bool, bool, bool);

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
            (s.piece, (text, s.fg, s.bg, s.bold, s.italic, s.underline))
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
                if (last.1, last.2, last.3, last.4, last.5)
                    == (part.1, part.2, part.3, part.4, part.5) =>
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

#[test]
fn a_color_keeps_the_pieces_after_it_in_theirs() {
    let template = "%c_red[%hp] %mana";
    let edited = edit(
        template,
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Named { index: 2 },
            background: false,
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
            },
            background: false,
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
            },
            background: false,
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
    for template in [
        "[%c_hp%{hp}hp]",
        "[%c_hp%pct_hp%%hp]",
        "[%c_hp%{hp:bar:6}hp]",
        "x %{pos:word} y",
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
}

#[test]
fn a_bar_takes_its_color_as_its_own() {
    let edited = edit(
        "x %{hp:bar:6} y",
        &EditOp::SetColor {
            piece: 1,
            color: ColorChoice::Named { index: 4 },
            background: false,
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
            },
            background: false,
        },
    );
    assert_eq!(edited, "x %{hp:bar:6} y");
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

/// At a glance, Vosh's first default design as the review approved it,
/// whose conditions nest.
const AT_A_GLANCE: &str = "%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %{c:245}%opponent_cond%c_default%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}%nl%{end}%c_hp%hp%{c:245}/%{maxhp}hp%c_default %c_mana%mana%{c:245}/%{maxmana}mn%c_default %c_move%move%{c:245}/%{maxmove}mv%c_default%{if:pos}  %{c:245}%pos%c_default%{end}%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}%{ifnot:fight}%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}%{end}%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end} ";

#[test]
fn every_preset_keeps_its_looks_through_every_op_on_every_piece() {
    let game = vosh_prompt::presets::same_as_the_game(
        "%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c",
        "",
        vosh_prompt::aabahran::Who::default(),
    );
    assert!(game.is_some());
    for preset in vosh_prompt::presets::aabahran(game) {
        keeps_its_looks_through_every_op_on_every_piece(&preset.template);
    }
    keeps_its_looks_through_every_op_on_every_piece(JAMES);
    keeps_its_looks_through_every_op_on_every_piece(AT_A_GLANCE);
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
        });
        ops.push(EditOp::Remove { piece });
        ops.push(EditOp::InsertText {
            at: piece,
            text: "x".into(),
        });
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
    vosh_prompt::edit::apply_at(template, op, &known)
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
