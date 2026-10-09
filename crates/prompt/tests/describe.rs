//! What the prompt card reads about a design: each piece's field, form,
//! When, color and style, what it reads now, the forms Show as and the
//! picker offer, and each token of the text with its piece.

use vosh_prompt::card::describe::{describe, forms, PieceView, TokenKindName};
use vosh_prompt::card::edit::{ColorChoice, FormatName, StyleChoice, When};
use vosh_prompt::config::DEFAULT_DESIGN;
use vosh_prompt::design::PieceKind;
use vosh_prompt::testkit::designs::{DETAILED, JAMES};
use vosh_prompt::testkit::now;
use vosh_prompt::values::Samples;
use vosh_prompt::{FieldRef, Resolved, Template, Values};

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

    fn label(&self, field: &FieldRef) -> String {
        Samples { now: now() }.label(field)
    }
}

/// The piece whose text is `text`.
fn piece<'a>(pieces: &'a [PieceView], text: &str) -> &'a PieceView {
    pieces
        .iter()
        .find(|p| p.text == text)
        .unwrap_or_else(|| panic!("no piece {text:?} in {pieces:#?}"))
}

fn segments(piece: &PieceView) -> Vec<&str> {
    piece.forms.iter().map(|f| f.segment.as_str()).collect()
}

#[test]
fn a_piece_reads_every_style_its_ground_and_its_underline() {
    let template =
        "%{bg:#3b4252}%s_dim%s_strike%s_inverse%s_blink%s_curly%{ul:#bf616a}%hp%s_off%bg_default x";
    let described = describe(&Template::parse(template), &Sampled { fight: false }, false);
    let hp = &described.pieces[0];
    assert_eq!(
        hp.background,
        ColorChoice::Rgb {
            r: 0x3b,
            g: 0x42,
            b: 0x52
        }
    );
    assert!(hp.dim && hp.strike && hp.inverse && hp.blink && hp.underline);
    assert!(!hp.bold && !hp.italic);
    assert_eq!(hp.underline_style, Some(StyleChoice::Curly));
    assert_eq!(
        hp.underline_color,
        ColorChoice::Rgb {
            r: 191,
            g: 97,
            b: 106
        }
    );
    // `%s_off` ends every style and keeps the underline's color, and
    // the terminal's ground reads as its own.
    let x = &described.pieces[1];
    assert_eq!(x.background, ColorChoice::Default);
    assert!(!x.dim && !x.strike && !x.inverse && !x.blink && !x.underline);
    assert_eq!(x.underline_style, None);
    assert_eq!(x.underline_color, hp.underline_color);
    // The single line reads as Underline, and a plain piece as nothing.
    let described = describe(
        &Template::parse("%s_underline%hp"),
        &Sampled { fight: false },
        false,
    );
    assert_eq!(
        described.pieces[0].underline_style,
        Some(StyleChoice::Underline)
    );
    assert_eq!(described.pieces[0].underline_color, ColorChoice::Default);
    assert_eq!(described.pieces[0].background, ColorChoice::Default);
}

#[test]
fn the_hp_value_reads_as_health_with_its_own_codes_and_inherited_italic() {
    // The hp value piece picked in his template.
    let described = describe(&Template::parse(JAMES), &Sampled { fight: false }, false);
    let hp = piece(&described.pieces, "%c_reset%s_italic%hp");
    assert_eq!(hp.kind, PieceKind::Value);
    assert_eq!(hp.label, "Health");
    assert_eq!(hp.field.as_deref(), Some("hp"));
    assert_eq!(hp.format, Some(FormatName::Value));
    assert_eq!(hp.meta.as_deref(), Some("1020 of 1020"));
    assert_eq!(hp.color, ColorChoice::Default);
    assert!(hp.italic && !hp.bold && !hp.underline);
    assert_eq!((hp.when, hp.when_fixed), (When::Always, false));
    assert!(hp.by_value && hp.shows);
    assert_eq!(
        segments(hp),
        ["1020", "1020/1020", "100%", "Game percent, no sign", "Bar"]
    );
    // The percent after it is By value, and Show as adds the percent with
    // no sign it shows now.
    let pct = piece(&described.pieces, "%c_hp%pct_hp");
    assert_eq!(
        pct.color,
        ColorChoice::ByValue {
            field: None,
            game: false,
            steps: false
        }
    );
    assert_eq!(pct.format, Some(FormatName::Pct));
    assert_eq!(
        segments(pct),
        [
            "1020",
            "1020/1020",
            "100%",
            "Game percent, no sign",
            "Bar",
            "100"
        ]
    );
    // The mana percent keeps its own true color.
    let mana = piece(&described.pieces, "%{c:128,200,255}%pct_mana");
    assert_eq!(
        mana.color,
        ColorChoice::Rgb {
            r: 128,
            g: 200,
            b: 255
        }
    );
    // The bracket is text in rgb 100 100 100.
    let bracket = piece(&described.pieces, "%{c:100,100,100}[");
    assert_eq!(bracket.kind, PieceKind::Text);
    assert_eq!(bracket.label, "Text");
    assert_eq!(bracket.literal.as_deref(), Some("["));
    assert_eq!(
        bracket.color,
        ColorChoice::Rgb {
            r: 100,
            g: 100,
            b: 100
        }
    );
    assert!(!bracket.by_value && bracket.forms.is_empty() && bracket.meta.is_none());
    // `%)h` stays text, a percent and all.
    let tail = piece(&described.pieces, "%c_reset%s_italic%)h ");
    assert_eq!(tail.literal.as_deref(), Some("%)h "));
}

#[test]
fn a_bar_in_a_fight_section_reads_in_a_fight_with_its_own_color() {
    // The opponent bar of Detailed under the Fight preview.
    let described = describe(&Template::parse(DETAILED), &Sampled { fight: true }, true);
    let bar = piece(&described.pieces, "%{opponent_hp:bar:10}");
    assert_eq!(bar.label, "Opponent health");
    assert_eq!(bar.format, Some(FormatName::Bar));
    assert_eq!(bar.width, Some(10));
    assert_eq!((bar.when, bar.when_fixed), (When::Fight, false));
    assert_eq!(
        bar.color,
        ColorChoice::ByValue {
            field: None,
            game: false,
            steps: false
        }
    );
    assert_eq!(bar.meta.as_deref(), Some("60 percent in this preview"));
    assert_eq!(segments(bar), ["60%", "Bar"]);
    // The line break in the same section.
    let nl = piece(&described.pieces, "%nl");
    assert_eq!(nl.kind, PieceKind::Nl);
    assert_eq!(nl.label, "Line break");
    assert_eq!((nl.when, nl.when_fixed), (When::Fight, false));
    assert!(nl.forms.is_empty() && nl.field.is_none());
    // The gold out of any fight section shows always.
    let gold = piece(&described.pieces, "%{gold}");
    assert_eq!((gold.when, gold.when_fixed), (When::Always, false));
    // A condition takes no cells.
    let cond = piece(&described.pieces, "%{if:fight}");
    assert_eq!(cond.kind, PieceKind::If);
    assert!(!cond.shows);
}

#[test]
fn a_fight_condition_outside_another_one_holds_the_part_in_place() {
    // The default design's tank sits in %{if:tank} inside %{if:fight}.
    let described = describe(
        &Template::parse(DEFAULT_DESIGN),
        &Sampled { fight: true },
        false,
    );
    let tank = described
        .pieces
        .iter()
        .find(|p| p.field.as_deref() == Some("tank"))
        .expect("the tank");
    assert_eq!((tank.when, tank.when_fixed), (When::Fight, true));
    assert_eq!(tank.meta.as_deref(), Some("Brask"));
}

#[test]
fn a_max_alone_shows_as_its_gauge_in_the_form_max() {
    let described = describe(
        &Template::parse("%opponent %{maxhp}"),
        &Sampled { fight: false },
        false,
    );
    let opponent = &described.pieces[0];
    assert_eq!(opponent.label, "Opponent");
    // A max alone is Health in the form Max.
    let max = piece(&described.pieces, "%{maxhp}");
    assert_eq!(max.field.as_deref(), Some("hp"));
    assert_eq!(max.format, Some(FormatName::Max));
    assert_eq!(
        segments(max),
        [
            "1020",
            "1020/1020",
            "1020",
            "100%",
            "Game percent, no sign",
            "Bar"
        ],
        "Show as keeps the form it shows now"
    );
}

#[test]
fn the_picker_offers_every_form_with_a_live_sample() {
    let values = Sampled { fight: false };
    let hp: Vec<(&str, String)> = forms(&FieldRef::new("hp"), &values)
        .iter()
        .map(|f| (f.label, f.sample.plain.clone()))
        .collect();
    assert_eq!(
        hp,
        [
            ("Current", "1020".to_string()),
            ("Current and max", "1020/1020".to_string()),
            ("Max", "1020".to_string()),
            ("Percent", "100%".to_string()),
            ("Game percent, no sign", "100".to_string()),
            ("Bar", "██████████".to_string()),
        ]
    );
    // Gold, a number, groups its thousands, shortens, and reads in
    // thousands as the old TinTin prompt wrote it.
    let gold: Vec<(&str, String)> = forms(&FieldRef::new("gold"), &values)
        .iter()
        .map(|f| (f.label, f.sample.plain.clone()))
        .collect();
    assert_eq!(
        gold,
        [
            ("Number", "1250".to_string()),
            ("Grouped", "1,250".to_string()),
            ("Short", "1.2k".to_string()),
            ("Thousands", "1.2K".to_string()),
        ]
    );
    // The game hour reads as a number, as Vosh's clock, and as the old
    // TinTin prompt wrote it.
    let hour: Vec<(&str, String)> = forms(&FieldRef::new("hour"), &values)
        .iter()
        .map(|f| (f.label, f.sample.plain.clone()))
        .collect();
    assert_eq!(
        hour,
        [
            ("Number", "14".to_string()),
            ("Clock", "2 pm".to_string()),
            ("Compact clock", "2PM".to_string()),
        ]
    );
    // The tick counts down, and up from when it last turned.
    let tick: Vec<(&str, String)> = forms(&FieldRef::new("tick"), &values)
        .iter()
        .take(3)
        .map(|f| (f.label, f.segment.clone()))
        .collect();
    assert_eq!(
        tick,
        [
            ("Seconds", "14".to_string()),
            ("With unit", "14s".to_string()),
            ("Since the tick", "46s".to_string()),
        ]
    );
    // The bar draws in theme green at full.
    let bar = &forms(&FieldRef::new("hp"), &values)[5];
    assert!(
        bar.sample.ansi.starts_with("\x1b[32m"),
        "{:?}",
        bar.sample.ansi
    );
    // Exits read as letters and in the game's style.
    let exits: Vec<(&str, String)> = forms(&FieldRef::new("exits"), &values)
        .iter()
        .map(|f| (f.label, f.sample.plain.clone()))
        .collect();
    assert_eq!(
        exits,
        [
            ("Letters", "S".to_string()),
            ("Game style", "[Exits: S]".to_string())
        ]
    );
    // Position reads short, as a word and in the game's style.
    let pos: Vec<&str> = forms(&FieldRef::new("pos"), &values)
        .iter()
        .map(|f| f.label)
        .collect();
    assert_eq!(pos, ["Short", "Word", "Game style"]);
    // A name only a script sets reads as text.
    let script: Vec<&str> = forms(&FieldRef::new("my_count"), &values)
        .iter()
        .map(|f| f.label)
        .collect();
    assert_eq!(script, ["Text"]);
}

#[test]
fn tokens_name_their_piece_in_utf16_units_and_mark_unknown_names() {
    let template = Template::parse("%c_hp%hp ♥ %nope%nl%{if:fight}x%{end}");
    let described = describe(&template, &Sampled { fight: false }, false);
    let tokens: Vec<(usize, usize, usize, TokenKindName, bool)> = described
        .tokens
        .iter()
        .map(|t| (t.start, t.end, t.piece, t.kind, t.known))
        .collect();
    assert_eq!(
        tokens,
        [
            (0, 5, 0, TokenKindName::Code, true),
            (5, 8, 0, TokenKindName::Value, true),
            (8, 11, 1, TokenKindName::Text, true),
            (11, 16, 2, TokenKindName::Value, false),
            (16, 19, 3, TokenKindName::Line, true),
            (19, 30, 4, TokenKindName::Condition, true),
            (30, 31, 5, TokenKindName::Text, true),
            (31, 37, 6, TokenKindName::Condition, true),
        ]
    );
    assert_eq!(described.tokens[3].name.as_deref(), Some("nope"));
}

const COLORED: &str = "[%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv]";

/// The design after Show as sets `format` on the piece whose text is
/// `text`, as the card writes it.
fn show_as(template: &str, text: &str, format: FormatName) -> String {
    let described = describe(&Template::parse(template), &Sampled { fight: false }, false);
    let at = piece(&described.pieces, text).piece;
    vosh_prompt::card::edit::apply(
        template,
        &vosh_prompt::card::edit::EditOp::SetFormat {
            piece: at,
            format: vosh_prompt::card::edit::FormatChoice::of(format),
        },
        &|_: &FieldRef| true,
    )
    .expect("the edit")
}

/// The piece that shows `field` as `kind` in `template`.
fn shown(template: &str, field: &str, kind: PieceKind) -> PieceView {
    describe(&Template::parse(template), &Sampled { fight: false }, false)
        .pieces
        .into_iter()
        .find(|p| p.field.as_deref() == Some(field) && p.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} {field} in {template:?}"))
}

#[test]
fn show_as_marks_current_and_max_and_percent_once_you_choose_them() {
    // Colored by how full, the mana part shown as Current and max.
    let cur_max = show_as(COLORED, "%c_mana%mana", FormatName::CurMax);
    let mana = shown(&cur_max, "mana", PieceKind::CurMax);
    assert_eq!(mana.format, Some(FormatName::CurMax));
    assert_eq!(
        segments(&mana),
        ["800", "800/800", "100%", "Game percent, no sign", "Bar"]
    );

    // Then as Percent, with no fifth segment for a percent with no sign.
    let percent = show_as(&cur_max, &mana.text, FormatName::Percent);
    let mana = shown(&percent, "mana", PieceKind::Percent);
    assert_eq!(mana.format, Some(FormatName::Percent));
    assert_eq!(
        segments(&mana),
        ["800", "800/800", "100%", "Game percent, no sign", "Bar"]
    );

    // Health as Percent, with the label text hp right after its sign.
    let hp = show_as(COLORED, "%c_hp%hp", FormatName::Percent);
    let health = shown(&hp, "hp", PieceKind::Percent);
    assert_eq!(health.format, Some(FormatName::Percent));
    assert_eq!(
        segments(&health),
        ["1020", "1020/1020", "100%", "Game percent, no sign", "Bar"]
    );
}

/// Mana at 300 of 800, 37.5 percent, where the rounded and the cut
/// percent part.
struct HalfMana;

impl Values for HalfMana {
    fn resolve(&self, field: &FieldRef) -> Resolved {
        match field.name.as_str() {
            "mana" => Resolved::Value(vosh_prompt::Value::Gauge {
                cur: 300,
                max: Some(800),
                pct: None,
            }),
            _ => Samples { now: now() }.resolve(field),
        }
    }

    fn label(&self, field: &FieldRef) -> String {
        Samples { now: now() }.label(field)
    }
}

#[test]
fn the_game_percent_reads_as_its_own_form_with_the_cut_percent() {
    // The picker draws it as the game cuts it, next to the rounded one.
    let mana: Vec<(&str, String)> = forms(&FieldRef::new("mana"), &HalfMana)
        .iter()
        .filter(|f| f.label.contains("ercent"))
        .map(|f| (f.label, f.sample.plain.clone()))
        .collect();
    assert_eq!(
        mana,
        [
            ("Percent", "38%".to_string()),
            ("Game percent, no sign", "37".to_string())
        ]
    );
    // A piece in it reads as Health in the form Game percent, no sign,
    // which Show as marks by its name, so you see the sign goes.
    let described = describe(
        &Template::parse("%{c:#d0d0d0}%{hp:pct:game}%{c:hp:steps}%%"),
        &Sampled { fight: false },
        false,
    );
    let hp = piece(&described.pieces, "%{c:#d0d0d0}%{hp:pct:game}");
    assert_eq!(hp.kind, PieceKind::Value);
    assert_eq!(hp.label, "Health");
    assert_eq!(hp.format, Some(FormatName::PctGame));
    assert_eq!(
        segments(hp),
        ["1020", "1020/1020", "100%", "Game percent, no sign", "Bar"]
    );
    // Its sign stays text of its own, so it keeps its own color.
    let sign = piece(&described.pieces, "%{c:hp:steps}%%");
    assert_eq!(sign.kind, PieceKind::Text);
    // Show as writes it, and back.
    let game = show_as(COLORED, "%c_mana%mana", FormatName::PctGame);
    assert!(
        game.contains("%c_mana%{mana:pct:game}%c_default/"),
        "{game}"
    );
    let back = show_as(&game, "%c_mana%{mana:pct:game}", FormatName::Value);
    assert_eq!(back, COLORED);
}

#[test]
fn a_color_by_steps_reads_as_by_value_with_steps() {
    let described = describe(
        &Template::parse("%pct_hp%{c:hp:steps}%% %{c:mana:steps}%mana"),
        &Sampled { fight: false },
        false,
    );
    // The sign takes the steps of Health, another value than its own.
    let sign = piece(&described.pieces, "%{c:hp:steps}%% ");
    assert_eq!(
        sign.color,
        ColorChoice::ByValue {
            field: Some("hp".into()),
            game: false,
            steps: true
        }
    );
    // Mana takes the steps of its own value.
    let mana = piece(&described.pieces, "%{c:mana:steps}%mana");
    assert_eq!(
        mana.color,
        ColorChoice::ByValue {
            field: None,
            game: false,
            steps: true
        }
    );
}

#[test]
fn a_push_to_the_right_reads_as_the_right_edge_and_a_layout_token() {
    let template = Template::parse("<%hp>%{right}%mana");
    let described = describe(&template, &Sampled { fight: false }, false);
    let push = piece(&described.pieces, "%{right}");
    assert_eq!(push.kind, PieceKind::Right);
    assert_eq!(push.label, "Right edge");
    assert_eq!((push.when, push.when_fixed), (When::Always, false));
    assert!(push.forms.is_empty() && push.field.is_none() && !push.shows);
    // Edit as text colors it as it colors a line break.
    let token = described
        .tokens
        .iter()
        .find(|t| t.piece == push.piece)
        .expect("its token");
    assert_eq!((token.start, token.end), (5, 13));
    assert_eq!(token.kind, TokenKindName::Line);
    assert!(token.known);
}

#[test]
fn a_change_reads_the_look_it_draws_in() {
    let template = "%s_dim%c_gray[%hp_change]%c_cyan%s_dim%mana_change";
    let described = describe(&Template::parse(template), &Sampled { fight: false }, false);
    // A color or dim before it does not apply to its sign color.
    let hp = piece(&described.pieces, "%hp_change");
    assert_eq!(hp.color, ColorChoice::Default);
    assert!(!hp.dim);
    // A color and dim of its own do.
    let mana = piece(&described.pieces, "%c_cyan%s_dim%mana_change");
    assert_eq!(mana.color, ColorChoice::Named { index: 6 });
    assert!(mana.dim);
    // So does a dim of its own with no color.
    let own = describe(
        &Template::parse("%c_gray[%s_dim%hp_change"),
        &Sampled { fight: false },
        false,
    );
    let hp = piece(&own.pieces, "%s_dim%hp_change");
    assert_eq!(hp.color, ColorChoice::Default);
    assert!(hp.dim);
}
