//! Rounded-rectangle renderer for panel chrome: genuine arcs, hairline
//! borders and soft shadows from one signed-distance-field shader.
//!
//! This module is the [`QuadRenderer`](super::QuadRenderer)'s sibling, not a
//! replacement: sharp document furniture — selections, carets, gutter bands —
//! stays on the plain quad pipeline, and this one exists for the shapes a
//! face's chrome needs that a sharp rectangle cannot honestly be:
//!
//! - **Rounded corners** that are real circular arcs at any scale, evaluated
//!   per fragment from the rounded-rect signed distance
//!   `length(max(|p| - b + r, 0)) + min(max(q.x, q.y), 0) - r`, antialiased
//!   over half a pixel.
//! - **Soft shadows**, which are the same shape with a wider falloff: an
//!   instance with `blur > 0` fades its coverage over `±blur` pixels around
//!   the edge instead of the half-pixel antialiasing ramp.
//! - **Sharp rectangles too**, when a chrome pass wants one ordered draw
//!   list: a `radius` of zero degenerates to a crisply antialiased plain
//!   rectangle, so backdrops, hairlines and caret bars can interleave with
//!   the rounded shapes in a single pipeline instead of being layered
//!   incorrectly across two.
//!
//! Unlike the plain quad pipeline's six-vertices-per-quad stream, this one is
//! **instanced**: each shape is a single 40-byte instance, and the vertex
//! stage expands a four-vertex triangle strip from `@builtin(vertex_index)`,
//! enlarged by a blur-proportional margin ([`MARGIN_FACTOR`] times the blur)
//! so a shadow has room to fall off outside its geometric rectangle.

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferBindingType, BufferUsages, ColorTargetState,
    ColorWrites, Device, FragmentState, MultisampleState, PipelineLayoutDescriptor, PrimitiveState,
    PrimitiveTopology, Queue, RenderPass, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat, VertexAttribute,
    VertexBufferLayout, VertexState, VertexStepMode,
};

use super::units::u32_to_f32;
use crate::theme::Color as ThemeColor;

/// Per-instance data as the GPU sees it: one shape per 40-byte instance.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct RoundedInstance {
    /// The rectangle as `[x, y, width, height]` in pixels.
    rect: [f32; 4],
    /// Fill color as `[r, g, b, a]`.
    color: [f32; 4],
    /// Corner radius in pixels, already clamped to the short half-side.
    radius: f32,
    /// Blur half-width in pixels; zero draws a crisp edge.
    blur: f32,
}

impl RoundedInstance {
    const ATTRIBS: [VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32, 3 => Float32];

    const fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniform data for viewport transformation.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct RoundedUniforms {
    /// Viewport size (width, height) for coordinate transformation
    viewport_size: [f32; 2],
    _padding: [f32; 2],
}

/// A rounded rectangle to be rendered, blur included.
#[derive(Debug, Clone, Copy)]
pub struct RoundedQuad {
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Fill color
    pub color: ThemeColor,
    /// Corner radius in pixels. Zero draws a sharp — but still antialiased —
    /// rectangle; anything larger is clamped to half the short side at
    /// packing time, so a radius that outgrows its rectangle degenerates to a
    /// capsule rather than an artifact.
    pub radius: f32,
    /// Blur half-width in pixels. Zero draws a crisp edge; a positive value
    /// fades coverage over `±blur` around the edge — the drop-shadow shape.
    pub blur: f32,
}

impl RoundedQuad {
    /// Creates a crisp rounded rectangle.
    #[must_use]
    pub const fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: ThemeColor,
        radius: f32,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
            radius,
            blur: 0.0,
        }
    }

    /// Creates a blurred rounded rectangle — the drop-shadow shape.
    #[must_use]
    pub const fn shadow(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: ThemeColor,
        radius: f32,
        blur: f32,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
            radius,
            blur,
        }
    }
}

/// How far, in multiples of an instance's blur, the vertex stage expands the
/// strip beyond the rectangle so the shadow falloff has room to reach zero.
///
/// The fragment coverage is fully transparent at a distance of `blur` outside
/// the edge, so any factor of at least one suffices; three leaves the falloff
/// nowhere near the strip's edge even if the ramp softens later. The constant
/// is prepended to the WGSL source at pipeline creation, so the shader and
/// this documentation cannot drift.
const MARGIN_FACTOR: f32 = 3.0;

/// Shader source for rounded-rect rendering. `MARGIN_FACTOR` is prepended as
/// a WGSL module constant when the module is compiled.
const ROUNDED_SHADER: &str = r"
struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct InstanceInput {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
    @location(3) blur: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Pixel position relative to the rectangle's centre.
    @location(1) local: vec2<f32>,
    @location(2) half_size: vec2<f32>,
    @location(3) radius: f32,
    @location(4) blur: f32,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: InstanceInput) -> VertexOutput {
    // Expand the strip past the rectangle so blur can fall off outside it;
    // at least one pixel even unblurred, for the antialiasing ramp.
    let margin = max(MARGIN_FACTOR * instance.blur, 1.0);
    // vertex_index 0..3 -> (0,0), (1,0), (0,1), (1,1): one triangle strip.
    let corner = vec2<f32>(f32(index & 1u), f32(index >> 1u));
    let origin = instance.rect.xy - vec2<f32>(margin, margin);
    let size = instance.rect.zw + 2.0 * vec2<f32>(margin, margin);
    let position = origin + corner * size;
    // Convert pixel coordinates to clip space (-1 to 1)
    let clip_x = (position.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let clip_y = 1.0 - (position.y / uniforms.viewport_size.y) * 2.0;
    var out: VertexOutput;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = instance.color;
    out.local = position - (instance.rect.xy + instance.rect.zw * 0.5);
    out.half_size = instance.rect.zw * 0.5;
    out.radius = instance.radius;
    out.blur = instance.blur;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Signed distance to the rounded rectangle: negative inside, zero on the
    // edge, positive outside — a genuinely circular arc at every corner.
    let q = abs(in.local) - in.half_size + vec2<f32>(in.radius, in.radius);
    let outside = length(max(q, vec2<f32>(0.0, 0.0)));
    let inside = min(max(q.x, q.y), 0.0);
    let d = outside + inside - in.radius;
    // Crisp edges resolve over half a pixel; a blurred instance widens the
    // same ramp to +/- blur, which is the drop shadow.
    let edge = max(in.blur, 0.5);
    let coverage = 1.0 - smoothstep(-edge, edge, d);
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
";

/// Maximum number of rounded quads that can be rendered in a single batch.
///
/// Chrome is a handful of shapes per panel; 256 is an order of magnitude of
/// headroom, priced at ten kilobytes of instance buffer.
const MAX_ROUNDED_QUADS: usize = 256;

/// Renderer for rounded rectangles, blurred or crisp.
///
/// This provides instanced rendering of the shapes overlay chrome is made of:
/// panels, hairline borders, drop shadows, selected-row bands, backdrops.
///
/// # Performance
///
/// One instance per shape against a pre-allocated CPU-side buffer, uploaded
/// once per batch — no per-frame allocations, mirroring the plain quad
/// renderer's discipline.
pub struct RoundedQuadRenderer {
    pipeline: RenderPipeline,
    instance_buffer: Buffer,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    viewport_size: [f32; 2],
    /// Pre-allocated CPU-side instance buffer to avoid per-frame allocations.
    /// Capacity: [`MAX_ROUNDED_QUADS`].
    cpu_instances: Vec<RoundedInstance>,
}

impl RoundedQuadRenderer {
    /// Creates a new rounded-quad renderer.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device
    /// * `format` - The texture format for rendering
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Create shader module, sharing MARGIN_FACTOR with the Rust side.
        let source = format!("const MARGIN_FACTOR: f32 = {MARGIN_FACTOR:?};\n{ROUNDED_SHADER}");
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Rounded Quad Shader"),
            source: ShaderSource::Wgsl(source.into()),
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rounded Quad Uniform Buffer"),
            size: std::mem::size_of::<RoundedUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Rounded Quad Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Rounded Quad Bind Group"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Rounded Quad Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        // Create render pipeline: a four-vertex strip per instance.
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Rounded Quad Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[RoundedInstance::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Create instance buffer (one instance per shape)
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rounded Quad Instance Buffer"),
            size: (std::mem::size_of::<RoundedInstance>() * MAX_ROUNDED_QUADS) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Pre-allocate CPU instance buffer to avoid per-frame allocations
        let cpu_instances = Vec::with_capacity(MAX_ROUNDED_QUADS);

        Self {
            pipeline,
            instance_buffer,
            uniform_buffer,
            bind_group,
            viewport_size: [1.0, 1.0],
            cpu_instances,
        }
    }

    /// Updates the viewport size.
    pub fn update_viewport(&mut self, queue: &Queue, width: u32, height: u32) {
        self.viewport_size = [u32_to_f32(width), u32_to_f32(height)];
        let uniforms = RoundedUniforms {
            viewport_size: self.viewport_size,
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Renders rounded quads, in slice order — later shapes blend over
    /// earlier ones, so one ordered list is one correctly layered chrome
    /// pass.
    ///
    /// # Arguments
    ///
    /// * `render_pass` - The render pass to draw into
    /// * `queue` - The GPU queue for uploading instances
    /// * `quads` - The shapes to render
    ///
    /// # Performance
    ///
    /// Uses a pre-allocated CPU buffer to avoid per-frame allocations.
    /// Only clears and refills the existing buffer.
    pub fn render<'a>(
        &'a mut self,
        render_pass: &mut RenderPass<'a>,
        queue: &Queue,
        quads: &[RoundedQuad],
    ) {
        // Clear and reuse the pre-allocated instance buffer (no allocation)
        pack_instances(&mut self.cpu_instances, quads);
        if self.cpu_instances.is_empty() {
            return;
        }

        // Upload instances
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_instances),
        );

        // Draw: four strip vertices per instance.
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));

        // The buffer was cleared above and refilled with at most
        // `MAX_ROUNDED_QUADS` instances, so the conversion cannot saturate.
        let instance_count = u32::try_from(self.cpu_instances.len()).unwrap_or(u32::MAX);
        render_pass.draw(0..4, 0..instance_count);
    }
}

/// Clears `instances` and refills it from `quads`, dropping what cannot be
/// drawn and truncating at [`MAX_ROUNDED_QUADS`] — the same batch-limit
/// statement the plain quad renderer makes.
fn pack_instances(instances: &mut Vec<RoundedInstance>, quads: &[RoundedQuad]) {
    instances.clear();
    instances.extend(
        quads
            .iter()
            .take(MAX_ROUNDED_QUADS)
            .filter_map(instance_for),
    );
}

/// One quad as its GPU instance, or `None` for a shape with nothing to draw.
///
/// The radius is clamped to half the rectangle's short side, so a two-pixel
/// hairline asked to keep an eight-pixel radius degenerates to a capsule
/// instead of an inverted distance field; non-finite radius or blur values
/// sanitize to zero rather than poisoning the fragment arithmetic.
fn instance_for(quad: &RoundedQuad) -> Option<RoundedInstance> {
    // The comparison is written to also reject NaN dimensions.
    if !(quad.width > 0.0 && quad.height > 0.0) {
        return None;
    }
    let radius = if quad.radius.is_finite() {
        quad.radius.clamp(0.0, quad.width.min(quad.height) / 2.0)
    } else {
        0.0
    };
    let blur = if quad.blur.is_finite() {
        quad.blur.max(0.0)
    } else {
        0.0
    };
    Some(RoundedInstance {
        rect: [quad.x, quad.y, quad.width, quad.height],
        color: [quad.color.r, quad.color.g, quad.color.b, quad.color.a],
        radius,
        blur,
    })
}

/// The strip expansion the vertex stage applies for a given blur — kept in
/// Rust only for the tests, sharing [`MARGIN_FACTOR`] with the shader source
/// so the two cannot drift.
#[cfg(test)]
fn margin_for(blur: f32) -> f32 {
    (MARGIN_FACTOR * blur).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_ROUNDED_QUADS, RoundedInstance, RoundedQuad, instance_for, margin_for, pack_instances,
    };
    use crate::theme::Color;

    /// A plain white shape for tests that only care about geometry.
    fn white(x: f32, y: f32, width: f32, height: f32, radius: f32) -> RoundedQuad {
        RoundedQuad::new(x, y, width, height, Color::new(1.0, 1.0, 1.0, 1.0), radius)
    }

    #[test]
    fn an_instance_is_forty_bytes_with_the_declared_layout() {
        assert_eq!(std::mem::size_of::<RoundedInstance>(), 40);
        assert_eq!(std::mem::offset_of!(RoundedInstance, rect), 0);
        assert_eq!(std::mem::offset_of!(RoundedInstance, color), 16);
        assert_eq!(std::mem::offset_of!(RoundedInstance, radius), 32);
        assert_eq!(std::mem::offset_of!(RoundedInstance, blur), 36);
    }

    #[test]
    fn the_radius_is_clamped_to_half_the_short_side() {
        let instance = instance_for(&white(0.0, 0.0, 100.0, 4.0, 8.0)).expect("drawable");
        assert!((instance.radius - 2.0).abs() < f32::EPSILON);
        // A radius that fits is kept exactly.
        let instance = instance_for(&white(0.0, 0.0, 100.0, 40.0, 8.0)).expect("drawable");
        assert!((instance.radius - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_degenerate_rect_is_dropped_not_drawn() {
        assert!(instance_for(&white(0.0, 0.0, 0.0, 10.0, 4.0)).is_none());
        assert!(instance_for(&white(0.0, 0.0, 10.0, -1.0, 4.0)).is_none());
        assert!(instance_for(&white(0.0, 0.0, f32::NAN, 10.0, 4.0)).is_none());
    }

    #[test]
    fn hostile_radius_and_blur_values_sanitize_to_zero() {
        let hostile = RoundedQuad {
            radius: f32::NAN,
            blur: f32::NEG_INFINITY,
            ..white(0.0, 0.0, 10.0, 10.0, 0.0)
        };
        let instance = instance_for(&hostile).expect("drawable");
        assert!((instance.radius - 0.0).abs() < f32::EPSILON);
        assert!((instance.blur - 0.0).abs() < f32::EPSILON);
        let negative = RoundedQuad {
            radius: -3.0,
            blur: -2.0,
            ..white(0.0, 0.0, 10.0, 10.0, 0.0)
        };
        let instance = instance_for(&negative).expect("drawable");
        assert!((instance.radius - 0.0).abs() < f32::EPSILON);
        assert!((instance.blur - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn packing_truncates_at_the_batch_limit() {
        let quads: Vec<RoundedQuad> = (0..MAX_ROUNDED_QUADS + 10)
            .map(|_| white(0.0, 0.0, 10.0, 10.0, 2.0))
            .collect();
        let mut instances = Vec::new();
        pack_instances(&mut instances, &quads);
        assert_eq!(instances.len(), MAX_ROUNDED_QUADS);
        // A refill replaces, never appends.
        pack_instances(&mut instances, &quads[..3]);
        assert_eq!(instances.len(), 3);
    }

    #[test]
    fn the_shadow_margin_covers_the_falloff() {
        // Coverage reaches zero at a distance of `blur` outside the edge, so
        // the expansion must always be at least that; and at least one pixel
        // even unblurred, for the antialiasing ramp.
        for blur in [0.0_f32, 0.5, 1.0, 16.0, 48.0, 96.0] {
            assert!(margin_for(blur) >= blur);
            assert!(margin_for(blur) >= 1.0);
        }
    }
}
