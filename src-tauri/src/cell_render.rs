//! Tier 3 native terminal renderer, M2c (see docs/native-renderer.md).
//!
//! The wgpu cell renderer: turns `term_grid`'s cells into pixels. Built
//! incrementally — color mapping first (this file's first commit), then a
//! glyph atlas, then the instanced pipeline that replaces the M1 test
//! triangle. The pipeline reads the grid each frame and draws a
//! background quad plus a glyph quad per cell.
//!
//! Glyphs rasterize through CoreGraphics with smoothing off, to match the
//! webview.

#![cfg(native_surface)]
// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use crate::term_grid::{CellFlags, LiftSpan, Underline};

/// Linear-ish rgba in 0..1, ready for a wgpu vertex/instance buffer.
pub(crate) type Rgba = [f32; 4];

// Defaults until theming lands (M4). Chosen to match Vosh's dark surface.
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
const ANSI_16: [Rgb; 16] = [
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

fn theme_bg() -> Rgb {
    unpack_rgb(THEME_BG.load(Ordering::Acquire), DEFAULT_BG)
}

/// The terminal background as sRGB bytes, for surfaces outside the grid
/// that must match it (the underlay layer backdrop and its first clear).
pub(crate) fn theme_bg_rgb() -> (u8, u8, u8) {
    let bg = theme_bg();
    (bg.r, bg.g, bg.b)
}

/// Where the pane sits inside the render target, in device pixels. The
/// macOS underlay surface spans the whole window, so the grid draws at the
/// pane's offset instead of at the target's origin. The caller keeps the
/// pane inside the target.
#[derive(Clone, Copy)]
pub(crate) struct Placement {
    pub x: u32,
    pub y: u32,
    /// Device pixels per CSS pixel, which the prompt bands scale by.
    pub scale: f32,
    /// The render target's size in device pixels. A prompt band reaches
    /// past the pane into it, as far as the target allows.
    pub target: [u32; 2],
    /// Blinking text is in its hidden half. Never while Blinking text is
    /// off.
    pub blink_hidden: bool,
}

/// What a frame drew that the surface acts on after it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Drawn {
    /// Where the split divider sits, as a fraction of the surface height,
    /// while the view is split.
    pub divider: Option<f32>,
    /// A cell on screen blinks, with a glyph or a line that its hidden
    /// half takes away, so the frame that flips it changes something.
    pub blinks: bool,
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

fn ansi16(idx: usize) -> Rgb {
    unpack_rgb(THEME_ANSI[idx].load(Ordering::Acquire), ANSI_16[idx])
}

/// A color with straight alpha: sRGB bytes plus an alpha in 0..1, the way
/// CSS writes `rgba()`. The chrome tokens arrive in this form, and several
/// of them are translucent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Paint {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Paint {
    fn opaque(c: Rgb) -> Self {
        Self::tint(c, 1.0)
    }

    fn tint(c: Rgb, a: f32) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a,
        }
    }
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
const DIVIDER_FALLBACK_ALPHA: f32 = 0.16;
const SELECTION_FALLBACK_ALPHA: f32 = 0.25;
const FIND_MATCH_FALLBACK_ALPHA: f32 = 0.35;
const CURRENT_MATCH_FALLBACK_ALPHA: f32 = 0.65;
const SCROLLBAR_FALLBACK_ALPHA: f32 = 0.45;
// A band before the page reports its row fill: the foreground washed into
// the ground.
const SELROW_FALLBACK_ALPHA: f32 = 0.08;
// The scrollbar track is the thumb color at this share of its alpha.
const SCROLLBAR_TRACK_SHARE: f32 = 0.2;

/// The colors one frame draws its chrome with: the page's tokens, the
/// divider setting over them, and palette fallbacks under them.
#[derive(Debug, PartialEq)]
struct ChromePaint {
    divider: Paint,
    selection: Paint,
    find_match: Paint,
    current_match: Paint,
    link: Paint,
    scrollbar: Paint,
    /// A lifted prompt's band.
    selrow: Paint,
    /// The band's inset ring, on light themes only.
    ring: Option<Paint>,
}

fn chrome_paint() -> ChromePaint {
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
fn resolve_chrome(
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

/// Parse a CSS color: `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa` (the `#`
/// optional), or `rgb()`/`rgba()` with comma or space separated channels
/// and an optional alpha as a fraction or a percentage. None for anything
/// else, so the caller falls back to its default.
pub(crate) fn parse_css_color(value: &str) -> Option<Paint> {
    let v = value.trim().to_ascii_lowercase();
    let hex = v.strip_prefix('#').unwrap_or(&v);
    if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).unwrap_or(0);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let (r, g, b, a) = if hex.len() <= 4 {
            let a = if hex.len() == 4 { digit(3) * 17 } else { 255 };
            (digit(0) * 17, digit(1) * 17, digit(2) * 17, a)
        } else {
            let a = if hex.len() == 8 { pair(6) } else { 255 };
            (pair(0), pair(2), pair(4), a)
        };
        return Some(Paint {
            r,
            g,
            b,
            a: f32::from(a) / 255.0,
        });
    }
    let inner = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 3 && parts.len() != 4 {
        return None;
    }
    let ch = |s: &str| {
        s.parse::<f32>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n.round().clamp(0.0, 255.0) as u8)
    };
    let a = match parts.get(3) {
        None => 1.0,
        Some(s) => match s.strip_suffix('%') {
            Some(pct) => pct.parse::<f32>().ok()? / 100.0,
            None => s.parse::<f32>().ok()?,
        },
    };
    if !a.is_finite() {
        return None;
    }
    Some(Paint {
        r: ch(parts[0])?,
        g: ch(parts[1])?,
        b: ch(parts[2])?,
        a: a.clamp(0.0, 1.0),
    })
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

// When set, your prompt shows lifted and each lift draws on a band.
static PROMPT_BANDS: AtomicBool = AtomicBool::new(false);

/// Draw a band under each lifted prompt, reported by the page from where
/// your prompt shows.
pub(crate) fn set_prompt_bands(on: bool) {
    PROMPT_BANDS.store(on, Ordering::Release);
}

fn prompt_bands() -> bool {
    PROMPT_BANDS.load(Ordering::Acquire)
}

// How far past its last glyph the newest lift's band reaches, in CSS px,
// as f32 bits: the prompt card's ↵ and caret on the open row.
static PROMPT_REACH: AtomicU32 = AtomicU32::new(0);

/// Widen the newest lift's band by `px` CSS px, reported by the page while
/// the prompt card draws a ↵ or its caret past the open row's last glyph,
/// so the band runs under them. 0 while the card is closed.
pub(crate) fn set_prompt_reach(px: f32) {
    PROMPT_REACH.store(px.max(0.0).to_bits(), Ordering::Release);
}

fn prompt_reach() -> f32 {
    f32::from_bits(PROMPT_REACH.load(Ordering::Acquire))
}

/// Widen the band of the lift `newest` among `boxes` by `reach` device px.
/// A band that steps in around your echo keeps its width.
fn widen_newest(rects: &mut [BandRect], boxes: &[LiftBox], newest: Option<u64>, reach: f32) {
    if reach <= 0.0 {
        return;
    }
    for (rect, b) in rects.iter_mut().zip(boxes) {
        if Some(b.id) == newest && rect.notch.is_none() {
            rect.w += reach;
        }
    }
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
fn wants_bold_font(fg: Color, flags: CellFlags) -> bool {
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
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_to_rgba(c: Rgb) -> Rgba {
    [
        srgb_to_linear(f32::from(c.r) / 255.0),
        srgb_to_linear(f32::from(c.g) / 255.0),
        srgb_to_linear(f32::from(c.b) / 255.0),
        1.0,
    ]
}

/// A paint as a quad color: linear rgb plus its alpha, which the shader
/// composites over whatever the quad covers.
fn paint_to_rgba(p: Paint) -> Rgba {
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
fn blend_over(top: Paint, under: Rgba) -> Rgba {
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

/// Apply cell attributes: bold brightens fg, dim darkens it, inverse swaps
/// fg/bg. Returns (fg, bg) rgba.
fn styled_colors(fg: Color, bg: Color, flags: CellFlags) -> (Rgba, Rgba) {
    let fg_color = if flags.bold { brighten(fg) } else { fg };
    let mut fg_rgba = color_to_rgba(fg_color);
    if flags.dim {
        fg_rgba[0] *= 0.6;
        fg_rgba[1] *= 0.6;
        fg_rgba[2] *= 0.6;
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
fn underline_color(flags: CellFlags, text: Rgba) -> Rgba {
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
fn drawn_char(ch: char, flags: CellFlags, blink_hidden: bool) -> char {
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
fn draws_lines(flags: CellFlags, blink_hidden: bool) -> bool {
    shows_text(flags, blink_hidden)
}

/// A cell blinks with something its hidden half takes away: a glyph, an
/// underline or a strike.
fn blinks_visibly(ch: char, flags: CellFlags) -> bool {
    let ink = !matches!(ch, ' ' | '\0') || flags.underline != Underline::None || flags.strikeout;
    flags.blink && shows_text(flags, false) && ink
}

// ---------------------------------------------------------------------------
// Text decorations
// ---------------------------------------------------------------------------

/// How far the underline sits below the baseline, in CSS pixels. The
/// Styles board sets `text-underline-offset: 3px`.
const UNDERLINE_DROP: f32 = 3.0;
/// The curly underline's band in CSS pixels, from the top of its crest
/// to the bottom of its trough. Chrome draws the board's wavy line 3.5
/// CSS pixels tall at a 1 px thickness.
const CURL_HEIGHT: f32 = 3.5;
/// A dash's share of its cell. The gap takes the rest, split evenly on
/// both sides, so a dashed run reads as one dash per character.
const DASH_SHARE: f32 = 0.65;

/// Where a cell's lines sit, in device pixels from the cell's top left
/// corner. Every line is one CSS pixel thick, the weight of the Styles
/// board, and every line stays inside its cell, so no line reaches into
/// the row below or past a split's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Decor {
    /// Line thickness, one CSS pixel and never under one device pixel.
    t: u32,
    /// Top row of the single, dotted, and dashed underlines.
    under: u32,
    /// Top rows of the double underline's two lines.
    double: [u32; 2],
    /// Top row and height of the curly underline's band.
    curl_top: u32,
    curl_h: u32,
    /// Top row of the strike, through the middle of the cell.
    strike: u32,
    /// Where each cell's dash starts, and its length.
    dash_x: u32,
    dash_w: u32,
}

/// The lines for a `cell_w` by `cell_h` cell whose baseline sits on row
/// `baseline`, at `scale` device pixels per CSS pixel. The underline sits
/// three CSS pixels under the baseline when the cell has room, and a
/// line that would hang past the cell's bottom rises until it fits. The
/// curl, the tallest line, shrinks before it rises, so it keeps a CSS
/// pixel of room under the letters at every line height.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn decor(cell_w: u32, cell_h: u32, baseline: u32, scale: f32) -> Decor {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let cell_h = cell_h.max(1);
    let t = (scale.round() as u32).clamp(1, cell_h);
    let want = baseline + (UNDERLINE_DROP * scale).round() as u32;
    let fit = |ink: u32| want.min(cell_h.saturating_sub(ink));
    // The double's gap is half a line, at least a pixel: Chrome draws the
    // board's double at 2x as two 2 px lines 1 px apart.
    let gap = (t / 2).max(1);
    let double_top = fit(2 * t + gap);
    // The curl keeps a CSS pixel of room under the letters, giving up
    // band height first, down to the least that still reads as a wave.
    let full_curl = ((CURL_HEIGHT * scale).round() as u32).max(t + 2);
    let least_curl = (t + 2).min(cell_h);
    let curl_top = fit(full_curl).max(baseline + t).min(cell_h - least_curl);
    let curl_h = full_curl.min(cell_h - curl_top);
    let dash_w = ((cell_w as f32 * DASH_SHARE).round() as u32).clamp(1, cell_w.max(2) - 1);
    Decor {
        t,
        under: fit(t),
        double: [double_top, double_top + t + gap],
        curl_top,
        curl_h,
        strike: (cell_h / 2).saturating_sub(t / 2),
        dash_x: cell_w.saturating_sub(dash_w) / 2,
        dash_w,
    }
}

/// A solid piece of a line: x and y from the cell's top left, then width
/// and height, in device pixels.
type LineRect = [u32; 4];

/// The solid pieces of a cell's underline, for the cell whose left edge
/// sits `x0` pixels from the grid's left. Dots count from the grid's
/// left edge, so they keep one pitch across neighbouring cells of any
/// width. Each cell centers one dash, so dashes keep the cell's pitch.
/// The curl draws from the atlas and has no solid pieces.
fn underline_rects(kind: Underline, x0: u32, cell_w: u32, d: &Decor) -> Vec<LineRect> {
    match kind {
        Underline::None | Underline::Curly => Vec::new(),
        Underline::Single => vec![[0, d.under, cell_w, d.t]],
        Underline::Double => vec![[0, d.double[0], cell_w, d.t], [0, d.double[1], cell_w, d.t]],
        Underline::Dashed => vec![[d.dash_x, d.under, d.dash_w, d.t]],
        Underline::Dotted => {
            let pitch = 2 * d.t;
            let end = x0 + cell_w;
            let mut dots = Vec::new();
            let mut dot = x0 / pitch * pitch;
            while dot < end {
                let from = dot.max(x0);
                let to = (dot + d.t).min(end);
                if from < to {
                    dots.push([from - x0, d.under, to - from, d.t]);
                }
                dot += pitch;
            }
            dots
        }
    }
}

/// The quads for a region's marks, as (column, row top, color, kind):
/// solid pieces sample the solid texel, and a curl samples its sprite at
/// one texel a pixel. Every quad sits on whole pixels, so each line
/// stays crisp. Strikes run the cell's width through its middle.
fn line_instances(
    underlines: &[(usize, f32, Rgba, Underline)],
    strikeouts: &[(usize, f32, Rgba, Underline)],
    d: &Decor,
    cell_w: u32,
    solid_uv: ([f32; 2], [f32; 2]),
    curl_uv: ([f32; 2], [f32; 2]),
) -> Vec<CellInstance> {
    let mut out = Vec::new();
    let solid = |x: u32, y: f32, [rx, ry, rw, rh]: LineRect, color: Rgba| CellInstance {
        offset: [(x + rx) as f32, y + ry as f32],
        size: [rw as f32, rh as f32],
        color,
        uv_min: solid_uv.0,
        uv_max: solid_uv.1,
    };
    for &(col, y_top, color, kind) in underlines {
        let x0 = col as u32 * cell_w;
        if kind == Underline::Curly {
            out.push(CellInstance {
                offset: [x0 as f32, y_top + d.curl_top as f32],
                size: [cell_w as f32, d.curl_h as f32],
                color,
                uv_min: curl_uv.0,
                uv_max: curl_uv.1,
            });
        } else {
            for rect in underline_rects(kind, x0, cell_w, d) {
                out.push(solid(x0, y_top, rect, color));
            }
        }
    }
    for &(col, y_top, color, _) in strikeouts {
        let x0 = col as u32 * cell_w;
        out.push(solid(x0, y_top, [0, d.strike, cell_w, d.t], color));
    }
    out
}

/// Coverage of the curly underline's sprite: one period of a sine wave
/// `w` pixels long, so it repeats once a cell and meets its neighbours at
/// the same height, inside a band `h` rows tall and stroked `t` pixels
/// thick. Each pixel's coverage falls off with its distance to the curve,
/// so the wave antialiases the way a canvas stroke does, and the crest
/// and trough land on whole rows. Row-major, `w * h` bytes.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn curl_coverage(w: u32, h: u32, t: u32) -> Vec<u8> {
    let (wf, hf, tf) = (w.max(1) as f32, h as f32, t as f32);
    let amp = ((hf - tf) / 2.0).max(0.0);
    let mid = hf / 2.0;
    let curve = |x: f32| mid - amp * (std::f32::consts::TAU * x / wf).sin();
    // Sample the curve finely across one period around each pixel. The
    // curve repeats, so the samples run past either edge of the cell.
    let steps = (w.max(1) * 32) as usize;
    let mut out = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut near = f32::MAX;
            for k in 0..=steps {
                let sx = px - wf / 2.0 + wf * k as f32 / steps as f32;
                let (dx, dy) = (sx - px, curve(sx) - py);
                near = near.min(dx.hypot(dy));
            }
            let cov = (tf / 2.0 + 0.5 - near).clamp(0.0, 1.0);
            out[(y * w + x) as usize] = (cov * 255.0).round() as u8;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Glyph atlas
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::font::CGGlyph;
use core_graphics::geometry::{CGAffineTransform, CGPoint, CGRect, CGSize};
use core_text::font::CTFont;
use font_kit::canvas::RasterizationOptions;
use font_kit::font::Font;
use font_kit::hinting::HintingOptions;
use pathfinder_geometry::transform2d::Transform2F;

// Italic slant: shear the top of the glyph rightward. The CoreGraphics text
// matrix's `c` term is the horizontal shear; positive leans the top right.
const ITALIC_SKEW: f32 = 0.21;

/// Pixel rect (x, y, w, h) of a fixed-size slot in the atlas grid.
fn slot_rect(index: u32, cols: u32, cell_w: u32, cell_h: u32) -> (u32, u32, u32, u32) {
    let col = index % cols;
    let row = index / cols;
    (col * cell_w, row * cell_h, cell_w, cell_h)
}

/// Convert a pixel rect in an atlas of size (aw, ah) to a UV rect
/// (top-left, bottom-right) in 0..1.
fn rect_to_uv(x: u32, y: u32, w: u32, h: u32, aw: u32, ah: u32) -> ([f32; 2], [f32; 2]) {
    let aw = aw.max(1) as f32;
    let ah = ah.max(1) as f32;
    (
        [x as f32 / aw, y as f32 / ah],
        [(x + w) as f32 / aw, (y + h) as f32 / ah],
    )
}

/// How far xterm drops its glyph box from the top of a cell taller than
/// the box, which is its `device.char.top`: half the spare height,
/// rounding half up the way `Math.round` does. Zero at line height 1.
pub(crate) fn centered_glyph_top(cell_h: u32, char_h: u32) -> u32 {
    cell_h.saturating_sub(char_h).div_ceil(2)
}

/// The native baseline's row inside a cell, for a glyph box `glyph_top`
/// below the cell top and a font ascent of `ascent` pixels.
fn native_baseline(glyph_top: u32, ascent: f32) -> u32 {
    glyph_top + ascent.round().max(0.0) as u32
}

/// A monospace glyph atlas: every glyph is rasterized into a uniform
/// cell-sized slot (with the glyph placed at its baseline inside the
/// slot), packed into one A8 coverage texture. The renderer draws each
/// cell's glyph quad over the whole cell rect and samples the slot, so no
/// per-glyph offset math is needed at draw time.
pub(crate) struct GlyphAtlas {
    font: Font,
    bold_font: Font,
    px: f32,
    cell_w: u32,
    cell_h: u32,
    // Each slot is wider than the layout cell so a slanted (italic) or
    // wide glyph can overhang to the right without being clipped; the
    // glyph quad is drawn at slot width and overhangs the next cell.
    slot_w: u32,
    ascent: f32,
    // How far the glyph box sits below the top of the cell. xterm centers
    // its box in a cell taller than the box (line height above 1), and the
    // atlas drops each glyph the same amount so the baselines agree.
    glyph_top: u32,
    cols: u32,
    rows: u32,
    atlas_w: u32,
    atlas_h: u32,
    pixels: Vec<u8>,
    // A degenerate UV at an always-opaque texel, so background and overlay
    // quads (which carry no glyph) sample coverage 1.0.
    solid_uv: ([f32; 2], [f32; 2]),
    // Keyed by (char, bold, italic): four faces (regular, bold, and a
    // synthetic slant of each) share one texture.
    slots: HashMap<(char, bool, bool), u32>,
    next: u32,
    // The thickness and band height the curly underline's sprite was last
    // drawn at, in the slot kept for it just before the solid block.
    curl: Option<(u32, u32)>,
}

impl GlyphAtlas {
    /// Build an atlas from loaded `fonts` at `px` pixels.
    pub(crate) fn from_fonts(fonts: AtlasFonts, px: f32) -> Self {
        Self::with_reported(
            fonts,
            px,
            crate::native_surface::reported_cell(),
            crate::native_surface::reported_char_height(),
        )
    }

    /// Build the atlas against xterm's reported device cell and glyph box
    /// height, or against the font's own metrics when the page has not
    /// reported yet.
    fn with_reported(
        fonts: AtlasFonts,
        px: f32,
        reported: Option<(u32, u32)>,
        char_h: Option<u32>,
    ) -> Self {
        let AtlasFonts {
            regular: font,
            bold: bold_font,
        } = fonts;
        let metrics = font.metrics();
        let scale = px / metrics.units_per_em as f32;
        let ascent = metrics.ascent * scale;
        // ascent - descent + line_gap is the line height (descent is negative).
        let cell_h_font =
            (((metrics.ascent - metrics.descent + metrics.line_gap) * scale).ceil() as u32).max(1);
        // Monospace: every advance is the same, so 'M' gives the cell width.
        let advance = match font.glyph_for_char('M').and_then(|g| font.advance(g).ok()) {
            Some(a) => a.x(),
            None => metrics.units_per_em as f32 * 0.6,
        };
        let cell_w_font = ((advance * scale).round() as u32).max(1);
        // Prefer xterm's reported device cell so spacing matches the webview
        // exactly; fall back to the font-derived size before it reports.
        let (cell_w, cell_h) =
            reported.map_or((cell_w_font, cell_h_font), |(w, h)| (w.max(1), h.max(1)));
        // The line height lives in the reported cell: xterm multiplies its
        // glyph box by it and centers the box in the result. Drop the glyphs
        // by the same amount, from the box height the page reports with the
        // cell, so the native baseline lands on xterm's at every line height.
        let glyph_top = match (reported, char_h) {
            (Some(_), Some(char_h)) => centered_glyph_top(cell_h, char_h),
            _ => 0,
        };
        // Slots get a full extra cell of width so italic overhang fits.
        let slot_w = cell_w * 2;
        tracing::debug!(
            cell_w,
            cell_h,
            cell_w_font,
            cell_h_font,
            glyph_top,
            "native-surface: atlas metrics"
        );
        // 32x32 = 1024 slots: printable ASCII across four faces (regular,
        // bold, italic, bold-italic) plus box-drawing/accented glyphs a MUD
        // accumulates, rasterized on demand (see the dynamic-atlas pass). The
        // last slot is reserved as a solid (opaque) block for bg/overlays.
        let cols = 32;
        let rows = 32;
        let atlas_w = cols * slot_w;
        let atlas_h = rows * cell_h;
        let mut pixels = vec![0u8; (atlas_w * atlas_h) as usize];
        let solid_index = cols * rows - 1;
        let (qx, qy, qw, qh) = slot_rect(solid_index, cols, slot_w, cell_h);
        for y in qy..qy + qh {
            for x in qx..qx + qw {
                pixels[(y * atlas_w + x) as usize] = 255;
            }
        }
        let solid_uv = rect_to_uv(qx + qw / 2, qy + qh / 2, 0, 0, atlas_w, atlas_h);
        Self {
            font,
            bold_font,
            px,
            cell_w,
            cell_h,
            slot_w,
            ascent,
            glyph_top,
            cols,
            rows,
            atlas_w,
            atlas_h,
            pixels,
            solid_uv,
            slots: HashMap::new(),
            next: 0,
            curl: None,
        }
    }

    pub(crate) fn cell_w(&self) -> u32 {
        self.cell_w
    }
    /// The baseline's row inside a cell: the centered glyph box's top plus
    /// the font ascent.
    fn baseline(&self) -> u32 {
        native_baseline(self.glyph_top, self.ascent)
    }
    pub(crate) fn cell_h(&self) -> u32 {
        self.cell_h
    }
    pub(crate) fn slot_w(&self) -> u32 {
        self.slot_w
    }
    pub(crate) fn solid_uv(&self) -> ([f32; 2], [f32; 2]) {
        self.solid_uv
    }
    pub(crate) fn atlas_size(&self) -> (u32, u32) {
        (self.atlas_w, self.atlas_h)
    }
    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Get (or rasterize on first use) the glyph for `c` in the (bold,
    /// italic) face, returning its UV rect. Falls back to the last slot when
    /// the grid fills.
    pub(crate) fn glyph_uv(&mut self, c: char, bold: bool, italic: bool) -> ([f32; 2], [f32; 2]) {
        let key = (c, bold, italic);
        let index = if let Some(&i) = self.slots.get(&key) {
            i
        } else {
            // Cap three below the count: the last slot is the solid block
            // and the one before it holds the curly underline.
            let i = self.next.min(self.cols * self.rows - 3);
            self.next += 1;
            self.rasterize_into(c, i, bold, italic);
            self.slots.insert(key, i);
            i
        };
        let (x, y, w, h) = slot_rect(index, self.cols, self.slot_w, self.cell_h);
        rect_to_uv(x, y, w, h, self.atlas_w, self.atlas_h)
    }

    /// The curly underline's sprite, one cell wide and `h` rows tall with
    /// lines `t` thick, as a UV rect. It is drawn into its slot on first
    /// use and again when either size changes, and the flag says the
    /// atlas pixels changed and need uploading.
    pub(crate) fn curl_uv(&mut self, t: u32, h: u32) -> (([f32; 2], [f32; 2]), bool) {
        let h = h.min(self.cell_h);
        let index = self.cols * self.rows - 2;
        let (sx, sy, sw, sh) = slot_rect(index, self.cols, self.slot_w, self.cell_h);
        let changed = self.curl != Some((t, h));
        if changed {
            for y in sy..sy + sh {
                let row = (y * self.atlas_w) as usize;
                self.pixels[row + sx as usize..row + (sx + sw) as usize].fill(0);
            }
            let w = self.cell_w as usize;
            let cov = curl_coverage(self.cell_w, h, t);
            for (y, src) in cov.chunks_exact(w).enumerate() {
                let dst = ((sy + y as u32) * self.atlas_w + sx) as usize;
                self.pixels[dst..dst + w].copy_from_slice(src);
            }
            self.curl = Some((t, h));
        }
        (
            rect_to_uv(sx, sy, self.cell_w, h, self.atlas_w, self.atlas_h),
            changed,
        )
    }

    /// UV rect for an already-rasterized glyph, or `None`. Read-only so the
    /// draw path can look up cached glyphs without mutating the atlas (the
    /// texture is uploaded once; non-cached chars fall back to blank).
    pub(crate) fn uv_if_cached(
        &self,
        c: char,
        bold: bool,
        italic: bool,
    ) -> Option<([f32; 2], [f32; 2])> {
        self.slots.get(&(c, bold, italic)).map(|&i| {
            let (x, y, w, h) = slot_rect(i, self.cols, self.slot_w, self.cell_h);
            rect_to_uv(x, y, w, h, self.atlas_w, self.atlas_h)
        })
    }

    /// Rasterize `c` (from the bold or regular face) through CoreGraphics with
    /// font smoothing off, matching the webview's antialiased glyphs, and blit
    /// its coverage into slot `index` at the cell baseline. Italic shears the
    /// glyph so CoreGraphics antialiases the slant.
    fn rasterize_into(&mut self, c: char, index: u32, bold: bool, italic: bool) {
        let face = if bold { &self.bold_font } else { &self.font };
        let Some(glyph_id) = face.glyph_for_char(c) else {
            return;
        };
        let skew = if italic { ITALIC_SKEW } else { 0.0 };
        // font-kit negates the shear into CoreGraphics' c term, so pass -skew
        // for the bounds to match the c = skew we set when rasterizing.
        let shear = Transform2F::row_major(1.0, 0.0, -skew, 1.0, 0.0, 0.0);
        let Ok(bounds) = face.raster_bounds(
            glyph_id,
            self.px,
            shear,
            HintingOptions::None,
            RasterizationOptions::GrayscaleAa,
        ) else {
            return;
        };
        let (bw, bh) = (bounds.width(), bounds.height());
        if bw <= 0 || bh <= 0 {
            return;
        }
        let h = bh as usize;
        // Pad the buffer width for the italic slant: raster_bounds can report
        // the upright width, so without this CoreGraphics clips the overhang
        // before it ever reaches the atlas.
        let extra = if italic {
            (skew.abs() * bh as f32).ceil() as usize + 2
        } else {
            0
        };
        let w = bw as usize + extra;
        let coverage = rasterize_glyph_cg(
            &face.native_font(),
            glyph_id,
            self.px,
            skew,
            bounds.origin_x(),
            bounds.origin_y(),
            w,
            h,
        );
        let (sx, sy, _, _) = slot_rect(index, self.cols, self.slot_w, self.cell_h);
        // The glyph's pen origin sits at the cell baseline; bounds.origin is
        // the ink's offset from it (negative y reaches above the baseline).
        let dst_x0 = sx as i32 + bounds.origin_x();
        let dst_y0 = sy as i32 + self.baseline() as i32 + bounds.origin_y();
        for row in 0..h {
            for col in 0..w {
                let cov = coverage[row * w + col];
                if cov == 0 {
                    continue;
                }
                let dst_x = dst_x0 + col as i32;
                let dst_y = dst_y0 + row as i32;
                if dst_x >= sx as i32
                    && (dst_x as u32) < sx + self.slot_w
                    && (dst_x as u32) < self.atlas_w
                    && dst_y >= sy as i32
                    && (dst_y as u32) < sy + self.cell_h
                    && (dst_y as u32) < self.atlas_h
                {
                    self.pixels[(dst_y as u32 * self.atlas_w + dst_x as u32) as usize] = cov;
                }
            }
        }
    }
}

/// Rasterize one glyph through CoreGraphics with font smoothing disabled, so
/// the coverage matches the webview's antialiased text rather than the heavier
/// smoothed look. Returns a `w * h` alpha coverage buffer (0 = no ink, 255 =
/// full ink). The glyph's bounding box is shifted to the buffer origin; `skew`
/// is the italic shear and `origin_x/origin_y` come from `raster_bounds`.
#[allow(clippy::too_many_arguments)]
fn rasterize_glyph_cg(
    font: &CTFont,
    glyph_id: u32,
    px: f32,
    skew: f32,
    origin_x: i32,
    origin_y: i32,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h];
    let gray = CGColorSpace::create_device_gray();
    let ctx = CGContext::create_bitmap_context(
        Some(pixels.as_mut_ptr().cast()),
        w,
        h,
        8,
        w,
        &gray,
        7, // kCGImageAlphaOnly: one byte per pixel = coverage
    );
    ctx.set_should_antialias(true);
    ctx.set_should_smooth_fonts(false);
    ctx.set_allows_font_smoothing(false);
    // Clear to alpha 0, draw the glyph at alpha 1: the byte is the coverage.
    ctx.set_gray_fill_color(0.0, 0.0);
    ctx.fill_rect(CGRect::new(
        &CGPoint::new(0.0, 0.0),
        &CGSize::new(w as f64, h as f64),
    ));
    ctx.set_gray_fill_color(1.0, 1.0);
    // CoreGraphics is bottom-left origin; flip so row 0 is the top.
    ctx.translate(0.0, h as f64);
    let cg_font = font.copy_to_CGFont();
    ctx.set_font(&cg_font);
    ctx.set_font_size(f64::from(px));
    ctx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
    // Shift the glyph's bounding box to the buffer origin; c is the shear.
    let matrix = CGAffineTransform::new(
        1.0,
        0.0,
        f64::from(skew),
        1.0,
        f64::from(-origin_x),
        f64::from(origin_y),
    );
    ctx.set_text_matrix(&matrix);
    ctx.show_glyphs_at_positions(&[glyph_id as CGGlyph], &[CGPoint::new(0.0, 0.0)]);
    pixels
}

/// Load the first matchable family from a CSS font-family stack, always
/// falling back to the system monospace. Generic CSS names map to
/// font-kit's generic families; everything else is a literal title.
// `select_best_match` mis-ranks faces (it returned Menlo Italic for a
// Normal request), so pick the upright regular face of a family by hand:
// load each face, keep the Normal-style one whose weight is closest to
// 400. font-kit's `copy_font_data` extracts that single face, so fontdue
// reads it at collection index 0.
fn weighted_face(
    source: &font_kit::source::SystemSource,
    family: &str,
    target_weight: f32,
) -> Option<font_kit::handle::Handle> {
    use font_kit::properties::Style;
    let fam = source.select_family_by_name(family).ok()?;
    let mut best: Option<(font_kit::handle::Handle, f32)> = None;
    for handle in fam.fonts() {
        let Ok(font) = handle.load() else { continue };
        let props = font.properties();
        if props.style != Style::Normal {
            continue;
        }
        let weight_dist = (props.weight.0 - target_weight).abs();
        if best.as_ref().map_or(true, |(_, d)| weight_dist < *d) {
            best = Some((handle.clone(), weight_dist));
        }
    }
    best.map(|(handle, _)| handle)
}

// Vosh ships this family and the webview renders with it. font-kit often
// fails to resolve it by its CSS family name (the file's internal family
// name differs), so load the bundled faces directly to match the webview
// exactly.
const JETBRAINS_REGULAR: &[u8] =
    include_bytes!("../../src/assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");
const JETBRAINS_BOLD: &[u8] =
    include_bytes!("../../src/assets/fonts/JetBrainsMonoNerdFont-Bold.ttf");

fn font_from_handle(handle: font_kit::handle::Handle) -> Option<Font> {
    let kit_font = handle.load().ok()?;
    tracing::info!(font = %kit_font.full_name(), "native-surface: atlas font (system)");
    Some(kit_font)
}

/// The regular and bold faces an atlas rasterizes from. Loading them
/// resolves the family through font-kit and reads every face of it,
/// which took 3 to 27 ms for a typical family and 300 to 665 ms for a
/// large CJK family, so a font change loads them on the blocking pool
/// and leaves the main thread only the atlas swap.
pub(crate) struct AtlasFonts {
    regular: Font,
    bold: Font,
}

impl AtlasFonts {
    /// The faces of the first loadable family in the CSS
    /// `family_stack`, falling back to the system monospace. The bold
    /// face falls back to the regular one. None if no font loads.
    pub(crate) fn load(family_stack: &str) -> Option<Self> {
        let regular = load_face(family_stack, false)?;
        let bold = load_face(family_stack, true).unwrap_or_else(|| regular.clone());
        Some(Self { regular, bold })
    }
}

/// The CSS family of the font Vosh bundles.
const BUNDLED_FAMILY: &str = "JetBrainsMono Bundled";

/// The family Berkeley Mono went by while Vosh bundled it, which saved
/// font lists still name.
const RETIRED_BERKELEY: &str = "BerkeleyMono Bundled";

/// The installed families that stand in for [`RETIRED_BERKELEY`]: the
/// Nerd Font build Vosh bundled, then the family the foundry sells.
const BERKELEY_FAMILIES: [&str; 2] = ["BerkeleyMono Nerd Font", "Berkeley Mono"];

/// The families both renderers try, in order, for the saved CSS font
/// list `stack`. Vosh no longer ships Berkeley Mono, so a Berkeley name
/// stands for your installed copy, and [`BUNDLED_FAMILY`] follows each
/// run of Berkeley names for a machine without one.
/// [`RETIRED_BERKELEY`] becomes [`BERKELEY_FAMILIES`]. A repeated name
/// drops out. `renderFontStack` in fontLoader.ts gives the webview the
/// same list, so xterm and the atlas land on the same face and cell.
fn rendered_families(stack: &str) -> Vec<String> {
    fn push(out: &mut Vec<String>, name: &str) {
        if !out.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            out.push(name.to_string());
        }
    }
    let mut out = Vec::new();
    let mut after_berkeley = false;
    for raw in stack.split(',') {
        let name = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if name.is_empty() {
            continue;
        }
        let berkeley = name.to_ascii_lowercase().contains("berkeley");
        if after_berkeley && !berkeley {
            push(&mut out, BUNDLED_FAMILY);
        }
        after_berkeley = berkeley;
        if name.eq_ignore_ascii_case(RETIRED_BERKELEY) {
            for family in BERKELEY_FAMILIES {
                push(&mut out, family);
            }
        } else {
            push(&mut out, name);
        }
    }
    if after_berkeley {
        push(&mut out, BUNDLED_FAMILY);
    }
    out
}

/// The first face of `family_stack` that loads.
fn load_face(family_stack: &str, bold: bool) -> Option<Font> {
    let source = font_kit::source::SystemSource::new();
    let weight = if bold { 700.0 } else { 400.0 };

    for name in rendered_families(family_stack) {
        let lower = name.to_ascii_lowercase();
        // Skip CSS generics; the Menlo/Courier fallback covers them.
        if matches!(
            lower.as_str(),
            "monospace"
                | "ui-monospace"
                | "serif"
                | "ui-serif"
                | "sans-serif"
                | "ui-sans-serif"
                | "system-ui"
        ) {
            continue;
        }
        // The bundled family, matched by the webview.
        if lower.contains("jetbrains") {
            let bytes = if bold {
                JETBRAINS_BOLD
            } else {
                JETBRAINS_REGULAR
            };
            if let Ok(font) = Font::from_bytes(Arc::new(bytes.to_vec()), 0) {
                tracing::info!(bold, "native-surface: atlas font = bundled JetBrainsMono");
                return Some(font);
            }
        }
        // Otherwise a system font, upright face closest to the weight.
        if let Some(font) = weighted_face(&source, &name, weight).and_then(font_from_handle) {
            return Some(font);
        }
    }

    // Platform monospace fallbacks: Menlo (macOS), Consolas (Windows),
    // then Courier New (everywhere).
    weighted_face(&source, "Menlo", weight)
        .or_else(|| weighted_face(&source, "Consolas", weight))
        .or_else(|| weighted_face(&source, "Courier New", weight))
        .and_then(font_from_handle)
}

// ---------------------------------------------------------------------------
// Per-cell GPU instance data
// ---------------------------------------------------------------------------

/// One quad instance. `offset` is the top-left in surface pixels and `size`
/// its width/height. The fragment shader samples the atlas coverage across
/// `uv_min..uv_max` and emits `color` premultiplied by that coverage, so a
/// quad pointing at the solid texel is an opaque fill (background, underline,
/// divider) and one pointing at a glyph slot is the glyph. Glyph quads are
/// drawn at slot width and may overhang the next cell. `repr(C)` so it maps
/// straight to a wgpu vertex buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CellInstance {
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub color: Rgba,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// Build one `CellInstance` per cell, row-major. Pure: the grid is read
/// through `cell` (returns char, fg, bg) and glyph atlas UVs through `uv`,
/// so it tests without a live grid or GPU.
/// Build the per-cell quads, split into two layers: opaque background fills
/// (one per cell, cell-sized, pointing at the solid texel) and glyph quads
/// (one per non-blank cell, slot-sized so an italic can overhang). Returned
/// separately so the caller can draw all backgrounds before any glyph, which
/// lets a glyph spill over its neighbor's background. Pure: testable without
/// a grid or GPU.
fn build_instances(
    cols: usize,
    rows: usize,
    cell_w: f32,
    cell_h: f32,
    y0: f32,
    slot_w: f32,
    solid_uv: ([f32; 2], [f32; 2]),
    mut cell: impl FnMut(usize, usize) -> (char, Rgba, Rgba, bool, bool),
    mut uv: impl FnMut(char, bool, bool) -> ([f32; 2], [f32; 2]),
) -> (Vec<CellInstance>, Vec<CellInstance>) {
    let mut backgrounds = Vec::with_capacity(cols * rows);
    let mut glyphs = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let (ch, fg, bg, bold, italic) = cell(col, row);
            let offset = [col as f32 * cell_w, y0 + row as f32 * cell_h];
            backgrounds.push(CellInstance {
                offset,
                size: [cell_w, cell_h],
                color: bg,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
            if ch != ' ' && ch != '\0' {
                let (uv_min, uv_max) = uv(ch, bold, italic);
                glyphs.push(CellInstance {
                    offset,
                    size: [slot_w, cell_h],
                    color: fg,
                    uv_min,
                    uv_max,
                });
            }
        }
    }
    (backgrounds, glyphs)
}

/// A drawable region of the surface: `vis` rows starting at pixel `y0`,
/// reading grid line `line0 + row`. The split draws two (history above the
/// divider, live tail below, each scissored); non-split draws one.
struct Region {
    y0: f32,
    vis: usize,
    line0: i32,
}

/// Underline/strike marks: (column, row top in pixels, color, kind). A
/// strike carries `Underline::None` as its kind.
type Marks = Vec<(usize, f32, Rgba, Underline)>;

/// Line-major inclusive containment of a cell in a selection range given as
/// start and end line/column.
fn cell_in_selection(bounds: Option<(i32, usize, i32, usize)>, line: i32, col: usize) -> bool {
    match bounds {
        Some((sl, sc, el, ec)) => {
            (line > sl || (line == sl && col >= sc)) && (line < el || (line == el && col <= ec))
        }
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Prompt bands
// ---------------------------------------------------------------------------

// The band under a lifted prompt in CSS px, as the prompt boards measure it
// and src/lib/promptBands.ts draws it on xterm. It reaches 4 past the text
// on each side and 2 above and below, at radius 4. Lifts on adjacent rows
// stop 1 inside their shared row edge, so 2 of ground stays between them.
// Both sides run fixtures/prompt-bands/cases.json, so keep them in step.
const BAND_X: f32 = 4.0;
const BAND_Y: f32 = 2.0;
const BAND_Y_ADJACENT: f32 = -1.0;
const BAND_RADIUS: f32 = 4.0;
// Light themes draw a 1 px inset ring on the band, as a box shadow does.
const BAND_RING: f32 = 1.0;
const LIGHT_RING: Paint = Paint {
    r: 0,
    g: 0,
    b: 0,
    a: 0.14,
};
// A prompt is never this tall, so a lift that starts this far past a
// region never reaches into it. src/lib/promptBands.ts uses the same bound.
const MAX_LIFT_ROWS: i32 = 64;

/// A lift as one region shows it. Its rows count from the region's first
/// row, below zero or past the region's last when the region cuts it. Its
/// columns run from its leftmost start to one past its rightmost glyph.
/// `notch` is one past the last glyph of its last row, when that row is
/// narrower than the widest and your echo, or anything else, shows after
/// the lift on it, so the band steps in there instead of running under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LiftBox {
    id: u64,
    top: i32,
    bottom: i32,
    left: usize,
    right: usize,
    notch: Option<usize>,
}

/// Gather `spans` into one box per lift, top first, keeping the lifts that
/// meet the `vis` rows of a region whose first row is grid line `line0`.
fn lift_boxes(spans: &[LiftSpan], line0: i32, vis: usize) -> Vec<LiftBox> {
    let mut boxes: Vec<LiftBox> = Vec::new();
    // Each box's last row as the spans reach it: its end and whether
    // something shows after it.
    let mut last: Vec<(usize, bool)> = Vec::new();
    for s in spans {
        let row = s.line - line0;
        match boxes.iter().position(|b| b.id == s.id) {
            Some(i) => {
                let b = &mut boxes[i];
                b.top = b.top.min(row);
                if row > b.bottom {
                    last[i] = (s.end, s.after);
                }
                b.bottom = b.bottom.max(row);
                b.left = b.left.min(s.first);
                b.right = b.right.max(s.end);
            }
            None => {
                boxes.push(LiftBox {
                    id: s.id,
                    top: row,
                    bottom: row,
                    left: s.first,
                    right: s.end,
                    notch: None,
                });
                last.push((s.end, s.after));
            }
        }
    }
    for (b, &(end, after)) in boxes.iter_mut().zip(&last) {
        if b.bottom > b.top && after && end > b.left && end < b.right {
            b.notch = Some(end);
        }
    }
    let vis = i32::try_from(vis).unwrap_or(i32::MAX);
    boxes.retain(|b| b.bottom >= 0 && b.top < vis);
    boxes.sort_by_key(|b| (b.top, b.bottom));
    boxes
}

/// A band's rectangle in pane pixels. A band with a notch leaves out the
/// part right of `notch[0]` and below `notch[1]`, both from its own left
/// and top, where your echo sits.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BandRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    notch: Option<[f32; 2]>,
}

/// Each box's band, placed as `layoutBands` places it on xterm, in device
/// pixels at `scale`, for cells `cell_w` by `cell_h` in a region whose
/// first row sits at `y0`.
fn band_rects(boxes: &[LiftBox], y0: f32, cell_w: f32, cell_h: f32, scale: f32) -> Vec<BandRect> {
    boxes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let above = i.checked_sub(1).map(|j| boxes[j]);
            let below = boxes.get(i + 1);
            let top_out = if above.is_some_and(|a| a.bottom + 1 == b.top) {
                BAND_Y_ADJACENT
            } else {
                BAND_Y
            };
            let bottom_out = if below.is_some_and(|n| b.bottom + 1 == n.top) {
                BAND_Y_ADJACENT
            } else {
                BAND_Y
            };
            let top = y0 + b.top as f32 * cell_h - top_out * scale;
            let bottom = y0 + (b.bottom + 1) as f32 * cell_h + bottom_out * scale;
            let left = b.left as f32 * cell_w - BAND_X * scale;
            let right = b.right as f32 * cell_w + BAND_X * scale;
            let notch = b.notch.map(|end| {
                [
                    end as f32 * cell_w + BAND_X * scale - left,
                    y0 + b.bottom as f32 * cell_h - top,
                ]
            });
            BandRect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
                notch,
            }
        })
        .collect()
}

/// The quads that draw `rects` through the band shader, moved by `shift`
/// into the band pass's viewport: a rounded fill in `fill`, then an inset
/// ring in `ring` when the theme is light. The shader reads the corner
/// radius and the ring width from `uv_min`, and the notch from `uv_max`,
/// zero for none.
fn band_instances(
    rects: &[BandRect],
    shift: [f32; 2],
    fill: Paint,
    ring: Option<Paint>,
    scale: f32,
) -> Vec<CellInstance> {
    let radius = BAND_RADIUS * scale;
    let mut out = Vec::with_capacity(rects.len() * 2);
    for r in rects {
        let quad = |color: Paint, ring_w: f32| CellInstance {
            offset: [r.x + shift[0], r.y + shift[1]],
            size: [r.w, r.h],
            color: paint_to_rgba(color),
            uv_min: [radius, ring_w],
            uv_max: r.notch.unwrap_or([0.0, 0.0]),
        };
        out.push(quad(fill, 0.0));
        if let Some(ring) = ring {
            out.push(quad(ring, BAND_RING * scale));
        }
    }
    out
}

/// The band pass's viewport, `[x, y, width, height]` in the target: the
/// pane grown by a band's reach on every side, kept inside the target.
fn band_viewport(pane: [u32; 4], target: [u32; 2], scale: f32) -> [u32; 4] {
    let [x, y, w, h] = pane;
    let pad_x = (BAND_X * scale).ceil() as u32;
    let pad_y = (BAND_Y * scale).ceil() as u32;
    let left = x.saturating_sub(pad_x);
    let top = y.saturating_sub(pad_y);
    let right = (x + w + pad_x).min(target[0].max(x + w));
    let bottom = (y + h + pad_y).min(target[1].max(y + h));
    [left, top, right - left, bottom - top]
}

/// A pipeline, its bind group, and the viewport it draws into, as
/// `[x, y, width, height]` in the target.
type Stage<'a> = (&'a wgpu::RenderPipeline, &'a wgpu::BindGroup, [u32; 4]);

/// Point `rpass` at `stage`.
fn set_stage<'a>(rpass: &mut wgpu::RenderPass<'a>, stage: Stage<'a>) {
    let (pipeline, bind_group, [x, y, w, h]) = stage;
    rpass.set_pipeline(pipeline);
    rpass.set_bind_group(0, bind_group, &[]);
    rpass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
}

/// Draw `range` of the band quads through `bands`, clipped to `clip`, then
/// point the pass back at `cells`, whose own clip the caller sets next.
fn draw_bands<'a>(
    rpass: &mut wgpu::RenderPass<'a>,
    bands: Stage<'a>,
    cells: Stage<'a>,
    clip: [u32; 4],
    range: std::ops::Range<u32>,
) {
    set_stage(rpass, bands);
    let [x, y, w, h] = clip;
    rpass.set_scissor_rect(x, y, w, h);
    rpass.draw(0..6, range);
    set_stage(rpass, cells);
}

/// The quad color for a cell of plain ground while bands draw: `tints`,
/// the selection and find match over it, bottom first, as one translucent
/// color that blends over whatever lies under the cell, a band or the
/// clear ground, as each would blend over it in sRGB. Clear with no tint,
/// so the band shows.
fn ground_tint(tints: &[Paint]) -> Rgba {
    // Premultiplied sRGB and coverage, stacked.
    let mut acc = [0.0f32; 3];
    let mut alpha = 0.0f32;
    for t in tints {
        let a = t.a.clamp(0.0, 1.0);
        for (i, c) in [t.r, t.g, t.b].into_iter().enumerate() {
            acc[i] = f32::from(c) / 255.0 * a + acc[i] * (1.0 - a);
        }
        alpha = a + alpha * (1.0 - a);
    }
    if alpha <= 0.0 {
        return [0.0; 4];
    }
    [
        srgb_to_linear(acc[0] / alpha),
        srgb_to_linear(acc[1] / alpha),
        srgb_to_linear(acc[2] / alpha),
        alpha,
    ]
}

// ---------------------------------------------------------------------------
// wgpu cell renderer
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    surface_size: [f32; 2],
    // Neither shader reads past the surface size. The pad keeps the
    // uniform at 16 bytes, the size both shaders declare.
    _pad: [f32; 2],
}

const CELL_SHADER: &str = r"
struct Uniforms { surface_size: vec2<f32>, _pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vi: u32,
    @location(0) offset: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vi];
    let px = offset + corner * size;
    let ndc = vec2<f32>(
        px.x / u.surface_size.x * 2.0 - 1.0,
        1.0 - px.y / u.surface_size.y * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(uv_min, uv_max, corner);
    out.color = color;
    return out;
}

fn lin_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // Coverage from the atlas (1.0 at the solid texel for fills). Emit the
    // color premultiplied by coverage and sRGB-encoded; the surface is a
    // non-sRGB format so hardware alpha blending composites in gamma space,
    // matching how xterm's canvas renderer antialiases. Premultiplied means
    // a glyph overhanging its cell blends cleanly over the neighbor.
    let cov = textureSample(atlas_tex, atlas_samp, in.uv).r * in.color.a;
    let srgb = lin_to_srgb(in.color.rgb);
    return vec4<f32>(srgb * cov, cov);
}
";

/// The prompt band: a rounded rectangle, or its inset ring, covered by
/// its signed distance the way a browser antialiases a border radius. The
/// instance's `uv_min` carries the radius and the ring width, zero for a
/// fill. Its `uv_max` carries the notch, zero for none: the band is then
/// the union of the rows above the last, full width down to the last
/// row's top, and every row as wide as the last one. The output is
/// premultiplied and sRGB encoded like the cells.
const BAND_SHADER: &str = r"
struct Uniforms { surface_size: vec2<f32>, _pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct BandOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) shape: vec2<f32>,
    @location(4) notch: vec2<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vi: u32,
    @location(0) offset: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
) -> BandOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vi];
    let px = offset + corner * size;
    var out: BandOut;
    out.pos = vec4<f32>(
        px.x / u.surface_size.x * 2.0 - 1.0,
        1.0 - px.y / u.surface_size.y * 2.0,
        0.0,
        1.0,
    );
    out.local = corner * size;
    out.size = size;
    out.color = color;
    out.shape = uv_min;
    out.notch = uv_max;
    return out;
}

// The signed distance from `p` to a rectangle `size` big at the origin,
// its corners rounded by `r`.
fn rounded(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let half = size * 0.5;
    let rr = min(r, min(half.x, half.y));
    let q = abs(p - half) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn lin_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs(in: BandOut) -> @location(0) vec4<f32> {
    var d = rounded(in.local, in.size, in.shape.x);
    if (in.notch.x > 0.0) {
        let upper = rounded(in.local, vec2<f32>(in.size.x, in.notch.y), in.shape.x);
        let lower = rounded(in.local, vec2<f32>(in.notch.x, in.size.y), in.shape.x);
        d = min(upper, lower);
    }
    var cov = clamp(0.5 - d, 0.0, 1.0);
    if (in.shape.y > 0.0) {
        cov = cov - clamp(0.5 - (d + in.shape.y), 0.0, 1.0);
    }
    let a = cov * in.color.a;
    return vec4<f32>(lin_to_srgb(in.color.rgb) * a, a);
}
";

/// Owns the glyph atlas texture and the instanced pipeline that draws the
/// terminal grid. One quad per cell; the fragment shader composites the
/// glyph over the cell background by atlas coverage.
pub(crate) struct CellRenderer {
    atlas: GlyphAtlas,
    texture: wgpu::Texture,
    space_uv: ([f32; 2], [f32; 2]),
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    /// The prompt bands: their own shader, and their own uniforms for the
    /// viewport that reaches past the pane.
    band_pipeline: wgpu::RenderPipeline,
    band_bind_group: wgpu::BindGroup,
    band_uniform_buffer: wgpu::Buffer,
}

impl CellRenderer {
    /// Load the fonts of `font_stack` and build the renderer from them.
    /// `None` if no font loads. It blocks while the fonts load, so a
    /// font change uses [`Self::with_fonts`] with fonts loaded on the
    /// blocking pool.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        font_stack: &str,
        font_px: f32,
    ) -> Option<Self> {
        let fonts = AtlasFonts::load(font_stack)?;
        Some(Self::with_fonts(device, queue, format, fonts, font_px))
    }

    /// Build the atlas from loaded `fonts` (printable ASCII
    /// pre-rasterized and uploaded once), the bind group, and the
    /// pipeline.
    pub(crate) fn with_fonts(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: AtlasFonts,
        font_px: f32,
    ) -> Self {
        let atlas = GlyphAtlas::from_fonts(fonts, font_px);
        Self::with_atlas(device, queue, format, atlas)
    }

    /// Build the renderer around `atlas`.
    fn with_atlas(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        mut atlas: GlyphAtlas,
    ) -> Self {
        for code in 0x20u8..0x7f {
            let _ = atlas.glyph_uv(code as char, false, false);
        }
        let space_uv = atlas
            .uv_if_cached(' ', false, false)
            .unwrap_or(([0.0, 0.0], [0.0, 0.0]));
        let (atlas_w, atlas_h) = atlas.atlas_size();

        let extent = wgpu::Extent3d {
            width: atlas_w,
            height: atlas_h,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-atlas"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas.pixels(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            extent,
        );
        let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let band_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("band-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let band_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("band-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: band_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cell-shader"),
            source: wgpu::ShaderSource::Wgsl(CELL_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cell-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CellInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // offset
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // size
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // uv_min
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // uv_max
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        };
        // Premultiplied-alpha over: the shader already multiplies
        // color by coverage, so src factor is One.
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline_for = |label: &str, module: &wgpu::ShaderModule| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: "vs",
                    buffers: std::slice::from_ref(&instance_layout),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: "fs",
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(premultiplied),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };
        let pipeline = pipeline_for("cell-pipeline", &shader);
        let band_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("band-shader"),
            source: wgpu::ShaderSource::Wgsl(BAND_SHADER.into()),
        });
        let band_pipeline = pipeline_for("band-pipeline", &band_shader);

        let instance_capacity = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances"),
            size: (instance_capacity * std::mem::size_of::<CellInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            atlas,
            texture,
            space_uv,
            pipeline,
            bind_group,
            uniform_buffer,
            instance_buffer,
            instance_capacity,
            band_pipeline,
            band_bind_group,
            band_uniform_buffer,
        }
    }

    /// Columns and rows that fill a surface of the given pixel size at the
    /// atlas cell size. Used to size the grid to the pane.
    pub(crate) fn grid_size_for(&self, surface_w: u32, surface_h: u32) -> (usize, usize) {
        let cols = (surface_w as f32 / self.atlas.cell_w() as f32)
            .floor()
            .max(1.0) as usize;
        let rows = (surface_h as f32 / self.atlas.cell_h() as f32)
            .floor()
            .max(1.0) as usize;
        (cols, rows)
    }

    /// Atlas cell size in pixels, so the mouse handler can map a point to a
    /// grid cell.
    pub(crate) fn cell_size_px(&self) -> (f32, f32) {
        (self.atlas.cell_w() as f32, self.atlas.cell_h() as f32)
    }

    /// Build instances from `grid` and draw them into `view`, clearing to
    /// the default background first.
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        grid: &crate::term_grid::TermGrid,
        surface_w: u32,
        surface_h: u32,
        split_ratio: f32,
        placement: Placement,
    ) -> Drawn {
        let cell_w = self.atlas.cell_w() as f32;
        let cell_h = self.atlas.cell_h() as f32;
        let cols = grid.columns();
        let rows = grid.screen_lines();
        let space_uv = self.space_uv;

        // Split-scrollback: when scrolled up, draw a frozen-history region on
        // top and the live tail below, separated by a draggable divider at
        // `split_ratio`. The divider tracks the pointer per PIXEL (no row
        // quantization — a row-snapped divider ratchets under the mouse);
        // each region keeps its rows cell-aligned internally and clips its
        // edge row mid-cell against the divider with a scissor rect.
        let offset = grid.display_offset() as i32;
        // Find matches (and the active one) drive a highlight pass and
        // suppress the split so the match shows in a single full view.
        let (find_matches, find_active_match) = crate::term_grid::find_snapshot();
        // Wash paint. Washed lines carry a distinctive quarter-strength
        // truecolor background (NamedColor::wash_tint in the trigger
        // crate) on the text of the line. That value is a SIGNAL, not
        // the final color: the bytes stay canonical so they survive
        // resize, reflow, and scrollback reload, and the row is painted
        // here in the ACTIVE THEME's color instead. Canonical teal on a
        // warm near-black ground never matched the palette around it.
        //
        // Each entry maps the canonical tint to the field this renderer
        // draws: the theme's color for that mark mixed down into the
        // terminal ground. There is no edge bar, so a washed row reads
        // as one quiet band, the way the rest of the window marks rows.
        // How far the field carries toward the mark color. Low enough
        // that a washed row reads as marked rather than painted.
        let wash_field_mix = 0.18_f32;
        let wash_paint: HashMap<[u8; 3], Rgba> = vosh_automation::trigger::NamedColor::ALL
            .iter()
            .enumerate()
            .map(|(idx, c)| {
                let (tr, tg, tb) = c.wash_tint();
                let mark = ansi16(idx);
                let ground = theme_bg();
                let mix = |m: u8, g: u8| {
                    (f32::from(g) + (f32::from(m) - f32::from(g)) * wash_field_mix).round() as u8
                };
                let field = Rgb {
                    r: mix(mark.r, ground.r),
                    g: mix(mark.g, ground.g),
                    b: mix(mark.b, ground.b),
                };
                ([tr, tg, tb], rgb_to_rgba(field))
            })
            .collect();
        let finding = !find_matches.is_empty();
        let split = offset > 0 && rows >= 6 && !finding;
        let divider_px = if split {
            let raw = split_ratio * surface_h as f32;
            Some(raw.clamp(cell_h, surface_h as f32 - cell_h).round())
        } else {
            None
        };

        let regions: Vec<Region> = match divider_px {
            Some(divider_px) => {
                // History on top, anchored to the top edge; its last row can
                // hang past the divider and gets scissored.
                let top_vis = (divider_px / cell_h).ceil() as usize;
                // The live rows below keep their absolute top-aligned
                // positions, IDENTICAL to the non-split view: the divider
                // only reveals or covers them. Re-anchoring them (to the
                // divider or the bottom edge) makes the whole live region
                // jump the moment the split opens. The first live row can
                // rise above the divider and gets scissored.
                let row_start = ((divider_px / cell_h).floor() as usize).min(rows - 1);
                vec![
                    Region {
                        y0: 0.0,
                        vis: top_vis,
                        line0: -offset,
                    },
                    Region {
                        y0: row_start as f32 * cell_h,
                        vis: rows - row_start,
                        line0: row_start as i32,
                    },
                ]
            }
            None => vec![Region {
                y0: 0.0,
                vis: rows,
                line0: -offset,
            }],
        };

        // Dynamic atlas: rasterize any visible glyph not yet cached, then
        // re-upload the atlas texture if it grew. Steady state (every glyph
        // already cached) costs only the lookups, no upload.
        let mut atlas_grew = false;
        let blink_hidden = placement.blink_hidden;
        // A cell on screen that blinks with something to hide.
        let mut blinks = false;
        for reg in &regions {
            for row in 0..reg.vis {
                for col in 0..cols {
                    let grid_line = reg.line0 + row as i32;
                    let (ch, fg, _, flags) = grid.cell_at_line(grid_line, col);
                    blinks |= blinks_visibly(ch, flags);
                    let ch = drawn_char(ch, flags, blink_hidden);
                    let bold = wants_bold_font(fg, flags);
                    if ch != ' ' && self.atlas.uv_if_cached(ch, bold, flags.italic).is_none() {
                        self.atlas.glyph_uv(ch, bold, flags.italic);
                        atlas_grew = true;
                    }
                }
            }
        }
        // Where the lines sit at this scale, and the curl's sprite for it.
        let decor = decor(
            self.atlas.cell_w(),
            self.atlas.cell_h(),
            self.atlas.baseline(),
            placement.scale,
        );
        let (curl_uv, curl_drawn) = self.atlas.curl_uv(decor.t, decor.curl_h);
        atlas_grew |= curl_drawn;
        if atlas_grew {
            let (aw, ah) = self.atlas.atlas_size();
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                self.atlas.pixels(),
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(aw),
                    rows_per_image: Some(ah),
                },
                wgpu::Extent3d {
                    width: aw,
                    height: ah,
                    depth_or_array_layers: 1,
                },
            );
        }

        let atlas = &self.atlas;
        // The exact fraction of surface height where the divider is drawn,
        // so the cursor rect and grab band line up with the rendered line.
        let divider_frac = divider_px.map(|px| px / surface_h as f32);
        let chrome = chrome_paint();
        let divider = paint_to_rgba(chrome.divider);

        // Selection highlight: compute the range once, composite the
        // selection color over each selected cell's own background.
        let selection = grid.selection_bounds();

        // Find-match highlight, stronger for the current match. Keyed by
        // grid line for an O(1) lookup per cell.
        let mut find_by_line: HashMap<i32, Vec<(usize, usize, bool)>> = HashMap::new();
        for &(line, start, end) in &find_matches {
            let active = find_active_match == Some((line, start, end));
            find_by_line
                .entry(line)
                .or_default()
                .push((start, end, active));
        }

        // URL under the pointer reads as a link: the link color and
        // underlined (it opens on Cmd+click).
        let hover = crate::native_surface::hover_url();
        let link = paint_to_rgba(chrome.link);

        let solid_uv = atlas.solid_uv();
        let slot_w = atlas.slot_w() as f32;
        // Marks carry the absolute pixel y of their row so region offsets
        // apply exactly once.
        let mut underlines: Marks = Vec::new();
        let mut strikeouts: Marks = Vec::new();
        // Shared per-cell styling: colors, selection, find highlight, hover,
        // and the underline/strike marks. Region closures wrap this with
        // their own line/pixel mapping.
        // Washed rows among the visible lines. A row is washed when its
        // first cell carries a wash signal and it holds text. The field
        // then runs the full width, painted here rather than by erasing
        // the row in the bytes, so a narrower terminal never wraps the
        // tint onto a row of its own. A row of signal-colored blanks is
        // the wrapped tail of an older wash that did erase its row, and
        // paints as plain ground.
        let mut washed: HashMap<i32, Rgba> = HashMap::new();
        for reg in &regions {
            for row in 0..reg.vis {
                let grid_line = reg.line0 + row as i32;
                let (_, _, first_bg, _) = grid.cell_at_line(grid_line, 0);
                let Color::Spec(rgb) = first_bg else {
                    continue;
                };
                let Some(&field) = wash_paint.get(&[rgb.r, rgb.g, rgb.b]) else {
                    continue;
                };
                let has_text = (0..cols).any(|col| {
                    let (ch, _, _, _) = grid.cell_at_line(grid_line, col);
                    ch != ' ' && ch != '\0'
                });
                if has_text {
                    washed.insert(grid_line, field);
                }
            }
        }
        // Your prompt shows lifted: each region reads the lifts that meet
        // it, and the cells of the ground the text sits on go clear so
        // the bands drawn under them show.
        let bands_on = prompt_bands();
        let mut region_boxes: Vec<Vec<LiftBox>> = Vec::new();
        if bands_on {
            for reg in &regions {
                let last = reg.line0 + reg.vis as i32 - 1;
                let spans = grid.lift_spans(reg.line0 - MAX_LIFT_ROWS, last + MAX_LIFT_ROWS);
                region_boxes.push(lift_boxes(&spans, reg.line0, reg.vis));
            }
        }
        let style_cell = |grid_line: i32,
                          col: usize,
                          y_top: f32,
                          underlines: &mut Marks,
                          strikeouts: &mut Marks| {
            let (ch, fg, bg, flags) = grid.cell_at_line(grid_line, col);
            let (mut fg_rgba, mut bg_rgba) = styled_colors(fg, bg, flags);
            // Repaint the canonical wash signal in theme colors, and carry
            // a washed row's field across its default-colored cells. Runs
            // before selection and find so both still win over a washed
            // row, the same as any other background.
            let field = washed.get(&grid_line).copied();
            let signal =
                matches!(bg, Color::Spec(rgb) if wash_paint.contains_key(&[rgb.r, rgb.g, rgb.b]));
            if signal {
                bg_rgba = field.unwrap_or_else(|| {
                    styled_colors(fg, Color::Named(NamedColor::Background), flags).1
                });
            } else if let Some(field) = field {
                if matches!(bg, Color::Named(NamedColor::Background)) && !flags.inverse {
                    bg_rgba = field;
                }
            }
            // The plain ground, which a band may lie under. It draws clear,
            // and its tints blend over whatever lies under it, so a band
            // keeps its shape under a selection or a find match.
            let ground = bands_on
                && matches!(bg, Color::Named(NamedColor::Background))
                && !flags.inverse
                && !signal
                && field.is_none();
            let mut tints = [chrome.selection; 2];
            let mut tinted = 0;
            if cell_in_selection(selection, grid_line, col) {
                tints[tinted] = chrome.selection;
                tinted += 1;
            }
            if let Some(ranges) = find_by_line.get(&grid_line) {
                for &(start, end, active) in ranges {
                    if col >= start && col < end {
                        tints[tinted] = if active {
                            chrome.current_match
                        } else {
                            chrome.find_match
                        };
                        tinted += 1;
                        break;
                    }
                }
            }
            if ground {
                bg_rgba = ground_tint(&tints[..tinted]);
            } else {
                for &tint in &tints[..tinted] {
                    bg_rgba = blend_over(tint, bg_rgba);
                }
            }
            let hovered =
                hover.is_some_and(|(hl, hs, he)| grid_line == hl && col >= hs && col < he);
            if hovered {
                fg_rgba = link;
            }
            if draws_lines(flags, blink_hidden) {
                // A link under the pointer reads as a plain underline in
                // the link color, whatever line the cell carries.
                if hovered {
                    underlines.push((col, y_top, link, Underline::Single));
                } else if flags.underline != Underline::None {
                    let color = underline_color(flags, fg_rgba);
                    underlines.push((col, y_top, color, flags.underline));
                }
                if flags.strikeout {
                    strikeouts.push((col, y_top, fg_rgba, Underline::None));
                }
            }
            (
                drawn_char(ch, flags, blink_hidden),
                fg_rgba,
                bg_rgba,
                wants_bold_font(fg, flags),
                flags.italic,
            )
        };

        // One instance buffer, one draw range per region (scissored to its
        // side of the divider) plus an unscissored overlay range. Within a
        // region: backgrounds, then underline/strike marks, then glyphs so
        // an italic can overhang its neighbor's background.
        let mut instances: Vec<CellInstance> = Vec::new();
        let mut region_ranges: Vec<std::ops::Range<u32>> = Vec::new();
        for reg in &regions {
            let start = instances.len() as u32;
            let (backgrounds, glyphs) = build_instances(
                cols,
                reg.vis,
                cell_w,
                cell_h,
                reg.y0,
                slot_w,
                solid_uv,
                |col, row| {
                    style_cell(
                        reg.line0 + row as i32,
                        col,
                        reg.y0 + row as f32 * cell_h,
                        &mut underlines,
                        &mut strikeouts,
                    )
                },
                |ch, bold, italic| atlas.uv_if_cached(ch, bold, italic).unwrap_or(space_uv),
            );
            instances.extend(backgrounds);
            instances.extend(line_instances(
                &underlines,
                &strikeouts,
                &decor,
                atlas.cell_w(),
                solid_uv,
                curl_uv,
            ));
            underlines.clear();
            strikeouts.clear();
            instances.extend(glyphs);
            region_ranges.push(start..instances.len() as u32);
        }
        // Overlays draw unscissored: the divider line at its exact pixel
        // and the scrollbar.
        let overlay_start = instances.len() as u32;
        if let Some(divider_px) = divider_px {
            let thickness = 2.0_f32;
            instances.push(CellInstance {
                offset: [0.0, divider_px - thickness * 0.5],
                size: [cols as f32 * cell_w, thickness],
                color: divider,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
        }

        // Overlay scrollbar on the right edge while scrolled: a subtle
        // track and a proportional thumb (the page keeps its xterm copy
        // hidden). Drag mapping lives in native_surface.
        let scrollback = grid.scrollback_len();
        if offset > 0 && scrollback > 0 {
            let total = (scrollback + rows) as f32;
            let sb_w = (cell_w * 0.45).clamp(4.0, 10.0);
            let x0 = surface_w as f32 - sb_w;
            let h = surface_h as f32;
            let thumb = paint_to_rgba(chrome.scrollbar);
            let mut track = thumb;
            track[3] *= SCROLLBAR_TRACK_SHARE;
            instances.push(CellInstance {
                offset: [x0, 0.0],
                size: [sb_w, h],
                color: track,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
            let thumb_h = (h * rows as f32 / total).max(24.0);
            let scroll_top = (scrollback - offset as usize) as f32;
            let thumb_y = ((h - thumb_h) * scroll_top / scrollback as f32).clamp(0.0, h - thumb_h);
            instances.push(CellInstance {
                offset: [x0, thumb_y],
                size: [sb_w, thumb_h],
                color: thumb,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
        }
        let overlay_range = overlay_start..instances.len() as u32;

        // The bands, one range per region, drawn under that region's cells
        // in a viewport that reaches past the pane by a band's reach.
        let pane = [placement.x, placement.y, surface_w, surface_h];
        let band_view = band_viewport(pane, placement.target, placement.scale);
        let shift = [
            (placement.x - band_view[0]) as f32,
            (placement.y - band_view[1]) as f32,
        ];
        let mut band_ranges: Vec<std::ops::Range<u32>> = Vec::new();
        let newest = region_boxes.iter().flatten().map(|b| b.id).max();
        let reach = prompt_reach() * placement.scale;
        for (reg, boxes) in regions.iter().zip(&region_boxes) {
            let start = instances.len() as u32;
            let mut rects = band_rects(boxes, reg.y0, cell_w, cell_h, placement.scale);
            widen_newest(&mut rects, boxes, newest, reach);
            instances.extend(band_instances(
                &rects,
                shift,
                chrome.selrow,
                chrome.ring,
                placement.scale,
            ));
            band_ranges.push(start..instances.len() as u32);
        }

        let uniforms = Uniforms {
            surface_size: [surface_w as f32, surface_h as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if band_ranges.iter().any(|r| !r.is_empty()) {
            let band_uniforms = Uniforms {
                surface_size: [band_view[2] as f32, band_view[3] as f32],
                _pad: [0.0; 2],
            };
            queue.write_buffer(
                &self.band_uniform_buffer,
                0,
                bytemuck::bytes_of(&band_uniforms),
            );
        }

        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cell-instances"),
                size: (self.instance_capacity * std::mem::size_of::<CellInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        // Clear to the terminal background so any sliver beyond the grid
        // matches the cells. The surface is a non-sRGB format and the shader
        // writes sRGB-encoded values, so the clear is the raw sRGB bg.
        let bg = theme_bg();
        let clear = [
            f32::from(bg.r) / 255.0,
            f32::from(bg.g) / 255.0,
            f32::from(bg.b) / 255.0,
        ];
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cell-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(clear[0]),
                        g: f64::from(clear[1]),
                        b: f64::from(clear[2]),
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        // Map the pane-local instance space onto the pane's rect in the
        // target. The clear above already painted the whole target, so the
        // area outside the pane shows the terminal background.
        let (ox, oy) = (placement.x, placement.y);
        let cells = (
            &self.pipeline,
            &self.bind_group,
            [ox, oy, surface_w, surface_h],
        );
        let bands = (&self.band_pipeline, &self.band_bind_group, band_view);
        // A region's bands, clipped to its rows grown by a band's reach,
        // from `from` to `to` in the target.
        let [bx, by, bw, bh] = band_view;
        let band_clip = |i: usize, from: u32, to: u32| {
            band_ranges
                .get(i)
                .filter(|r| !r.is_empty())
                .map(|r| (r.clone(), [bx, from, bw, to.saturating_sub(from).max(1)]))
        };
        set_stage(&mut rpass, cells);
        match divider_px {
            Some(divider_px) => {
                // Each region clips its overhanging edge row at the divider.
                let div = (divider_px as u32).min(surface_h.saturating_sub(1)).max(1);
                if let Some((range, clip)) = band_clip(0, by, oy + div) {
                    draw_bands(&mut rpass, bands, cells, clip, range);
                }
                rpass.set_scissor_rect(ox, oy, surface_w, div);
                rpass.draw(0..6, region_ranges[0].clone());
                if let Some((range, clip)) = band_clip(1, oy + div, by + bh) {
                    draw_bands(&mut rpass, bands, cells, clip, range);
                }
                rpass.set_scissor_rect(ox, oy + div, surface_w, surface_h - div);
                rpass.draw(0..6, region_ranges[1].clone());
                rpass.set_scissor_rect(ox, oy, surface_w, surface_h);
                rpass.draw(0..6, overlay_range);
            }
            None => {
                if let Some((range, clip)) = band_clip(0, by, by + bh) {
                    draw_bands(&mut rpass, bands, cells, clip, range);
                }
                rpass.set_scissor_rect(ox, oy, surface_w, surface_h);
                rpass.draw(0..6, region_ranges[0].start..overlay_range.end);
            }
        }
        drop(rpass);
        Drawn {
            divider: divider_frac,
            blinks,
        }
    }
}

#[cfg(test)]
// The tests assert exact float values that are copied verbatim through the
// instance builder (offsets are products of small integers, colors are
// passed through untouched), so strict equality is the correct check.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn spec_passes_through() {
        assert_eq!(
            color_to_rgba(Color::Spec(Rgb { r: 255, g: 0, b: 0 })),
            [1.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn named_red_is_ansi_one() {
        assert_eq!(
            color_to_rgba(Color::Named(NamedColor::Red)),
            rgb_to_rgba(ANSI_16[1])
        );
    }

    #[test]
    fn indexed_low_range_is_ansi_palette() {
        assert_eq!(color_to_rgba(Color::Indexed(9)), rgb_to_rgba(ANSI_16[9]));
    }

    #[test]
    fn indexed_cube_corners() {
        // 16 = cube (0,0,0) = black; 231 = cube (5,5,5) = full white.
        assert_eq!(color_to_rgba(Color::Indexed(16)), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(color_to_rgba(Color::Indexed(231)), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn indexed_grayscale_ramp_starts_at_eight() {
        // 232 is the first gray (8,8,8); compare through rgb_to_rgba so the
        // sRGB linearization applies to both sides.
        assert_eq!(
            color_to_rgba(Color::Indexed(232)),
            rgb_to_rgba(Rgb { r: 8, g: 8, b: 8 })
        );
    }

    #[test]
    fn the_underline_takes_its_sgr_58_color_or_the_text_color() {
        let text = color_to_rgba(Color::Named(NamedColor::Red));
        let plain = CellFlags {
            underline: Underline::Curly,
            ..CellFlags::default()
        };
        assert_eq!(underline_color(plain, text), text);
        let rose = Color::Spec(Rgb {
            r: 191,
            g: 97,
            b: 106,
        });
        let colored = CellFlags {
            underline_color: Some(rose),
            ..plain
        };
        assert_eq!(underline_color(colored, text), color_to_rgba(rose));
        // An inverse cell draws its text in the ground color, and so does
        // its line when SGR 58 is unset.
        let (inverse_text, _) = styled_colors(
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Blue),
            CellFlags {
                inverse: true,
                ..plain
            },
        );
        assert_eq!(
            underline_color(plain, inverse_text),
            color_to_rgba(Color::Named(NamedColor::Blue))
        );
    }

    #[test]
    fn a_bold_cell_brightens_a_low_palette_underline_color_as_xterm_does() {
        let text = color_to_rgba(Color::Named(NamedColor::BrightRed));
        let line = |bold: bool, color: Color| {
            underline_color(
                CellFlags {
                    bold,
                    underline: Underline::Dashed,
                    underline_color: Some(color),
                    ..CellFlags::default()
                },
                text,
            )
        };
        // xterm moves palette 0 to 7 up to 8 to 15 on a bold cell.
        for (index, bright) in [(0, 8), (1, 9), (7, 15)] {
            assert_eq!(
                line(true, Color::Indexed(index)),
                color_to_rgba(Color::Indexed(bright))
            );
            assert_eq!(
                line(false, Color::Indexed(index)),
                color_to_rgba(Color::Indexed(index))
            );
        }
        // Brighter indexes and true color keep their own color.
        for color in [
            Color::Indexed(8),
            Color::Indexed(196),
            Color::Spec(Rgb { r: 1, g: 2, b: 3 }),
        ] {
            assert_eq!(line(true, color), color_to_rgba(color));
        }
    }

    #[test]
    fn hidden_text_draws_no_glyph_and_no_lines() {
        let hidden = CellFlags {
            hidden: true,
            underline: Underline::Single,
            strikeout: true,
            ..CellFlags::default()
        };
        assert_eq!(drawn_char('H', hidden, false), ' ');
        assert!(!draws_lines(hidden, false));
        let shown = CellFlags {
            hidden: false,
            ..hidden
        };
        assert_eq!(drawn_char('H', shown, false), 'H');
        assert!(draws_lines(shown, false));
    }

    #[test]
    fn a_blinking_cell_hides_its_glyph_and_lines_in_the_off_phase() {
        let blink = CellFlags {
            blink: true,
            underline: Underline::Single,
            strikeout: true,
            ..CellFlags::default()
        };
        // The shown half draws it all.
        assert_eq!(drawn_char('B', blink, false), 'B');
        assert!(draws_lines(blink, false));
        // The hidden half draws neither glyph nor line, as xterm does.
        assert_eq!(drawn_char('B', blink, true), ' ');
        assert!(!draws_lines(blink, true));
        // A steady cell draws the same in both halves.
        let steady = CellFlags {
            blink: false,
            ..blink
        };
        assert_eq!(drawn_char('B', steady, true), 'B');
        assert!(draws_lines(steady, true));
    }

    #[test]
    fn a_blink_counts_when_its_hidden_half_takes_something_away() {
        let blink = CellFlags {
            blink: true,
            ..CellFlags::default()
        };
        assert!(blinks_visibly('x', blink));
        // A bare blank changes nothing when it flips.
        assert!(!blinks_visibly(' ', blink) && !blinks_visibly('\0', blink));
        // An underlined or struck blank loses its line.
        let underlined = CellFlags {
            underline: Underline::Curly,
            ..blink
        };
        let struck = CellFlags {
            strikeout: true,
            ..blink
        };
        assert!(blinks_visibly(' ', underlined) && blinks_visibly(' ', struck));
        // Hidden text and steady text never flip.
        let hidden = CellFlags {
            hidden: true,
            ..underlined
        };
        let steady = CellFlags {
            blink: false,
            ..underlined
        };
        assert!(!blinks_visibly('x', hidden) && !blinks_visibly('x', steady));
    }

    #[test]
    fn blink_flips_every_600_ms_on_the_wall_clock() {
        assert!(blink_shown(0) && blink_shown(599));
        assert!(!blink_shown(600) && !blink_shown(1199));
        assert!(blink_shown(1200));
        assert_eq!(until_blink_flip(0), std::time::Duration::from_millis(600));
        assert_eq!(until_blink_flip(599), std::time::Duration::from_millis(1));
        assert_eq!(
            until_blink_flip(1250),
            std::time::Duration::from_millis(550)
        );
    }

    /// Berkeley Mono at 12 CSS px and line height 1.2, the cell xterm
    /// reports: 7 by 18 at 1x with the baseline on row 13, and 14 by 34
    /// at 2x with the baseline on row 26.
    fn decor_1x() -> Decor {
        decor(7, 18, 13, 1.0)
    }

    fn decor_2x() -> Decor {
        decor(14, 34, 26, 2.0)
    }

    /// Paint a run of `n` cells underlined `kind` into a coverage grid one
    /// cell row tall, the way the renderer lays out each cell's pieces.
    fn paint_run(kind: Underline, n: u32, cell_w: u32, cell_h: u32, d: &Decor) -> Vec<Vec<u8>> {
        let mut px = vec![vec![0u8; (n * cell_w) as usize]; cell_h as usize];
        for c in 0..n {
            let x0 = c * cell_w;
            if kind == Underline::Curly {
                let cov = curl_coverage(cell_w, d.curl_h, d.t);
                for y in 0..d.curl_h {
                    for x in 0..cell_w {
                        px[(d.curl_top + y) as usize][(x0 + x) as usize] =
                            cov[(y * cell_w + x) as usize];
                    }
                }
            } else {
                for [x, y, w, h] in underline_rects(kind, x0, cell_w, d) {
                    for yy in y..y + h {
                        for xx in x..x + w {
                            px[yy as usize][(x0 + xx) as usize] = 255;
                        }
                    }
                }
            }
        }
        px
    }

    /// The rows that carry any ink.
    fn inked_rows(px: &[Vec<u8>]) -> Vec<usize> {
        (0..px.len())
            .filter(|&y| px[y].iter().any(|&c| c > 0))
            .collect()
    }

    /// The ink of one row as runs of (start, end).
    fn runs(row: &[u8]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = None;
        for (x, &c) in row.iter().chain(std::iter::once(&0)).enumerate() {
            match (start, c > 0) {
                (None, true) => start = Some(x),
                (Some(s), false) => {
                    out.push((s, x));
                    start = None;
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn every_line_is_one_css_pixel_thick_and_stays_in_its_cell() {
        assert_eq!(
            decor_1x(),
            Decor {
                t: 1,
                under: 16,
                double: [15, 17],
                curl_top: 14,
                curl_h: 4,
                strike: 9,
                dash_x: 1,
                dash_w: 5,
            }
        );
        assert_eq!(
            decor_2x(),
            Decor {
                t: 2,
                under: 32,
                double: [29, 32],
                curl_top: 28,
                curl_h: 6,
                strike: 16,
                dash_x: 2,
                dash_w: 9,
            }
        );
        // At line height 1 the cell is the glyph box, and every line
        // still fits inside it.
        for d in [decor(7, 15, 11, 1.0), decor(14, 29, 23, 2.0)] {
            let cell_h = if d.t == 1 { 15 } else { 29 };
            assert!(d.under + d.t <= cell_h);
            assert!(d.double[1] + d.t <= cell_h);
            assert!(d.double[1] > d.double[0] + d.t);
            assert!(d.curl_top + d.curl_h <= cell_h);
        }
    }

    #[test]
    fn the_curl_keeps_a_css_pixel_clear_of_the_letters() {
        // Berkeley Mono at 12 CSS px: line heights 1, 1.1, and 1.2 at 1x
        // and 2x, as (cell width, cell height, baseline, scale).
        for (cell_w, cell_h, baseline, scale) in [
            (7, 15, 11, 1.0),
            (7, 16, 12, 1.0),
            (7, 18, 13, 1.0),
            (14, 29, 23, 2.0),
            (14, 31, 24, 2.0),
            (14, 34, 26, 2.0),
        ] {
            let d = decor(cell_w, cell_h, baseline, scale);
            let at = format!("{cell_h} px cell at {scale}x");
            assert!(d.curl_top >= baseline + d.t, "{at}: {d:?}");
            assert!(d.curl_top + d.curl_h <= cell_h, "{at}: {d:?}");
            assert!(d.curl_h >= d.t + 2, "{at}: {d:?}");
        }
        // Compact at 2x: the band gives up height before it gives up room.
        let d = decor(14, 31, 24, 2.0);
        assert_eq!((d.curl_top, d.curl_h), (26, 5));
        // A tall cell keeps the board's full band, three CSS px down.
        let d = decor(14, 44, 26, 2.0);
        assert_eq!((d.curl_top, d.curl_h), (32, 7));
    }

    #[test]
    fn single_and_double_underlines_fill_whole_rows_across_a_run() {
        let d = decor_2x();
        let single = paint_run(Underline::Single, 3, 14, 34, &d);
        assert_eq!(inked_rows(&single), vec![32, 33]);
        assert!(single[32].iter().chain(&single[33]).all(|&c| c == 255));
        let double = paint_run(Underline::Double, 3, 14, 34, &d);
        assert_eq!(inked_rows(&double), vec![29, 30, 32, 33]);
        for y in [29, 30, 32, 33] {
            assert!(double[y].iter().all(|&c| c == 255), "row {y}");
        }
        let d = decor_1x();
        let double = paint_run(Underline::Double, 3, 7, 18, &d);
        assert_eq!(inked_rows(&double), vec![15, 17]);
    }

    #[test]
    fn dots_keep_one_pitch_across_cells_of_any_width() {
        for (cell_w, cell_h, d) in [
            (7, 18, decor_1x()),
            (14, 34, decor_2x()),
            (15, 34, decor(15, 34, 26, 2.0)),
        ] {
            let px = paint_run(Underline::Dotted, 4, cell_w, cell_h, &d);
            assert_eq!(
                inked_rows(&px),
                (d.under..d.under + d.t)
                    .map(|y| y as usize)
                    .collect::<Vec<_>>()
            );
            let row = &px[d.under as usize];
            for (x, &c) in row.iter().enumerate() {
                let dot = (x as u32 / d.t) % 2 == 0;
                assert_eq!(c == 255, dot, "cell {cell_w} x {x}");
            }
        }
    }

    #[test]
    fn each_cell_draws_one_dash_in_the_same_place() {
        let d = decor_2x();
        let px = paint_run(Underline::Dashed, 3, 14, 34, &d);
        assert_eq!(inked_rows(&px), vec![32, 33]);
        assert_eq!(runs(&px[32]), vec![(2, 11), (16, 25), (30, 39)]);
        assert_eq!(px[32], px[33]);
        let d = decor_1x();
        let px = paint_run(Underline::Dashed, 3, 7, 18, &d);
        assert_eq!(runs(&px[16]), vec![(1, 6), (8, 13), (15, 20)]);
    }

    #[test]
    fn the_curl_repeats_once_a_cell_and_joins_its_neighbours() {
        for (cell_w, cell_h, d) in [(7, 18, decor_1x()), (14, 34, decor_2x())] {
            let (w, h) = (cell_w as usize, d.curl_h as usize);
            let cov = curl_coverage(cell_w, d.curl_h, d.t);
            let at = |x: usize, y: usize| i32::from(cov[y * w + x]);
            // An unbroken stroke: every column carries at least a full
            // line's worth of ink, more where the wave runs steep.
            for x in 0..w {
                let ink: i32 = (0..h).map(|y| at(x, y)).sum();
                assert!(ink >= 255 * d.t as i32 * 9 / 10, "column {x} inks {ink}");
            }
            // The wave reaches both edges of its band.
            assert!((0..w).any(|x| at(x, 0) >= 128));
            assert!((0..w).any(|x| at(x, h - 1) >= 128));
            // It leaves a cell at the height it enters the next one: the
            // last column mirrors the first about the band's middle.
            for y in 0..h {
                assert!((at(0, y) - at(w - 1, h - 1 - y)).abs() <= 2, "row {y}");
            }
            // A run of cells keeps inside the cells.
            let px = paint_run(Underline::Curly, 3, cell_w, cell_h, &d);
            let rows = inked_rows(&px);
            assert!(*rows.first().unwrap_or(&0) >= d.curl_top as usize);
            assert!(*rows.last().unwrap_or(&0) < cell_h as usize);
        }
        // At 2x the crest lands on whole pixels: two full rows at the top.
        let cov = curl_coverage(14, 7, 2);
        assert!(cov[3] >= 240 && cov[14 + 3] >= 240);
    }

    #[test]
    fn slot_rect_walks_left_to_right_then_down() {
        // 16-wide grid of 10x20 slots: index 0 top-left, 16 starts row 2.
        assert_eq!(slot_rect(0, 16, 10, 20), (0, 0, 10, 20));
        assert_eq!(slot_rect(15, 16, 10, 20), (150, 0, 10, 20));
        assert_eq!(slot_rect(16, 16, 10, 20), (0, 20, 10, 20));
    }

    #[test]
    fn rect_to_uv_normalizes_to_unit_range() {
        let (min, max) = rect_to_uv(0, 0, 10, 20, 100, 200);
        assert_eq!(min, [0.0, 0.0]);
        assert_eq!(max, [0.1, 0.1]);
        let (min, _) = rect_to_uv(50, 100, 10, 20, 100, 200);
        assert_eq!(min, [0.5, 0.5]);
    }

    #[test]
    fn atlas_rasterizes_glyph_coverage() {
        // Skip gracefully if the test host has no loadable monospace font.
        let Some(fonts) = AtlasFonts::load("monospace") else {
            return;
        };
        let mut atlas = GlyphAtlas::from_fonts(fonts, 16.0);
        assert!(atlas.cell_w() > 0 && atlas.cell_h() > 0);
        let _ = atlas.glyph_uv('A', false, false);
        let _ = atlas.glyph_uv(' ', false, false);

        let coverage = |a: &GlyphAtlas, index: u32| -> u32 {
            let (sx, sy, w, h) = slot_rect(index, a.cols, a.cell_w, a.cell_h);
            let mut sum = 0u32;
            for y in sy..sy + h {
                for x in sx..sx + w {
                    sum += u32::from(a.pixels[(y * a.atlas_w + x) as usize]);
                }
            }
            sum
        };
        assert!(coverage(&atlas, 0) > 0, "A should have ink");
        assert_eq!(coverage(&atlas, 1), 0, "space should be blank");
    }

    #[test]
    fn centered_glyph_top_matches_xterm_char_top() {
        // xterm leaves no gap at line height 1, else Math.round((cell - char) / 2).
        assert_eq!(centered_glyph_top(34, 34), 0);
        assert_eq!(centered_glyph_top(37, 34), 2);
        assert_eq!(centered_glyph_top(40, 34), 3);
        assert_eq!(centered_glyph_top(45, 34), 6);
        assert_eq!(centered_glyph_top(30, 34), 0);
    }

    /// Where xterm's WebGL renderer puts the alphabetic baseline inside a
    /// cell, from the same font metrics. The glyph box is ceil(ascent +
    /// descent) device pixels, centered in the cell at Math.round of half
    /// the spare height. Text sits on the ideographic baseline at the box
    /// bottom, which `WebKit` places round(descent) below the alphabetic one.
    fn xterm_baseline(cell_h: u32, ascent: f64, descent: f64) -> f64 {
        let char_h = (ascent + descent).ceil();
        let top = ((f64::from(cell_h) - char_h) / 2.0 + 0.5).floor();
        top + char_h - descent.round()
    }

    #[test]
    fn native_baseline_matches_xterm_at_every_line_height() {
        for bytes in [JETBRAINS_REGULAR, JETBRAINS_BOLD] {
            let font = Font::from_bytes(Arc::new(bytes.to_vec()), 0).unwrap();
            let m = font.metrics();
            for css_px in 11..=18u32 {
                for dpr in [1u32, 2] {
                    let scale = f64::from(css_px * dpr) / f64::from(m.units_per_em);
                    let ascent = f64::from(m.ascent) * scale;
                    let descent = -f64::from(m.descent) * scale;
                    let char_h = (ascent + descent).ceil() as u32;
                    // Compact, default, and loose.
                    for line_height in [1.1, 1.2, 1.35] {
                        let cell_h = (f64::from(char_h) * line_height).floor() as u32;
                        let native = native_baseline(
                            centered_glyph_top(cell_h, char_h),
                            (f64::from(m.ascent) * scale) as f32,
                        );
                        let xterm = xterm_baseline(cell_h, ascent, descent);
                        assert!(
                            (f64::from(native) - xterm).abs() <= 1.0,
                            "{css_px}px at {dpr}x, line height {line_height}: \
                             native {native}, xterm {xterm}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_taller_cell_drops_each_glyph_to_the_centered_baseline() {
        // The lowest inked row of 'H' in its slot, which sits on the baseline.
        let lowest_ink = |atlas: &mut GlyphAtlas| -> u32 {
            let _ = atlas.glyph_uv('H', false, false);
            let (sx, sy, w, h) = slot_rect(0, atlas.cols, atlas.slot_w, atlas.cell_h);
            (sy..sy + h)
                .rev()
                .find(|&y| (sx..sx + w).any(|x| atlas.pixels[(y * atlas.atlas_w + x) as usize] > 0))
                .map(|y| y - sy)
                .expect("H has ink")
        };
        // JetBrains Mono at 14 px on a 2x screen: a 37 px glyph box in the
        // 44 px cell xterm reports at the default line height.
        let jetbrains =
            || AtlasFonts::load("JetBrainsMono Bundled").expect("Vosh bundles JetBrains Mono");
        let mut flat = GlyphAtlas::with_reported(jetbrains(), 28.0, Some((17, 44)), None);
        let mut centered = GlyphAtlas::with_reported(jetbrains(), 28.0, Some((17, 44)), Some(37));
        assert_eq!(centered.glyph_top, 4);
        assert_eq!(centered.baseline(), flat.baseline() + 4);
        assert_eq!(lowest_ink(&mut centered), lowest_ink(&mut flat) + 4);
        // No report yet means the font's own cell and no drop.
        let unreported = GlyphAtlas::with_reported(jetbrains(), 28.0, None, Some(37));
        assert_eq!(unreported.glyph_top, 0);
    }

    #[test]
    fn font_lists_match_the_shared_fixtures() {
        // The same cases run against renderFontStack in
        // src/lib/fontLoader.ts, so xterm and the atlas try the same
        // families in the same order.
        let text = include_str!("../../fixtures/font-stacks/cases.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty(), "expected entries");
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let want: Vec<String> = serde_json::from_value(case["families"].clone()).unwrap();
            let got = rendered_families(case["stack"].as_str().unwrap());
            assert_eq!(got, want, "case `{name}`");
            assert_eq!(
                rendered_families(&got.join(", ")),
                got,
                "case `{name}` is not stable under a second pass"
            );
        }
    }

    #[test]
    fn a_berkeley_name_without_the_font_lands_on_the_bundled_jetbrains_mono() {
        let fonts = AtlasFonts::load("\"Berkeley Mono Vosh Test\", Menlo, monospace")
            .expect("Vosh bundles JetBrains Mono");
        assert_eq!(
            fonts.regular.postscript_name().as_deref(),
            Some("JetBrainsMonoNF-Regular")
        );
        assert_eq!(
            fonts.bold.postscript_name().as_deref(),
            Some("JetBrainsMonoNF-Bold")
        );
        // The retired bundled name takes an installed Berkeley Mono, and
        // JetBrains Mono on a machine without one.
        let retired = AtlasFonts::load("\"BerkeleyMono Bundled\", Menlo, monospace")
            .expect("Vosh bundles JetBrains Mono");
        let face = retired.regular.postscript_name().unwrap_or_default();
        assert!(
            face.starts_with("BerkeleyMono") || face == "JetBrainsMonoNF-Regular",
            "{face}"
        );
    }

    fn paint(r: u8, g: u8, b: u8, a: f32) -> Paint {
        Paint { r, g, b, a }
    }

    #[test]
    fn parse_css_color_accepts_hex_and_rgb_forms() {
        assert_eq!(
            parse_css_color("#3a404c"),
            Some(paint(0x3a, 0x40, 0x4c, 1.0))
        );
        assert_eq!(
            parse_css_color("3a404c"),
            Some(paint(0x3a, 0x40, 0x4c, 1.0))
        );
        assert_eq!(parse_css_color("#fff"), Some(paint(255, 255, 255, 1.0)));
        assert_eq!(parse_css_color("rgb(1, 2, 3)"), Some(paint(1, 2, 3, 1.0)));
        assert_eq!(
            parse_css_color("rgba(10,20,30,0.5)"),
            Some(paint(10, 20, 30, 0.5))
        );
        assert_eq!(parse_css_color("bright-red"), None);
        assert_eq!(parse_css_color(""), None);
    }

    #[test]
    fn parse_css_color_reads_alpha_in_every_form() {
        assert_eq!(
            parse_css_color("#ffffff80"),
            Some(paint(255, 255, 255, 128.0 / 255.0))
        );
        assert_eq!(
            parse_css_color("#0008"),
            Some(paint(0, 0, 0, 136.0 / 255.0))
        );
        assert_eq!(
            parse_css_color("rgba(136, 192, 208, 0.22)"),
            Some(paint(136, 192, 208, 0.22))
        );
        assert_eq!(
            parse_css_color("rgb(136 192 208 / 22%)"),
            Some(paint(136, 192, 208, 0.22))
        );
        // Alpha clamps into 0..1, and a bad channel or alpha rejects.
        assert_eq!(parse_css_color("rgba(1,2,3,4)"), Some(paint(1, 2, 3, 1.0)));
        assert_eq!(parse_css_color("rgba(1,2,3,x)"), None);
        assert_eq!(parse_css_color("rgb(1,2)"), None);
        assert_eq!(parse_css_color("rgb(1,2,3,4,5)"), None);
    }

    #[test]
    fn blend_over_composites_in_srgb_space() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let red = rgb_to_rgba(Rgb { r: 200, g: 0, b: 0 });
        // Opaque replaces, clear keeps the cell (up to the round trip
        // through sRGB).
        assert_eq!(blend_over(paint(200, 0, 0, 1.0), black), red);
        let kept = blend_over(paint(0, 0, 255, 0.0), red);
        for (got, want) in kept.iter().zip(red.iter()) {
            assert!((got - want).abs() < 1e-5);
        }
        // Half white over black is sRGB mid gray, as CSS rgba() draws it.
        let mid = blend_over(paint(255, 255, 255, 0.5), black);
        assert!((linear_to_srgb(mid[0]) - 0.5).abs() < 1e-4);
        assert_eq!(mid[3], 1.0);
    }

    #[test]
    fn paint_to_rgba_carries_alpha() {
        let c = paint_to_rgba(paint(255, 0, 0, 0.25));
        assert_eq!(c, [1.0, 0.0, 0.0, 0.25]);
    }

    #[test]
    fn chrome_falls_back_to_the_palette_without_tokens() {
        let fg = Rgb {
            r: 0xe5,
            g: 0xe9,
            b: 0xf0,
        };
        let yellow = Rgb {
            r: 0xeb,
            g: 0xcb,
            b: 0x8b,
        };
        let blue = Rgb {
            r: 0x81,
            g: 0xa1,
            b: 0xc1,
        };
        let chrome = resolve_chrome(ChromeTokens::UNSET, None, None, fg, yellow, blue);
        assert_eq!(chrome.divider, Paint::tint(fg, DIVIDER_FALLBACK_ALPHA));
        assert_eq!(chrome.selection, Paint::tint(fg, SELECTION_FALLBACK_ALPHA));
        assert_eq!(
            chrome.find_match,
            Paint::tint(yellow, FIND_MATCH_FALLBACK_ALPHA)
        );
        assert_eq!(
            chrome.current_match,
            Paint::tint(yellow, CURRENT_MATCH_FALLBACK_ALPHA)
        );
        assert_eq!(chrome.link, Paint::opaque(blue));
        assert_eq!(chrome.scrollbar, Paint::tint(fg, SCROLLBAR_FALLBACK_ALPHA));
        assert_eq!(chrome.selrow, Paint::tint(fg, SELROW_FALLBACK_ALPHA));
        assert_eq!(chrome.ring, None);
    }

    #[test]
    fn chrome_prefers_tokens_and_the_divider_setting() {
        let grey = Rgb {
            r: 0x80,
            g: 0x80,
            b: 0x80,
        };
        let theme_sel = Rgb {
            r: 0x2a,
            g: 0x3b,
            b: 0x5e,
        };
        let token = paint(0x88, 0xc0, 0xd0, 0.22);
        let setting = paint(0xff, 0, 0, 1.0);
        let tokens = ChromeTokens {
            divider: Some(token),
            selection: Some(token),
            find_match: Some(token),
            current_match: Some(token),
            link: Some(token),
            scrollbar: Some(token),
            selrow: Some(token),
            light: true,
        };
        let chrome = resolve_chrome(tokens, Some(setting), Some(theme_sel), grey, grey, grey);
        assert_eq!(chrome.divider, setting);
        assert_eq!(chrome.selection, token);
        assert_eq!(chrome.find_match, token);
        assert_eq!(chrome.current_match, token);
        assert_eq!(chrome.link, token);
        assert_eq!(chrome.scrollbar, token);
        assert_eq!(chrome.selrow, token);
        assert_eq!(chrome.ring, Some(LIGHT_RING));
        // Without the setting the divider takes its token. Without a token
        // the selection takes the theme's opaque one before the fallback.
        let chrome = resolve_chrome(
            ChromeTokens {
                divider: Some(token),
                ..ChromeTokens::UNSET
            },
            None,
            Some(theme_sel),
            grey,
            grey,
            grey,
        );
        assert_eq!(chrome.divider, token);
        assert_eq!(chrome.selection, Paint::opaque(theme_sel));
    }

    fn span(id: u64, line: i32, first: usize, end: usize) -> LiftSpan {
        LiftSpan {
            id,
            line,
            first,
            end,
            after: false,
        }
    }

    /// A lift row with your echo after it.
    fn span_before_echo(id: u64, line: i32, first: usize, end: usize) -> LiftSpan {
        LiftSpan {
            after: true,
            ..span(id, line, first, end)
        }
    }

    #[test]
    fn a_band_steps_in_around_your_echo_on_its_last_row() {
        // The fight prompt: a tank line, the gauge row, and the vitals
        // row your echo follows.
        let spans = [
            span(1, 3, 0, 25),
            span(1, 4, 0, 52),
            span_before_echo(1, 5, 0, 42),
        ];
        let boxes = lift_boxes(&spans, 0, 10);
        assert_eq!(boxes[0].notch, Some(42));
        let [band] = band_rects(&boxes, 0.0, 7.8, 17.5, 1.0)[..] else {
            panic!("one band");
        };
        assert!((band.w - (52.0 * 7.8 + 8.0)).abs() < 1e-3);
        let notch = band.notch.expect("the notch");
        // 4 px past the last row's last glyph, down from that row's top.
        assert!((notch[0] - (42.0 * 7.8 + 8.0)).abs() < 1e-3);
        assert_eq!(notch[1], 2.0 * 17.5 + 2.0);
        let quad = band_instances(&[band], [0.0, 0.0], paint(1, 2, 3, 1.0), None, 1.0);
        assert_eq!(quad[0].uv_max, notch);
        // Nothing after it, the last row widest, or one row: one box.
        for spans in [
            vec![span(1, 3, 0, 25), span(1, 4, 0, 52), span(1, 5, 0, 42)],
            vec![span(1, 4, 0, 30), span_before_echo(1, 5, 0, 42)],
            vec![span_before_echo(1, 5, 0, 42)],
        ] {
            let boxes = lift_boxes(&spans, 0, 10);
            assert_eq!(boxes[0].notch, None, "{spans:?}");
            let rects = band_rects(&boxes, 0.0, 7.8, 17.5, 1.0);
            assert_eq!(rects[0].notch, None);
            let quad = band_instances(&rects, [0.0, 0.0], paint(1, 2, 3, 1.0), None, 1.0);
            assert_eq!(quad[0].uv_max, [0.0, 0.0]);
        }
    }

    #[test]
    fn lift_boxes_gather_each_lift_across_its_rows() {
        let spans = [
            span(1, 3, 0, 13),
            span(1, 4, 0, 35),
            span(2, 6, 4, 9),
            span(3, 40, 0, 8),
        ];
        let boxes = lift_boxes(&spans, 2, 10);
        assert_eq!(
            boxes,
            [
                LiftBox {
                    id: 1,
                    top: 1,
                    bottom: 2,
                    left: 0,
                    right: 35,
                    notch: None,
                },
                LiftBox {
                    id: 2,
                    top: 4,
                    bottom: 4,
                    left: 4,
                    right: 9,
                    notch: None,
                },
            ]
        );
        // A lift the region cuts keeps its rows past the region's edge.
        let cut = lift_boxes(&[span(1, -1, 0, 5), span(1, 0, 0, 7)], 0, 3);
        assert_eq!((cut[0].top, cut[0].bottom, cut[0].right), (-1, 0, 7));
    }

    #[test]
    fn a_band_reaches_as_far_as_the_boards_measure() {
        // P4 at 1x: 35 cells of 7.8 by 17.5 draw 281 by 21.5.
        let boxes = [LiftBox {
            id: 1,
            top: 3,
            bottom: 3,
            left: 0,
            right: 35,
            notch: None,
        }];
        let [band] = band_rects(&boxes, 0.0, 7.8, 17.5, 1.0)[..] else {
            panic!("one band");
        };
        assert_eq!(band.x, -4.0);
        assert_eq!(band.y, 3.0 * 17.5 - 2.0);
        assert!((band.w - 281.0).abs() < 1e-3);
        assert_eq!(band.h, 21.5);
        // At 2x every reach doubles, cells included.
        let [band] = band_rects(&boxes, 10.0, 15.6, 35.0, 2.0)[..] else {
            panic!("one band");
        };
        assert_eq!(band.x, -8.0);
        assert_eq!(band.y, 10.0 + 3.0 * 35.0 - 4.0);
        assert_eq!(band.h, 43.0);
    }

    #[test]
    fn lifts_on_adjacent_rows_keep_two_pixels_of_ground_between_them() {
        let lift = |id, row| LiftBox {
            id,
            top: row,
            bottom: row,
            left: 0,
            right: 10,
            notch: None,
        };
        let rects = band_rects(&[lift(1, 4), lift(2, 5)], 0.0, 10.0, 20.0, 1.0);
        assert_eq!(rects[0].y + rects[0].h, 5.0 * 20.0 - 1.0);
        assert_eq!(rects[1].y, 5.0 * 20.0 + 1.0);
        assert_eq!(rects[1].y - (rects[0].y + rects[0].h), 2.0);
        // Outer edges keep the full reach.
        assert_eq!(rects[0].y, 4.0 * 20.0 - 2.0);
        assert_eq!(rects[1].y + rects[1].h, 6.0 * 20.0 + 2.0);
        // A row of ground between them keeps the full reach too.
        let rects = band_rects(&[lift(1, 4), lift(2, 6)], 0.0, 10.0, 20.0, 1.0);
        assert_eq!(rects[0].y + rects[0].h, 5.0 * 20.0 + 2.0);
    }

    #[test]
    fn the_newest_band_reaches_past_its_glyphs_for_the_card() {
        let lift = |id, row| LiftBox {
            id,
            top: row,
            bottom: row,
            left: 0,
            right: 10,
            notch: None,
        };
        let boxes = [lift(1, 4), lift(2, 6)];
        let mut rects = band_rects(&boxes, 0.0, 10.0, 20.0, 1.0);
        let widths: Vec<f32> = rects.iter().map(|r| r.w).collect();
        widen_newest(&mut rects, &boxes, Some(2), 12.0);
        assert_eq!(rects[0].w, widths[0]);
        assert_eq!(rects[1].w, widths[1] + 12.0);
        // Nothing to widen with no reach, or a newest lift out of view.
        let mut same = band_rects(&boxes, 0.0, 10.0, 20.0, 1.0);
        widen_newest(&mut same, &boxes, Some(9), 12.0);
        widen_newest(&mut same, &boxes, Some(2), 0.0);
        assert_eq!(same.iter().map(|r| r.w).collect::<Vec<_>>(), widths);
    }

    /// fixtures/prompt-bands/cases.json, which layoutBands and widenNewest
    /// in src/lib/promptBands.ts run too.
    #[derive(serde::Deserialize)]
    struct BandCases {
        constants: BandConstants,
        cases: Vec<BandCase>,
    }

    #[derive(serde::Deserialize)]
    struct BandConstants {
        band_x: f32,
        band_y: f32,
        band_y_adjacent: f32,
        band_radius: f32,
        max_lift_rows: i32,
    }

    #[derive(serde::Deserialize)]
    struct BandCase {
        name: String,
        cell: CaseCell,
        viewport_y: i32,
        #[serde(default)]
        reach: f32,
        lifts: Vec<CaseLift>,
        bands: Vec<CaseBand>,
    }

    #[derive(serde::Deserialize)]
    struct CaseCell {
        w: f32,
        h: f32,
    }

    #[derive(serde::Deserialize)]
    struct CaseLift {
        id: u64,
        top: i32,
        bottom: i32,
        left: usize,
        right: usize,
        notch: Option<usize>,
    }

    #[derive(serde::Deserialize)]
    struct CaseBand {
        id: u64,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
        notch: Option<CaseNotch>,
    }

    #[derive(serde::Deserialize)]
    struct CaseNotch {
        x: f32,
        y: f32,
    }

    /// The spans the grid reports for `lift`: each row from its first
    /// glyph to its widest, and its last row up to the notch with your
    /// echo after it when it has one.
    fn spans_of(lift: &CaseLift) -> Vec<LiftSpan> {
        (lift.top..=lift.bottom)
            .map(|line| {
                let last = line == lift.bottom;
                LiftSpan {
                    id: lift.id,
                    line,
                    first: lift.left,
                    end: if last {
                        lift.notch.unwrap_or(lift.right)
                    } else {
                        lift.right
                    },
                    after: last && lift.notch.is_some(),
                }
            })
            .collect()
    }

    #[test]
    fn bands_match_the_cases_xterm_draws() {
        let text = include_str!("../../fixtures/prompt-bands/cases.json");
        let fixture: BandCases = serde_json::from_str(text).expect("the band cases parse");
        let c = &fixture.constants;
        assert_eq!(
            (BAND_X, BAND_Y, BAND_Y_ADJACENT, BAND_RADIUS, MAX_LIFT_ROWS),
            (
                c.band_x,
                c.band_y,
                c.band_y_adjacent,
                c.band_radius,
                c.max_lift_rows
            )
        );
        assert!(!fixture.cases.is_empty());
        let close = |got: f32, want: f32| (got - want).abs() < 1e-3;
        for case in &fixture.cases {
            let name = &case.name;
            // The region starts at the viewport's first row, as the page's
            // viewport does, and shows every row a case uses.
            let spans: Vec<LiftSpan> = case.lifts.iter().flat_map(spans_of).collect();
            let boxes = lift_boxes(&spans, case.viewport_y, 1000);
            let newest = boxes.iter().map(|b| b.id).max();
            // The cases are CSS px. At 2x every length doubles, and a
            // region lower on the surface moves every band down with it.
            for (scale, y0) in [(1.0_f32, 0.0_f32), (2.0, 10.0)] {
                let (cell_w, cell_h) = (case.cell.w * scale, case.cell.h * scale);
                let mut rects = band_rects(&boxes, y0, cell_w, cell_h, scale);
                widen_newest(&mut rects, &boxes, newest, case.reach * scale);
                let ids: Vec<u64> = boxes.iter().map(|b| b.id).collect();
                let want_ids: Vec<u64> = case.bands.iter().map(|b| b.id).collect();
                assert_eq!(ids, want_ids, "{name}");
                for (rect, want) in rects.iter().zip(&case.bands) {
                    assert!(
                        close(rect.x, want.left * scale)
                            && close(rect.y, y0 + want.top * scale)
                            && close(rect.w, want.width * scale)
                            && close(rect.h, want.height * scale),
                        "{name} at {scale}x: {rect:?}"
                    );
                    match (rect.notch, &want.notch) {
                        (None, None) => {}
                        (Some([x, y]), Some(n)) => assert!(
                            close(x, n.x * scale) && close(y, n.y * scale),
                            "{name} at {scale}x: {rect:?}"
                        ),
                        _ => panic!("{name} at {scale}x: notch {:?}", rect.notch),
                    }
                }
            }
        }
    }

    #[test]
    fn band_quads_carry_the_radius_and_a_ring_only_when_light() {
        let rect = BandRect {
            x: -8.0,
            y: 4.0,
            w: 100.0,
            h: 43.0,
            notch: None,
        };
        let fill = paint(0x3b, 0x42, 0x52, 1.0);
        let dark = band_instances(&[rect], [8.0, 4.0], fill, None, 2.0);
        assert_eq!(dark.len(), 1);
        assert_eq!(dark[0].offset, [0.0, 8.0]);
        assert_eq!(dark[0].size, [100.0, 43.0]);
        assert_eq!(dark[0].uv_min, [8.0, 0.0]);
        assert_eq!(dark[0].color, paint_to_rgba(fill));
        let light = band_instances(&[rect], [0.0, 0.0], fill, Some(LIGHT_RING), 2.0);
        assert_eq!(light.len(), 2);
        assert_eq!(light[1].uv_min, [8.0, 2.0]);
        assert_eq!(light[1].color, paint_to_rgba(LIGHT_RING));
    }

    #[test]
    fn the_band_viewport_reaches_past_the_pane_inside_the_target() {
        // Under the underlay the pane sits inside the window.
        assert_eq!(
            band_viewport([32, 12, 800, 600], [1000, 800], 2.0),
            [24, 8, 816, 608]
        );
        // A pane that fills its target stays the target.
        assert_eq!(
            band_viewport([0, 0, 800, 600], [800, 600], 2.0),
            [0, 0, 800, 600]
        );
    }

    /// What the surface shows for `cell`, a quad color, drawn over an
    /// opaque `under`, as the premultiplied blend composites it: in sRGB.
    fn composite(cell: Rgba, under: Rgba) -> [u8; 3] {
        let a = cell[3];
        let mut out = [0u8; 3];
        for i in 0..3 {
            let top = linear_to_srgb(cell[i]) * a;
            let v = top + linear_to_srgb(under[i]) * (1.0 - a);
            out[i] = (v * 255.0).round() as u8;
        }
        out
    }

    fn opaque_srgb(c: Rgba) -> [u8; 3] {
        composite(c, c)
    }

    #[test]
    fn a_tint_over_a_band_blends_over_whatever_lies_under_it() {
        // Nord: the selection and a find match over a lifted prompt's
        // band, and over the plain ground beside it.
        let selection = paint(0x88, 0xc0, 0xd0, 0.4);
        let find = paint(0xeb, 0xcb, 0x8b, 0.28);
        let band = rgb_to_rgba(Rgb {
            r: 0x3b,
            g: 0x42,
            b: 0x52,
        });
        let ground = rgb_to_rgba(Rgb {
            r: 0x2e,
            g: 0x34,
            b: 0x40,
        });
        for tints in [vec![selection], vec![selection, find], vec![find]] {
            let cell = ground_tint(&tints);
            assert!(cell[3] < 1.0, "the band still shows through");
            for under in [band, ground] {
                let want = tints.iter().fold(under, |c, &t| blend_over(t, c));
                let got = composite(cell, under);
                let want = opaque_srgb(want);
                for i in 0..3 {
                    assert!(got[i].abs_diff(want[i]) <= 1, "{tints:?} {got:?} {want:?}");
                }
            }
        }
        // No tint leaves the ground clear.
        assert_eq!(ground_tint(&[])[3], 0.0);
    }

    #[test]
    fn build_instances_lays_out_row_major_with_colors_and_uv() {
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let solid = ([0.99, 0.99], [0.99, 0.99]);
        let (backgrounds, glyphs) = build_instances(
            2,
            1,
            10.0,
            20.0,
            0.0,
            20.0,
            solid,
            |col, _row| (if col == 0 { 'a' } else { 'b' }, white, black, false, false),
            |ch, _bold, _italic| {
                if ch == 'a' {
                    ([0.0, 0.0], [0.1, 0.1])
                } else {
                    ([0.1, 0.0], [0.2, 0.1])
                }
            },
        );
        assert_eq!(backgrounds.len(), 2);
        assert_eq!(glyphs.len(), 2);
        // Cell (0,0) at origin; cell (1,0) one cell to the right.
        assert_eq!(backgrounds[0].offset, [0.0, 0.0]);
        assert_eq!(backgrounds[1].offset, [10.0, 0.0]);
        // Background fills carry the bg color and the solid texel UV.
        assert_eq!(backgrounds[0].color, black);
        assert_eq!(backgrounds[0].uv_min, solid.0);
        // Glyph quads carry the fg color, slot width, and per-char UVs.
        assert_eq!(glyphs[0].color, white);
        assert_eq!(glyphs[0].size, [20.0, 20.0]);
        assert_eq!(glyphs[0].uv_min, [0.0, 0.0]);
        assert_eq!(glyphs[1].uv_min, [0.1, 0.0]);
    }

    #[test]
    fn build_instances_second_row_offsets_down() {
        let c = [0.5, 0.5, 0.5, 1.0];
        let (backgrounds, _glyphs) = build_instances(
            1,
            2,
            8.0,
            16.0,
            0.0,
            16.0,
            ([0.99, 0.99], [0.99, 0.99]),
            |_, _| ('x', c, c, false, false),
            |_, _, _| ([0.0, 0.0], [0.0, 0.0]),
        );
        assert_eq!(backgrounds[0].offset, [0.0, 0.0]);
        assert_eq!(backgrounds[1].offset, [0.0, 16.0]);
    }

    #[test]
    fn marks_become_whole_pixel_quads_and_the_curl_samples_its_sprite() {
        let d = decor_2x();
        let solid = ([0.5, 0.5], [0.5, 0.5]);
        let curl = ([0.25, 0.75], [0.3, 0.8]);
        let red = [1.0, 0.0, 0.0, 1.0];
        let under = [
            (2, 34.0, red, Underline::Curly),
            (3, 34.0, red, Underline::Double),
        ];
        let strike = [(4, 68.0, red, Underline::None)];
        let quads = line_instances(&under, &strike, &d, 14, solid, curl);
        assert_eq!(quads.len(), 4);
        assert_eq!(quads[0].offset, [28.0, 34.0 + 28.0]);
        assert_eq!(quads[0].size, [14.0, 6.0]);
        assert_eq!((quads[0].uv_min, quads[0].uv_max), curl);
        assert_eq!(quads[1].offset, [42.0, 34.0 + 29.0]);
        assert_eq!(quads[2].offset, [42.0, 34.0 + 32.0]);
        assert_eq!(quads[2].size, [14.0, 2.0]);
        assert_eq!(quads[3].offset, [56.0, 68.0 + 16.0]);
        assert_eq!(quads[3].size, [14.0, 2.0]);
        assert_eq!((quads[3].uv_min, quads[3].uv_max), solid);
    }

    #[test]
    fn the_curl_sprite_redraws_only_when_its_size_changes() {
        let Some(fonts) = AtlasFonts::load("JetBrainsMono Bundled") else {
            return;
        };
        let mut atlas = GlyphAtlas::with_reported(fonts, 24.0, Some((14, 34)), Some(29));
        let (uv, drawn) = atlas.curl_uv(2, 7);
        assert!(drawn);
        assert_eq!(atlas.curl_uv(2, 7), (uv, false));
        assert!(atlas.curl_uv(1, 4).1);
        // The sprite sits in the slot before the solid block and holds
        // exactly the curl's coverage.
        let (aw, _) = atlas.atlas_size();
        let (sx, sy, _, _) = slot_rect(32 * 32 - 2, 32, atlas.slot_w(), 34);
        let cov = curl_coverage(14, 4, 1);
        for y in 0..4 {
            let at = ((sy + y) * aw + sx) as usize;
            assert_eq!(
                &atlas.pixels()[at..at + 14],
                &cov[(y * 14) as usize..(y * 14 + 14) as usize]
            );
        }
        // Glyphs never take the curl's slot.
        for i in 0..1100u32 {
            let c = char::from_u32(0x4e00 + i).unwrap_or('x');
            let _ = atlas.glyph_uv(c, false, false);
        }
        let (sprite, drawn) = atlas.curl_uv(1, 4);
        assert!(!drawn);
        let at = (sy * aw + sx) as usize;
        assert_eq!(&atlas.pixels()[at..at + 14], &cov[..14]);
        assert_eq!(
            sprite.0,
            [
                sx as f32 / aw as f32,
                sy as f32 / atlas.atlas_size().1 as f32
            ]
        );
    }

    // Offscreen renders through the real pipeline. They need a GPU, so
    // each one returns early when no adapter is around.

    /// Nord, the Styles board's theme, as explicit true color so the
    /// renders do not lean on the theme other tests set.
    const NORD: &[u8] = b"\x1b[0;38;2;229;233;240;48;2;46;52;64m";

    /// The Styles board's text style rows, then a probe row of blank
    /// cells for each line in magenta: single, double, curly, dotted,
    /// dashed, and strike, eight cells each from column 0, 10, 20, 30,
    /// 40, and 50.
    fn styles_specimen() -> Vec<u8> {
        let mut out = Vec::new();
        let row = |out: &mut Vec<u8>, parts: &[&[u8]]| {
            out.extend_from_slice(NORD);
            out.extend_from_slice(b"\x1b[K");
            for part in parts {
                out.extend_from_slice(part);
                out.extend_from_slice(NORD);
            }
            out.extend_from_slice(b"\r\n");
        };
        row(
            &mut out,
            &[
                b"\x1b[1mBold",
                b"   \x1b[2mDim",
                b"   \x1b[3mItalic",
                b"   \x1b[1;3mBold italic",
                b"   \x1b[8mHidden",
                b"   ",
            ],
        );
        row(
            &mut out,
            &[
                b"\x1b[4mUnderline",
                b"   \x1b[4:2mDouble",
                b"   \x1b[4:3mCurly",
                b"   \x1b[4:4mDotted",
                b"   \x1b[4:5mDashed",
            ],
        );
        row(
            &mut out,
            &[
                b"\x1b[4:3;58:2::191:97:106mCurly in its own color",
                b"   \x1b[9mStrikethrough",
                b"   \x1b[7mReverse",
                b"   \x1b[5mBlink",
            ],
        );
        let mut probe: Vec<Vec<u8>> = Vec::new();
        for (i, sgr) in ["4:1", "4:2", "4:3", "4:4", "4:5"].iter().enumerate() {
            let gap = if i == 0 { "" } else { "  " };
            probe.push(format!("{gap}\x1b[{sgr};58:2::255:0:255m        ").into_bytes());
        }
        probe.push(b"  \x1b[9;38;2;255;0;255m        ".to_vec());
        let parts: Vec<&[u8]> = probe.iter().map(Vec::as_slice).collect();
        row(&mut out, &parts);
        // The last line end would scroll the top row away.
        out.truncate(out.len() - 2);
        out
    }

    /// One offscreen frame: its RGBA pixels, size, cell, and lines.
    struct Frame {
        rgba: Vec<u8>,
        w: u32,
        h: u32,
        cell: (u32, u32),
        decor: Decor,
        /// The frame drew a cell that blinks.
        blinks: bool,
    }

    /// Render `bytes` on a `cols` by `rows` grid at `scale`, in the font
    /// Vosh bundles at 12 CSS px and `line_height` with the cell xterm
    /// reports.
    fn render_offscreen(
        bytes: &[u8],
        cols: usize,
        rows: usize,
        scale: f32,
        line_height: f32,
    ) -> Option<Frame> {
        render_frame(bytes, cols, rows, scale, line_height, false, false)
    }

    /// [`render_offscreen`] in the hidden half of a blink when
    /// `blink_hidden`, with every cell selected when `select_all`.
    fn render_frame(
        bytes: &[u8],
        cols: usize,
        rows: usize,
        scale: f32,
        line_height: f32,
        blink_hidden: bool,
        select_all: bool,
    ) -> Option<Frame> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .ok()?;
        let px = 12.0 * scale;
        // xterm's device cell: the font's advance and glyph box, and the
        // box times the line height.
        let probe =
            GlyphAtlas::with_reported(AtlasFonts::load("JetBrainsMono Bundled")?, px, None, None);
        let (cell_w, char_h) = (probe.cell_w(), probe.cell_h());
        let cell_h = (char_h as f32 * line_height).floor() as u32;
        let atlas = GlyphAtlas::with_reported(
            AtlasFonts::load("JetBrainsMono Bundled")?,
            px,
            Some((cell_w, cell_h)),
            Some(char_h),
        );
        let decor = decor(cell_w, cell_h, atlas.baseline(), scale);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut renderer = CellRenderer::with_atlas(&device, &queue, format, atlas);
        let mut grid = crate::term_grid::TermGrid::new(cols, rows);
        grid.feed(bytes);
        if select_all {
            grid.select_all();
        }
        let (w, h) = (cols as u32 * cell_w, rows as u32 * cell_h);
        let extent = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("styles-target"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let placement = Placement {
            x: 0,
            y: 0,
            scale,
            target: [w, h],
            blink_hidden,
        };
        let drawn = renderer.draw(
            &device,
            &queue,
            &mut encoder,
            &view,
            &grid,
            w,
            h,
            0.5,
            placement,
        );
        let row_bytes = (w * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("styles-readback"),
            size: u64::from(row_bytes * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes),
                    rows_per_image: Some(h),
                },
            },
            extent,
        );
        queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in mapped.chunks_exact(row_bytes as usize) {
            rgba.extend_from_slice(&row[..(w * 4) as usize]);
        }
        Some(Frame {
            rgba,
            w,
            h,
            cell: (cell_w, cell_h),
            decor,
            blinks: drawn.blinks,
        })
    }

    /// Write `frame` as a PNG, opaque, through flate2's zlib and CRC.
    fn write_png(path: &std::path::Path, frame: &Frame) {
        use std::io::Write;
        let mut raw = Vec::with_capacity(((frame.w * 4 + 1) * frame.h) as usize);
        for row in frame.rgba.chunks_exact((frame.w * 4) as usize) {
            raw.push(0);
            raw.extend(row.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]));
        }
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&raw).expect("zlib");
        let idat = z.finish().expect("zlib");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8], data: &[u8]| {
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            png.extend_from_slice(kind);
            png.extend_from_slice(data);
            png.extend_from_slice(&crc.sum().to_be_bytes());
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&frame.w.to_be_bytes());
        ihdr.extend_from_slice(&frame.h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(b"IHDR", &ihdr);
        chunk(b"IDAT", &idat);
        chunk(b"IEND", &[]);
        std::fs::write(path, png).expect("write png");
    }

    #[test]
    fn the_grid_draws_every_line_where_its_geometry_says() {
        const NORD_BG: [i32; 3] = [46, 52, 64];
        const MAGENTA: [i32; 3] = [255, 0, 255];
        for scale in [1.0_f32, 2.0] {
            let Some(frame) = render_offscreen(&styles_specimen(), 64, 4, scale, 1.2) else {
                return;
            };
            if let Some(dir) = std::env::var_os("VOSH_TEXT_STYLE_RENDERS") {
                let name = format!("grid_styles_{}x.png", scale as u32);
                write_png(&std::path::Path::new(&dir).join(name), &frame);
            }
            let (cw, ch) = frame.cell;
            let d = frame.decor;
            let pixel = |x: u32, y: u32| {
                let at = ((y * frame.w + x) * 4) as usize;
                [0, 1, 2].map(|i| i32::from(frame.rgba[at + i]))
            };
            // The probe row: blank cells, so only the lines carry ink.
            let top = 3 * ch;
            let kinds = [
                Underline::Single,
                Underline::Double,
                Underline::Curly,
                Underline::Dotted,
                Underline::Dashed,
                Underline::None,
            ];
            for (k, kind) in kinds.into_iter().enumerate() {
                let first = k as u32 * 10;
                let mut want = vec![0u8; (8 * cw * ch) as usize];
                for c in 0..8 {
                    let x0 = (first + c) * cw;
                    let mut ink = |x: u32, y: u32, cov: u8| {
                        want[(y * 8 * cw + x - first * cw) as usize] = cov;
                    };
                    match kind {
                        Underline::Curly => {
                            let cov = curl_coverage(cw, d.curl_h, d.t);
                            for y in 0..d.curl_h {
                                for x in 0..cw {
                                    ink(x0 + x, d.curl_top + y, cov[(y * cw + x) as usize]);
                                }
                            }
                        }
                        Underline::None => {
                            for y in d.strike..d.strike + d.t {
                                for x in 0..cw {
                                    ink(x0 + x, y, 255);
                                }
                            }
                        }
                        _ => {
                            for [rx, ry, rw, rh] in underline_rects(kind, x0, cw, &d) {
                                for y in ry..ry + rh {
                                    for x in rx..rx + rw {
                                        ink(x0 + x, y, 255);
                                    }
                                }
                            }
                        }
                    }
                }
                for y in 0..ch {
                    for x in 0..8 * cw {
                        let cov = i32::from(want[(y * 8 * cw + x) as usize]);
                        let got = pixel(first * cw + x, top + y);
                        for i in 0..3 {
                            let expect = NORD_BG[i] + (MAGENTA[i] - NORD_BG[i]) * cov / 255;
                            assert!(
                                (got[i] - expect).abs() <= 3,
                                "{kind:?} at {scale}x, x {x} y {y}: got {got:?}, want {expect} in channel {i}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_blinking_cell_keeps_only_its_ground_through_the_hidden_half() {
        // Nord text that blinks, underlined and struck through, then two
        // steady blanks. Every cell is selected, so the ground each one
        // keeps is the selection's over Nord.
        let bytes = [NORD, b"\x1b[5;4;9mBlink\x1b[25;24;29m  ".as_slice()].concat();
        for scale in [1.0_f32, 2.0] {
            let shown = render_frame(&bytes, 7, 1, scale, 1.2, false, true);
            let hidden = render_frame(&bytes, 7, 1, scale, 1.2, true, true);
            let (Some(shown), Some(hidden)) = (shown, hidden) else {
                return;
            };
            if let Some(dir) = std::env::var_os("VOSH_TEXT_STYLE_RENDERS") {
                for (frame, half) in [(&shown, "shown"), (&hidden, "hidden")] {
                    let name = format!("grid_blink_{half}_{}x.png", scale as u32);
                    write_png(&std::path::Path::new(&dir).join(name), frame);
                }
            }
            assert!(shown.blinks && hidden.blinks);
            let (cw, ch) = shown.cell;
            let w = shown.w;
            let pixel = |f: &Frame, x: u32, y: u32| {
                let at = ((y * w + x) * 4) as usize;
                [0, 1, 2].map(|i| i32::from(f.rgba[at + i]))
            };
            let close = |a: [i32; 3], b: [i32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 2);
            // The steady blank at column 6 shows the selected ground.
            let ground = pixel(&hidden, 6 * cw + cw / 2, ch / 2);
            let line = shown.decor.under..shown.decor.under + shown.decor.t;
            let (mut ink, mut underline) = (0, 0);
            for y in 0..ch {
                for x in 0..5 * cw {
                    let (on, off) = (pixel(&shown, x, y), pixel(&hidden, x, y));
                    // The hidden half is bare ground: no glyph, no
                    // underline and no strike, as xterm draws it.
                    assert!(close(off, ground), "ink at {scale}x, x {x} y {y}: {off:?}");
                    if line.contains(&y) {
                        underline += usize::from(!close(on, ground));
                    } else {
                        ink += usize::from(!close(on, ground));
                    }
                }
            }
            assert!(ink > 0, "the shown half drew no text at {scale}x");
            assert!(
                underline > 0,
                "the shown half drew no underline at {scale}x"
            );
        }
    }

    #[test]
    fn a_frame_with_no_blinking_text_reports_none() {
        let steady = [NORD, b"\x1b[4;9mSteady\x1b[0m \x1b[5m \x1b[0m".as_slice()].concat();
        let Some(frame) = render_offscreen(&steady, 9, 1, 1.0, 1.2) else {
            return;
        };
        // A blinking blank changes nothing when it flips.
        assert!(!frame.blinks);
        for blinks in [b"\x1b[5mx".as_slice(), b"\x1b[5;4m \x1b[0m"] {
            let Some(frame) = render_offscreen(blinks, 4, 1, 1.0, 1.2) else {
                return;
            };
            // A blinking letter, or a blank that loses its underline.
            assert!(frame.blinks);
        }
    }

    #[test]
    fn the_curl_clears_the_letters_at_every_line_height() {
        // Nord text over the Nord ground, with a magenta curl. Text ink
        // lifts the green channel off the ground and the curl drops it.
        // The letters count from a third of the way to the text color,
        // which leaves out the faint overshoot of a round bottom (C, u)
        // on the baseline row.
        const GROUND_G: i32 = 52;
        const TEXT_G: i32 = 233;
        let bytes = [NORD, b"\x1b[4:3;58:2::255:0:255mCurl\x1b[0m".as_slice()].concat();
        for scale in [1.0_f32, 2.0] {
            for line_height in [1.0_f32, 1.1, 1.2] {
                let Some(frame) = render_offscreen(&bytes, 4, 1, scale, line_height) else {
                    return;
                };
                if let Some(dir) = std::env::var_os("VOSH_TEXT_STYLE_RENDERS") {
                    let name = format!("grid_curl_{}x_lh{}.png", scale as u32, line_height);
                    write_png(&std::path::Path::new(&dir).join(name), &frame);
                }
                let (rgba, w) = (&frame.rgba, frame.w);
                let green =
                    |y: u32| (0..w).map(move |x| i32::from(rgba[((y * w + x) * 4 + 1) as usize]));
                let rows = 0..frame.cell.1;
                let text_last = rows
                    .clone()
                    .filter(|&y| green(y).any(|g| g > GROUND_G + (TEXT_G - GROUND_G) / 3))
                    .max();
                let curl_first = rows.filter(|&y| green(y).any(|g| g < GROUND_G - 8)).min();
                let (Some(text_last), Some(curl_first)) = (text_last, curl_first) else {
                    panic!("no ink at {scale}x, line height {line_height}");
                };
                assert!(
                    curl_first > text_last + frame.decor.t,
                    "{scale}x, line height {line_height}: letters end on row {text_last}, \
                     the curl starts on row {curl_first}"
                );
            }
        }
    }
}
