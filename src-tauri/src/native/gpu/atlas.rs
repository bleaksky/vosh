//! The glyph atlas. The fonts the renderer loads, rasterized through
//! CoreGraphics into one coverage texture. It also holds the font stack
//! rules, the twin of `renderFontStack` in src/lib/fontLoader.ts.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::font::CGGlyph;
use core_graphics::geometry::{CGAffineTransform, CGPoint, CGRect, CGSize};
use core_text::font::CTFont;
use font_kit::canvas::RasterizationOptions;
use font_kit::font::Font;
use font_kit::hinting::HintingOptions;
use pathfinder_geometry::transform2d::Transform2F;

use super::decor::curl_coverage;

// Italic slant: shear the top of the glyph rightward. The CoreGraphics text
// matrix's `c` term is the horizontal shear; positive leans the top right.
const ITALIC_SKEW: f32 = 0.21;

/// Pixel rect (x, y, w, h) of a fixed-size slot in the atlas grid.
pub(super) fn slot_rect(index: u32, cols: u32, cell_w: u32, cell_h: u32) -> (u32, u32, u32, u32) {
    let col = index % cols;
    let row = index / cols;
    (col * cell_w, row * cell_h, cell_w, cell_h)
}

/// Convert a pixel rect in an atlas of size (aw, ah) to a UV rect
/// (top-left, bottom-right) in 0..1.
pub(super) fn rect_to_uv(x: u32, y: u32, w: u32, h: u32, aw: u32, ah: u32) -> ([f32; 2], [f32; 2]) {
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
pub(super) fn native_baseline(glyph_top: u32, ascent: f32) -> u32 {
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
    pub(super) cell_w: u32,
    pub(super) cell_h: u32,
    // Each slot is wider than the layout cell so a slanted (italic) or
    // wide glyph can overhang to the right without being clipped; the
    // glyph quad is drawn at slot width and overhangs the next cell.
    pub(super) slot_w: u32,
    ascent: f32,
    // How far the glyph box sits below the top of the cell. xterm centers
    // its box in a cell taller than the box (line height above 1), and the
    // atlas drops each glyph the same amount so the baselines agree.
    pub(super) glyph_top: u32,
    pub(super) cols: u32,
    rows: u32,
    pub(super) atlas_w: u32,
    atlas_h: u32,
    pub(super) pixels: Vec<u8>,
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
    /// Build an atlas from loaded `fonts` at `px` pixels, against xterm's
    /// reported device cell and glyph box height, or against the font's
    /// own metrics when the page has not reported yet.
    pub(crate) fn from_fonts(
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
    pub(super) fn baseline(&self) -> u32 {
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

/// The files of the font Vosh bundles, which the page ships in
/// `fonts/`, regular then bold.
pub(crate) const BUNDLED_FILES: [&str; 2] = [
    "JetBrainsMonoNerdFont-Regular.ttf",
    "JetBrainsMonoNerdFont-Bold.ttf",
];

/// The faces of the font Vosh bundles, [`BUNDLED_FAMILY`], which the
/// webview renders with. font-kit often fails to resolve the family by
/// its CSS name (the file's internal family name differs), so the atlas
/// loads these faces directly to match the webview exactly. Launch
/// reads the page's copy and hands it in, so the binary carries none of
/// its own.
struct BundledFaces {
    regular: Arc<Vec<u8>>,
    bold: Arc<Vec<u8>>,
}

static BUNDLED: OnceLock<BundledFaces> = OnceLock::new();

/// Hand in the bundled faces. Only the first call counts.
pub(crate) fn hand_in_bundled(regular: Vec<u8>, bold: Vec<u8>) {
    let _ = BUNDLED.set(BundledFaces {
        regular: Arc::new(regular),
        bold: Arc::new(bold),
    });
}

/// The bundled bold or regular face, once launch has handed it in.
pub(super) fn bundled_face(bold: bool) -> Option<Arc<Vec<u8>>> {
    let faces = BUNDLED.get()?;
    Some(Arc::clone(if bold { &faces.bold } else { &faces.regular }))
}

/// A bundled font file from the repo's public/fonts. A dev build reads
/// it there, because its asset resolver reads dist, which `tauri dev`
/// never writes, and the tests have no page assets at all.
#[cfg(any(dev, test))]
pub(crate) fn repo_font(file: &str) -> Option<Vec<u8>> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../public/fonts");
    std::fs::read(std::path::Path::new(dir).join(file)).ok()
}

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
    pub(super) regular: Font,
    pub(super) bold: Font,
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

/// The family name of the font Vosh bundled before its current one, which
/// saved font lists still name. Vosh never ships it again, so the name
/// stands for [`BUNDLED_FAMILY`].
const RETIRED_BUNDLED: &str = "BerkeleyMono Bundled";

/// The default font list from those days. A profile that never picked a
/// font still holds it, and it draws as [`DEFAULT_FAMILIES`] does.
const RETIRED_DEFAULT: &str =
    "BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace";

/// The families of the default font list.
const DEFAULT_FAMILIES: [&str; 5] = [
    BUNDLED_FAMILY,
    "Menlo",
    "Consolas",
    "ui-monospace",
    "monospace",
];

/// The families both renderers try, in order, for the saved CSS font
/// list `stack`. [`RETIRED_DEFAULT`] becomes [`DEFAULT_FAMILIES`], and
/// [`RETIRED_BUNDLED`] becomes [`BUNDLED_FAMILY`]. A repeated name drops
/// out. `renderFontStack` in fontLoader.ts gives the webview the same
/// list, so xterm and the atlas land on the same face and cell.
pub(super) fn rendered_families(stack: &str) -> Vec<String> {
    if stack.trim().eq_ignore_ascii_case(RETIRED_DEFAULT) {
        return DEFAULT_FAMILIES.map(String::from).to_vec();
    }
    let mut out: Vec<String> = Vec::new();
    for raw in stack.split(',') {
        let mut name = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if name.is_empty() {
            continue;
        }
        if name.eq_ignore_ascii_case(RETIRED_BUNDLED) {
            name = BUNDLED_FAMILY;
        }
        if !out.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            out.push(name.to_string());
        }
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
        // The bundled family, matched by the webview. Without the bundled
        // faces it falls through to the system lookup below.
        if lower.contains("jetbrains") {
            if let Some(Ok(font)) = bundled_face(bold).map(|bytes| Font::from_bytes(bytes, 0)) {
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
