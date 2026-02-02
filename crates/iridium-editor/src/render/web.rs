//! Browser/WebGPU rendering support.
//!
//! This module provides the bridge between HTML canvas elements and the
//! wgpu render pipeline for browser-based rendering.
//!
//! # Platform Support
//!
//! This module is primarily designed for `wasm32` targets where it provides
//! canvas integration and `requestAnimationFrame` support. Some types are
//! available on all platforms for configuration purposes.

use wgpu::{CompositeAlphaMode, PresentMode, TextureFormat};

/// Configuration for web rendering.
///
/// This configuration type is available on all platforms but is primarily
/// used when creating a [`WebSurface`] on wasm32 targets.
#[derive(Debug, Clone)]
pub struct WebRenderConfig {
    /// Texture format for the surface
    pub format: TextureFormat,
    /// Present mode (Fifo = vsync, Immediate = no vsync)
    pub present_mode: PresentMode,
    /// Alpha mode for compositing
    pub alpha_mode: CompositeAlphaMode,
}

impl Default for WebRenderConfig {
    fn default() -> Self {
        Self {
            // Use non-sRGB format for WebGPU browser compatibility
            // Browsers only support bgra8unorm and rgba8unorm for canvas
            format: TextureFormat::Bgra8Unorm,
            present_mode: PresentMode::Fifo, // vsync by default
            alpha_mode: CompositeAlphaMode::Opaque,
        }
    }
}

// ============================================================================
// WASM32-specific implementation
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::sync::Arc;

    use wgpu::{
        Backends, Device, Instance, InstanceDescriptor, InstanceFlags, Queue, Surface,
        SurfaceConfiguration, SurfaceTarget, TextureUsages,
    };

    use super::WebRenderConfig;
    use crate::editor::IridiumError;

    /// A web surface wrapping an HTML canvas element.
    ///
    /// This provides the connection between the browser's canvas and wgpu's
    /// rendering system. Only available on wasm32 targets.
    pub struct WebSurface {
        surface: Surface<'static>,
        config: SurfaceConfiguration,
        device: Arc<Device>,
        queue: Arc<Queue>,
        width: u32,
        height: u32,
    }

    impl std::fmt::Debug for WebSurface {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WebSurface")
                .field("width", &self.width)
                .field("height", &self.height)
                .field("format", &self.config.format)
                .finish_non_exhaustive()
        }
    }

    impl WebSurface {
        /// Creates a new web surface from a canvas element.
        ///
        /// # Arguments
        ///
        /// * `canvas` - The HTML canvas element to render to
        /// * `width` - Initial width in pixels
        /// * `height` - Initial height in pixels
        ///
        /// # Errors
        ///
        /// Returns an error if WebGPU initialization fails.
        pub async fn from_canvas(
            canvas: web_sys::HtmlCanvasElement,
            width: u32,
            height: u32,
        ) -> Result<Self, IridiumError> {
            Self::from_canvas_with_config(canvas, width, height, WebRenderConfig::default()).await
        }

        /// Creates a new web surface with custom configuration.
        ///
        /// # Errors
        ///
        /// Returns an error if WebGPU initialization fails.
        pub async fn from_canvas_with_config(
            canvas: web_sys::HtmlCanvasElement,
            width: u32,
            height: u32,
            render_config: WebRenderConfig,
        ) -> Result<Self, IridiumError> {
            // Create wgpu instance for WebGPU
            let instance = Instance::new(&InstanceDescriptor {
                backends: Backends::BROWSER_WEBGPU,
                flags: InstanceFlags::default(),
                ..Default::default()
            });

            // Create surface from canvas
            let surface_target = SurfaceTarget::Canvas(canvas);
            let surface = instance.create_surface(surface_target).map_err(|e| {
                IridiumError::GpuInitFailed {
                    message: format!("Failed to create surface from canvas: {e}"),
                }
            })?;

            // Request adapter compatible with the surface
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .map_err(|e| IridiumError::GpuInitFailed {
                    message: format!("Failed to get WebGPU adapter: {e}"),
                })?;

            // Log GPU adapter info for debugging
            let info = adapter.get_info();
            web_sys::console::log_1(
                &format!(
                    "[Iridium] GPU: {} ({:?}) - Driver: {}",
                    info.name,
                    info.backend,
                    info.driver_info
                )
                .into(),
            );

            // Request device
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("Iridium Web Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    ..Default::default()
                })
                .await
                .map_err(|e| IridiumError::GpuInitFailed {
                    message: format!("Failed to create WebGPU device: {e}"),
                })?;

            let device = Arc::new(device);
            let queue = Arc::new(queue);

            // Query surface capabilities to get a supported format
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                // Prefer non-sRGB formats for WebGPU browser compatibility
                .find(|f| {
                    matches!(
                        f,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                    )
                })
                // Fallback to first available or configured format
                .or_else(|| caps.formats.first().copied())
                .unwrap_or(render_config.format);

            // Configure the surface
            let config = SurfaceConfiguration {
                usage: TextureUsages::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: render_config.present_mode,
                alpha_mode: render_config.alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };

            surface.configure(&device, &config);

            Ok(Self {
                surface,
                config,
                device,
                queue,
                width,
                height,
            })
        }

        /// Returns the wgpu device.
        #[must_use]
        pub fn device(&self) -> &Device {
            &self.device
        }

        /// Returns a shared reference to the device.
        #[must_use]
        pub fn device_arc(&self) -> Arc<Device> {
            Arc::clone(&self.device)
        }

        /// Returns the wgpu queue.
        #[must_use]
        pub fn queue(&self) -> &Queue {
            &self.queue
        }

        /// Returns a shared reference to the queue.
        #[must_use]
        pub fn queue_arc(&self) -> Arc<Queue> {
            Arc::clone(&self.queue)
        }

        /// Returns the texture format.
        #[must_use]
        pub const fn format(&self) -> wgpu::TextureFormat {
            self.config.format
        }

        /// Returns the current width.
        #[must_use]
        pub const fn width(&self) -> u32 {
            self.width
        }

        /// Returns the current height.
        #[must_use]
        pub const fn height(&self) -> u32 {
            self.height
        }

        /// Resizes the surface.
        ///
        /// This should be called when the canvas size changes.
        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }

            self.width = width;
            self.height = height;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }

        /// Gets the current frame's texture for rendering.
        ///
        /// # Errors
        ///
        /// Returns an error if the surface texture cannot be acquired.
        pub fn get_current_texture(&self) -> Result<wgpu::SurfaceTexture, IridiumError> {
            self.surface
                .get_current_texture()
                .map_err(|e| IridiumError::GpuInitFailed {
                    message: format!("Failed to get surface texture: {e}"),
                })
        }

        /// Renders a frame using the provided render function.
        ///
        /// This handles acquiring the surface texture, creating a view,
        /// and presenting the result.
        ///
        /// # Arguments
        ///
        /// * `render_fn` - Function that receives the texture view and performs rendering
        ///
        /// # Errors
        ///
        /// Returns an error if rendering fails.
        pub fn render_frame<F>(&self, render_fn: F) -> Result<(), IridiumError>
        where
            F: FnOnce(&wgpu::TextureView, &Device, &Queue) -> Result<(), IridiumError>,
        {
            let output = self.get_current_texture()?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            render_fn(&view, &self.device, &self.queue)?;

            output.present();
            Ok(())
        }
    }

    /// Starts an animation loop using requestAnimationFrame.
    ///
    /// # Arguments
    ///
    /// * `callback` - Function called each frame with the timestamp
    ///
    /// # Note
    ///
    /// The callback receives the DOMHighResTimeStamp from requestAnimationFrame.
    pub fn request_animation_frame(callback: impl FnOnce(f64) + 'static) {
        use wasm_bindgen::prelude::*;

        let window = web_sys::window().expect("no global window");
        let closure = Closure::once_into_js(callback);
        window
            .request_animation_frame(closure.as_ref().unchecked_ref())
            .expect("requestAnimationFrame failed");
    }

    /// Gets the current performance timestamp in milliseconds.
    #[must_use]
    pub fn performance_now() -> f64 {
        web_sys::window()
            .expect("no global window")
            .performance()
            .expect("no performance object")
            .now()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{WebSurface, performance_now, request_animation_frame};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_render_config_default() {
        let config = WebRenderConfig::default();
        // Bgra8Unorm (not sRGB) is used for WebGPU browser compatibility
        assert_eq!(config.format, TextureFormat::Bgra8Unorm);
        assert_eq!(config.present_mode, PresentMode::Fifo);
    }
}
