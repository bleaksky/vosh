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
// Built up across several commits; the pieces are wired into the draw
// path at the end of M2c. Until then some helpers are exercised only by
// tests, so allow them to sit unused in the lib build.
#![allow(dead_code)]
// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

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

fn rgb_to_rgba(c: Rgb) -> Rgba {
    [
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
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
        NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => DEFAULT_FG,
        NamedColor::Background => DEFAULT_BG,
        NamedColor::DimBlack => dim(ANSI_16[0]),
        NamedColor::DimRed => dim(ANSI_16[1]),
        NamedColor::DimGreen => dim(ANSI_16[2]),
        NamedColor::DimYellow => dim(ANSI_16[3]),
        NamedColor::DimBlue => dim(ANSI_16[4]),
        NamedColor::DimMagenta => dim(ANSI_16[5]),
        NamedColor::DimCyan => dim(ANSI_16[6]),
        NamedColor::DimWhite => dim(ANSI_16[7]),
        NamedColor::DimForeground => dim(DEFAULT_FG),
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
    /// Build an atlas from the best system monospace font at `px`. Returns
    /// `None` if no font can be loaded.
    pub(crate) fn new(px: f32) -> Option<Self> {
        let font = load_monospace_font()?;
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

fn load_monospace_font() -> Option<Font> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;
    let kit_font = SystemSource::new()
        .select_best_match(&[FamilyName::Monospace], &Properties::new())
        .ok()?
        .load()
        .ok()?;
    let data = kit_font.copy_font_data()?;
    Font::from_bytes(&data[..], fontdue::FontSettings::default()).ok()
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
        assert_eq!(
            color_to_rgba(Color::Indexed(232)),
            [8.0 / 255.0, 8.0 / 255.0, 8.0 / 255.0, 1.0]
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
        let Some(mut atlas) = GlyphAtlas::new(16.0) else {
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
}
