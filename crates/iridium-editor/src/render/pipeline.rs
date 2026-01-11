//! GPU render pipeline using wgpu.
//!
//! This module handles wgpu device/queue initialization, surface management,
//! and the basic render pass structure for GPU-accelerated rendering.

use super::error::RenderError;
use tracing::{debug, info, warn};

/// Configuration for the render pipeline.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Preferred power mode for GPU selection.
    pub power_preference: wgpu::PowerPreference,
    /// Whether to enable GPU validation layers (debug mode).
    pub enable_validation: bool,
    /// The texture format to use for rendering.
    pub format: Option<wgpu::TextureFormat>,
    /// Maximum texture dimension supported.
    pub max_texture_dimension: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            power_preference: wgpu::PowerPreference::HighPerformance,
            enable_validation: cfg!(debug_assertions),
            format: None,
            max_texture_dimension: 8192,
        }
    }
}

/// GPU resources for rendering.
///
/// Contains the core wgpu resources: device and queue. These are used
/// for all GPU operations including buffer creation, command submission,
/// and rendering.
#[derive(Debug)]
pub struct GpuResources {
    /// The wgpu device for creating GPU resources.
    pub device: wgpu::Device,
    /// The command queue for submitting work to the GPU.
    pub queue: wgpu::Queue,
    /// Information about the selected adapter.
    pub adapter_info: wgpu::AdapterInfo,
}

/// The main render pipeline for the editor.
///
/// Manages GPU resources and provides the infrastructure for rendering
/// text and UI elements using wgpu.
#[derive(Debug)]
pub struct RenderPipeline {
    /// The GPU resources (device, queue).
    gpu: GpuResources,
    /// The current configuration.
    config: RenderConfig,
    /// The current render target dimensions.
    dimensions: (u32, u32),
    /// The texture format being used.
    format: wgpu::TextureFormat,
}

impl RenderPipeline {
    /// Creates a new render pipeline asynchronously.
    ///
    /// This initializes wgpu, selects an appropriate GPU adapter,
    /// and creates the device and queue.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No compatible GPU adapter is found
    /// - Device creation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use iridium_editor::render::RenderPipeline;
    ///
    /// # async fn example() -> Result<(), iridium_editor::RenderError> {
    /// let pipeline = RenderPipeline::new(1920, 1080).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        Self::with_config(width, height, RenderConfig::default()).await
    }

    /// Creates a new render pipeline with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    pub async fn with_config(
        width: u32,
        height: u32,
        config: RenderConfig,
    ) -> Result<Self, RenderError> {
        // Validate dimensions
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions {
                width,
                height,
                max_dimension: config.max_texture_dimension,
            });
        }

        if width > config.max_texture_dimension || height > config.max_texture_dimension {
            return Err(RenderError::InvalidDimensions {
                width,
                height,
                max_dimension: config.max_texture_dimension,
            });
        }

        info!(
            "Initializing render pipeline: {}x{}, power_preference={:?}",
            width, height, config.power_preference
        );

        // Create wgpu instance
        let instance = Self::create_instance(&config);

        // Request adapter
        let adapter = Self::request_adapter(&instance, &config).await?;
        let adapter_info = adapter.get_info();

        info!(
            "Selected GPU adapter: {} ({:?})",
            adapter_info.name, adapter_info.backend
        );

        // Create device and queue
        let (device, queue) = Self::create_device(&adapter, &config).await?;

        // Determine texture format
        let format = config.format.unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        debug!("Using texture format: {:?}", format);

        let gpu = GpuResources {
            device,
            queue,
            adapter_info,
        };

        Ok(Self {
            gpu,
            config,
            dimensions: (width, height),
            format,
        })
    }

    /// Creates the wgpu instance with appropriate backends.
    fn create_instance(config: &RenderConfig) -> wgpu::Instance {
        let backends = wgpu::Backends::all();
        let flags = if config.enable_validation {
            wgpu::InstanceFlags::debugging()
        } else {
            wgpu::InstanceFlags::empty()
        };

        wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            flags,
            ..Default::default()
        })
    }

    /// Requests a GPU adapter matching the configuration.
    async fn request_adapter(
        instance: &wgpu::Instance,
        config: &RenderConfig,
    ) -> Result<wgpu::Adapter, RenderError> {
        instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: config.power_preference,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| {
                warn!(
                    "No adapter found with power_preference={:?}",
                    config.power_preference
                );
                RenderError::adapter_not_found(config.power_preference, false)
            })
    }

    /// Creates the GPU device and command queue.
    async fn create_device(
        adapter: &wgpu::Adapter,
        config: &RenderConfig,
    ) -> Result<(wgpu::Device, wgpu::Queue), RenderError> {
        let limits = wgpu::Limits {
            max_texture_dimension_2d: config.max_texture_dimension,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        };

        let features = wgpu::Features::empty();

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("iridium_device"),
            required_features: features,
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        };

        let result = adapter.request_device(&device_descriptor).await;

        match result {
            Ok((device, queue)) => {
                debug!("GPU device created successfully");
                Ok((device, queue))
            }
            Err(error) => {
                let adapter_info = adapter.get_info();
                Err(RenderError::device_creation_failed(error, &adapter_info))
            }
        }
    }

    /// Returns a reference to the GPU device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    /// Returns a reference to the command queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    /// Returns the adapter information.
    #[must_use]
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.gpu.adapter_info
    }

    /// Returns the current texture format.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Returns the current render target dimensions.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        self.dimensions
    }

    /// Resizes the render target.
    ///
    /// # Errors
    ///
    /// Returns an error if the new dimensions are invalid.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions {
                width,
                height,
                max_dimension: self.config.max_texture_dimension,
            });
        }

        if width > self.config.max_texture_dimension || height > self.config.max_texture_dimension {
            return Err(RenderError::InvalidDimensions {
                width,
                height,
                max_dimension: self.config.max_texture_dimension,
            });
        }

        debug!("Resizing render target to {}x{}", width, height);
        self.dimensions = (width, height);
        Ok(())
    }

    /// Creates a command encoder for recording GPU commands.
    ///
    /// Use this to record render passes and other GPU operations,
    /// then submit via `submit_commands`.
    #[must_use]
    pub fn create_command_encoder(&self, label: &str) -> wgpu::CommandEncoder {
        self.gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) })
    }

    /// Submits command buffers to the GPU for execution.
    pub fn submit_commands(&self, commands: impl IntoIterator<Item = wgpu::CommandBuffer>) {
        self.gpu.queue.submit(commands);
    }

    /// Begins a render pass for rendering to the given texture view.
    ///
    /// This is a convenience method for creating a standard render pass
    /// with a clear color.
    pub fn begin_render_pass<'a>(
        encoder: &'a mut wgpu::CommandEncoder,
        view: &'a wgpu::TextureView,
        clear_color: wgpu::Color,
        label: &'a str,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Creates a render texture that can be used as a render target.
    ///
    /// # Errors
    ///
    /// Returns an error if texture creation fails.
    pub fn create_render_texture(
        &self,
        width: u32,
        height: u32,
        label: &str,
    ) -> Result<wgpu::Texture, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions {
                width,
                height,
                max_dimension: self.config.max_texture_dimension,
            });
        }

        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        Ok(texture)
    }

    /// Creates a buffer for GPU data.
    ///
    /// # Errors
    ///
    /// Returns an error if buffer creation fails.
    pub fn create_buffer(
        &self,
        size: u64,
        usage: wgpu::BufferUsages,
        label: &str,
    ) -> Result<wgpu::Buffer, RenderError> {
        if size == 0 {
            return Err(RenderError::BufferOperationFailed {
                message: "Buffer size must be non-zero".to_string(),
                buffer_name: label.to_string(),
                size,
            });
        }

        let buffer = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        });

        Ok(buffer)
    }

    /// Polls the device for completed work.
    ///
    /// Call this periodically in the main loop to process GPU callbacks.
    pub fn poll(&self) {
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_config_default() {
        let config = RenderConfig::default();
        assert_eq!(
            config.power_preference,
            wgpu::PowerPreference::HighPerformance
        );
        assert_eq!(config.max_texture_dimension, 8192);
    }

    #[test]
    fn test_invalid_dimensions() {
        // Note: We can't test async creation in sync tests, but we can test
        // the validation logic by checking the error type
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { RenderPipeline::new(0, 100).await });

        assert!(matches!(result, Err(RenderError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_dimensions_too_large() {
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { RenderPipeline::new(10000, 100).await });

        assert!(matches!(result, Err(RenderError::InvalidDimensions { .. })));
    }
}
