//! What a frame draws from the grid. `build_frame` lays out one frame's
//! quads from the grid and the inputs its caller reads, with the regions
//! a split shows and the marks and selection a cell carries. It needs no
//! GPU, so the frame tests run on a machine without one.
//!
//! Each cell's colors and its bold face still come from the style
//! statics in `style`, the theme's foreground, background and ANSI
//! palette and the bright bold setting, which the page sets. The wash
//! inputs color only the field of a washed row.

use std::collections::HashMap;
use std::ops::Range;

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use vosh_automation::trigger::color::wash_field;

use super::bands::{
    band_instances, band_rects, band_viewport, ground_tint, lift_boxes, widen_newest, LiftBox,
    MAX_LIFT_ROWS,
};
use super::decor::{line_instances, Decor};
use super::style::{
    blend_over, blinks_visibly, dimmed, drawn_char, draws_lines, paint_to_rgba, rgb_to_rgba,
    styled_colors, underline_color, wants_bold_font, ChromePaint, Rgba, SCROLLBAR_TRACK_SHARE,
};
use super::{Drawn, Placement};
use crate::native::grid::find::FindMatch;
use crate::native::grid::{TermGrid, Underline, SPLIT_MIN_ROWS};

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

/// The regions a frame draws, and while the view is split, the divider's
/// y in pane pixels.
///
/// Split-scrollback: when scrolled up, draw a frozen-history region on
/// top and the live tail below, separated by a draggable divider at
/// `split_ratio`. The divider tracks the pointer per PIXEL (no row
/// quantization — a row-snapped divider ratchets under the mouse); each
/// region keeps its rows cell-aligned internally and clips its edge row
/// mid-cell against the divider with a scissor rect. Find matches
/// (`finding`) suppress the split so the match shows in a single full
/// view.
pub(super) fn split_regions(
    grid: &TermGrid,
    finding: bool,
    split_ratio: f32,
    surface_h: u32,
    cell_h: f32,
) -> (Vec<Region>, Option<f32>) {
    let rows = grid.screen_lines();
    let offset = grid.display_offset() as i32;
    let split = offset > 0 && rows >= SPLIT_MIN_ROWS && !finding;
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
    (regions, divider_px)
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

/// What a frame draws besides the grid. The caller reads each of these
/// once, from the pointer, find, the atlas and the page's settings.
pub(super) struct FrameInputs {
    /// The link under the pointer, as grid line, first column and end.
    pub(super) hover: Option<(i32, usize, usize)>,
    /// Every find match, and the one find is on.
    pub(super) find: Vec<FindMatch>,
    pub(super) find_active: Option<FindMatch>,
    /// The atlas cell's width and height in device pixels.
    pub(super) cell: (u32, u32),
    /// A glyph slot's width, past the cell so an italic can overhang.
    pub(super) slot_w: u32,
    /// Where a cell's lines sit at this scale.
    pub(super) decor: Decor,
    /// The opaque texel every fill samples, and the curly underline's
    /// sprite.
    pub(super) solid_uv: ([f32; 2], [f32; 2]),
    pub(super) curl_uv: ([f32; 2], [f32; 2]),
    /// The pane's width and height in device pixels.
    pub(super) surface: (u32, u32),
    /// Where the divider sits while the view is split, as a fraction of
    /// the pane height.
    pub(super) split_ratio: f32,
    pub(super) placement: Placement,
    pub(super) chrome: ChromePaint,
    /// The terminal ground and the ANSI 0 to 15 palette, which a washed
    /// row's field mixes. The cells read theirs from the style statics.
    pub(super) wash_ground: Rgb,
    pub(super) wash_palette: [Rgb; 16],
    /// Your prompt shows lifted, so each lift draws on a band, and how far
    /// past its glyphs the newest band reaches, in CSS px.
    pub(super) bands: bool,
    pub(super) reach: f32,
}

/// A frame laid out: its quads, the ranges of them each draw takes, and
/// what the surface acts on after it.
pub(super) struct FrameQuads {
    pub(super) instances: Vec<CellInstance>,
    /// One range per region, scissored to its side of the divider.
    pub(super) regions: Vec<Range<u32>>,
    /// The divider and the scrollbar, drawn unscissored.
    pub(super) overlay: Range<u32>,
    /// One range of bands per region, none while bands are off.
    pub(super) bands: Vec<Range<u32>>,
    /// The band pass's viewport in the target, `[x, y, width, height]`.
    pub(super) band_view: [u32; 4],
    pub(super) drawn: Drawn,
}

/// Lay out one frame of `grid`. Each region's cells, their underlines and
/// strikes, the divider and scrollbar, and the bands under lifted
/// prompts, with each glyph at the atlas place `glyph_uv` gives.
pub(super) fn build_frame(
    grid: &TermGrid,
    inputs: &FrameInputs,
    glyph_uv: impl Fn(char, bool, bool) -> ([f32; 2], [f32; 2]),
) -> FrameQuads {
    let cell_w = inputs.cell.0 as f32;
    let cell_h = inputs.cell.1 as f32;
    let (surface_w, surface_h) = inputs.surface;
    let placement = inputs.placement;
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let offset = grid.display_offset() as i32;
    let (regions, divider_px) = split_regions(
        grid,
        !inputs.find.is_empty(),
        inputs.split_ratio,
        surface_h,
        cell_h,
    );
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
    // terminal ground (wash_field in the trigger crate). There is no edge bar, so a washed row reads
    // as one quiet band, the way the rest of the window marks rows.
    let wash_paint: HashMap<[u8; 3], Rgba> = vosh_automation::trigger::NamedColor::ALL
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            let (tr, tg, tb) = c.wash_tint();
            let mark = inputs.wash_palette[idx];
            let ground = inputs.wash_ground;
            let (r, g, b) = wash_field((mark.r, mark.g, mark.b), (ground.r, ground.g, ground.b));
            ([tr, tg, tb], rgb_to_rgba(Rgb { r, g, b }))
        })
        .collect();

    let blink_hidden = placement.blink_hidden;
    // A cell on screen that blinks with something to hide.
    let mut blinks = false;
    // The exact fraction of surface height where the divider is drawn,
    // so the grab band and the page's resize cursor line up with the
    // rendered line.
    let divider_frac = divider_px.map(|px| px / surface_h as f32);
    let chrome = &inputs.chrome;
    let divider = paint_to_rgba(chrome.divider);

    // Selection highlight: compute the range once, composite the
    // selection color over each selected cell's own background, and draw
    // its text in the selection text when the page sends one.
    let selection = grid.selection_bounds();
    let selection_text = chrome.selection_text.map(paint_to_rgba);

    // Find-match highlight, stronger for the current match. Keyed by
    // grid line for an O(1) lookup per cell.
    let mut find_by_line: HashMap<i32, Vec<(usize, usize, bool)>> = HashMap::new();
    for &(line, start, end) in &inputs.find {
        let active = inputs.find_active == Some((line, start, end));
        find_by_line
            .entry(line)
            .or_default()
            .push((start, end, active));
    }

    // URL under the pointer reads as a link: the link color and
    // underlined (it opens on Cmd+click).
    let hover = inputs.hover;
    let link = paint_to_rgba(chrome.link);

    let solid_uv = inputs.solid_uv;
    let slot_w = inputs.slot_w as f32;
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
    let bands_on = inputs.bands;
    let mut region_boxes: Vec<Vec<LiftBox>> = Vec::new();
    if bands_on {
        for reg in &regions {
            let last = reg.line0 + reg.vis as i32 - 1;
            let spans = grid.lift_spans(reg.line0 - MAX_LIFT_ROWS, last + MAX_LIFT_ROWS);
            region_boxes.push(lift_boxes(&spans, reg.line0, reg.vis));
        }
    }
    let mut style_cell =
        |grid_line: i32, col: usize, y_top: f32, underlines: &mut Marks, strikeouts: &mut Marks| {
            let (ch, fg, bg, flags) = grid.cell_at_line(grid_line, col);
            blinks |= blinks_visibly(ch, flags);
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
                // Dim text stays dim on the selection, as xterm draws it.
                if let Some(text) = selection_text {
                    fg_rgba = if flags.dim { dimmed(text) } else { text };
                }
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
    let mut region_ranges: Vec<Range<u32>> = Vec::new();
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
            &glyph_uv,
        );
        instances.extend(backgrounds);
        instances.extend(line_instances(
            &underlines,
            &strikeouts,
            &inputs.decor,
            inputs.cell.0,
            solid_uv,
            inputs.curl_uv,
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
    // hidden). Drag mapping lives in native/surface/pointer.rs.
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
    let mut band_ranges: Vec<Range<u32>> = Vec::new();
    let newest = region_boxes.iter().flatten().map(|b| b.id).max();
    let reach = inputs.reach * placement.scale;
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

    FrameQuads {
        instances,
        regions: region_ranges,
        overlay: overlay_range,
        bands: band_ranges,
        band_view,
        drawn: Drawn {
            divider: divider_frac,
            blinks,
        },
    }
}
