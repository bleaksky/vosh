//! Text decorations. Where a cell's underlines and strike sit, the quads
//! that draw them, and the curly underline's sprite.

use super::frame::CellInstance;
use super::style::Rgba;
use crate::native::grid::Underline;

/// How far the underline sits below the baseline, in CSS pixels, the
/// `text-underline-offset: 3px` the page sets.
const UNDERLINE_DROP: f32 = 3.0;
/// The curly underline's band in CSS pixels, from the top of its crest
/// to the bottom of its trough. Chrome draws a wavy line 3.5
/// CSS pixels tall at a 1 px thickness.
const CURL_HEIGHT: f32 = 3.5;
/// A dash's share of its cell. The gap takes the rest, split evenly on
/// both sides, so a dashed run reads as one dash per character.
const DASH_SHARE: f32 = 0.65;

/// Where a cell's lines sit, in device pixels from the cell's top left
/// corner. Every line is one CSS pixel thick, and every line stays inside its cell, so no line reaches into
/// the row below or past a split's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Decor {
    /// Line thickness, one CSS pixel and never under one device pixel.
    pub(super) t: u32,
    /// Top row of the single, dotted, and dashed underlines.
    pub(super) under: u32,
    /// Top rows of the double underline's two lines.
    pub(super) double: [u32; 2],
    /// Top row and height of the curly underline's band.
    pub(super) curl_top: u32,
    pub(super) curl_h: u32,
    /// Top row of the strike, through the middle of the cell.
    pub(super) strike: u32,
    /// Where each cell's dash starts, and its length.
    pub(super) dash_x: u32,
    pub(super) dash_w: u32,
}

/// The lines for a `cell_w` by `cell_h` cell whose baseline sits on row
/// `baseline`, at `scale` device pixels per CSS pixel. The underline sits
/// three CSS pixels under the baseline when the cell has room, and a
/// line that would hang past the cell's bottom rises until it fits. The
/// curl, the tallest line, shrinks before it rises, so it keeps a CSS
/// pixel of room under the letters at every line height.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn decor(cell_w: u32, cell_h: u32, baseline: u32, scale: f32) -> Decor {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let cell_h = cell_h.max(1);
    let t = (scale.round() as u32).clamp(1, cell_h);
    let want = baseline + (UNDERLINE_DROP * scale).round() as u32;
    let fit = |ink: u32| want.min(cell_h.saturating_sub(ink));
    // The double's gap is half a line, at least a pixel: Chrome draws a
    // double underline at 2x as two 2 px lines 1 px apart.
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
pub(super) fn underline_rects(kind: Underline, x0: u32, cell_w: u32, d: &Decor) -> Vec<LineRect> {
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
pub(super) fn line_instances(
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
pub(super) fn curl_coverage(w: u32, h: u32, t: u32) -> Vec<u8> {
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
