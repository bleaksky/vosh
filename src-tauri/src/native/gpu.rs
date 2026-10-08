//! Tier 3 native terminal renderer (see docs/native-renderer.md).
//!
//! The wgpu cell renderer: turns `grid`'s cells into pixels. The
//! pipeline reads the grid each frame and draws a background quad plus a
//! glyph quad per cell.
//!
//! Glyphs rasterize through CoreGraphics with smoothing off, to match the
//! webview.
//!
//! This file holds `CellRenderer`, the pipeline that draws a frame, and
//! what a frame takes and reports. Its two shaders sit in `gpu/shaders/`.
//! `style` holds the colors and the rules that color a cell, `atlas` the
//! glyph atlas and its fonts, `decor` the underlines and the strike,
//! `bands` the bands under a lifted prompt, and `frame` lays out the
//! quads of each frame without the GPU.

// Pixel-coordinate float math on small integers (atlas dimensions, glyph
// coords) that are always far inside f32's exact-integer range.
#![allow(clippy::cast_precision_loss)]
// Geometry code reads clearest with x/y/w/h destructures.
#![allow(clippy::many_single_char_names)]

use crate::native::grid::find::FindMatch;

pub(crate) mod atlas;
pub(crate) mod bands;
mod decor;
mod frame;
pub(crate) mod style;

use atlas::{AtlasFonts, GlyphAtlas};
use bands::{draw_bands, prompt_reach, set_stage};
use decor::decor;
use frame::{build_frame, split_regions, CellInstance, FrameInputs};
use style::{ansi16, chrome_paint, drawn_char, theme_bg, wants_bold_font};

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
        reported: Option<(u32, u32)>,
        char_h: Option<u32>,
    ) -> Option<Self> {
        let fonts = AtlasFonts::load(font_stack)?;
        Some(Self::with_fonts(
            device, queue, format, fonts, font_px, reported, char_h,
        ))
    }

    /// Build the atlas from loaded `fonts` (printable ASCII
    /// pre-rasterized and uploaded once), the bind group, and the
    /// pipeline. `reported` and `char_h` are xterm's device cell and
    /// glyph box height, as [`GlyphAtlas::from_fonts`] takes them.
    pub(crate) fn with_fonts(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: AtlasFonts,
        font_px: f32,
        reported: Option<(u32, u32)>,
        char_h: Option<u32>,
    ) -> Self {
        let atlas = GlyphAtlas::from_fonts(fonts, font_px, reported, char_h);
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

    /// Atlas cell size in pixels, so the pointer code can map a point to a
    /// grid cell.
    pub(crate) fn cell_size_px(&self) -> (f32, f32) {
        (self.atlas.cell_w() as f32, self.atlas.cell_h() as f32)
    }

    /// Lay out a frame of `grid` and draw it into `view`, clearing to
    /// the default background first. `hover` is the link under the
    /// pointer, `find` and `find_active` the find matches and the one
    /// find is on, and `bands` whether a band goes under each lifted
    /// prompt.
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        grid: &crate::native::grid::TermGrid,
        hover: Option<(i32, usize, usize)>,
        find: Vec<FindMatch>,
        find_active: Option<FindMatch>,
        bands: bool,
        surface_w: u32,
        surface_h: u32,
        split_ratio: f32,
        placement: Placement,
    ) -> Drawn {
        let cols = grid.columns();
        let (regions, divider_px) = split_regions(
            grid,
            !find.is_empty(),
            split_ratio,
            surface_h,
            self.atlas.cell_h() as f32,
        );

        // Dynamic atlas: rasterize any visible glyph not yet cached, then
        // re-upload the atlas texture if it grew. Steady state (every glyph
        // already cached) costs only the lookups, no upload.
        let mut atlas_grew = false;
        for reg in &regions {
            for row in 0..reg.vis {
                for col in 0..cols {
                    let grid_line = reg.line0 + row as i32;
                    let (ch, fg, _, flags) = grid.cell_at_line(grid_line, col);
                    let ch = drawn_char(ch, flags, placement.blink_hidden);
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

        let inputs = FrameInputs {
            hover,
            find,
            find_active,
            cell: (self.atlas.cell_w(), self.atlas.cell_h()),
            slot_w: self.atlas.slot_w(),
            decor,
            solid_uv: self.atlas.solid_uv(),
            curl_uv,
            surface: (surface_w, surface_h),
            split_ratio,
            placement,
            chrome: chrome_paint(),
            wash_ground: theme_bg(),
            wash_palette: std::array::from_fn(ansi16),
            bands,
            reach: prompt_reach(),
        };
        let atlas = &self.atlas;
        let space_uv = self.space_uv;
        let frame = build_frame(grid, &inputs, |ch, bold, italic| {
            atlas.uv_if_cached(ch, bold, italic).unwrap_or(space_uv)
        });

        let uniforms = Uniforms {
            surface_size: [surface_w as f32, surface_h as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if frame.bands.iter().any(|r| !r.is_empty()) {
            let band_uniforms = Uniforms {
                surface_size: [frame.band_view[2] as f32, frame.band_view[3] as f32],
                _pad: [0.0; 2],
            };
            queue.write_buffer(
                &self.band_uniform_buffer,
                0,
                bytemuck::bytes_of(&band_uniforms),
            );
        }

        if frame.instances.len() > self.instance_capacity {
            self.instance_capacity = frame.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cell-instances"),
                size: (self.instance_capacity * std::mem::size_of::<CellInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&frame.instances),
        );

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
        let bands = (&self.band_pipeline, &self.band_bind_group, frame.band_view);
        // A region's bands, clipped to its rows grown by a band's reach,
        // from `from` to `to` in the target.
        let [bx, by, bw, bh] = frame.band_view;
        let band_clip = |i: usize, from: u32, to: u32| {
            frame
                .bands
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
                rpass.draw(0..6, frame.regions[0].clone());
                if let Some((range, clip)) = band_clip(1, oy + div, by + bh) {
                    draw_bands(&mut rpass, bands, cells, clip, range);
                }
                rpass.set_scissor_rect(ox, oy + div, surface_w, surface_h - div);
                rpass.draw(0..6, frame.regions[1].clone());
                rpass.set_scissor_rect(ox, oy, surface_w, surface_h);
                rpass.draw(0..6, frame.overlay.clone());
            }
            None => {
                if let Some((range, clip)) = band_clip(0, by, by + bh) {
                    draw_bands(&mut rpass, bands, cells, clip, range);
                }
                rpass.set_scissor_rect(ox, oy, surface_w, surface_h);
                rpass.draw(0..6, frame.regions[0].start..frame.overlay.end);
            }
        }
        drop(rpass);
        frame.drawn
    }
}

#[cfg(test)]
// The tests assert exact float values that are copied verbatim through the
// instance builder (offsets are products of small integers, colors are
// passed through untouched), so strict equality is the correct check.
#[allow(clippy::float_cmp)]
mod tests;
