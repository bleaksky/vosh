//! Tier 3 native terminal renderer, M2c (see docs/native-renderer.md).
//!
//! The wgpu cell renderer: turns `term_grid`'s cells into pixels. Built
//! incrementally — color mapping first (this file's first commit), then a
//! glyph atlas, then the instanced pipeline that replaces the M1 test
//! triangle. The pipeline reads the grid each frame and draws a
//! background quad plus a glyph quad per cell.
//!
//! Compiles on every `native_surface` platform. Glyph rasterization is the
//! one platform-varying piece: CoreGraphics on macOS (smoothing off, to
//! match the webview), font-kit's DirectWrite/FreeType elsewhere.

#![cfg(native_surface)]
// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use crate::term_grid::CellFlags;

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

fn theme_fg() -> Rgb {
    unpack_rgb(THEME_FG.load(Ordering::Acquire), DEFAULT_FG)
}

fn theme_selection() -> Rgb {
    unpack_rgb(
        THEME_SEL.load(Ordering::Acquire),
        Rgb {
            r: 0x2a,
            g: 0x3b,
            b: 0x5e,
        },
    )
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

// The split divider color from the settings (0 = unset, theme default).
static DIVIDER_RGB: AtomicU32 = AtomicU32::new(0);

/// Set (or clear) the split divider color, reported by the frontend from
/// the `split_divider_color` setting.
pub(crate) fn set_divider_color(color: Option<(u8, u8, u8)>) {
    let bits = color.map_or(0, |(r, g, b)| pack_rgb(r, g, b));
    DIVIDER_RGB.store(bits, Ordering::Release);
}

fn divider_rgb() -> Rgb {
    unpack_rgb(
        DIVIDER_RGB.load(Ordering::Acquire),
        Rgb {
            r: 0x3a,
            g: 0x40,
            b: 0x4c,
        },
    )
}

/// Parse the divider color setting: `#rgb`, `#rrggbb` (the `#` optional),
/// or `rgb()`/`rgba()` with integer channels (alpha ignored — the divider
/// draws opaque). None for anything else, falling back to the default.
pub(crate) fn parse_css_color(value: &str) -> Option<(u8, u8, u8)> {
    let v = value.trim().to_ascii_lowercase();
    let hex = v.strip_prefix('#').unwrap_or(&v);
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let ch = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0);
        return Some((ch(&hex[0..2]), ch(&hex[2..4]), ch(&hex[4..6])));
    }
    if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let ch = |s: &str| u8::from_str_radix(s, 16).unwrap_or(0) * 17;
        return Some((ch(&hex[0..1]), ch(&hex[1..2]), ch(&hex[2..3])));
    }
    let inner = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let ch = |s: &str| s.parse::<u32>().ok().map(|n| n.min(255) as u8);
    Some((ch(parts[0])?, ch(parts[1])?, ch(parts[2])?))
}

// When set, draw bright (ANSI 8-15) colored text with the bold font weight.
static BRIGHT_BOLD: AtomicBool = AtomicBool::new(false);

/// Toggle drawing bright-colored text with the bold font, reported by the
/// frontend from the `bright_bold` setting.
pub(crate) fn set_bright_bold(on: bool) {
    BRIGHT_BOLD.store(on, Ordering::Release);
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

fn rgb_to_rgba(c: Rgb) -> Rgba {
    [
        srgb_to_linear(f32::from(c.r) / 255.0),
        srgb_to_linear(f32::from(c.g) / 255.0),
        srgb_to_linear(f32::from(c.b) / 255.0),
        1.0,
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

// ---------------------------------------------------------------------------
// Glyph atlas
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(target_os = "macos")]
use core_graphics::color_space::CGColorSpace;
#[cfg(target_os = "macos")]
use core_graphics::context::{CGContext, CGTextDrawingMode};
#[cfg(target_os = "macos")]
use core_graphics::font::CGGlyph;
#[cfg(target_os = "macos")]
use core_graphics::geometry::{CGAffineTransform, CGPoint, CGRect, CGSize};
#[cfg(target_os = "macos")]
use core_text::font::CTFont;
use font_kit::canvas::RasterizationOptions;
#[cfg(not(target_os = "macos"))]
use font_kit::canvas::{Canvas, Format};
use font_kit::font::Font;
use font_kit::hinting::HintingOptions;
use pathfinder_geometry::transform2d::Transform2F;
#[cfg(not(target_os = "macos"))]
use pathfinder_geometry::vector::Vector2I;

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
}

impl GlyphAtlas {
    /// Build an atlas from the first loadable family in the CSS
    /// `family_stack` (falling back to the system monospace) at `px`
    /// pixels. Returns `None` if no font can be loaded.
    pub(crate) fn new(family_stack: &str, px: f32) -> Option<Self> {
        let font = load_font(family_stack, false)?;
        let bold_font = load_font(family_stack, true).or_else(|| load_font(family_stack, false))?;
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
        let (cell_w, cell_h) = crate::native_surface::reported_cell()
            .map_or((cell_w_font, cell_h_font), |(w, h)| (w.max(1), h.max(1)));
        // Slots get a full extra cell of width so italic overhang fits.
        let slot_w = cell_w * 2;
        tracing::debug!(
            cell_w,
            cell_h,
            cell_w_font,
            cell_h_font,
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
        Some(Self {
            font,
            bold_font,
            px,
            cell_w,
            cell_h,
            slot_w,
            ascent,
            cols,
            rows,
            atlas_w,
            atlas_h,
            pixels,
            solid_uv,
            slots: HashMap::new(),
            next: 0,
        })
    }

    pub(crate) fn cell_w(&self) -> u32 {
        self.cell_w
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
            // Cap two below the count: the last slot is the solid block.
            let i = self.next.min(self.cols * self.rows - 2);
            self.next += 1;
            self.rasterize_into(c, i, bold, italic);
            self.slots.insert(key, i);
            i
        };
        let (x, y, w, h) = slot_rect(index, self.cols, self.slot_w, self.cell_h);
        rect_to_uv(x, y, w, h, self.atlas_w, self.atlas_h)
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
        #[cfg(target_os = "macos")]
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
        #[cfg(not(target_os = "macos"))]
        let coverage = rasterize_glyph_fk(
            face,
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
        let dst_y0 = sy as i32 + self.ascent.round() as i32 + bounds.origin_y();
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

/// Rasterize one glyph through font-kit's platform rasterizer
/// (`DirectWrite` on Windows, `FreeType` on Linux) into a `w * h` alpha
/// coverage buffer.
/// The glyph's bounding box is shifted to the buffer origin; `skew` is the
/// italic shear and `origin_x/origin_y` come from `raster_bounds`.
#[cfg(not(target_os = "macos"))]
#[allow(clippy::too_many_arguments)]
fn rasterize_glyph_fk(
    font: &Font,
    glyph_id: u32,
    px: f32,
    skew: f32,
    origin_x: i32,
    origin_y: i32,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let mut canvas = Canvas::new(Vector2I::new(w as i32, h as i32), Format::A8);
    // Shift the glyph so its bounding box sits at the canvas origin; the
    // negated shear matches the sign convention used for raster_bounds.
    let transform =
        Transform2F::row_major(1.0, 0.0, -skew, 1.0, -origin_x as f32, -origin_y as f32);
    if font
        .rasterize_glyph(
            &mut canvas,
            glyph_id,
            px,
            transform,
            HintingOptions::None,
            RasterizationOptions::GrayscaleAa,
        )
        .is_err()
    {
        return vec![0u8; w * h];
    }
    // Repack: the canvas stride can exceed the row width.
    let mut pixels = vec![0u8; w * h];
    for row in 0..h {
        let src = row * canvas.stride;
        pixels[row * w..(row + 1) * w].copy_from_slice(&canvas.pixels[src..src + w]);
    }
    pixels
}

/// Rasterize one glyph through CoreGraphics with font smoothing disabled, so
/// the coverage matches the webview's antialiased text rather than the heavier
/// smoothed look. Returns a `w * h` alpha coverage buffer (0 = no ink, 255 =
/// full ink). The glyph's bounding box is shifted to the buffer origin; `skew`
/// is the italic shear and `origin_x/origin_y` come from `raster_bounds`.
#[cfg(target_os = "macos")]
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

// Vosh ships these two families and the webview renders with them. font-kit
// often fails to resolve them by their CSS family name (the file's internal
// family name differs), so load the bundled regular faces directly to match
// the webview exactly.
const BERKELEY_REGULAR: &[u8] =
    include_bytes!("../../src/assets/fonts/BerkeleyMonoNerdFont-Regular.ttf");
const BERKELEY_BOLD: &[u8] = include_bytes!("../../src/assets/fonts/BerkeleyMonoNerdFont-Bold.ttf");
const JETBRAINS_REGULAR: &[u8] =
    include_bytes!("../../src/assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");
const JETBRAINS_BOLD: &[u8] =
    include_bytes!("../../src/assets/fonts/JetBrainsMonoNerdFont-Bold.ttf");

fn font_from_handle(handle: &font_kit::handle::Handle) -> Option<Font> {
    let kit_font = handle.load().ok()?;
    tracing::info!(font = %kit_font.full_name(), "native-surface: atlas font (system)");
    Some(kit_font)
}

fn load_font(family_stack: &str, bold: bool) -> Option<Font> {
    let source = font_kit::source::SystemSource::new();
    let weight = if bold { 700.0 } else { 400.0 };

    for raw in family_stack.split(',') {
        let name = raw.trim().trim_matches('"').trim_matches('\'').trim();
        let lower = name.to_ascii_lowercase();
        // Skip CSS generics; the Menlo/Courier fallback covers them.
        if matches!(
            lower.as_str(),
            "" | "monospace"
                | "ui-monospace"
                | "serif"
                | "ui-serif"
                | "sans-serif"
                | "ui-sans-serif"
                | "system-ui"
        ) {
            continue;
        }
        // Bundled families, matched by the webview.
        if lower.contains("berkeley") {
            let bytes = if bold {
                BERKELEY_BOLD
            } else {
                BERKELEY_REGULAR
            };
            if let Ok(font) = Font::from_bytes(Arc::new(bytes.to_vec()), 0) {
                tracing::info!(bold, "native-surface: atlas font = bundled BerkeleyMono");
                return Some(font);
            }
        }
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
        if let Some(font) = weighted_face(&source, name, weight)
            .as_ref()
            .and_then(font_from_handle)
        {
            return Some(font);
        }
    }

    // Platform monospace fallbacks: Menlo (macOS), Consolas (Windows),
    // then Courier New (everywhere).
    weighted_face(&source, "Menlo", weight)
        .or_else(|| weighted_face(&source, "Consolas", weight))
        .or_else(|| weighted_face(&source, "Courier New", weight))
        .as_ref()
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

/// Underline/strike marks: (column, row top in pixels, color).
type Marks = Vec<(usize, f32, Rgba)>;

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
// wgpu cell renderer
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    surface_size: [f32; 2],
    cell_size: [f32; 2],
}

const CELL_SHADER: &str = r"
struct Uniforms { surface_size: vec2<f32>, cell_size: vec2<f32> };
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
}

impl CellRenderer {
    /// Build the atlas (printable ASCII pre-rasterized and uploaded once),
    /// the bind group, and the pipeline. `None` if no font loads.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        font_stack: &str,
        font_px: f32,
    ) -> Option<Self> {
        let mut atlas = GlyphAtlas::new(font_stack, font_px)?;
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[instance_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Premultiplied-alpha over: the shader already multiplies
                    // color by coverage, so src factor is One.
                    blend: Some(wgpu::BlendState {
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
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let instance_capacity = 4096;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances"),
            size: (instance_capacity * std::mem::size_of::<CellInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Some(Self {
            atlas,
            texture,
            space_uv,
            pipeline,
            bind_group,
            uniform_buffer,
            instance_buffer,
            instance_capacity,
        })
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
    ) -> Option<f32> {
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
        // Wash accent bars. Washed lines carry a distinctive quarter-
        // strength truecolor background (NamedColor::wash_tint in the
        // trigger crate), so the bar derives straight from the grid:
        // any row whose first cell wears a known wash tint gets a
        // left-edge bar in the full-strength color. No side channel to
        // drift — bars survive resize, reflow, and scrollback reload
        // wherever the wash bytes themselves do.
        let wash_accents: HashMap<[u8; 3], Rgba> = vosh_trigger::NamedColor::ALL
            .iter()
            .map(|c| {
                let (tr, tg, tb) = c.wash_tint();
                let (r, g, b) = c.rgb();
                ([tr, tg, tb], color_to_rgba(Color::Spec(Rgb { r, g, b })))
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
        for reg in &regions {
            for row in 0..reg.vis {
                for col in 0..cols {
                    let grid_line = reg.line0 + row as i32;
                    let (ch, fg, _, flags) = grid.cell_at_line(grid_line, col);
                    let bold = wants_bold_font(fg, flags);
                    if ch != ' ' && self.atlas.uv_if_cached(ch, bold, flags.italic).is_none() {
                        self.atlas.glyph_uv(ch, bold, flags.italic);
                        atlas_grew = true;
                    }
                }
            }
        }
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
        let divider = rgb_to_rgba(divider_rgb());

        // Selection highlight: compute the range once, recolor selected
        // cell backgrounds with the theme selection color.
        let selection = grid.selection_bounds();
        let selection_bg = rgb_to_rgba(theme_selection());

        // Find-match highlight: amber for matches, brighter for the active
        // one. Keyed by grid line for an O(1) lookup per cell.
        let mut find_by_line: HashMap<i32, Vec<(usize, usize, bool)>> = HashMap::new();
        for &(line, start, end) in &find_matches {
            let active = find_active_match == Some((line, start, end));
            find_by_line
                .entry(line)
                .or_default()
                .push((start, end, active));
        }
        let find_bg = color_to_rgba(Color::Spec(Rgb {
            r: 0x55,
            g: 0x44,
            b: 0x12,
        }));
        let find_active_bg = color_to_rgba(Color::Spec(Rgb {
            r: 0x99,
            g: 0x77,
            b: 0x22,
        }));

        // URL under the pointer reads as a link: blue and underlined (it
        // opens on Cmd+click).
        let hover = crate::native_surface::hover_url();
        let link_blue = rgb_to_rgba(Rgb {
            r: 0x58,
            g: 0xa6,
            b: 0xff,
        });

        let solid_uv = atlas.solid_uv();
        let slot_w = atlas.slot_w() as f32;
        // Marks carry the absolute pixel y of their row so region offsets
        // apply exactly once.
        let mut underlines: Vec<(usize, f32, Rgba)> = Vec::new();
        let mut strikeouts: Vec<(usize, f32, Rgba)> = Vec::new();
        // Shared per-cell styling: colors, selection, find highlight, hover,
        // and the underline/strike marks. Region closures wrap this with
        // their own line/pixel mapping.
        let style_cell = |grid_line: i32,
                          col: usize,
                          y_top: f32,
                          underlines: &mut Marks,
                          strikeouts: &mut Marks| {
            let (ch, fg, bg, flags) = grid.cell_at_line(grid_line, col);
            let (mut fg_rgba, mut bg_rgba) = styled_colors(fg, bg, flags);
            if cell_in_selection(selection, grid_line, col) {
                bg_rgba = selection_bg;
            }
            if let Some(ranges) = find_by_line.get(&grid_line) {
                for &(start, end, active) in ranges {
                    if col >= start && col < end {
                        bg_rgba = if active { find_active_bg } else { find_bg };
                        break;
                    }
                }
            }
            let hovered =
                hover.is_some_and(|(hl, hs, he)| grid_line == hl && col >= hs && col < he);
            if hovered {
                fg_rgba = link_blue;
            }
            if flags.underline || hovered {
                underlines.push((col, y_top, fg_rgba));
            }
            if flags.strikeout {
                strikeouts.push((col, y_top, fg_rgba));
            }
            (
                ch,
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
            // Underline quads: a thin line at the bottom of each marked cell.
            for (col, y_top, color) in underlines.drain(..) {
                instances.push(CellInstance {
                    offset: [col as f32 * cell_w, y_top + cell_h - 2.0],
                    size: [cell_w, 1.5],
                    color,
                    uv_min: solid_uv.0,
                    uv_max: solid_uv.1,
                });
            }
            // Strikethrough quads: a thin line across the cell mid-height.
            for (col, y_top, color) in strikeouts.drain(..) {
                instances.push(CellInstance {
                    offset: [col as f32 * cell_w, y_top + cell_h * 0.5],
                    size: [cell_w, 1.5],
                    color,
                    uv_min: solid_uv.0,
                    uv_max: solid_uv.1,
                });
            }
            // Wash accent bars: one slim full-height quad at the left
            // edge of each washed row. Width scales with the cell so it
            // lands near 2 logical px on hidpi surfaces.
            let bar_w = (cell_h / 8.0).clamp(2.0, 4.0);
            for row in 0..reg.vis {
                let grid_line = reg.line0 + row as i32;
                let (_, _, bg, _) = grid.cell_at_line(grid_line, 0);
                if let Color::Spec(rgb) = bg {
                    if let Some(&color) = wash_accents.get(&[rgb.r, rgb.g, rgb.b]) {
                        instances.push(CellInstance {
                            offset: [0.0, reg.y0 + row as f32 * cell_h],
                            size: [bar_w, cell_h],
                            color,
                            uv_min: solid_uv.0,
                            uv_max: solid_uv.1,
                        });
                    }
                }
            }
            instances.extend(glyphs);
            region_ranges.push(start..instances.len() as u32);
        }
        // Overlays draw unscissored: the divider line at its exact pixel
        // and the scroll-depth pill.
        let overlay_start = instances.len() as u32;
        // Sunk-well vignette: stacked translucent black strips fading in
        // from the top and left edges, approximating the mockup's two
        // inset shadows (the DOM cannot draw over the opaque surface, so
        // the renderer carries the cue itself). The pipeline blends
        // premultiplied alpha and black is zero in every channel, so each
        // strip just darkens whatever it covers by its alpha. Drawn in
        // the overlay pass so the split view gets the strips once at the
        // surface top, not repeated per region; the divider and pills
        // push after and stay crisp on top. Reach scales with the cell so
        // it lands near 14 physical px (top) and 10 (left) at the default
        // hidpi metrics.
        let top_vignette: [f32; 5] = [0.30, 0.22, 0.15, 0.09, 0.04];
        let left_vignette: [f32; 5] = [0.20, 0.15, 0.10, 0.06, 0.03];
        let top_step = (cell_h * 0.42).clamp(8.0, 20.0) / top_vignette.len() as f32;
        let left_step = (cell_h * 0.30).clamp(6.0, 14.0) / left_vignette.len() as f32;
        for (i, &alpha) in top_vignette.iter().enumerate() {
            instances.push(CellInstance {
                offset: [0.0, i as f32 * top_step],
                size: [surface_w as f32, top_step],
                color: [0.0, 0.0, 0.0, alpha],
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
        }
        for (i, &alpha) in left_vignette.iter().enumerate() {
            instances.push(CellInstance {
                offset: [i as f32 * left_step, 0.0],
                size: [left_step, surface_h as f32],
                color: [0.0, 0.0, 0.0, alpha],
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
        }
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

        // Scroll-depth indicator: "<back>/<max>" in a pill at the top-right
        // when scrolled up. The DOM cannot show this (it sits behind the
        // opaque surface), so it is drawn here.
        if offset > 0 {
            let text = format!("{offset}/{}", grid.scrollback_len());
            let n = text.chars().count() as f32;
            let pill_w = (n + 1.0) * cell_w;
            let x0 = (surface_w as f32 - pill_w).max(0.0);
            let ind_fg = rgb_to_rgba(Rgb {
                r: 0xc0,
                g: 0xc8,
                b: 0xd4,
            });
            let ind_bg = rgb_to_rgba(Rgb {
                r: 0x1a,
                g: 0x20,
                b: 0x2c,
            });
            instances.push(CellInstance {
                offset: [x0, 0.0],
                size: [pill_w, cell_h],
                color: ind_bg,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
            for (i, ch) in text.chars().enumerate() {
                let uv = atlas.uv_if_cached(ch, false, false).unwrap_or(space_uv);
                instances.push(CellInstance {
                    offset: [x0 + (i as f32 + 0.5) * cell_w, 0.0],
                    size: [slot_w, cell_h],
                    color: ind_fg,
                    uv_min: uv.0,
                    uv_max: uv.1,
                });
            }
        }
        // Transient "copied N chars" toast in the bottom-right, confirming
        // a selection copy. Same pill styling as the scroll indicator.
        if let Some(text) = crate::native_surface::copy_notice() {
            let n = text.chars().count() as f32;
            let pill_w = (n + 1.0) * cell_w;
            let x0 = (surface_w as f32 - pill_w - cell_w).max(0.0);
            let y0 = (surface_h as f32 - cell_h * 1.5).max(0.0);
            let toast_fg = rgb_to_rgba(Rgb {
                r: 0xc0,
                g: 0xc8,
                b: 0xd4,
            });
            let toast_bg = rgb_to_rgba(Rgb {
                r: 0x1a,
                g: 0x20,
                b: 0x2c,
            });
            instances.push(CellInstance {
                offset: [x0, y0],
                size: [pill_w, cell_h],
                color: toast_bg,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
            for (i, ch) in text.chars().enumerate() {
                let uv = atlas.uv_if_cached(ch, false, false).unwrap_or(space_uv);
                instances.push(CellInstance {
                    offset: [x0 + (i as f32 + 0.5) * cell_w, y0],
                    size: [slot_w, cell_h],
                    color: toast_fg,
                    uv_min: uv.0,
                    uv_max: uv.1,
                });
            }
        }

        // Overlay scrollbar on the right edge while scrolled: a subtle
        // track and a proportional thumb (the xterm scrollbar sits hidden
        // behind the opaque surface). Drag mapping lives in native_surface.
        let scrollback = grid.scrollback_len();
        if offset > 0 && scrollback > 0 {
            let total = (scrollback + rows) as f32;
            let sb_w = (cell_w * 0.45).clamp(4.0, 10.0);
            let x0 = surface_w as f32 - sb_w;
            let h = surface_h as f32;
            let mut track = rgb_to_rgba(divider_rgb());
            track[3] = 0.3;
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
            let mut thumb = rgb_to_rgba(Rgb {
                r: 0x8a,
                g: 0x92,
                b: 0xa0,
            });
            thumb[3] = 0.85;
            instances.push(CellInstance {
                offset: [x0, thumb_y],
                size: [sb_w, thumb_h],
                color: thumb,
                uv_min: solid_uv.0,
                uv_max: solid_uv.1,
            });
        }
        let overlay_range = overlay_start..instances.len() as u32;

        let uniforms = Uniforms {
            surface_size: [surface_w as f32, surface_h as f32],
            cell_size: [cell_w, cell_h],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

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
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        match divider_px {
            Some(divider_px) => {
                // Each region clips its overhanging edge row at the divider.
                let div = (divider_px as u32).min(surface_h.saturating_sub(1)).max(1);
                rpass.set_scissor_rect(0, 0, surface_w, div);
                rpass.draw(0..6, region_ranges[0].clone());
                rpass.set_scissor_rect(0, div, surface_w, surface_h - div);
                rpass.draw(0..6, region_ranges[1].clone());
                rpass.set_scissor_rect(0, 0, surface_w, surface_h);
                rpass.draw(0..6, overlay_range);
            }
            None => {
                rpass.draw(0..6, region_ranges[0].start..overlay_range.end);
            }
        }
        drop(rpass);
        divider_frac
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
        let Some(mut atlas) = GlyphAtlas::new("monospace", 16.0) else {
            return;
        };
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
    fn parse_css_color_accepts_hex_and_rgb_forms() {
        assert_eq!(parse_css_color("#3a404c"), Some((0x3a, 0x40, 0x4c)));
        assert_eq!(parse_css_color("3a404c"), Some((0x3a, 0x40, 0x4c)));
        assert_eq!(parse_css_color("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_css_color("rgb(1, 2, 3)"), Some((1, 2, 3)));
        assert_eq!(parse_css_color("rgba(10,20,30,0.5)"), Some((10, 20, 30)));
        assert_eq!(parse_css_color("bright-red"), None);
        assert_eq!(parse_css_color(""), None);
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
}
