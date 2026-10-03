//! What a frame draws from the grid. The quads it builds, the regions a
//! split shows, and the marks and selection a cell carries.

use super::style::Rgba;
use crate::native::grid::Underline;

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
pub(super) fn build_instances(
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
pub(super) struct Region {
    pub(super) y0: f32,
    pub(super) vis: usize,
    pub(super) line0: i32,
}

/// Underline/strike marks: (column, row top in pixels, color, kind). A
/// strike carries `Underline::None` as its kind.
pub(super) type Marks = Vec<(usize, f32, Rgba, Underline)>;

/// Line-major inclusive containment of a cell in a selection range given as
/// start and end line/column.
pub(super) fn cell_in_selection(
    bounds: Option<(i32, usize, i32, usize)>,
    line: i32,
    col: usize,
) -> bool {
    match bounds {
        Some((sl, sc, el, ec)) => {
            (line > sl || (line == sl && col >= sc)) && (line < el || (line == el && col <= ec))
        }
        None => false,
    }
}
