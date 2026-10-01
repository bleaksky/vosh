//! Designs Vosh ships (section 7.1 of the build spec and the 2026-09-30
//! addendum, item 10): its default, and the ones the card offers to
//! start from.
//!
//! On Aabahran every preset draws from the game's codes and GMCP, which
//! sends every value they read. Same as the game is written from your
//! PROMPT setting, every code as its piece in the game's look. Another
//! game's presets use only the values its capture and GMCP supply.
//!
//! Every preset but Start empty ends in a space, as the game's own
//! prompt does, so your echo starts a cell after it.

use serde::Serialize;

use crate::aabahran::codes::Code as GameCode;
use crate::aabahran::colors::Color as GameColor;
use crate::aabahran::lex::{self, Piece as GamePiece, Token as GameToken};
use crate::aabahran::{Which, Who};
use crate::template::{
    write_tokens, Code, ColorSpec, FieldRef, Format, Style, TokenKind, ValueRef,
};

/// Vosh's default, the design Vosh draws for a profile that has none of
/// its own. It mirrors the pinned band of the gallery mockup, with the
/// tank in place of your opponent, as James asked on 2026-09-30.
///
/// Out of a fight it draws one row: your health, mana and moves as
/// current over max, or current alone when nothing gives the max, then
/// the exits in brackets and your gold. In a fight a row comes first
/// with the tank's name, a colon and the tank's health as a gauge ten
/// cells wide, the gauge the mockup gave your opponent. Solo you are the
/// tank, so it names you.
///
/// Only your three numbers and the gauge carry color, by one scale:
/// green above two thirds, yellow above one third, red below. Every
/// label, max and bracket is 256 color 245, a middle gray, and the
/// tank's name keeps the terminal's color. Since the design reads the
/// tank, the game's tank line folds into the tank row, so the prompt
/// takes two rows at most. It ends in a space, as the game's own prompt
/// does, so your echo never touches it.
pub const DEFAULT_DESIGN: &str = concat!(
    // The tank row, only in a fight and only when Vosh knows the tank.
    // The gauge draws only when it knows the tank's health, hidden
    // included, so a PROMPT with %n and no %P never prints it as typed.
    "%{if:fight}%{if:tank}%tank:%{if:tank_hp} %{tank_hp:bar:10}%{end}%nl%{end}%{end}",
    // Your vitals, each current over max and colored by how full, or
    // current alone in the terminal's color when nothing gives the max.
    // One that does not apply, such as mana for a class with none on
    // another game, draws nothing.
    "%{if:hp}%{if:maxhp}%c_hp%{end}%hp%{c:245}%{if:maxhp}/%{maxhp}%{end}hp%c_default%{end}",
    "%{if:mana} %{if:maxmana}%c_mana%{end}%mana%{c:245}%{if:maxmana}/%{maxmana}%{end}mn%c_default%{end}",
    "%{if:move} %{if:maxmove}%c_move%{end}%move%{c:245}%{if:maxmove}/%{maxmove}%{end}mv%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    " ",
);

/// At a glance, Vosh's default from 2026-09-30 until James asked for the
/// mockup's band. In a fight its top row named your opponent with a
/// gauge, the percent and the game's condition words, then the tank in a
/// group. Its vitals row added your position and language, Wizi and
/// Incog, and the tracked affects you were missing.
pub const AT_A_GLANCE: &str = concat!(
    // The fight row.
    "%{if:fight}",
    "%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% ",
    "%{c:245}%opponent_cond%c_default",
    // Alone you are always the tank, and Group.Info is {}.
    "%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}",
    "%nl%{end}",
    // Your vitals, each current over max and colored by how full, or
    // current alone in the terminal's color when nothing gives the max.
    // One that does not apply, such as mana for a class with none on
    // another game, draws nothing.
    "%{if:hp}%{if:maxhp}%c_hp%{end}%hp%{c:245}%{if:maxhp}/%{maxhp}%{end}hp%c_default%{end}",
    "%{if:mana} %{if:maxmana}%c_mana%{end}%mana%{c:245}%{if:maxmana}/%{maxmana}%{end}mn%c_default%{end}",
    "%{if:move} %{if:maxmove}%c_move%{end}%move%{c:245}%{if:maxmove}/%{maxmove}%{end}mv%c_default%{end}",
    // Position and language, one space apart when both show.
    "%{if:pos}  %{c:245}%pos%c_default%{end}",
    "%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    // Wizi and Incog show only when the game prints the immortal
    // prefix, and only out of a fight.
    "%{ifnot:fight}",
    "%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}",
    "%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}",
    "%{end}",
    "%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end}",
    " ",
);

/// At a glance as it first shipped, before the review guarded each vital.
const AT_A_GLANCE_FIRST: &str = concat!(
    "%{if:fight}",
    "%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% ",
    "%{c:245}%opponent_cond%c_default",
    "%{if:group_size}%{if:tank}  %{c:245}tank %c_tank_hp%tank%c_default%{end}%{end}",
    "%nl%{end}",
    "%c_hp%hp%{c:245}/%{maxhp}hp%c_default ",
    "%c_mana%mana%{c:245}/%{maxmana}mn%c_default ",
    "%c_move%move%{c:245}/%{maxmove}mv%c_default",
    "%{if:pos}  %{c:245}%pos%c_default%{end}",
    "%{if:lang}%{ifnot:pos} %{end} %{c:245}%lang%c_default%{end}",
    "%{if:exits}  %{c:245}[%c_default%exits%{c:245}]%c_default%{end}",
    "%{if:gold}  %{gold:grouped}%{c:245}g%c_default%{end}",
    "%{ifnot:fight}",
    "%{if:wizi}  %{c:245}wizi %wizi%c_default%{end}",
    "%{if:incog}%{ifnot:wizi} %{end} %{c:245}incog %incog%c_default%{end}",
    "%{end}",
    "%{if:missing}  %{c:245}missing %c_yellow%{missing:names}%c_default%{end}",
    " ",
);

/// The designs earlier builds shipped as Vosh's default, byte for byte.
/// You only ever got one from Vosh, at a fresh profile, by turning
/// drawing on with no design, or with `#prompt default`, so a profile
/// that holds one takes [`DEFAULT_DESIGN`] when it loads
/// ([`crate::PromptConfig::upgrade_retired_default`]).
pub const RETIRED_DEFAULTS: [&str; 2] = [AT_A_GLANCE, AT_A_GLANCE_FIRST];

/// A design to start from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preset {
    /// `game`, `minimal`, `how_full`, `percent`, `bars`, `detailed` or
    /// `empty`.
    pub id: &'static str,
    pub label: &'static str,
    pub template: String,
}

const MINIMAL: &str = "%{hp}h %{mana}m %{move}v > ";
const HOW_FULL: &str = "[%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv] ";
const PERCENT: &str =
    "hp %c_hp%pct_hp%%%c_default mn %c_mana%pct_mana%%%c_default mv %c_move%pct_move%%%c_default ";
const BARS: &str = "hp %hp_bar:6 mn %mana_bar:6 mv %move_bar:6 ";
const DETAILED: &str = "%{if:fight}%opponent %{opponent_hp:bar:10} %{opponent_hp:pct}%% %opponent_cond%nl%{end}%c_hp%hp%c_default/%{maxhp}hp %c_mana%mana%c_default/%{maxmana}mn %c_move%move%c_default/%{maxmove}mv %{c:8}tick%c_default %tick%{if:exits} %{c:8}[%c_default%exits%{c:8}]%c_default%{end} %{gold}g%{if:missing} %c_3%missing missing%c_default%{end} ";

fn preset(id: &'static str, label: &'static str, template: impl Into<String>) -> Preset {
    Preset {
        id,
        label,
        template: template.into(),
    }
}

/// The presets for Aabahran's codes, `game` being Same as the game when
/// your settings can be written that way.
pub fn aabahran(game: Option<String>) -> Vec<Preset> {
    let mut out = Vec::new();
    if let Some(game) = game {
        out.push(preset("game", "Same as the game", game));
    }
    out.extend([
        preset("minimal", "Minimal", MINIMAL),
        preset("how_full", "Colored by how full", HOW_FULL),
        preset("percent", "Percent", PERCENT),
        preset("bars", "Bars", BARS),
        preset("detailed", "Detailed", DETAILED),
        preset("empty", "Start empty", ""),
    ]);
    out
}

/// A vital pair as the presets write it.
struct Vital {
    cur: &'static str,
    max: &'static str,
    /// The letter Minimal writes after it.
    short: &'static str,
    /// The word the other presets write.
    word: &'static str,
}

const VITALS: [Vital; 3] = [
    Vital {
        cur: "hp",
        max: "maxhp",
        short: "h",
        word: "hp",
    },
    Vital {
        cur: "mana",
        max: "maxmana",
        short: "m",
        word: "mn",
    },
    Vital {
        cur: "move",
        max: "maxmove",
        short: "v",
        word: "mv",
    },
];

/// The presets for another game, from the values `supplied` says its
/// capture and GMCP give. A preset that would draw no value is left out,
/// so only Start empty is left when nothing is supplied.
pub fn other(supplied: &dyn Fn(&str) -> bool) -> Vec<Preset> {
    let have: Vec<&Vital> = VITALS.iter().filter(|v| supplied(v.cur)).collect();
    let full: Vec<&Vital> = have.iter().copied().filter(|v| supplied(v.max)).collect();
    let mut out = Vec::new();
    if !have.is_empty() {
        let minimal: Vec<String> = have
            .iter()
            .map(|v| format!("%{{{}}}{}", v.cur, v.short))
            .collect();
        out.push(preset(
            "minimal",
            "Minimal",
            format!("{} > ", minimal.join(" ")),
        ));
        let how_full: Vec<String> = have
            .iter()
            .map(|v| {
                if supplied(v.max) {
                    format!(
                        "%c_{cur}%{cur}%c_default/%{{{max}}}{word}",
                        cur = v.cur,
                        max = v.max,
                        word = v.word
                    )
                } else {
                    format!("%{{{}}}{}", v.cur, v.word)
                }
            })
            .collect();
        out.push(preset(
            "how_full",
            "Colored by how full",
            format!("[{}] ", how_full.join(" ")),
        ));
        if !full.is_empty() {
            let percent: Vec<String> = full
                .iter()
                .map(|v| format!("{w} %c_{c}%pct_{c}%%%c_default", w = v.word, c = v.cur))
                .collect();
            out.push(preset(
                "percent",
                "Percent",
                format!("{} ", percent.join(" ")),
            ));
            let bars: Vec<String> = full
                .iter()
                .map(|v| format!("{} %{}_bar:6", v.word, v.cur))
                .collect();
            out.push(preset("bars", "Bars", format!("{} ", bars.join(" "))));
        }
        out.push(preset(
            "detailed",
            "Detailed",
            format!("{} %{{c:8}}tick%c_default %tick ", how_full.join(" ")),
        ));
    }
    out.push(preset("empty", "Start empty", ""));
    out
}

// ---------------------------------------------------------------------
// Same as the game
// ---------------------------------------------------------------------

/// Same as the game for your settings as the game stores them: the
/// immortal prefix, each code as its piece in the game's look, the tank
/// line inside `%{if:tank}`, and your backtick colors as theme color
/// tokens. With a fight prompt set, each setting draws where the game
/// draws it. None when a color runs into a code, which the game cannot
/// print in a way Vosh can follow.
pub fn same_as_the_game(prompt: &str, fprompt: &str, who: Who) -> Option<String> {
    let mut tokens = prefix();
    let normal = if prompt.is_empty() {
        fallback()
    } else {
        setting(prompt, Which::Prompt, who)?
    };
    if fprompt.is_empty() {
        tokens.extend(normal);
    } else {
        let fight = setting(fprompt, Which::Fight, who)?;
        let field = FieldRef::new("fight");
        tokens.push(TokenKind::IfNot(field.clone()));
        tokens.extend(normal);
        tokens.push(TokenKind::End);
        tokens.push(TokenKind::If(field));
        tokens.extend(fight);
        tokens.push(TokenKind::End);
    }
    Some(write_tokens(&tokens))
}

fn value(name: &str, format: Format) -> TokenKind {
    TokenKind::Value(ValueRef {
        field: FieldRef::new(name),
        format,
    })
}

fn plain(name: &str) -> TokenKind {
    value(name, Format::Value)
}

fn text(s: &str) -> TokenKind {
    TokenKind::Text(s.to_string())
}

/// `(Wizi 60) (Incog 60) `, in the gray the game sends it in (correction
/// 10), only while you are wizi or incog.
fn prefix() -> Vec<TokenKind> {
    let mut out = Vec::new();
    for (name, word) in [("wizi", "Wizi"), ("incog", "Incog")] {
        out.extend([
            TokenKind::If(FieldRef::new(name)),
            TokenKind::Code(Code::Fg(ColorSpec::Index(240))),
            text(&format!("({word} ")),
            plain(name),
            text(")"),
            TokenKind::Code(Code::Reset),
            text(" "),
            TokenKind::End,
        ]);
    }
    out
}

/// The game's prompt for an empty setting, `<Nhp Nm Nmv> `.
fn fallback() -> Vec<TokenKind> {
    vec![
        text("<"),
        plain("hp"),
        text("hp "),
        plain("mana"),
        text("m "),
        plain("move"),
        text("mv> "),
    ]
}

/// One setting as tokens, ending in a space.
fn setting(stored: &str, which: Which, who: Who) -> Option<Vec<TokenKind>> {
    let read = lex::pass_one(stored, which);
    let tank_breaks: Vec<std::ops::Range<usize>> = read
        .tokens
        .iter()
        .filter(|l| l.token == GameToken::TankBreak)
        .map(|l| l.span.clone())
        .collect();
    let pieces = lex::pass_two(&read.tokens, which, &|code| code.edges(who)).ok()?;
    let mut out = Vec::new();
    let mut in_tank = false;
    for placed in &pieces {
        let tank = match placed.piece {
            GamePiece::Code(code) => code.is_tank(),
            GamePiece::Break => tank_breaks.contains(&placed.span),
            _ => false,
        };
        if tank && !in_tank {
            out.push(TokenKind::If(FieldRef::new("tank")));
        }
        if !tank && in_tank {
            out.push(TokenKind::End);
        }
        in_tank = tank;
        match placed.piece {
            GamePiece::Text(c) => match out.last_mut() {
                Some(TokenKind::Text(last)) => last.push(c),
                _ => out.push(TokenKind::Text(c.to_string())),
            },
            GamePiece::Color(color) => out.extend(color_tokens(color)),
            GamePiece::Break => out.push(TokenKind::Nl),
            GamePiece::Code(code) => out.extend(code_tokens(code)),
        }
    }
    if in_tank {
        out.push(TokenKind::End);
    }
    // The line end a setting ends on starts the game's next row, which
    // the drawn prompt does not keep.
    while let Some(at) = out.iter().rposition(|t| !matches!(t, TokenKind::Code(_))) {
        if out[at] != TokenKind::Nl {
            break;
        }
        out.remove(at);
    }
    let ends_in_space = out
        .iter()
        .rev()
        .find_map(|t| match t {
            TokenKind::Text(s) => Some(s.ends_with(' ')),
            TokenKind::Code(_) => None,
            _ => Some(false),
        })
        .unwrap_or(false);
    if !ends_in_space {
        out.push(text(" "));
    }
    Some(out)
}

/// A backtick color as the game writes it, `ESC[0;1;31m`: every color
/// and style off, then the color and styles, as theme tokens. Blink has
/// no token and drops.
fn color_tokens(color: GameColor) -> Vec<TokenKind> {
    let code = |c: Code| TokenKind::Code(c);
    match color {
        GameColor::Table { index, .. } => {
            let mut out = vec![code(Code::Reset)];
            let sgr = crate::aabahran::colors::sgr(index).unwrap_or_default();
            let params = sgr
                .trim_start_matches("\x1b[")
                .trim_end_matches('m')
                .split(';');
            for param in params {
                match param.parse::<u8>() {
                    Ok(1) => out.push(code(Code::Style(Style::Bold))),
                    Ok(4) => out.push(code(Code::Style(Style::Underline))),
                    Ok(n @ 30..=37) => out.push(code(Code::Fg(ColorSpec::Named(n - 30)))),
                    _ => {}
                }
            }
            out
        }
        GameColor::Fg256(n) => u8::try_from(n)
            .map(|n| vec![code(Code::Fg(ColorSpec::Index(n)))])
            .unwrap_or_default(),
        GameColor::Bg256(n) => u8::try_from(n)
            .map(|n| vec![code(Code::Bg(ColorSpec::Index(n)))])
            .unwrap_or_default(),
    }
}

/// A code as the piece that draws what it prints, in the game's look.
fn code_tokens(code: GameCode) -> Vec<TokenKind> {
    let game = |name: &str| vec![value(name, Format::Game)];
    let one = |name: &str| vec![plain(name)];
    match code {
        GameCode::Hp => vec![
            TokenKind::Code(Code::Fg(ColorSpec::ByValue {
                field: FieldRef::new("hp"),
                game: true,
            })),
            plain("hp"),
            TokenKind::Code(Code::Reset),
        ],
        GameCode::HpPct => vec![value("hp", Format::Pct)],
        GameCode::ManaPct => vec![value("mana", Format::Pct)],
        GameCode::MovePct => vec![value("move", Format::Pct)],
        GameCode::Lang => game("lang"),
        GameCode::Pos => game("pos"),
        GameCode::Exits => game("exits"),
        GameCode::TankBar => game("tank_hp"),
        GameCode::Slot(_) | GameCode::Moon(1..=3) => match code.name() {
            Some(name) => game(name),
            None => Vec::new(),
        },
        GameCode::Moon(_) => vec![text("-")],
        GameCode::Tank => vec![plain("tank"), text(": ")],
        GameCode::TankPct => vec![text("["), plain("tank_hp"), text("]")],
        other => other.name().map(one).unwrap_or_default(),
    }
}
