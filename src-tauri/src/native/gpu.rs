//! Tier 3 native terminal renderer, M2c (see docs/native-renderer.md).
//!
//! The wgpu cell renderer: turns `grid`'s cells into pixels. Built
//! incrementally — color mapping first (this file's first commit), then a
//! glyph atlas, then the instanced pipeline that replaces the M1 test
//! triangle. The pipeline reads the grid each frame and draws a
//! background quad plus a glyph quad per cell.
//!
//! Glyphs rasterize through CoreGraphics with smoothing off, to match the
//! webview.
//!
//! This file holds `CellRenderer`, the pipeline that draws a frame, and
//! what a frame takes and reports. Its two shaders sit in `gpu/shaders/`.
//! `style` holds the colors and the rules that color a cell, `atlas` the
//! glyph atlas and its fonts, `decor` the underlines and the strike,
//! `bands` the bands under a lifted prompt, and `frame` the quads a frame
//! builds.

// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use crate::native::grid::Underline;

pub(crate) mod atlas;
pub(crate) mod bands;
mod decor;
mod frame;
pub(crate) mod style;

use atlas::{AtlasFonts, GlyphAtlas};
use bands::{
    band_instances, band_rects, band_viewport, draw_bands, ground_tint, lift_boxes, prompt_bands,
    prompt_reach, set_stage, widen_newest, LiftBox, MAX_LIFT_ROWS,
};
use decor::{decor, line_instances};
use frame::{build_instances, cell_in_selection, CellInstance, Marks, Region};
use style::{
    ansi16, blend_over, blinks_visibly, chrome_paint, drawn_char, draws_lines, paint_to_rgba,
    rgb_to_rgba, styled_colors, theme_bg, underline_color, wants_bold_font, Rgba,
    SCROLLBAR_TRACK_SHARE,
};

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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    surface_size: [f32; 2],
    // Neither shader reads past the surface size. The pad keeps the
    // uniform at 16 bytes, the size both shaders declare.
    _pad: [f32; 2],
}

const CELL_SHADER: &str = include_str!("gpu/shaders/cell.wgsl");

/// The prompt band: a rounded rectangle, or its inset ring, covered by
/// its signed distance the way a browser antialiases a border radius. The
/// instance's `uv_min` carries the radius and the ring width, zero for a
/// fill. Its `uv_max` carries the notch, zero for none: the band is then
/// the union of the rows above the last, full width down to the last
/// row's top, and every row as wide as the last one. The output is
/// premultiplied and sRGB encoded like the cells.
const BAND_SHADER: &str = include_str!("gpu/shaders/band.wgsl");

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
        grid: &crate::native::grid::TermGrid,
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
        let (find_matches, find_active_match) = crate::native::grid::find::find_snapshot();
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
        let hover = crate::native::surface::hover_url();
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
        // hidden). Drag mapping lives in native/surface.rs.
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
mod tests;
