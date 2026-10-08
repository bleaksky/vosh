use super::atlas::{
    bundled_face, centered_glyph_top, hand_in_bundled, native_baseline, rect_to_uv,
    rendered_families, repo_font, slot_rect, BUNDLED_FILES,
};
use super::bands::{
    band_instances, band_rects, band_viewport, ground_tint, lift_boxes, widen_newest, BandRect,
    LiftBox, BAND_RADIUS, BAND_X, BAND_Y, BAND_Y_ADJACENT, LIGHT_RING, MAX_LIFT_ROWS,
};
use super::decor::{curl_coverage, line_instances, underline_rects, Decor};
use super::frame::{build_frame, build_instances, FrameInputs, FrameQuads};
use super::style::{
    blend_over, blink_shown, blinks_visibly, color_to_rgba, dimmed, draws_lines, linear_to_srgb,
    paint_to_rgba, resolve_chrome, rgb_to_rgba, styled_colors, underline_color, until_blink_flip,
    ChromePaint, ChromeTokens, Rgba, ANSI_16, CURRENT_MATCH_FALLBACK_ALPHA, DIVIDER_FALLBACK_ALPHA,
    FIND_MATCH_FALLBACK_ALPHA, SCROLLBAR_FALLBACK_ALPHA, SCROLLBAR_TRACK_SHARE,
    SELECTION_FALLBACK_ALPHA, SELROW_FALLBACK_ALPHA,
};
use super::*;
use crate::color::Paint;
use crate::native::grid::regions::LiftSpan;
use crate::native::grid::{CellFlags, TermGrid, Underline};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use font_kit::font::Font;
use std::ops::Range;
use vosh_prompt::stage::{lift_end, lift_start, Output};

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

/// A 12 CSS px font at line height 1.2, the cell xterm
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
    // A 12 CSS px font: line heights 1, 1.1, and 1.2 at 1x
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
    let mut atlas = GlyphAtlas::from_fonts(fonts, 16.0, None, None);
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

/// Hand the atlas the repo's copy of the bundled font, as launch hands
/// it the page's, so the bundled family loads it.
fn hand_in_bundled_jetbrains() {
    let [regular, bold] = BUNDLED_FILES
        .map(|file| repo_font(file).expect("the repo holds the bundled JetBrains Mono"));
    hand_in_bundled(regular, bold);
}

#[test]
fn native_baseline_matches_xterm_at_every_line_height() {
    hand_in_bundled_jetbrains();
    for bold in [false, true] {
        let font = Font::from_bytes(bundled_face(bold).unwrap(), 0).unwrap();
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
    hand_in_bundled_jetbrains();
    let jetbrains =
        || AtlasFonts::load("JetBrainsMono Bundled").expect("Vosh bundles JetBrains Mono");
    let mut flat = GlyphAtlas::from_fonts(jetbrains(), 28.0, Some((17, 44)), None);
    let mut centered = GlyphAtlas::from_fonts(jetbrains(), 28.0, Some((17, 44)), Some(37));
    assert_eq!(centered.glyph_top, 4);
    assert_eq!(centered.baseline(), flat.baseline() + 4);
    assert_eq!(lowest_ink(&mut centered), lowest_ink(&mut flat) + 4);
    // No report yet means the font's own cell and no drop.
    let unreported = GlyphAtlas::from_fonts(jetbrains(), 28.0, None, Some(37));
    assert_eq!(unreported.glyph_top, 0);
}

#[test]
fn font_lists_match_the_shared_fixtures() {
    // The same cases run against renderFontStack in
    // src/lib/fontLoader.ts, so xterm and the atlas try the same
    // families in the same order.
    let text = include_str!("../../../../fixtures/font-stacks/cases.json");
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
fn the_retired_lists_land_on_the_bundled_jetbrains_mono() {
    hand_in_bundled_jetbrains();
    for stack in [
        "\"BerkeleyMono Bundled\", Menlo, monospace",
        "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace",
    ] {
        let fonts = AtlasFonts::load(stack).expect("Vosh bundles JetBrains Mono");
        assert_eq!(
            fonts.regular.postscript_name().as_deref(),
            Some("JetBrainsMonoNF-Regular"),
            "{stack}"
        );
        assert_eq!(
            fonts.bold.postscript_name().as_deref(),
            Some("JetBrainsMonoNF-Bold"),
            "{stack}"
        );
    }
}

fn paint(r: u8, g: u8, b: u8, a: f32) -> Paint {
    Paint { r, g, b, a }
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
    // Without a selection text each selected cell keeps its own color.
    assert_eq!(chrome.selection_text, None);
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
        selection_text: Some(token),
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
    assert_eq!(chrome.selection_text, Some(token));
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
    // A band at 1x: 35 cells of 7.8 by 17.5 draw 281 by 21.5.
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
/// in src/terminal/xterm/liftBands.ts run too.
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
    let text = include_str!("../../../../fixtures/prompt-bands/cases.json");
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
    hand_in_bundled_jetbrains();
    let Some(fonts) = AtlasFonts::load("JetBrainsMono Bundled") else {
        return;
    };
    let mut atlas = GlyphAtlas::from_fonts(fonts, 24.0, Some((14, 34)), Some(29));
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
    hand_in_bundled_jetbrains();
    let probe = GlyphAtlas::from_fonts(AtlasFonts::load("JetBrainsMono Bundled")?, px, None, None);
    let (cell_w, char_h) = (probe.cell_w(), probe.cell_h());
    let cell_h = (char_h as f32 * line_height).floor() as u32;
    let atlas = GlyphAtlas::from_fonts(
        AtlasFonts::load("JetBrainsMono Bundled")?,
        px,
        Some((cell_w, cell_h)),
        Some(char_h),
    );
    let decor = decor(cell_w, cell_h, atlas.baseline(), scale);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = CellRenderer::with_atlas(&device, &queue, format, atlas);
    let mut grid = crate::native::grid::TermGrid::new(cols, rows);
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
    // No link under the pointer, no find and no bands.
    let drawn = renderer.draw(
        &device,
        &queue,
        &mut encoder,
        &view,
        &grid,
        None,
        Vec::new(),
        None,
        false,
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

// Frame tests. They drive build_frame with a fed grid and a fixed glyph
// lookup, so they run on a machine without a GPU, where the tests that
// render above return early.

const FRAME_CELL: (u32, u32) = (10, 20);
const FRAME_GLYPH_UV: ([f32; 2], [f32; 2]) = ([0.25, 0.25], [0.5, 0.5]);
const FRAME_SOLID_UV: ([f32; 2], [f32; 2]) = ([0.9, 0.9], [0.9, 0.9]);
const FRAME_CURL_UV: ([f32; 2], [f32; 2]) = ([0.6, 0.6], [0.7, 0.7]);
/// The size of each kind of quad in a test frame. A cell's ground, its
/// glyph in a slot twice the cell wide, and a line one pixel thick under
/// or through it.
const GROUND_QUAD: [f32; 2] = [10.0, 20.0];
const GLYPH_QUAD: [f32; 2] = [20.0, 20.0];
const LINE_QUAD: [f32; 2] = [10.0, 1.0];

/// What a frame of `grid` takes at 1x with nothing hovered, found or
/// lifted, in a pane exactly as big as the grid. A washed row mixes its
/// field up from a black ground, and each chrome color differs from the
/// others and from the text.
fn frame_inputs(grid: &TermGrid) -> FrameInputs {
    let (cell_w, cell_h) = FRAME_CELL;
    let w = grid.columns() as u32 * cell_w;
    let h = grid.screen_lines() as u32 * cell_h;
    FrameInputs {
        hover: None,
        find: Vec::new(),
        find_active: None,
        cell: FRAME_CELL,
        slot_w: 2 * cell_w,
        decor: decor(cell_w, cell_h, 15, 1.0),
        solid_uv: FRAME_SOLID_UV,
        curl_uv: FRAME_CURL_UV,
        surface: (w, h),
        split_ratio: 0.66,
        placement: Placement {
            x: 0,
            y: 0,
            scale: 1.0,
            target: [w, h],
            blink_hidden: false,
        },
        chrome: ChromePaint {
            divider: paint(1, 2, 3, 1.0),
            selection: paint(60, 80, 120, 0.5),
            selection_text: None,
            find_match: paint(200, 180, 0, 0.35),
            current_match: paint(255, 140, 0, 0.65),
            link: paint(90, 160, 255, 1.0),
            scrollbar: paint(120, 120, 120, 0.45),
            selrow: paint(40, 44, 52, 1.0),
            ring: None,
        },
        wash_ground: Rgb { r: 0, g: 0, b: 0 },
        wash_palette: ANSI_16,
        bands: false,
        reach: 0.0,
    }
}

fn lay_out(grid: &TermGrid, inputs: &FrameInputs) -> FrameQuads {
    build_frame(grid, inputs, |_, _, _| FRAME_GLYPH_UV)
}

/// The quads in `range` of `frame` that are `size` big, in draw order.
fn quads_in(frame: &FrameQuads, range: &Range<u32>, size: [f32; 2]) -> Vec<CellInstance> {
    frame.instances[range.start as usize..range.end as usize]
        .iter()
        .filter(|q| q.size == size)
        .copied()
        .collect()
}

/// Every quad of `frame` that is `size` big, in draw order.
fn quads(frame: &FrameQuads, size: [f32; 2]) -> Vec<CellInstance> {
    quads_in(frame, &(0..frame.instances.len() as u32), size)
}

/// The ground of the cell at `col` on screen row `row`, in a frame that
/// is not split.
fn ground_at(frame: &FrameQuads, col: usize, row: usize) -> Rgba {
    let at = [col as f32 * 10.0, row as f32 * 20.0];
    quads(frame, GROUND_QUAD)
        .into_iter()
        .find(|q| q.offset == at)
        .expect("every cell has a ground")
        .color
}

/// A grid `rows` tall fed `lines` numbered lines, scrolled `up` lines
/// into its history.
fn scrolled_grid(cols: usize, rows: usize, lines: usize, up: i32) -> TermGrid {
    let mut grid = TermGrid::new(cols, rows);
    let text: Vec<String> = (0..lines).map(|n| n.to_string()).collect();
    grid.feed(text.join("\r\n").as_bytes());
    grid.scroll(up);
    grid
}

#[test]
fn the_split_opens_two_regions_at_the_drawn_divider_and_find_closes_it() {
    // Ten rows scrolled five lines up.
    let grid = scrolled_grid(4, 10, 30, 5);
    let mut inputs = frame_inputs(&grid);
    let frame = lay_out(&grid, &inputs);
    // 0.66 of the 200 px pane is 132 px. History fills the seven rows
    // the divider reaches into, and the live tail the four from row 6
    // down, each scissored at the divider.
    assert_eq!(frame.drawn.divider, Some(132.0 / 200.0));
    assert_eq!(frame.regions.len(), 2);
    assert_eq!(
        quads_in(&frame, &frame.regions[0], GROUND_QUAD).len(),
        7 * 4
    );
    let live = quads_in(&frame, &frame.regions[1], GROUND_QUAD);
    assert_eq!(live.len(), 4 * 4);
    assert_eq!(live[0].offset, [0.0, 120.0]);
    // The divider line, two pixels thick, centers on 132 px in the
    // divider color.
    let divider = quads(&frame, [40.0, 2.0]);
    assert_eq!(divider.len(), 1);
    assert_eq!(divider[0].offset, [0.0, 131.0]);
    assert_eq!(divider[0].color, paint_to_rgba(inputs.chrome.divider));
    // While find is open the view shows whole, with no divider.
    inputs.find = vec![(0, 0, 1)];
    let found = lay_out(&grid, &inputs);
    assert_eq!(found.drawn.divider, None);
    assert_eq!(found.regions.len(), 1);
    assert_eq!(quads(&found, [40.0, 2.0]), []);
}

#[test]
fn find_matches_take_the_match_color_and_the_one_find_is_on_takes_its_own() {
    let mut grid = TermGrid::new(10, 1);
    grid.feed(b"alpha beta");
    let mut inputs = frame_inputs(&grid);
    inputs.find = vec![(0, 0, 5), (0, 6, 10)];
    inputs.find_active = Some((0, 6, 10));
    let frame = lay_out(&grid, &inputs);
    // The space between the matches keeps the plain ground.
    let plain = ground_at(&frame, 5, 0);
    let found = blend_over(inputs.chrome.find_match, plain);
    let current = blend_over(inputs.chrome.current_match, plain);
    assert_ne!(found, current);
    for col in 0..5 {
        assert_eq!(ground_at(&frame, col, 0), found, "column {col}");
    }
    for col in 6..10 {
        assert_eq!(ground_at(&frame, col, 0), current, "column {col}");
    }
}

#[test]
fn selected_cells_draw_in_the_selection_text_the_page_sends() {
    // Red text, a dim word and plain text, with "d dim" selected.
    let mut grid = TermGrid::new(12, 1);
    grid.feed(b"\x1b[31mred \x1b[2mdim\x1b[0m word");
    grid.start_selection(0, 2);
    grid.extend_selection(0, 7, false);
    assert_eq!(grid.selection_bounds(), Some((0, 2, 0, 6)));
    let mut inputs = frame_inputs(&grid);
    let glyph_colors = |frame: &FrameQuads| -> Vec<(usize, Rgba)> {
        quads(frame, GLYPH_QUAD)
            .iter()
            .map(|g| ((g.offset[0] / 10.0) as usize, g.color))
            .collect()
    };
    // Without a selection text every glyph keeps its own color, as the
    // grid drew before the page sent one.
    let own = glyph_colors(&lay_out(&grid, &inputs));
    assert_eq!(own.len(), 10);
    // With one, each selected glyph takes it, dim text at the dim share,
    // and the rest keep their own.
    let text = paint(0xf2, 0xef, 0xee, 1.0);
    inputs.chrome.selection_text = Some(text);
    let selected = glyph_colors(&lay_out(&grid, &inputs));
    for ((col, got), (_, mine)) in selected.iter().zip(&own) {
        let want = match col {
            2 => paint_to_rgba(text),
            4..=6 => dimmed(paint_to_rgba(text)),
            _ => *mine,
        };
        assert_eq!(*got, want, "column {col}");
        assert_eq!(got == mine, !(2..=6).contains(col), "column {col}");
    }
    // A hovered link still takes the link color over the selection text.
    inputs.hover = Some((0, 4, 7));
    let link = paint_to_rgba(inputs.chrome.link);
    for (col, got) in glyph_colors(&lay_out(&grid, &inputs)) {
        if (4..7).contains(&col) {
            assert_eq!(got, link, "column {col}");
        }
    }
}

#[test]
fn a_hovered_link_takes_the_link_color_and_a_single_underline() {
    // The link carries a double underline of its own.
    let mut grid = TermGrid::new(30, 1);
    grid.feed(b"go to \x1b[4:2mhttps://example.org\x1b[0m now");
    let mut inputs = frame_inputs(&grid);
    assert_eq!(quads(&lay_out(&grid, &inputs), LINE_QUAD).len(), 2 * 19);
    inputs.hover = Some((0, 6, 25));
    let frame = lay_out(&grid, &inputs);
    let link = paint_to_rgba(inputs.chrome.link);
    let glyphs = quads(&frame, GLYPH_QUAD);
    assert_eq!(glyphs.len(), 26);
    for glyph in &glyphs {
        let col = (glyph.offset[0] / 10.0) as usize;
        assert_eq!(glyph.color == link, (6..25).contains(&col), "column {col}");
    }
    // Hovered, one plain underline under each cell of the link, in the
    // link color.
    let lines = quads(&frame, LINE_QUAD);
    assert_eq!(lines.len(), 19);
    for (line, col) in lines.iter().zip(6..25) {
        assert_eq!(line.offset, [col as f32 * 10.0, inputs.decor.under as f32]);
        assert_eq!(line.color, link);
    }
}

#[test]
fn the_scrollbar_track_spans_the_pane_and_the_thumb_sits_at_the_offset() {
    // Five rows over fifteen lines of history, scrolled five lines up.
    // The thumb shows five rows of twenty, a quarter of the pane, and
    // sits ten lines of fifteen down its travel.
    let grid = scrolled_grid(4, 5, 20, 5);
    assert_eq!(grid.scrollback_len(), 15);
    let inputs = frame_inputs(&grid);
    let frame = lay_out(&grid, &inputs);
    let thumb = paint_to_rgba(inputs.chrome.scrollbar);
    let mut track = thumb;
    track[3] *= SCROLLBAR_TRACK_SHARE;
    let bar = |offset, size, color| CellInstance {
        offset,
        size,
        color,
        uv_min: FRAME_SOLID_UV.0,
        uv_max: FRAME_SOLID_UV.1,
    };
    // 0.45 of the 10 px cell, at the right edge of the 40 px pane.
    assert_eq!(
        frame.instances[frame.overlay.start as usize..frame.overlay.end as usize],
        [
            bar([35.5, 0.0], [4.5, 100.0], track),
            bar([35.5, 50.0], [4.5, 25.0], thumb),
        ]
    );
}

#[test]
fn blinking_text_reports_blinks_and_its_hidden_half_keeps_only_the_ground() {
    let mut grid = TermGrid::new(7, 1);
    grid.feed(b"\x1b[5;4;9mBlink\x1b[0m");
    let mut inputs = frame_inputs(&grid);
    let shown = lay_out(&grid, &inputs);
    inputs.placement.blink_hidden = true;
    let hidden = lay_out(&grid, &inputs);
    assert!(shown.drawn.blinks && hidden.drawn.blinks);
    // Shown, five letters, each underlined and struck through.
    assert_eq!(quads(&shown, GLYPH_QUAD).len(), 5);
    assert_eq!(quads(&shown, LINE_QUAD).len(), 10);
    // Hidden, no glyph and no line, and every cell keeps its ground.
    assert_eq!(quads(&hidden, GLYPH_QUAD), []);
    assert_eq!(quads(&hidden, LINE_QUAD), []);
    assert_eq!(quads(&hidden, GROUND_QUAD), quads(&shown, GROUND_QUAD));
    // Steady text asks for no flip.
    let mut steady = TermGrid::new(7, 1);
    steady.feed(b"\x1b[4mSteady\x1b[0m");
    assert!(!lay_out(&steady, &frame_inputs(&steady)).drawn.blinks);
}

#[test]
fn a_lift_draws_on_a_band_only_while_bands_are_on() {
    let mut grid = TermGrid::new(20, 2);
    let mut out = Output::new(false);
    out.text(&[lift_start(1).as_slice(), b"100hp", &lift_end(1)].concat());
    grid.session_output(&out);
    let mut inputs = frame_inputs(&grid);
    inputs.bands = true;
    let lifted = lay_out(&grid, &inputs);
    // One fill in the row fill color, and no ring on a dark theme. It
    // reaches 4 px past the five letters on each side and 2 px past
    // the row above and below.
    assert_eq!(lifted.bands.len(), 1);
    let band = &lifted.instances[lifted.bands[0].start as usize..lifted.bands[0].end as usize];
    assert_eq!(band.len(), 1);
    assert_eq!(band[0].offset, [-4.0, -2.0]);
    assert_eq!(band[0].size, [58.0, 24.0]);
    assert_eq!(band[0].color, paint_to_rgba(inputs.chrome.selrow));
    // The ground under the letters draws clear, so the band shows.
    assert_eq!(ground_at(&lifted, 0, 0), [0.0; 4]);
    inputs.bands = false;
    let flat = lay_out(&grid, &inputs);
    assert_eq!(flat.bands, []);
    assert_eq!(ground_at(&flat, 0, 0)[3], 1.0);
}

#[test]
fn a_washed_row_carries_its_field_across_the_row() {
    let (r, g, b) = vosh_automation::trigger::NamedColor::Red.wash_tint();
    let mut grid = TermGrid::new(12, 2);
    grid.feed(format!("\x1b[48;2;{r};{g};{b}mwashed\x1b[0m line\r\nplain").as_bytes());
    let frame = lay_out(&grid, &frame_inputs(&grid));
    // ANSI red, 205, carried 18 percent of the way up from the black
    // ground.
    let field = rgb_to_rgba(Rgb { r: 37, g: 0, b: 0 });
    // The cells that carry the signal, and the plain ground after them.
    for col in 0..12 {
        assert_eq!(ground_at(&frame, col, 0), field, "column {col}");
    }
    // The row below is not washed.
    assert_ne!(ground_at(&frame, 0, 1), field);
}
