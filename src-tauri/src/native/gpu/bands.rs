//! The band under a lifted prompt. Where each lift's band sits, the quads
//! and the pass that draw it, and the settings the page reports for it.
//! Its twin is `layoutBands` in src/terminal/xterm/liftBands.ts, and both run
//! fixtures/prompt-bands/cases.json.

use std::sync::atomic::{AtomicU32, Ordering};

use super::frame::CellInstance;
use super::style::{paint_to_rgba, srgb_to_linear, Rgba};
use crate::color::Paint;
use crate::native::grid::regions::LiftSpan;

// The band under a lifted prompt in CSS px, as
// src/terminal/xterm/liftBands.ts draws it on xterm. It reaches 4 past the text
// on each side and 2 above and below, at radius 4. Lifts on adjacent rows
// stop 1 inside their shared row edge, so 2 of ground stays between them.
// Both sides run fixtures/prompt-bands/cases.json, so keep them in step.
pub(super) const BAND_X: f32 = 4.0;
pub(super) const BAND_Y: f32 = 2.0;
pub(super) const BAND_Y_ADJACENT: f32 = -1.0;
pub(super) const BAND_RADIUS: f32 = 4.0;
// Light themes draw a 1 px inset ring on the band, as a box shadow does.
const BAND_RING: f32 = 1.0;
pub(super) const LIGHT_RING: Paint = Paint {
    r: 0,
    g: 0,
    b: 0,
    a: 0.14,
};
// A prompt is never this tall, so a lift that starts this far past a
// region never reaches into it. src/terminal/xterm/liftBands.ts uses the same bound.
pub(super) const MAX_LIFT_ROWS: i32 = 64;

/// A lift as one region shows it. Its rows count from the region's first
/// row, below zero or past the region's last when the region cuts it. Its
/// columns run from its leftmost start to one past its rightmost glyph.
/// `notch` is one past the last glyph of its last row, when that row is
/// narrower than the widest and your echo, or anything else, shows after
/// the lift on it, so the band steps in there instead of running under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LiftBox {
    pub(super) id: u64,
    pub(super) top: i32,
    pub(super) bottom: i32,
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) notch: Option<usize>,
}

/// Gather `spans` into one box per lift, top first, keeping the lifts that
/// meet the `vis` rows of a region whose first row is grid line `line0`.
pub(super) fn lift_boxes(spans: &[LiftSpan], line0: i32, vis: usize) -> Vec<LiftBox> {
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
pub(super) struct BandRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) notch: Option<[f32; 2]>,
}

/// Each box's band, placed as `layoutBands` places it on xterm, in device
/// pixels at `scale`, for cells `cell_w` by `cell_h` in a region whose
/// first row sits at `y0`.
pub(super) fn band_rects(
    boxes: &[LiftBox],
    y0: f32,
    cell_w: f32,
    cell_h: f32,
    scale: f32,
) -> Vec<BandRect> {
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

/// Widen the band of the lift `newest` among `boxes` by `reach` device px.
/// A band that steps in around your echo keeps its width.
pub(super) fn widen_newest(
    rects: &mut [BandRect],
    boxes: &[LiftBox],
    newest: Option<u64>,
    reach: f32,
) {
    if reach <= 0.0 {
        return;
    }
    for (rect, b) in rects.iter_mut().zip(boxes) {
        if Some(b.id) == newest && rect.notch.is_none() {
            rect.w += reach;
        }
    }
}

/// The quads that draw `rects` through the band shader, moved by `shift`
/// into the band pass's viewport: a rounded fill in `fill`, then an inset
/// ring in `ring` when the theme is light. The shader reads the corner
/// radius and the ring width from `uv_min`, and the notch from `uv_max`,
/// zero for none.
pub(super) fn band_instances(
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
pub(super) fn band_viewport(pane: [u32; 4], target: [u32; 2], scale: f32) -> [u32; 4] {
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
pub(super) fn set_stage<'a>(rpass: &mut wgpu::RenderPass<'a>, stage: Stage<'a>) {
    let (pipeline, bind_group, [x, y, w, h]) = stage;
    rpass.set_pipeline(pipeline);
    rpass.set_bind_group(0, bind_group, &[]);
    rpass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
}

/// Draw `range` of the band quads through `bands`, clipped to `clip`, then
/// point the pass back at `cells`, whose own clip the caller sets next.
pub(super) fn draw_bands<'a>(
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
pub(super) fn ground_tint(tints: &[Paint]) -> Rgba {
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

// How far past its last glyph the newest lift's band reaches, in CSS px,
// as f32 bits: the prompt card's ↵ and caret on the open row.
static PROMPT_REACH: AtomicU32 = AtomicU32::new(0);

/// Widen the newest lift's band by `px` CSS px, reported by the page while
/// the prompt card draws a ↵ or its caret past the open row's last glyph,
/// so the band runs under them. 0 while the card is closed.
pub(crate) fn set_prompt_reach(px: f32) {
    PROMPT_REACH.store(px.max(0.0).to_bits(), Ordering::Release);
}

pub(super) fn prompt_reach() -> f32 {
    f32::from_bits(PROMPT_REACH.load(Ordering::Acquire))
}
