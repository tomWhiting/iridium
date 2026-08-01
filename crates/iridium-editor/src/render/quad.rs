//! Simple quad/rectangle renderer for cursors, selections, and highlights.
//!
//! This module provides GPU-accelerated rendering of colored rectangles,
//! which is used for:
//! - Cursor rendering
//! - Selection highlights
//! - Current line highlighting
//! - Search match highlighting

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

/// Vertex data for a quad corner.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl QuadVertex {
    const ATTRIBS: [VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    const fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniform data for viewport transformation.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadUniforms {
    /// Viewport size (width, height) for coordinate transformation
    viewport_size: [f32; 2],
    _padding: [f32; 2],
}

/// A rectangle to be rendered.
#[derive(Debug, Clone, Copy)]
pub struct Quad {
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
}

impl Quad {
    /// Creates a new quad.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32, color: ThemeColor) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }
}

/// Shader source for quad rendering.
const QUAD_SHADER: &str = r"
struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert pixel coordinates to clip space (-1 to 1)
    let clip_x = (in.position.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let clip_y = 1.0 - (in.position.y / uniforms.viewport_size.y) * 2.0;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
";

/// Maximum number of quads that can be rendered in a single batch.
const MAX_QUADS: usize = 1024;

/// Renderer for colored rectangles.
///
/// This provides efficient batched rendering of colored quads for
/// cursors, selections, and other rectangular highlights.
///
/// # Performance
///
/// The renderer uses a pre-allocated CPU-side vertex buffer to avoid
/// per-frame allocations. For 120fps rendering, this eliminates GC
/// pressure and allocation overhead.
pub struct QuadRenderer {
    pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    viewport_size: [f32; 2],
    /// Pre-allocated CPU-side vertex buffer to avoid per-frame allocations.
    /// Capacity: `MAX_QUADS` * 6 vertices.
    cpu_vertices: Vec<QuadVertex>,
}

impl QuadRenderer {
    /// Creates a new quad renderer.
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device
    /// * `format` - The texture format for rendering
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Create shader module
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Quad Uniform Buffer"),
            size: std::mem::size_of::<QuadUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Quad Bind Group Layout"),
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
            label: Some("Quad Bind Group"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Quad Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[QuadVertex::desc()],
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
                topology: PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Create vertex buffer (6 vertices per quad - 2 triangles)
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Quad Vertex Buffer"),
            size: (std::mem::size_of::<QuadVertex>() * 6 * MAX_QUADS) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Pre-allocate CPU vertex buffer to avoid per-frame allocations
        let cpu_vertices = Vec::with_capacity(MAX_QUADS * 6);

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            viewport_size: [1.0, 1.0],
            cpu_vertices,
        }
    }

    /// Updates the viewport size.
    pub fn update_viewport(&mut self, queue: &Queue, width: u32, height: u32) {
        self.viewport_size = [u32_to_f32(width), u32_to_f32(height)];
        let uniforms = QuadUniforms {
            viewport_size: self.viewport_size,
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Renders quads.
    ///
    /// # Arguments
    ///
    /// * `render_pass` - The render pass to draw into
    /// * `queue` - The GPU queue for uploading vertices
    /// * `quads` - The quads to render
    ///
    /// # Performance
    ///
    /// Uses a pre-allocated CPU buffer to avoid per-frame allocations.
    /// Only clears and refills the existing buffer.
    pub fn render<'a>(
        &'a mut self,
        render_pass: &mut RenderPass<'a>,
        queue: &Queue,
        quads: &[Quad],
    ) {
        if quads.is_empty() {
            return;
        }

        // Clear and reuse the pre-allocated vertex buffer (no allocation)
        self.cpu_vertices.clear();

        for quad in quads.iter().take(MAX_QUADS) {
            let color = [quad.color.r, quad.color.g, quad.color.b, quad.color.a];
            let x0 = quad.x;
            let y0 = quad.y;
            let x1 = quad.x + quad.width;
            let y1 = quad.y + quad.height;

            // Two triangles for a quad
            self.cpu_vertices.push(QuadVertex {
                position: [x0, y0],
                color,
            });
            self.cpu_vertices.push(QuadVertex {
                position: [x1, y0],
                color,
            });
            self.cpu_vertices.push(QuadVertex {
                position: [x0, y1],
                color,
            });
            self.cpu_vertices.push(QuadVertex {
                position: [x1, y0],
                color,
            });
            self.cpu_vertices.push(QuadVertex {
                position: [x1, y1],
                color,
            });
            self.cpu_vertices.push(QuadVertex {
                position: [x0, y1],
                color,
            });
        }

        // Upload vertices
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_vertices),
        );

        // Draw
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // The buffer was cleared above and refilled with at most `MAX_QUADS * 6`
        // vertices, so the conversion cannot saturate.
        let vertex_count = u32::try_from(self.cpu_vertices.len()).unwrap_or(u32::MAX);
        render_pass.draw(0..vertex_count, 0..1);
    }
}
