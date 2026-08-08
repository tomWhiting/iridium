//! The headless device, and the one way these tests fail.

use std::io::Write as _;

use iridium_editor::render::FrameCompositor;

/// The default offscreen frame width.
///
/// Chosen so `WIDTH * 4` is a multiple of wgpu's 256-byte copy-row alignment,
/// which is what lets [`super::frame::read_pixels`] return tightly packed rows
/// with no unpadding step. [`super::frame::target`] asserts the property rather
/// than trusting it, so a test that picks its own width cannot quietly get a
/// buffer full of padding.
pub const WIDTH: u32 = 512;

/// The default offscreen frame height.
pub const HEIGHT: u32 = 384;

/// [`WIDTH`] and [`HEIGHT`] as `usize`, for indexing read-back pixels.
///
/// Derived, never restated: a second literal here would be the same defect
/// this module exists to remove, one axis down.
pub const WIDTH_USIZE: usize = WIDTH as usize;
/// See [`WIDTH_USIZE`].
pub const HEIGHT_USIZE: usize = HEIGHT as usize;

/// [`HEIGHT`] as `f32`, for the scroll clamp's viewport argument.
///
/// Derived rather than written out again, for the same reason as
/// [`HEIGHT_USIZE`]. clippy objects that `u32 as f32` is lossy in general, and
/// it is right in general — `f32` carries 24 bits of mantissa against `u32`'s
/// 32. It is not lossy for this value, and the assertion below is what makes
/// that a fact rather than a claim: if [`HEIGHT`] ever changes to something
/// `f32` cannot hold exactly, this fails to compile.
#[expect(
    clippy::cast_precision_loss,
    reason = "exactness is asserted at compile time immediately below"
)]
pub const HEIGHT_F32: f32 = HEIGHT as f32;

/// Every non-negative integer below 2^24 is exactly representable in `f32`,
/// because that is precisely how many integers its mantissa can name. Stating
/// the condition in integer arithmetic proves the cast above is exact without
/// performing a second, equally lint-worthy cast back.
const _: () = assert!(
    HEIGHT < (1 << f32::MANTISSA_DIGITS),
    "HEIGHT is too large to survive the cast to f32 exactly"
);

/// The face font, so shaping exercises real glyphs and real metrics.
pub static FONT: &[u8] =
    include_bytes!("../../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the faces' base size.
pub const FONT_SIZE: f32 = 14.0;

/// Reports a failure the harness cannot proceed past, and exits non-zero.
///
/// Loud by exit status. A missing GPU must fail the run rather than silently
/// pass over work that never happened — a green test of nothing is worse than
/// a red one.
///
/// `harness` names the test binary, because this exits the process rather than
/// panicking and so never produces a backtrace to identify itself from.
pub fn die(harness: &str, message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{harness}: {message}");
    std::process::exit(1)
}

/// The headless GPU objects one test composes with.
pub struct Gpu {
    /// The device frames are composed with.
    pub device: wgpu::Device,
    /// The queue frames are submitted to.
    pub queue: wgpu::Queue,
    /// The name of the test binary that asked for this device.
    ///
    /// Carried so every label and every diagnostic below still says which
    /// harness produced it, which was free when each file had its own copy of
    /// this code and has to be passed explicitly now that they share one.
    pub harness: &'static str,
}

impl Gpu {
    /// Reports a failure and exits, naming this harness. See [`die`].
    pub fn die(&self, message: &str) -> ! {
        die(self.harness, message)
    }

    /// A label for a GPU object, prefixed with the harness that owns it.
    ///
    /// wgpu labels appear in validation errors and in captures, so a shared
    /// harness that labelled everything the same would make three test
    /// binaries indistinguishable in exactly the place the label is read.
    pub fn label(&self, what: &str) -> String {
        format!("Iridium {} {what}", self.harness)
    }
}

/// Brings up a device with no surface anywhere near it.
///
/// Fails loudly when no adapter or device can be had.
pub fn gpu(harness: &'static str) -> Gpu {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(adapter) => adapter,
        Err(error) => die(
            harness,
            &format!("no headless GPU adapter is available: {error}"),
        ),
    };
    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some(&format!("Iridium {harness} Test Device")),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(error) => die(harness, &format!("the GPU device request failed: {error}")),
        };
    Gpu {
        device,
        queue,
        harness,
    }
}

/// A compositor at the default frame size, with the font loaded.
///
/// Every caller configures it identically — same format, same font size, same
/// font bytes, in the same order — which is what makes frames from different
/// tests comparable at all.
pub fn compositor(gpu: &Gpu) -> FrameCompositor {
    compositor_sized(gpu, WIDTH, HEIGHT)
}

/// A compositor at an explicit frame size. See [`compositor`].
pub fn compositor_sized(gpu: &Gpu, width: u32, height: u32) -> FrameCompositor {
    let mut compositor = match FrameCompositor::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Bgra8Unorm,
        width,
        height,
    ) {
        Ok(compositor) => compositor,
        Err(error) => gpu.die(&format!("the compositor could not be created: {error}")),
    };
    compositor.set_font_size(FONT_SIZE);
    // Asserted rather than ignored: `FONT` is compiled in, so a refusal means
    // the vendored bytes are not a readable face — and every measurement in
    // every test built on this harness would silently fall back to an
    // approximated advance width instead of the real one.
    assert!(
        compositor.load_font(FONT.to_vec()),
        "the vendored test font holds no readable face"
    );
    compositor
}
