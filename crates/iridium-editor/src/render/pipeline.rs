//! GPU render pipeline setup and management.
//!
//! This module handles wgpu device initialization, render pass creation,
//! and frame rendering for the editor.

use std::sync::Arc;

use wgpu::{
    Adapter, Backends, Color, CommandEncoder, Device, DeviceDescriptor, Features, Instance,
    InstanceDescriptor, InstanceFlags, Limits, LoadOp, Operations, PowerPreference, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp,
    TextureFormat, TextureView, Trace,
};

use crate::editor::IridiumError;

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
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver.clone(),
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
            }
        };

        // Collect GPU info for diagnostics
        let gpu_info = GpuInfo::from_adapter(&adapter);

        // Step 3: Request device and queue
        let device_descriptor = DeviceDescriptor {
            label: Some("Iridium Editor Device"),
            required_features: config.required_features,
            required_limits: config.limits.clone(),
            memory_hints: Default::default(),
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
    async fn create_adapter_error(
        instance: &Instance,
        error: wgpu::RequestAdapterError,
    ) -> IridiumError {
        let adapters: Vec<Adapter> = instance.enumerate_adapters(Backends::all()).await;

        let message = if adapters.is_empty() {
            format!(
                "No GPU adapters found: {}\n\n\
                 WebGPU/wgpu requires a compatible GPU.\n\
                 Please ensure your system has:\n\
                 - A GPU with Vulkan, Metal, DX12, or WebGPU support\n\
                 - Up-to-date graphics drivers installed\n\
                 - For browsers: WebGPU-capable browser (Chrome 113+, Firefox 121+)",
                error
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
                "No suitable GPU adapter found: {}\n\n\
                 Available adapters:\n{}\n\n\
                 Try adjusting power preference or required features.",
                error, adapter_list
            )
        };

        IridiumError::GpuInitFailed { message }
    }

    /// Creates a detailed error message when device request fails.
    fn create_device_error(
        adapter: &Adapter,
        gpu_info: &GpuInfo,
        config: &RenderConfig,
        error: wgpu::RequestDeviceError,
    ) -> IridiumError {
        let adapter_limits = adapter.limits();

        let message = format!(
            "Failed to create GPU device: {}\n\n\
             GPU: {} ({}, {})\n\
             Driver: {}\n\n\
             Requested limits vs adapter limits:\n\
             - max_texture_dimension_2d: {} vs {}\n\
             - max_bind_groups: {} vs {}\n\
             - max_uniform_buffer_binding_size: {} vs {}\n\n\
             Try using more compatible limits (e.g., Limits::downlevel_webgl2_defaults()).",
            error,
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
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // Note: Actual GPU initialization tests require hardware
    // and are run as integration tests in tests/integration/
}
