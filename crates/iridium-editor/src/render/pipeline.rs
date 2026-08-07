//! GPU render pipeline setup and management.
//!
//! This module handles wgpu device initialization, render pass creation,
//! and frame rendering for the editor.

use std::sync::Arc;

use wgpu::{
    Adapter, Backends, Color, CommandEncoder, Device, DeviceDescriptor, Features, Instance,
    InstanceDescriptor, InstanceFlags, Limits, LoadOp, MemoryHints, Operations, PowerPreference,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp,
    TextureFormat, TextureView, Trace,
};

use crate::editor::IridiumError;
use crate::render::minimap::{MinimapDimensions, MinimapRect, MinimapRenderer, ViewportIndicator};
use crate::theme::{Color as ThemeColor, EditorColors, SyntaxColors, Theme};

/// Default texture format for rendering.
/// BGRA8 is commonly supported across platforms and provides good compatibility.
pub const DEFAULT_TEXTURE_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

/// GPU adapter information for diagnostics.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Name of the GPU adapter
    pub name: String,
    /// Backend being used (Vulkan, Metal, DX12, etc.)
    pub backend: String,
    /// Device type (Integrated, Discrete, etc.)
    pub device_type: String,
    /// Driver information if available
    pub driver: String,
}

impl GpuInfo {
    /// Creates GPU info from a wgpu adapter.
    fn from_adapter(adapter: &Adapter) -> Self {
        let info = adapter.get_info();
        Self {
            name: info.name,
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver,
        }
    }
}

/// Configuration for the render pipeline.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Preferred power usage (low power for battery, high performance for desktop)
    pub power_preference: PowerPreference,
    /// Required features for the device
    pub required_features: Features,
    /// Limits for the device
    pub limits: Limits,
    /// Texture format for the surface
    pub texture_format: TextureFormat,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            power_preference: PowerPreference::HighPerformance,
            required_features: Features::empty(),
            limits: Limits::downlevel_webgl2_defaults(),
            texture_format: DEFAULT_TEXTURE_FORMAT,
        }
    }
}

/// The main render pipeline for the editor.
///
/// This handles wgpu device initialization, render pass creation,
/// and frame rendering. The pipeline is the central GPU resource
/// manager for the editor.
///
/// # GPU Initialization
///
/// The pipeline initializes GPU resources in this order:
/// 1. Create wgpu Instance
/// 2. Request an Adapter (GPU device)
/// 3. Request a Device and Queue from the adapter
///
/// # Error Handling
///
/// All GPU errors are surfaced with detailed diagnostic information
/// per the constitution's "No Silent Failures" principle.
#[derive(Debug)]
pub struct RenderPipeline {
    /// The wgpu device for GPU operations
    device: Arc<Device>,
    /// The command queue for submitting work to the GPU
    queue: Arc<Queue>,
    /// Information about the GPU adapter
    gpu_info: GpuInfo,
    /// Texture format being used
    texture_format: TextureFormat,
}

impl RenderPipeline {
    /// Creates a new render pipeline with default configuration.
    ///
    /// This initializes the GPU device and queue. The initialization
    /// is performed synchronously using `pollster::block_on`.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails. The error message
    /// includes detailed diagnostic information about why initialization
    /// failed (e.g., no suitable adapter found, device limits not met).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use iridium_editor::render::RenderPipeline;
    ///
    /// let pipeline = RenderPipeline::new()?;
    /// println!("GPU: {}", pipeline.gpu_info().name);
    /// ```
    pub fn new() -> Result<Self, IridiumError> {
        Self::with_config(RenderConfig::default())
    }

    /// Creates a new render pipeline with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails with the given
    /// configuration. This might happen if the requested features
    /// or limits are not supported by the available hardware.
    pub fn with_config(config: RenderConfig) -> Result<Self, IridiumError> {
        // Use pollster to block on async initialization
        pollster::block_on(Self::init_async(config))
    }

    /// Asynchronous GPU initialization.
    ///
    /// This is the core initialization logic that can be awaited
    /// in async contexts or blocked on synchronously.
    ///
    /// # Why two lints are silenced on `wasm32` only
    ///
    /// `wgpu`'s `Adapter`, `Device` and `Queue` are `Send + Sync` natively and
    /// are neither on `wasm32`, where the web backend wraps JavaScript objects
    /// that cannot leave their thread. Same source, different lint verdict per
    /// target — which is why the attribute is `cfg_attr`'d rather than
    /// unconditional. `#[expect]` is strict in both directions, so an
    /// unconditional one would itself be an unfulfilled-expectation warning on
    /// every native build.
    ///
    /// Neither lint is describing a defect here:
    ///
    /// - `future_not_send` — nothing spawns this future onto a thread pool. It
    ///   is driven by `pollster::block_on` in [`Self::with_config`] natively
    ///   and by `wasm_bindgen_futures` in the browser, both single-threaded.
    ///   This build has no shared-memory threading on `wasm32` at all, so the
    ///   property the lint protects cannot be violated on the target where it
    ///   fires.
    /// - `arc_with_non_send_sync` — the suggestion is `Rc`, and taking it would
    ///   fork [`RenderPipeline`]'s field types per target. The native path
    ///   genuinely needs `Arc`; one struct cannot hold both. The cost of the
    ///   atomic refcount on a single-threaded target is real and it is the
    ///   price of one definition instead of two.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            clippy::future_not_send,
            clippy::arc_with_non_send_sync,
            reason = "wgpu's handles are !Send + !Sync on wasm32 only, and this \
    target is single-threaded; see the doc comment above"
        )
    )]
    async fn init_async(config: RenderConfig) -> Result<Self, IridiumError> {
        // Step 1: Create wgpu instance
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            flags: InstanceFlags::default(),
            ..Default::default()
        });

        // Step 2: Request an adapter (GPU device)
        let adapter_result = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: config.power_preference,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await;

        let adapter = match adapter_result {
            Ok(adapter) => adapter,
            Err(e) => {
                return Err(Self::create_adapter_error(&instance, e).await);
            },
        };

        // Collect GPU info for diagnostics
        let gpu_info = GpuInfo::from_adapter(&adapter);

        // Step 3: Request device and queue
        let device_descriptor = DeviceDescriptor {
            label: Some("Iridium Editor Device"),
            required_features: config.required_features,
            required_limits: config.limits.clone(),
            memory_hints: MemoryHints::default(),
            trace: Trace::Off,
            ..Default::default()
        };

        let (device, queue) = adapter
            .request_device(&device_descriptor)
            .await
            .map_err(|e| Self::create_device_error(&adapter, &gpu_info, &config, e))?;

        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            gpu_info,
            texture_format: config.texture_format,
        })
    }

    /// Creates a detailed error message when no adapter is found.
    ///
    /// Carries the same `wasm32`-only exemption as [`Self::init_async`], and
    /// for the same reason: it awaits `enumerate_adapters`, whose future holds
    /// `!Send` wgpu handles on that target and only that target.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            clippy::future_not_send,
            reason = "wgpu's handles are !Send on wasm32 only, and this target \
    is single-threaded; see `init_async`"
        )
    )]
    async fn create_adapter_error(
        instance: &Instance,
        error: wgpu::RequestAdapterError,
    ) -> IridiumError {
        let adapters: Vec<Adapter> = instance.enumerate_adapters(Backends::all()).await;

        let message = if adapters.is_empty() {
            format!(
                "No GPU adapters found: {error}\n\n\
                 WebGPU/wgpu requires a compatible GPU.\n\
                 Please ensure your system has:\n\
                 - A GPU with Vulkan, Metal, DX12, or WebGPU support\n\
                 - Up-to-date graphics drivers installed\n\
                 - For browsers: WebGPU-capable browser (Chrome 113+, Firefox 121+)"
            )
        } else {
            let adapter_list: String = adapters
                .iter()
                .enumerate()
                .map(|(i, a): (usize, &Adapter)| {
                    let info = a.get_info();
                    format!(
                        "  {}. {} ({:?}, {:?})",
                        i + 1,
                        info.name,
                        info.backend,
                        info.device_type
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                "No suitable GPU adapter found: {error}\n\n\
                 Available adapters:\n{adapter_list}\n\n\
                 Try adjusting power preference or required features."
            )
        };

        IridiumError::GpuInitFailed { message }
    }

    /// Creates a detailed error message when device request fails.
    #[allow(clippy::needless_pass_by_value)]
    fn create_device_error(
        adapter: &Adapter,
        gpu_info: &GpuInfo,
        config: &RenderConfig,
        error: wgpu::RequestDeviceError,
    ) -> IridiumError {
        let adapter_limits = adapter.limits();

        let message = format!(
            "Failed to create GPU device: {error}\n\n\
             GPU: {} ({}, {})\n\
             Driver: {}\n\n\
             Requested limits vs adapter limits:\n\
             - max_texture_dimension_2d: {} vs {}\n\
             - max_bind_groups: {} vs {}\n\
             - max_uniform_buffer_binding_size: {} vs {}\n\n\
             Try using more compatible limits (e.g., Limits::downlevel_webgl2_defaults()).",
            gpu_info.name,
            gpu_info.backend,
            gpu_info.device_type,
            gpu_info.driver,
            config.limits.max_texture_dimension_2d,
            adapter_limits.max_texture_dimension_2d,
            config.limits.max_bind_groups,
            adapter_limits.max_bind_groups,
            config.limits.max_uniform_buffer_binding_size,
            adapter_limits.max_uniform_buffer_binding_size,
        );

        IridiumError::GpuInitFailed { message }
    }

    /// Returns a reference to the wgpu device.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Returns a shared reference to the wgpu device.
    #[must_use]
    pub fn device_arc(&self) -> Arc<Device> {
        Arc::clone(&self.device)
    }

    /// Returns a reference to the command queue.
    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Returns a shared reference to the command queue.
    #[must_use]
    pub fn queue_arc(&self) -> Arc<Queue> {
        Arc::clone(&self.queue)
    }

    /// Returns information about the GPU adapter.
    #[must_use]
    pub const fn gpu_info(&self) -> &GpuInfo {
        &self.gpu_info
    }

    /// Returns the texture format being used.
    #[must_use]
    pub const fn texture_format(&self) -> TextureFormat {
        self.texture_format
    }

    /// Creates a command encoder for recording GPU commands.
    ///
    /// The encoder is used to record render passes and other GPU
    /// operations before submitting them to the queue.
    #[must_use]
    pub fn create_command_encoder(&self, label: Option<&str>) -> CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label })
    }

    /// Begins a render pass for rendering to the given texture view.
    ///
    /// This creates a basic render pass that clears to the given color
    /// and stores the result. The render pass can then be used to draw
    /// text and other content.
    ///
    /// # Arguments
    ///
    /// * `encoder` - The command encoder to record the render pass
    /// * `target` - The texture view to render to
    /// * `clear_color` - The color to clear the render target to
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut encoder = pipeline.create_command_encoder(Some("Frame"));
    /// {
    ///     let _pass = pipeline.begin_render_pass(
    ///         &mut encoder,
    ///         &surface_view,
    ///         Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
    ///     );
    ///     // Draw text and other content here
    /// }
    /// pipeline.submit(encoder);
    /// ```
    pub fn begin_render_pass<'a>(
        &self,
        encoder: &'a mut CommandEncoder,
        target: &'a TextureView,
        clear_color: Color,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Iridium Render Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(clear_color),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Submits a command encoder to the GPU queue.
    ///
    /// This finishes the encoder and submits the recorded commands
    /// for execution on the GPU.
    pub fn submit(&self, encoder: CommandEncoder) {
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Converts a theme color to a wgpu Color for render passes (T150).
    ///
    /// This converts from Iridium's theme color format (f32 0.0-1.0)
    /// to wgpu's Color format (f64).
    #[must_use]
    pub fn theme_color_to_wgpu(color: ThemeColor) -> Color {
        Color {
            r: f64::from(color.r),
            g: f64::from(color.g),
            b: f64::from(color.b),
            a: f64::from(color.a),
        }
    }

    /// Creates a clear color from the theme's background color (T150).
    ///
    /// Convenience method for getting the appropriate clear color
    /// when beginning a render pass.
    #[must_use]
    pub fn clear_color_from_theme(theme: &Theme) -> Color {
        Self::theme_color_to_wgpu(theme.editor.background)
    }

    /// Begins a render pass with the theme's background as clear color (T150).
    ///
    /// This is a convenience method that uses the theme's editor background
    /// color to clear the render target.
    ///
    /// # Arguments
    ///
    /// * `encoder` - The command encoder to record the render pass
    /// * `target` - The texture view to render to
    /// * `theme` - The theme to get the background color from
    pub fn begin_themed_render_pass<'a>(
        &self,
        encoder: &'a mut CommandEncoder,
        target: &'a TextureView,
        theme: &Theme,
    ) -> wgpu::RenderPass<'a> {
        self.begin_render_pass(encoder, target, Self::clear_color_from_theme(theme))
    }
}

/// Theme uniform data for GPU shaders (T150).
///
/// This struct contains theme colors packed into a format suitable for
/// uploading to GPU uniform buffers. Each color is stored as [f32; 4]
/// in RGBA order.
///
/// # WGSL Shader Usage
///
/// ```wgsl
/// struct ThemeColors {
///     background: vec4<f32>,
///     foreground: vec4<f32>,
///     selection: vec4<f32>,
///     cursor: vec4<f32>,
///     current_line: vec4<f32>,
///     // ... additional colors
/// }
///
/// @group(0) @binding(0)
/// var<uniform> theme: ThemeColors;
/// ```
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ThemeUniforms {
    /// Editor background color
    pub background: [f32; 4],
    /// Default text color
    pub foreground: [f32; 4],
    /// Selection highlight color
    pub selection: [f32; 4],
    /// Inactive selection color
    pub selection_inactive: [f32; 4],
    /// Cursor color
    pub cursor: [f32; 4],
    /// Line number color
    pub line_number: [f32; 4],
    /// Active line number color
    pub line_number_active: [f32; 4],
    /// Current line highlight color
    pub current_line: [f32; 4],
    /// Gutter background color
    pub gutter: [f32; 4],
    /// Search match highlight color
    pub search_match: [f32; 4],
    /// Current search match highlight color
    pub search_match_current: [f32; 4],
}

impl ThemeUniforms {
    /// Creates theme uniforms from editor colors (T150).
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn from_editor_colors(colors: &EditorColors) -> Self {
        Self {
            background: colors.background.to_array(),
            foreground: colors.foreground.to_array(),
            selection: colors.selection.to_array(),
            selection_inactive: colors.selection_inactive.to_array(),
            cursor: colors.cursor.to_array(),
            line_number: colors.line_number.to_array(),
            line_number_active: colors.line_number_active.to_array(),
            current_line: colors.current_line.to_array(),
            gutter: colors.gutter.to_array(),
            search_match: colors.search_match.to_array(),
            search_match_current: colors.search_match_current.to_array(),
        }
    }

    /// Creates theme uniforms from a theme (T150).
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn from_theme(theme: &Theme) -> Self {
        Self::from_editor_colors(&theme.editor)
    }

    /// Returns the byte size of the uniform struct.
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn size() -> u64 {
        std::mem::size_of::<Self>() as u64
    }
}

/// Syntax color uniform data for GPU shaders (T150).
///
/// Contains syntax highlighting colors for shader-based rendering.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SyntaxUniforms {
    /// Keyword color
    pub keyword: [f32; 4],
    /// String literal color
    pub string: [f32; 4],
    /// Number literal color
    pub number: [f32; 4],
    /// Comment color
    pub comment: [f32; 4],
    /// Function name color
    pub function: [f32; 4],
    /// Variable name color
    pub variable: [f32; 4],
    /// Type name color
    pub type_name: [f32; 4],
    /// Operator color
    pub operator: [f32; 4],
    /// Punctuation color
    pub punctuation: [f32; 4],
    /// Property color
    pub property: [f32; 4],
    /// Constant color
    pub constant: [f32; 4],
    /// Tag color (HTML/XML)
    pub tag: [f32; 4],
    /// Attribute color
    pub attribute: [f32; 4],
    /// Error color
    pub error: [f32; 4],
}

impl SyntaxUniforms {
    /// Creates syntax uniforms from syntax colors (T150).
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn from_syntax_colors(colors: &SyntaxColors) -> Self {
        Self {
            keyword: colors.keyword.to_array(),
            string: colors.string.to_array(),
            number: colors.number.to_array(),
            comment: colors.comment.to_array(),
            function: colors.function.to_array(),
            variable: colors.variable.to_array(),
            type_name: colors.type_name.to_array(),
            operator: colors.operator.to_array(),
            punctuation: colors.punctuation.to_array(),
            property: colors.property.to_array(),
            constant: colors.constant.to_array(),
            tag: colors.tag.to_array(),
            attribute: colors.attribute.to_array(),
            error: colors.error.to_array(),
        }
    }

    /// Creates syntax uniforms from a theme (T150).
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn from_theme(theme: &Theme) -> Self {
        Self::from_syntax_colors(&theme.syntax)
    }

    /// Returns the byte size of the uniform struct.
    #[must_use]
    #[allow(dead_code)] // Part of T150 API for future shader integration
    pub const fn size() -> u64 {
        std::mem::size_of::<Self>() as u64
    }
}

/// Minimap render data for GPU submission (T145).
///
/// This struct contains all the data needed to render the minimap in a single
/// draw call. It converts minimap primitives into GPU-ready vertex data.
///
/// # Render Order
///
/// The minimap should be rendered in this order:
/// 1. Background (full minimap area)
/// 2. Content rects (syntax-colored lines)
/// 3. Viewport indicator (semi-transparent overlay)
#[derive(Debug, Clone)]
pub struct MinimapRenderData {
    /// Background rectangle covering the entire minimap
    pub background: MinimapRect,
    /// Background color for the minimap
    pub background_color: ThemeColor,
    /// Content rectangles with their colors (syntax-colored segments)
    pub content_rects: Vec<(MinimapRect, ThemeColor)>,
    /// Viewport indicator overlay
    pub viewport_indicator: Option<(MinimapRect, ThemeColor)>,
    /// Whether the minimap is visible
    pub visible: bool,
}

#[allow(dead_code)] // Part of T145 API for minimap rendering
impl MinimapRenderData {
    /// Creates empty render data (minimap hidden).
    #[must_use]
    pub const fn hidden() -> Self {
        Self {
            background: MinimapRect::new(0.0, 0.0, 0.0, 0.0),
            background_color: ThemeColor::new(0.0, 0.0, 0.0, 0.0),
            content_rects: Vec::new(),
            viewport_indicator: None,
            visible: false,
        }
    }

    /// Creates render data from minimap renderer state (T145).
    ///
    /// This method extracts all the rendering data from the minimap renderer
    /// and viewport indicator, converting it to a format suitable for GPU
    /// rendering.
    ///
    /// # Arguments
    ///
    /// * `renderer` - The minimap renderer with prepared line data
    /// * `dimensions` - Computed minimap dimensions
    /// * `indicator` - Viewport indicator for current scroll position
    /// * `theme` - Theme for background and default colors
    #[must_use]
    pub fn from_renderer(
        renderer: &MinimapRenderer,
        dimensions: &MinimapDimensions,
        indicator: &ViewportIndicator,
        theme: &Theme,
    ) -> Self {
        if !renderer.is_enabled() {
            return Self::hidden();
        }

        // Generate content rectangles from cached lines
        let content_rects: Vec<(MinimapRect, ThemeColor)> =
            renderer.generate_rects(dimensions).into_iter().collect();

        // Create viewport indicator overlay
        let viewport_indicator = Some((indicator.rect(), indicator.color));

        // Use a slightly darker background for the minimap
        let background_color = Self::darken_color(theme.editor.background, 0.1);

        Self {
            background: dimensions.bounds(),
            background_color,
            content_rects,
            viewport_indicator,
            visible: true,
        }
    }

    /// Returns the total number of rectangles to render.
    #[must_use]
    pub fn rect_count(&self) -> usize {
        if !self.visible {
            return 0;
        }
        // 1 for background + content rects + 1 for viewport indicator
        1 + self.content_rects.len() + usize::from(self.viewport_indicator.is_some())
    }

    /// Collects all rectangles and colors for batch rendering.
    ///
    /// Returns rectangles in render order: background, content, viewport indicator.
    #[must_use]
    pub fn collect_rects(&self) -> Vec<(MinimapRect, ThemeColor)> {
        if !self.visible {
            return Vec::new();
        }

        let mut rects = Vec::with_capacity(self.rect_count());

        // Background
        rects.push((self.background, self.background_color));

        // Content
        rects.extend(self.content_rects.iter().copied());

        // Viewport indicator
        if let Some(indicator) = self.viewport_indicator {
            rects.push(indicator);
        }

        rects
    }

    /// Converts all rectangles to vertex data for GPU rendering.
    ///
    /// Each rectangle produces 6 vertices (2 triangles) with position and color.
    /// Format: `[x, y, r, g, b, a]` per vertex.
    #[must_use]
    pub fn to_vertex_data(&self) -> Vec<f32> {
        let rects = self.collect_rects();
        let mut vertices = Vec::with_capacity(rects.len() * 6 * 6); // 6 vertices * 6 floats each

        for (rect, color) in rects {
            // Triangle 1: top-left, top-right, bottom-left
            // Triangle 2: bottom-left, top-right, bottom-right
            let x0 = rect.x;
            let y0 = rect.y;
            let x1 = rect.x + rect.width;
            let y1 = rect.y + rect.height;

            let r = color.r;
            let g = color.g;
            let b = color.b;
            let a = color.a;

            // Triangle 1
            vertices.extend_from_slice(&[x0, y0, r, g, b, a]); // top-left
            vertices.extend_from_slice(&[x1, y0, r, g, b, a]); // top-right
            vertices.extend_from_slice(&[x0, y1, r, g, b, a]); // bottom-left

            // Triangle 2
            vertices.extend_from_slice(&[x0, y1, r, g, b, a]); // bottom-left
            vertices.extend_from_slice(&[x1, y0, r, g, b, a]); // top-right
            vertices.extend_from_slice(&[x1, y1, r, g, b, a]); // bottom-right
        }

        vertices
    }

    /// Darkens a color by a given factor.
    fn darken_color(color: ThemeColor, factor: f32) -> ThemeColor {
        ThemeColor::new(
            color.r * (1.0 - factor),
            color.g * (1.0 - factor),
            color.b * (1.0 - factor),
            color.a,
        )
    }
}

impl Default for MinimapRenderData {
    fn default() -> Self {
        Self::hidden()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the bit patterns of a color's components.
    ///
    /// Uniform structs copy theme colors verbatim, so the correct assertion is
    /// bitwise identity rather than an approximate float comparison.
    fn color_bits(color: ThemeColor) -> [u32; 4] {
        color.to_array().map(f32::to_bits)
    }

    #[test]
    fn render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(config.power_preference, PowerPreference::HighPerformance);
        assert_eq!(config.texture_format, DEFAULT_TEXTURE_FORMAT);
    }

    #[test]
    fn gpu_info_fields() {
        // This test verifies the GpuInfo struct has the expected fields
        let info = GpuInfo {
            name: "Test GPU".to_string(),
            backend: "Vulkan".to_string(),
            device_type: "Discrete".to_string(),
            driver: "Test Driver".to_string(),
        };
        assert_eq!(info.name, "Test GPU");
        assert_eq!(info.backend, "Vulkan");
    }

    #[test]
    fn theme_uniforms_from_theme() {
        let theme = Theme::dark();
        let uniforms = ThemeUniforms::from_theme(&theme);

        // Verify background color matches theme. The uniform is a verbatim copy
        // of the theme color, so compare bit patterns rather than float values.
        assert_eq!(
            uniforms.background.map(f32::to_bits),
            color_bits(theme.editor.background)
        );
    }

    #[test]
    fn syntax_uniforms_from_theme() {
        let theme = Theme::light();
        let uniforms = SyntaxUniforms::from_theme(&theme);

        // Verify keyword color matches theme. The uniform is a verbatim copy of
        // the theme color, so compare bit patterns rather than float values.
        assert_eq!(
            uniforms.keyword.map(f32::to_bits),
            color_bits(theme.syntax.keyword)
        );
    }

    #[test]
    fn theme_uniforms_size() {
        // 11 colors * 4 floats * 4 bytes = 176 bytes
        assert_eq!(ThemeUniforms::size(), 176);
    }

    #[test]
    fn syntax_uniforms_size() {
        // 14 colors * 4 floats * 4 bytes = 224 bytes
        assert_eq!(SyntaxUniforms::size(), 224);
    }

    #[test]
    fn theme_color_to_wgpu_conversion() {
        let theme_color = ThemeColor::new(0.5, 0.25, 0.75, 1.0);
        let wgpu_color = RenderPipeline::theme_color_to_wgpu(theme_color);

        assert!((wgpu_color.r - 0.5).abs() < 0.001);
        assert!((wgpu_color.g - 0.25).abs() < 0.001);
        assert!((wgpu_color.b - 0.75).abs() < 0.001);
        assert!((wgpu_color.a - 1.0).abs() < 0.001);
    }

    #[test]
    fn clear_color_from_dark_theme() {
        let theme = Theme::dark();
        let clear_color = RenderPipeline::clear_color_from_theme(&theme);

        // Dark theme should have a dark background
        assert!(clear_color.r < 0.2);
        assert!(clear_color.g < 0.2);
        assert!(clear_color.b < 0.2);
    }

    // Note: Actual GPU initialization tests require hardware
    // and are run as integration tests in tests/integration/

    #[test]
    fn minimap_render_data_hidden() {
        let data = MinimapRenderData::hidden();
        assert!(!data.visible);
        assert_eq!(data.rect_count(), 0);
        assert!(data.collect_rects().is_empty());
        assert!(data.to_vertex_data().is_empty());
    }

    #[test]
    fn minimap_render_data_default() {
        let data = MinimapRenderData::default();
        assert!(!data.visible);
    }

    #[test]
    fn minimap_render_data_with_content() {
        let rect = MinimapRect::new(100.0, 0.0, 120.0, 600.0);
        let color = ThemeColor::new(0.1, 0.1, 0.1, 1.0);
        let content_rect = MinimapRect::new(100.0, 10.0, 50.0, 2.0);
        let content_color = ThemeColor::new(1.0, 0.0, 0.0, 1.0);

        let data = MinimapRenderData {
            background: rect,
            background_color: color,
            content_rects: vec![(content_rect, content_color)],
            viewport_indicator: Some((
                MinimapRect::new(100.0, 0.0, 120.0, 100.0),
                ThemeColor::new(1.0, 1.0, 1.0, 0.15),
            )),
            visible: true,
        };

        // 1 background + 1 content + 1 viewport indicator = 3
        assert_eq!(data.rect_count(), 3);
        assert_eq!(data.collect_rects().len(), 3);
    }

    #[test]
    fn minimap_render_data_vertex_generation() {
        let rect = MinimapRect::new(10.0, 20.0, 30.0, 40.0);
        let color = ThemeColor::new(1.0, 0.0, 0.5, 0.8);

        let data = MinimapRenderData {
            background: rect,
            background_color: color,
            content_rects: Vec::new(),
            viewport_indicator: None,
            visible: true,
        };

        let vertices = data.to_vertex_data();

        // 1 rect * 6 vertices * 6 floats = 36 floats
        assert_eq!(vertices.len(), 36);

        // Verify first vertex (top-left): x, y, r, g, b, a
        assert!((vertices[0] - 10.0).abs() < f32::EPSILON); // x
        assert!((vertices[1] - 20.0).abs() < f32::EPSILON); // y
        assert!((vertices[2] - 1.0).abs() < f32::EPSILON); // r
        assert!((vertices[3] - 0.0).abs() < f32::EPSILON); // g
        assert!((vertices[4] - 0.5).abs() < f32::EPSILON); // b
        assert!((vertices[5] - 0.8).abs() < f32::EPSILON); // a
    }

    #[test]
    fn minimap_render_data_darken_color() {
        let original = ThemeColor::new(0.5, 0.5, 0.5, 1.0);
        let darkened = MinimapRenderData::darken_color(original, 0.2);

        assert!((darkened.r - 0.4).abs() < 0.001);
        assert!((darkened.g - 0.4).abs() < 0.001);
        assert!((darkened.b - 0.4).abs() < 0.001);
        assert!((darkened.a - 1.0).abs() < 0.001); // Alpha unchanged
    }
}
