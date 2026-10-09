//! The colors the renderer draws with. The theme and palette the page
//! reports, the chrome colors over them, and the rules that turn a cell's
//! colors and attributes into what it draws. It also keeps the blink
//! clock, the twin of src/lib/blink.ts.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use super::bands::LIGHT_RING;
use crate::color::Paint;
use crate::native::grid::{CellFlags, Underline};

/// Linear-ish rgba in 0..1, ready for a wgpu vertex/instance buffer.
pub(crate) type Rgba = [f32; 4];

// Defaults until the page sends a theme. Chosen to match Vosh's dark
// surface.
const DEFAULT_FG: Rgb = Rgb {
    r: 0xcc,
    g: 0xcc,
    b: 0xcc,
};
const DEFAULT_BG: Rgb = Rgb {
    r: 0x10,
    g: 0x12,
    b: 0x18,
};

// Standard ANSI 16-color palette (xterm values). 0-7 normal, 8-15 bright.
pub(super) const ANSI_16: [Rgb; 16] = [
    Rgb {
        r: 0x00,
        g: 0x00,
        b: 0x00,
    },
    Rgb {
        r: 0xcd,
        g: 0x00,
        b: 0x00,
    },
    Rgb {
        r: 0x00,
        g: 0xcd,
        b: 0x00,
    },
    Rgb {
        r: 0xcd,
        g: 0xcd,
        b: 0x00,
    },
    Rgb {
        r: 0x00,
        g: 0x00,
        b: 0xee,
    },
    Rgb {
        r: 0xcd,
        g: 0x00,
        b: 0xcd,
    },
    Rgb {
        r: 0x00,
        g: 0xcd,
        b: 0xcd,
    },
    Rgb {
        r: 0xe5,
        g: 0xe5,
        b: 0xe5,
    },
    Rgb {
        r: 0x7f,
        g: 0x7f,
        b: 0x7f,
    },
    Rgb {
        r: 0xff,
        g: 0x00,
        b: 0x00,
    },
    Rgb {
        r: 0x00,
        g: 0xff,
        b: 0x00,
    },
    Rgb {
        r: 0xff,
        g: 0xff,
        b: 0x00,
    },
    Rgb {
        r: 0x5c,
        g: 0x5c,
        b: 0xff,
    },
    Rgb {
        r: 0xff,
        g: 0x00,
        b: 0xff,
    },
    Rgb {
        r: 0x00,
        g: 0xff,
        b: 0xff,
    },
    Rgb {
        r: 0xff,
        g: 0xff,
        b: 0xff,
    },
];

// Theme colors reported by the frontend (0 = unset, use the defaults).
// Packed 0x01_rr_gg_bb so a fully-black theme color is still "set".
static THEME_BG: AtomicU32 = AtomicU32::new(0);
static THEME_FG: AtomicU32 = AtomicU32::new(0);
static THEME_SEL: AtomicU32 = AtomicU32::new(0);

fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    0x0100_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

fn unpack_rgb(bits: u32, default: Rgb) -> Rgb {
    if bits == 0 {
        default
    } else {
        Rgb {
            r: (bits >> 16) as u8,
            g: (bits >> 8) as u8,
            b: bits as u8,
        }
    }
}

/// Set the terminal surface theme colors (background, foreground,
/// selection), reported by the frontend on theme change.
pub(crate) fn set_theme(bg: (u8, u8, u8), fg: (u8, u8, u8), sel: (u8, u8, u8)) {
    THEME_BG.store(pack_rgb(bg.0, bg.1, bg.2), Ordering::Release);
    THEME_FG.store(pack_rgb(fg.0, fg.1, fg.2), Ordering::Release);
    THEME_SEL.store(pack_rgb(sel.0, sel.1, sel.2), Ordering::Release);
}

pub(super) fn theme_bg() -> Rgb {
    unpack_rgb(THEME_BG.load(Ordering::Acquire), DEFAULT_BG)
}

/// The terminal background as sRGB bytes, for surfaces outside the grid
/// that must match it (the underlay layer backdrop and its first clear).
pub(crate) fn theme_bg_rgb() -> (u8, u8, u8) {
    let bg = theme_bg();
    (bg.r, bg.g, bg.b)
}

fn theme_fg() -> Rgb {
    unpack_rgb(THEME_FG.load(Ordering::Acquire), DEFAULT_FG)
}

/// The opaque selection color from `set_theme`, if the page reported one.
fn theme_selection() -> Option<Rgb> {
    let bits = THEME_SEL.load(Ordering::Acquire);
    (bits != 0).then(|| unpack_rgb(bits, DEFAULT_BG))
}

// The effective ANSI 0-15 palette the frontend last reported. The frontend
// resolves the `themeTerminalColors` toggle (canonical xterm-256 when off,
// the theme's ANSI when on), so the surface matches xterm either way.
// Unset entries (0) fall back to the canonical ANSI_16.
static THEME_ANSI: [AtomicU32; 16] = [const { AtomicU32::new(0) }; 16];

/// Set the ANSI 0-15 palette from the frontend's resolved theme.
pub(crate) fn set_palette(ansi: &[(u8, u8, u8)]) {
    for (slot, c) in THEME_ANSI.iter().zip(ansi.iter()) {
        slot.store(pack_rgb(c.0, c.1, c.2), Ordering::Release);
    }
}

pub(super) fn ansi16(idx: usize) -> Rgb {
    unpack_rgb(THEME_ANSI[idx].load(Ordering::Acquire), ANSI_16[idx])
}

// The split divider color from the settings (None = unset, theme default).
static DIVIDER_OVERRIDE: Mutex<Option<Paint>> = Mutex::new(None);

/// Set (or clear) the split divider color, reported by the frontend from
/// the `split_divider_color` setting.
pub(crate) fn set_divider_color(color: Option<Paint>) {
    if let Ok(mut slot) = DIVIDER_OVERRIDE.lock() {
        *slot = color;
    }
}

/// Chrome colors the page derives along with the rest of its theme tokens,
/// for the parts of the surface the terminal palette does not cover. Each
/// `None` falls back to a color derived from the terminal palette, so a
/// light theme never gets dark chips before the page reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeTokens {
    /// The split scrollback divider line.
    pub divider: Option<Paint>,
    /// Selected cells, composited over each cell's own background.
    pub selection: Option<Paint>,
    /// The text of selected cells. None keeps each cell's own color.
    pub selection_text: Option<Paint>,
    /// Every find match, composited like the selection.
    pub find_match: Option<Paint>,
    /// The match the find bar is on.
    pub current_match: Option<Paint>,
    /// A hovered URL's text and underline.
    pub link: Option<Paint>,
    /// The overlay scrollbar thumb. The track is the same color, fainter.
    pub scrollbar: Option<Paint>,
    /// The selected row fill, which a lifted prompt's band takes.
    pub selrow: Option<Paint>,
    /// The page's theme is light, so a band carries its inset ring.
    pub light: bool,
}

impl ChromeTokens {
    pub(crate) const UNSET: Self = Self {
        divider: None,
        selection: None,
        selection_text: None,
        find_match: None,
        current_match: None,
        link: None,
        scrollbar: None,
        selrow: None,
        light: false,
    };
}

static TOKENS: Mutex<ChromeTokens> = Mutex::new(ChromeTokens::UNSET);

/// Replace the chrome tokens, reported by the page on theme change.
pub(crate) fn set_tokens(tokens: ChromeTokens) {
    if let Ok(mut slot) = TOKENS.lock() {
        *slot = tokens;
    }
}

fn tokens() -> ChromeTokens {
    TOKENS.lock().map_or(ChromeTokens::UNSET, |t| *t)
}

// Fallback strengths for chrome the page has not colored yet. The
// divider and scrollbar are the foreground at a hairline and a text tier
// alpha, find matches are ANSI yellow washed into the cell.
pub(super) const DIVIDER_FALLBACK_ALPHA: f32 = 0.16;
pub(super) const SELECTION_FALLBACK_ALPHA: f32 = 0.25;
pub(super) const FIND_MATCH_FALLBACK_ALPHA: f32 = 0.35;
pub(super) const CURRENT_MATCH_FALLBACK_ALPHA: f32 = 0.65;
pub(super) const SCROLLBAR_FALLBACK_ALPHA: f32 = 0.45;
// A band before the page reports its row fill: the foreground washed into
// the ground.
pub(super) const SELROW_FALLBACK_ALPHA: f32 = 0.08;
// The scrollbar track is the thumb color at this share of its alpha.
pub(super) const SCROLLBAR_TRACK_SHARE: f32 = 0.2;

/// The colors one frame draws its chrome with: the page's tokens, the
/// divider setting over them, and palette fallbacks under them.
#[derive(Debug, PartialEq)]
pub(super) struct ChromePaint {
    pub(super) divider: Paint,
    pub(super) selection: Paint,
    /// The text of selected cells, or None to keep each cell's own color.
    pub(super) selection_text: Option<Paint>,
    pub(super) find_match: Paint,
    pub(super) current_match: Paint,
    pub(super) link: Paint,
    pub(super) scrollbar: Paint,
    /// A lifted prompt's band.
    pub(super) selrow: Paint,
    /// The band's inset ring, on light themes only.
    pub(super) ring: Option<Paint>,
}

pub(super) fn chrome_paint() -> ChromePaint {
    let divider_override = DIVIDER_OVERRIDE.lock().ok().and_then(|d| *d);
    resolve_chrome(
        tokens(),
        divider_override,
        theme_selection(),
        theme_fg(),
        ansi16(3),
        ansi16(12),
    )
}

/// Pick each chrome color: the divider setting first (divider only), then
/// the page's token, then the theme's opaque selection (selection only),
/// then a fallback from the foreground, ANSI yellow, or ANSI bright blue.
/// Pure, so the precedence tests without the globals.
pub(super) fn resolve_chrome(
    t: ChromeTokens,
    divider_override: Option<Paint>,
    theme_sel: Option<Rgb>,
    fg: Rgb,
    yellow: Rgb,
    bright_blue: Rgb,
) -> ChromePaint {
    ChromePaint {
        divider: divider_override
            .or(t.divider)
            .unwrap_or_else(|| Paint::tint(fg, DIVIDER_FALLBACK_ALPHA)),
        selection: t
            .selection
            .or_else(|| theme_sel.map(Paint::opaque))
            .unwrap_or_else(|| Paint::tint(fg, SELECTION_FALLBACK_ALPHA)),
        selection_text: t.selection_text,
        find_match: t
            .find_match
            .unwrap_or_else(|| Paint::tint(yellow, FIND_MATCH_FALLBACK_ALPHA)),
        current_match: t
            .current_match
            .unwrap_or_else(|| Paint::tint(yellow, CURRENT_MATCH_FALLBACK_ALPHA)),
        link: t.link.unwrap_or_else(|| Paint::opaque(bright_blue)),
        scrollbar: t
            .scrollbar
            .unwrap_or_else(|| Paint::tint(fg, SCROLLBAR_FALLBACK_ALPHA)),
        selrow: t
            .selrow
            .unwrap_or_else(|| Paint::tint(fg, SELROW_FALLBACK_ALPHA)),
        ring: t.light.then_some(LIGHT_RING),
    }
}

// When set, draw bright (ANSI 8-15) colored text with the bold font weight.
static BRIGHT_BOLD: AtomicBool = AtomicBool::new(false);

/// Toggle drawing bright-colored text with the bold font, reported by the
/// frontend from the `bright_bold` setting.
pub(crate) fn set_bright_bold(on: bool) {
    BRIGHT_BOLD.store(on, Ordering::Release);
}

// Blinking text, the setting of that name. Off until the page reports it,
// so a frame drawn before then holds still.
static BLINK_TEXT: AtomicBool = AtomicBool::new(false);

/// Turn blinking text on or off, reported by the page from the Blinking
/// text setting and the reduce motion setting of the system. Off, every
/// blinking cell draws steady.
pub(crate) fn set_blink_text(on: bool) {
    BLINK_TEXT.store(on, Ordering::Release);
}

pub(crate) fn blink_text() -> bool {
    BLINK_TEXT.load(Ordering::Acquire)
}

/// How long blinking text shows and how long it hides, the blink of
/// xterm's cursor (`CursorBlinkStateManager`). xterm's text blink and the
/// page's pinned prompt use it too (`BLINK_MS` in src/lib/blink.ts).
pub(crate) const BLINK_MS: u64 = 600;

/// Blinking text shows at `now_ms`, milliseconds since the Unix epoch.
/// Every renderer counts the halves from the epoch, so whatever blinks
/// on the screen flips together.
pub(crate) fn blink_shown(now_ms: u64) -> bool {
    (now_ms / BLINK_MS) % 2 == 0
}

/// How long from `now_ms` until blinking text flips.
pub(crate) fn until_blink_flip(now_ms: u64) -> std::time::Duration {
    std::time::Duration::from_millis(BLINK_MS - now_ms % BLINK_MS)
}

/// Milliseconds since the Unix epoch, as the page's `Date.now()` counts.
pub(crate) fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// True when `fg` is a bright ANSI color (8-15), named or indexed.
fn is_bright_ansi(fg: Color) -> bool {
    matches!(
        fg,
        Color::Named(
            NamedColor::BrightBlack
                | NamedColor::BrightRed
                | NamedColor::BrightGreen
                | NamedColor::BrightYellow
                | NamedColor::BrightBlue
                | NamedColor::BrightMagenta
                | NamedColor::BrightCyan
                | NamedColor::BrightWhite
        ) | Color::Indexed(8..=15)
    )
}

/// True when the cell should use the bold face. A cell whose *effective*
/// color is a bright ANSI color (8-15, counting bold-promoted base colors the
/// way MUDs encode bright) is bold only when the bright-bold setting is on.
/// Genuinely-explicit bold on a non-bright color (e.g. a bold 256-color
/// prompt token) keeps the bold font regardless.
pub(super) fn wants_bold_font(fg: Color, flags: CellFlags) -> bool {
    let effective = if flags.bold { brighten(fg) } else { fg };
    if is_bright_ansi(effective) {
        BRIGHT_BOLD.load(Ordering::Acquire)
    } else {
        flags.bold
    }
}

// The wgpu surface is sRGB, so the GPU sRGB-encodes whatever the fragment
// shader writes. Our palette values are already sRGB (xterm hex), so we
// linearize them here; the encode on write then round-trips to the
// intended color. It also makes the glyph-coverage blend correct (linear).
pub(super) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub(super) fn rgb_to_rgba(c: Rgb) -> Rgba {
    [
        srgb_to_linear(f32::from(c.r) / 255.0),
        srgb_to_linear(f32::from(c.g) / 255.0),
        srgb_to_linear(f32::from(c.b) / 255.0),
        1.0,
    ]
}

/// A paint as a quad color: linear rgb plus its alpha, which the shader
/// composites over whatever the quad covers.
pub(super) fn paint_to_rgba(p: Paint) -> Rgba {
    let mut c = rgb_to_rgba(Rgb {
        r: p.r,
        g: p.g,
        b: p.b,
    });
    c[3] = p.a;
    c
}

/// Composite a paint over an opaque cell background in sRGB space, the way
/// CSS composites `rgba()` and the surface blends. The cell keeps its own
/// alpha, so the background quad still covers the clear.
pub(super) fn blend_over(top: Paint, under: Rgba) -> Rgba {
    let a = top.a.clamp(0.0, 1.0);
    let mix =
        |t: u8, u: f32| srgb_to_linear((f32::from(t) / 255.0) * a + linear_to_srgb(u) * (1.0 - a));
    [
        mix(top.r, under[0]),
        mix(top.g, under[1]),
        mix(top.b, under[2]),
        under[3],
    ]
}

fn dim(c: Rgb) -> Rgb {
    Rgb {
        r: (u16::from(c.r) * 2 / 3) as u8,
        g: (u16::from(c.g) * 2 / 3) as u8,
        b: (u16::from(c.b) * 2 / 3) as u8,
    }
}

fn named_to_rgb(n: NamedColor) -> Rgb {
    match n {
        NamedColor::Black => ansi16(0),
        NamedColor::Red => ansi16(1),
        NamedColor::Green => ansi16(2),
        NamedColor::Yellow => ansi16(3),
        NamedColor::Blue => ansi16(4),
        NamedColor::Magenta => ansi16(5),
        NamedColor::Cyan => ansi16(6),
        NamedColor::White => ansi16(7),
        NamedColor::BrightBlack => ansi16(8),
        NamedColor::BrightRed => ansi16(9),
        NamedColor::BrightGreen => ansi16(10),
        NamedColor::BrightYellow => ansi16(11),
        NamedColor::BrightBlue => ansi16(12),
        NamedColor::BrightMagenta => ansi16(13),
        NamedColor::BrightCyan => ansi16(14),
        NamedColor::BrightWhite => ansi16(15),
        NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => theme_fg(),
        NamedColor::Background => theme_bg(),
        NamedColor::DimBlack => dim(ansi16(0)),
        NamedColor::DimRed => dim(ansi16(1)),
        NamedColor::DimGreen => dim(ansi16(2)),
        NamedColor::DimYellow => dim(ansi16(3)),
        NamedColor::DimBlue => dim(ansi16(4)),
        NamedColor::DimMagenta => dim(ansi16(5)),
        NamedColor::DimCyan => dim(ansi16(6)),
        NamedColor::DimWhite => dim(ansi16(7)),
        NamedColor::DimForeground => dim(theme_fg()),
    }
}

// xterm 256-color cube + grayscale ramp.
fn indexed_to_rgb(i: u8) -> Rgb {
    match i {
        0..=15 => ansi16(i as usize),
        16..=231 => {
            let i = i - 16;
            let component = |v: u8| -> u8 {
                if v == 0 {
                    0
                } else {
                    55 + v * 40
                }
            };
            Rgb {
                r: component(i / 36),
                g: component((i % 36) / 6),
                b: component(i % 6),
            }
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Map an alacritty cell color to rgba.
pub(crate) fn color_to_rgba(color: Color) -> Rgba {
    let rgb = match color {
        Color::Named(n) => named_to_rgb(n),
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => indexed_to_rgb(i),
    };
    rgb_to_rgba(rgb)
}

/// Bold promotes a normal named color to its bright variant (the MUD-common
/// reading of bold); other colors are unchanged.
fn brighten(color: Color) -> Color {
    let Color::Named(named) = color else {
        return color;
    };
    Color::Named(match named {
        NamedColor::Black => NamedColor::BrightBlack,
        NamedColor::Red => NamedColor::BrightRed,
        NamedColor::Green => NamedColor::BrightGreen,
        NamedColor::Yellow => NamedColor::BrightYellow,
        NamedColor::Blue => NamedColor::BrightBlue,
        NamedColor::Magenta => NamedColor::BrightMagenta,
        NamedColor::Cyan => NamedColor::BrightCyan,
        NamedColor::White => NamedColor::BrightWhite,
        NamedColor::Foreground => NamedColor::BrightForeground,
        other => other,
    })
}

/// Dim text (SGR 2) at 0.6 of its linear color.
pub(super) fn dimmed(c: Rgba) -> Rgba {
    [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6, c[3]]
}

/// Apply cell attributes: bold brightens fg, dim darkens it, inverse swaps
/// fg/bg. Returns (fg, bg) rgba.
pub(super) fn styled_colors(fg: Color, bg: Color, flags: CellFlags) -> (Rgba, Rgba) {
    let fg_color = if flags.bold { brighten(fg) } else { fg };
    let mut fg_rgba = color_to_rgba(fg_color);
    if flags.dim {
        fg_rgba = dimmed(fg_rgba);
    }
    let bg_rgba = color_to_rgba(bg);
    if flags.inverse {
        (bg_rgba, fg_rgba)
    } else {
        (fg_rgba, bg_rgba)
    }
}

/// The underline's color. SGR 58 colors the line apart from the text,
/// and without it the line takes `text`, the cell's drawn text color with
/// dim and inverse already applied, the way xterm draws it. A bold cell
/// moves an SGR 58 palette color 0 to 7 up to its bright twin, as xterm
/// does for the line.
pub(super) fn underline_color(flags: CellFlags, text: Rgba) -> Rgba {
    flags.underline_color.map_or(text, |color| {
        color_to_rgba(match color {
            Color::Indexed(i) if flags.bold && i < 8 => Color::Indexed(i + 8),
            other => other,
        })
    })
}

/// Whether a cell shows its text: not while SGR 8 hides it, nor in the
/// hidden half of a blink (`blink_hidden`).
fn shows_text(flags: CellFlags, blink_hidden: bool) -> bool {
    let blinked_away = blink_hidden && flags.blink;
    !flags.hidden && !blinked_away
}

/// The glyph a cell draws. SGR 8 hides the text and keeps the cell's
/// place and ground, as xterm does, and so does the hidden half of a
/// blink.
pub(super) fn drawn_char(ch: char, flags: CellFlags, blink_hidden: bool) -> char {
    if shows_text(flags, blink_hidden) {
        ch
    } else {
        ' '
    }
}

/// Whether a cell draws its underline and strike. Hidden text draws
/// neither, the same as xterm, which skips the whole glyph. xterm skips
/// it in the hidden half of a blink too, lines and all, so the grid and
/// the pinned band do the same.
pub(super) fn draws_lines(flags: CellFlags, blink_hidden: bool) -> bool {
    shows_text(flags, blink_hidden)
}

/// A cell blinks with something its hidden half takes away: a glyph, an
/// underline or a strike.
pub(super) fn blinks_visibly(ch: char, flags: CellFlags) -> bool {
    let ink = !matches!(ch, ' ' | '\0') || flags.underline != Underline::None || flags.strikeout;
    flags.blink && shows_text(flags, false) && ink
}
