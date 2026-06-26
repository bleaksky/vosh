//! Tier 3 native terminal renderer, M2c (see docs/native-renderer.md).
//!
//! The wgpu cell renderer: turns `term_grid`'s cells into pixels. Built
//! incrementally — color mapping first (this file's first commit), then a
//! glyph atlas, then the instanced pipeline that replaces the M1 test
//! triangle. The pipeline reads the grid each frame and draws a
//! background quad plus a glyph quad per cell.
//!
//! macOS only for now (it targets the Metal surface in `native_surface`).

#![cfg(target_os = "macos")]
// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use std::sync::atomic::{AtomicU32, Ordering};

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
        NamedColor::Black => ANSI_16[0],
        NamedColor::Red => ANSI_16[1],
        NamedColor::Green => ANSI_16[2],
        NamedColor::Yellow => ANSI_16[3],
        NamedColor::Blue => ANSI_16[4],
        NamedColor::Magenta => ANSI_16[5],
        NamedColor::Cyan => ANSI_16[6],
        NamedColor::White => ANSI_16[7],
        NamedColor::BrightBlack => ANSI_16[8],
        NamedColor::BrightRed => ANSI_16[9],
        NamedColor::BrightGreen => ANSI_16[10],
        NamedColor::BrightYellow => ANSI_16[11],
        NamedColor::BrightBlue => ANSI_16[12],
        NamedColor::BrightMagenta => ANSI_16[13],
        NamedColor::BrightCyan => ANSI_16[14],
        NamedColor::BrightWhite => ANSI_16[15],
        NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => theme_fg(),
        NamedColor::Background => theme_bg(),
        NamedColor::DimBlack => dim(ANSI_16[0]),
        NamedColor::DimRed => dim(ANSI_16[1]),
        NamedColor::DimGreen => dim(ANSI_16[2]),
        NamedColor::DimYellow => dim(ANSI_16[3]),
        NamedColor::DimBlue => dim(ANSI_16[4]),
        NamedColor::DimMagenta => dim(ANSI_16[5]),
        NamedColor::DimCyan => dim(ANSI_16[6]),
        NamedColor::DimWhite => dim(ANSI_16[7]),
        NamedColor::DimForeground => dim(theme_fg()),
    }
}

// xterm 256-color cube + grayscale ramp.
fn indexed_to_rgb(i: u8) -> Rgb {
    match i {
        0..=15 => ANSI_16[i as usize],
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

use fontdue::Font;

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
    px: f32,
    cell_w: u32,
    cell_h: u32,
    ascent: f32,
    cols: u32,
    rows: u32,
    atlas_w: u32,
    atlas_h: u32,
    pixels: Vec<u8>,
    slots: HashMap<char, u32>,
    next: u32,
}

impl GlyphAtlas {
    /// Build an atlas from the first loadable family in the CSS
    /// `family_stack` (falling back to the system monospace) at `px`
    /// pixels. Returns `None` if no font can be loaded.
    pub(crate) fn new(family_stack: &str, px: f32) -> Option<Self> {
        let font = load_font(family_stack)?;
        let line = font.horizontal_line_metrics(px)?;
        let cell_h = (line.new_line_size.ceil() as u32).max(1);
        let cell_w = (font.metrics('M', px).advance_width.ceil() as u32).max(1);
        let cols = 16;
        let rows = 16;
        let atlas_w = cols * cell_w;
        let atlas_h = rows * cell_h;
        Some(Self {
            font,
            px,
            cell_w,
            cell_h,
            ascent: line.ascent,
            cols,
            rows,
            atlas_w,
            atlas_h,
            pixels: vec![0u8; (atlas_w * atlas_h) as usize],
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
    pub(crate) fn atlas_size(&self) -> (u32, u32) {
        (self.atlas_w, self.atlas_h)
    }
    pub(crate) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Get (or rasterize on first use) the glyph for `c`, returning its UV
    /// rect in the atlas. Falls back to the last slot when the grid fills.
    pub(crate) fn glyph_uv(&mut self, c: char) -> ([f32; 2], [f32; 2]) {
        let index = if let Some(&i) = self.slots.get(&c) {
            i
        } else {
            let i = self.next.min(self.cols * self.rows - 1);
            self.next += 1;
            self.rasterize_into(c, i);
            self.slots.insert(c, i);
            i
        };
        let (x, y, w, h) = slot_rect(index, self.cols, self.cell_w, self.cell_h);
        rect_to_uv(x, y, w, h, self.atlas_w, self.atlas_h)
    }

    /// UV rect for an already-rasterized glyph, or `None`. Read-only so the
    /// draw path can look up cached glyphs without mutating the atlas (the
    /// texture is uploaded once; non-cached chars fall back to blank).
    pub(crate) fn uv_if_cached(&self, c: char) -> Option<([f32; 2], [f32; 2])> {
        self.slots.get(&c).map(|&i| {
            let (x, y, w, h) = slot_rect(i, self.cols, self.cell_w, self.cell_h);
            rect_to_uv(x, y, w, h, self.atlas_w, self.atlas_h)
        })
    }

    /// Rasterize `c` and blit its coverage into slot `index`, positioned at
    /// the baseline. NOTE: glyph placement (xmin/ascent math) is the part
    /// that needs a visual check.
    fn rasterize_into(&mut self, c: char, index: u32) {
        let (metrics, bitmap) = self.font.rasterize(c, self.px);
        if metrics.width == 0 || metrics.height == 0 {
            return;
        }
        let (sx, sy, _, _) = slot_rect(index, self.cols, self.cell_w, self.cell_h);
        let gx = metrics.xmin.max(0) as u32;
        let top = (self.ascent - metrics.height as f32 - metrics.ymin as f32).round();
        let gy = top.max(0.0) as u32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let dst_x = sx + gx + col as u32;
                let dst_y = sy + gy + row as u32;
                if dst_x < sx + self.cell_w
                    && dst_y < sy + self.cell_h
                    && dst_x < self.atlas_w
                    && dst_y < self.atlas_h
                {
                    let di = (dst_y * self.atlas_w + dst_x) as usize;
                    self.pixels[di] = bitmap[row * metrics.width + col];
                }
            }
        }
    }
}

/// Load the first matchable family from a CSS font-family stack, always
/// falling back to the system monospace. Generic CSS names map to
/// font-kit's generic families; everything else is a literal title.
// `select_best_match` mis-ranks faces (it returned Menlo Italic for a
// Normal request), so pick the upright regular face of a family by hand:
// load each face, keep the Normal-style one whose weight is closest to
// 400. font-kit's `copy_font_data` extracts that single face, so fontdue
// reads it at collection index 0.
fn regular_face(
    source: &font_kit::source::SystemSource,
    family: &str,
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
        let weight_dist = (props.weight.0 - 400.0).abs();
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
const JETBRAINS_REGULAR: &[u8] =
    include_bytes!("../../src/assets/fonts/JetBrainsMonoNerdFont-Regular.ttf");

fn font_from_handle(handle: &font_kit::handle::Handle) -> Option<Font> {
    let kit_font = handle.load().ok()?;
    tracing::info!(font = %kit_font.full_name(), "native-surface: atlas font (system)");
    let data = kit_font.copy_font_data()?;
    Font::from_bytes(&data[..], fontdue::FontSettings::default()).ok()
}

fn load_font(family_stack: &str) -> Option<Font> {
    let source = font_kit::source::SystemSource::new();

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
            if let Ok(font) = Font::from_bytes(BERKELEY_REGULAR, fontdue::FontSettings::default()) {
                tracing::info!("native-surface: atlas font = bundled BerkeleyMono");
                return Some(font);
            }
        }
        if lower.contains("jetbrains") {
            if let Ok(font) = Font::from_bytes(JETBRAINS_REGULAR, fontdue::FontSettings::default())
            {
                tracing::info!("native-surface: atlas font = bundled JetBrainsMono");
                return Some(font);
            }
        }
        // Otherwise a system font, upright regular face.
        if let Some(font) = regular_face(&source, name)
            .as_ref()
            .and_then(font_from_handle)
        {
            return Some(font);
        }
    }

    regular_face(&source, "Menlo")
        .or_else(|| regular_face(&source, "Courier New"))
        .as_ref()
        .and_then(font_from_handle)
}

// ---------------------------------------------------------------------------
// Per-cell GPU instance data
// ---------------------------------------------------------------------------

/// One quad instance. `offset` is the top-left in surface pixels and
/// `size` its width/height, so most instances are cell-sized but the split
/// divider can be a thin full-width line. The vertex shader builds the
/// rect and converts to clip space; the fragment shader composites `fg`
/// over `bg` by the atlas coverage sampled across `uv_min..uv_max`.
/// `repr(C)` so it maps straight to a wgpu vertex buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CellInstance {
    pub offset: [f32; 2],
    pub size: [f32; 2],
    pub bg: Rgba,
    pub fg: Rgba,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// Build one `CellInstance` per cell, row-major. Pure: the grid is read
/// through `cell` (returns char, fg, bg) and glyph atlas UVs through `uv`,
/// so it tests without a live grid or GPU.
fn build_instances(
    cols: usize,
    rows: usize,
    cell_w: f32,
    cell_h: f32,
    mut cell: impl FnMut(usize, usize) -> (char, Rgba, Rgba),
    mut uv: impl FnMut(char) -> ([f32; 2], [f32; 2]),
) -> Vec<CellInstance> {
    let mut out = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let (ch, fg, bg) = cell(col, row);
            let (uv_min, uv_max) = uv(ch);
            out.push(CellInstance {
                offset: [col as f32 * cell_w, row as f32 * cell_h],
                size: [cell_w, cell_h],
                bg,
                fg,
                uv_min,
                uv_max,
            });
        }
    }
    out
}

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
    @location(1) bg: vec4<f32>,
    @location(2) fg: vec4<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vi: u32,
    @location(0) offset: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) bg: vec4<f32>,
    @location(3) fg: vec4<f32>,
    @location(4) uv_min: vec2<f32>,
    @location(5) uv_max: vec2<f32>,
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
    out.bg = bg;
    out.fg = fg;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let cov = textureSample(atlas_tex, atlas_samp, in.uv).r;
    return vec4<f32>(mix(in.bg.rgb, in.fg.rgb, cov), 1.0);
}
";

/// Owns the glyph atlas texture and the instanced pipeline that draws the
/// terminal grid. One quad per cell; the fragment shader composites the
/// glyph over the cell background by atlas coverage.
pub(crate) struct CellRenderer {
    atlas: GlyphAtlas,
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
            let _ = atlas.glyph_uv(code as char);
        }
        let space_uv = atlas.uv_if_cached(' ').unwrap_or(([0.0, 0.0], [0.0, 0.0]));
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
                // bg
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // fg
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // uv_min
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // uv_max
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 5,
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
                targets: &[Some(format.into())],
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
        let atlas = &self.atlas;

        // Split-scrollback: when scrolled up, draw the top region from the
        // scroll offset (frozen history) and the bottom from the live tail
        // (offset 0), with a draggable divider at `split_ratio`. Collapses
        // to full-live at the bottom (offset 0).
        let offset = grid.display_offset() as i32;
        let split = offset > 0 && rows >= 6;
        let top_rows = if split {
            ((rows as f32 * split_ratio) as usize).clamp(1, rows - 1)
        } else {
            rows
        };
        // The exact fraction of surface height where the divider is drawn,
        // so the cursor rect lines up with the rendered line.
        let divider_frac = if split {
            Some(top_rows as f32 * cell_h / surface_h as f32)
        } else {
            None
        };
        let divider = color_to_rgba(Color::Spec(Rgb {
            r: 0x3a,
            g: 0x40,
            b: 0x4c,
        }));

        // Selection highlight: compute the range once, recolor selected
        // cell backgrounds with the theme selection color.
        let selection = grid.selection_bounds();
        let selection_bg = rgb_to_rgba(theme_selection());

        let mut underlines: Vec<(usize, usize, Rgba)> = Vec::new();
        let mut instances = build_instances(
            cols,
            rows,
            cell_w,
            cell_h,
            |col, row| {
                let grid_line = if split && row >= top_rows {
                    row as i32 // live tail (offset 0)
                } else {
                    row as i32 - offset // top region / non-split (scrolled)
                };
                let (ch, fg, bg, flags) = grid.cell_at_line(grid_line, col);
                let (fg_rgba, mut bg_rgba) = styled_colors(fg, bg, flags);
                if cell_in_selection(selection, grid_line, col) {
                    bg_rgba = selection_bg;
                }
                if flags.underline {
                    underlines.push((col, row, fg_rgba));
                }
                (ch, fg_rgba, bg_rgba)
            },
            |ch| atlas.uv_if_cached(ch).unwrap_or(space_uv),
        );

        // Underline quads: a thin line at the bottom of each underlined cell.
        for (col, row, color) in underlines {
            instances.push(CellInstance {
                offset: [col as f32 * cell_w, (row + 1) as f32 * cell_h - 2.0],
                size: [cell_w, 1.5],
                bg: color,
                fg: color,
                uv_min: space_uv.0,
                uv_max: space_uv.1,
            });
        }

        // Thin full-width divider line at the split boundary, overlaying
        // the cells (drawn last). No cell row is consumed.
        if split {
            let thickness = 2.0_f32;
            instances.push(CellInstance {
                offset: [0.0, top_rows as f32 * cell_h - thickness * 0.5],
                size: [cols as f32 * cell_w, thickness],
                bg: divider,
                fg: divider,
                uv_min: space_uv.0,
                uv_max: space_uv.1,
            });
        }

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
        // matches the cells (linearized like everything else).
        let clear = color_to_rgba(Color::Named(NamedColor::Background));
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
        rpass.draw(0..6, 0..instances.len() as u32);
        drop(rpass);
        divider_frac
    }
}

#[cfg(test)]
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
        let _ = atlas.glyph_uv('A');
        let _ = atlas.glyph_uv(' ');

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
    fn build_instances_lays_out_row_major_with_colors_and_uv() {
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let instances = build_instances(
            2,
            1,
            10.0,
            20.0,
            |col, _row| (if col == 0 { 'a' } else { 'b' }, white, black),
            |ch| {
                if ch == 'a' {
                    ([0.0, 0.0], [0.1, 0.1])
                } else {
                    ([0.1, 0.0], [0.2, 0.1])
                }
            },
        );
        assert_eq!(instances.len(), 2);
        // Cell (0,0) at origin; cell (1,0) one cell to the right.
        assert_eq!(instances[0].offset, [0.0, 0.0]);
        assert_eq!(instances[1].offset, [10.0, 0.0]);
        // Colors and per-char UVs carry through.
        assert_eq!(instances[0].fg, white);
        assert_eq!(instances[0].bg, black);
        assert_eq!(instances[0].uv_min, [0.0, 0.0]);
        assert_eq!(instances[1].uv_min, [0.1, 0.0]);
    }

    #[test]
    fn build_instances_second_row_offsets_down() {
        let c = [0.5, 0.5, 0.5, 1.0];
        let instances = build_instances(
            1,
            2,
            8.0,
            16.0,
            |_, _| ('x', c, c),
            |_| ([0.0, 0.0], [0.0, 0.0]),
        );
        assert_eq!(instances[0].offset, [0.0, 0.0]);
        assert_eq!(instances[1].offset, [0.0, 16.0]);
    }
}
