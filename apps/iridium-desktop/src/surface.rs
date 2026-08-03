//! The wgpu surface on a native window.
//!
//! [`NativeSurface`] is the native twin of the kernel's
//! `render::web::WebSurface`: the same shape — surface, configuration, device
//! and queue behind one type with a `resize` and a closure-driven
//! `render_frame` — over a winit window instead of a canvas, and
//! `Backends::PRIMARY` instead of `BROWSER_WEBGPU`. The shape matters more
//! than the code shared with the web face (almost none): the
//! [`FrameCompositor`](iridium_editor::render::FrameCompositor) takes a bare
//! texture view plus device, queue and dimensions, and this type is what
//! acquires, exposes and presents them on a window — the one job the plan
//! leaves on the face's side of the seam.
//!
//! Two honest differences from the twin, both stated rather than smoothed
//! over:
//!
//! - **Acquisition can be retried here.** A native swapchain reports
//!   `Lost`/`Outdated` during live resizes; the surface reconfigures and
//!   retries once, which is the standard recovery and impossible to need on a
//!   canvas.
//! - **The format prefers non-sRGB.** Not a browser restriction natively, but
//!   the compositor's colors were tuned against the web face's `Bgra8Unorm`;
//!   choosing the same family keeps the two faces pixel-comparable.

use std::sync::Arc;

use iridium_editor::IridiumError;
use wgpu::{
    Backends, CompositeAlphaMode, Device, Instance, InstanceDescriptor, PresentMode, Queue,
    Surface, SurfaceConfiguration, SurfaceError, TextureFormat, TextureUsages,
};
use winit::window::Window;

/// A wgpu surface wrapping a native window.
///
/// One instance belongs to one window, held by `Arc` so the surface's
/// `'static` lifetime is honest: the window outlives every frame acquired
/// from it.
pub struct NativeSurface {
    /// The configured swapchain surface.
    surface: Surface<'static>,
    /// The live configuration, re-applied on resize and on lost frames.
    config: SurfaceConfiguration,
    /// The device frames are composed with.
    device: Arc<Device>,
    /// The queue frames are submitted to.
    queue: Arc<Queue>,
    /// Current width in physical pixels.
    width: u32,
    /// Current height in physical pixels.
    height: u32,
}

impl std::fmt::Debug for NativeSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSurface")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.config.format)
            .finish_non_exhaustive()
    }
}

impl NativeSurface {
    /// Creates a surface on a window, blocking on adapter and device
    /// acquisition.
    ///
    /// Blocking is deliberate: this runs once, at startup, before there is a
    /// frame to be late for, and `pollster` parking the thread is simpler and
    /// no slower than threading an async runtime through window creation.
    /// Dimensions are physical pixels; zero is clamped to one because a
    /// zero-sized swapchain is a validation error, not a small window.
    ///
    /// # Errors
    ///
    /// Returns an error when no adapter serves the surface or the device
    /// request fails.
    pub fn from_window(window: Arc<Window>, width: u32, height: u32) -> Result<Self, IridiumError> {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|e| IridiumError::GpuInitFailed {
                message: format!("Failed to create surface from window: {e}"),
            })?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| IridiumError::GpuInitFailed {
            message: format!("Failed to get GPU adapter: {e}"),
        })?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Iridium Desktop Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| IridiumError::GpuInitFailed {
            message: format!("Failed to create GPU device: {e}"),
        })?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Prefer the non-sRGB 8-bit formats for parity with the web face; see
        // the module documentation. Every current backend offers one.
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| matches!(f, TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm))
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(TextureFormat::Bgra8Unorm);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            // Vsync. The latency-lean modes are step 3's to measure, not this
            // slice's to guess at.
            present_mode: PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Ok(Self {
            width: config.width,
            height: config.height,
            surface,
            config,
            device,
            queue,
        })
    }

    /// Returns the wgpu device.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Returns the wgpu queue.
    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Returns the texture format the surface was configured with.
    #[must_use]
    pub const fn format(&self) -> TextureFormat {
        self.config.format
    }

    /// Returns the current width in physical pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the current height in physical pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Resizes the surface to new physical pixel dimensions.
    ///
    /// A zero dimension is ignored rather than clamped: it means the window
    /// is minimized or mid-collapse, and the surface keeps its last honest
    /// size until a real one arrives — exactly the twin's behavior.
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

    /// Acquires the current frame, reconfiguring and retrying once when the
    /// swapchain reports itself lost or outdated.
    ///
    /// # Errors
    ///
    /// Returns an error when acquisition fails for any other reason, or twice.
    fn acquire(&self) -> Result<wgpu::SurfaceTexture, IridiumError> {
        match self.surface.get_current_texture() {
            Ok(frame) => Ok(frame),
            Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .map_err(|e| IridiumError::GpuInitFailed {
                        message: format!("Failed to get surface texture after reconfigure: {e}"),
                    })
            },
            Err(e) => Err(IridiumError::GpuInitFailed {
                message: format!("Failed to get surface texture: {e}"),
            }),
        }
    }

    /// Renders a frame through the provided function and presents it.
    ///
    /// Acquisition, view creation and presentation stay here; everything
    /// between them — the compositor call — is the caller's, handed the view,
    /// device and queue exactly as the twin hands them.
    ///
    /// # Errors
    ///
    /// Returns an error when the frame cannot be acquired or the render
    /// function fails; a failed frame is not presented.
    pub fn render_frame<F>(&self, render_fn: F) -> Result<(), IridiumError>
    where
        F: FnOnce(&wgpu::TextureView, &Device, &Queue) -> Result<(), IridiumError>,
    {
        let output = self.acquire()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        render_fn(&view, &self.device, &self.queue)?;

        output.present();
        Ok(())
    }
}
