//! The no-language ruling, proven at this face's own seam: a document with
//! no language set renders the plain frame — uniform foreground text, no
//! keyword colours — and setting a language is what turns colour on.
//!
//! This is the defect found live: a `.txt` opened in the desktop app showed
//! Rust-ish keyword colours, because the compositor's built-in keyword
//! fallback fired whenever the resolver had nothing, including when there
//! was no language at all. The fallback's one legitimate job is the bridge —
//! a language *is* set but its spans are not available this frame.
//!
//! Headless by construction, exactly like `chrome_screenshots`: no surface,
//! no window, an offscreen texture per test, pixels read back and compared
//! byte-for-byte against the plain frame a syntax-disabled compositor
//! composes — the ruling's definition of "uniform text colour".

use std::io::Write as _;
use std::sync::mpsc;

use iridium_desktop::highlight::HighlightCache;
use iridium_editor::render::{FrameCompositor, FrameTarget};
use iridium_editor::{Editor, Language};

/// Offscreen frame width in physical pixels — `width * 4` is a multiple of
/// wgpu's 256-byte row alignment, so the readback needs no padding strip.
const WIDTH: u32 = 512;

/// Offscreen frame height in physical pixels.
const HEIGHT: u32 = 384;

/// The same face font the desktop shell embeds.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the face's base size.
const FONT_SIZE: f32 = 14.0;

/// The headless GPU objects one test composes with.
struct Gpu {
    /// The device frames are composed with.
    device: wgpu::Device,
    /// The queue frames are submitted to.
    queue: wgpu::Queue,
}

/// One offscreen render target, standing in for a swapchain frame.
struct Target {
    /// The texture, held so the view stays valid for readback.
    texture: wgpu::Texture,
    /// The render target view handed to the compositor.
    view: wgpu::TextureView,
}

/// Reports a failure the harness cannot proceed past and exits non-zero —
/// loud by exit status. A missing GPU must fail the run rather than silently
/// pass over work that did not happen.
fn die(message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "plain_frames: {message}");
    std::process::exit(1)
}

/// Brings up a device with no surface anywhere near it.
fn gpu() -> Gpu {
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
        Err(error) => die(&format!("no headless GPU adapter is available: {error}")),
    };
    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Iridium Plain Frames Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(error) => die(&format!("the GPU device request failed: {error}")),
        };
    Gpu { device, queue }
}

/// An offscreen render target at the test dimensions.
fn target(gpu: &Gpu) -> Target {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Plain Frames Target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Target { texture, view }
}

/// A compositor configured exactly as every comparand in these tests is:
/// same format, same font size, same font bytes, in the same order.
fn compositor(gpu: &Gpu) -> FrameCompositor {
    let mut compositor = match FrameCompositor::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Bgra8Unorm,
        WIDTH,
        HEIGHT,
    ) {
        Ok(compositor) => compositor,
        Err(error) => die(&format!("the compositor could not be created: {error}")),
    };
    compositor.set_font_size(FONT_SIZE);
    compositor.load_font(FONT.to_vec());
    compositor
}

/// One compose through the desktop face's own resolver, blink pinned, GPU
/// work waited for — the per-frame assembly `App::redraw` performs, minus
/// the window.
fn compose(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    cache: &HighlightCache,
    gpu: &Gpu,
    tgt: &Target,
) {
    let mut highlights = cache.resolver(&editor.state().theme.syntax);
    compositor.reset_blink();
    if let Err(error) = compositor.compose(
        editor,
        editor.fold_state(),
        0.0,
        &mut highlights,
        FrameTarget {
            view: &tgt.view,
            device: &gpu.device,
            queue: &gpu.queue,
            width: WIDTH,
            height: HEIGHT,
        },
    ) {
        die(&format!("compose failed: {error}"));
    }
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        die(&format!("waiting for the GPU failed: {error}"));
    }
}

/// Reads the target's pixels back as bytes.
fn pixels(gpu: &Gpu, tgt: &Target) -> Vec<u8> {
    let bytes_per_row = WIDTH * 4;
    let size = u64::from(bytes_per_row) * u64::from(HEIGHT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Iridium Plain Frames Readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Iridium Plain Frames Readback Encoder"),
        });
    encoder.copy_texture_to_buffer(
        tgt.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        die(&format!("waiting for the readback failed: {error}"));
    }
    match receiver.recv() {
        Ok(Ok(())) => {},
        Ok(Err(error)) => die(&format!("mapping the readback buffer failed: {error}")),
        Err(_) => die("the map callback never ran"),
    }
    let data = slice.get_mapped_range().to_vec();
    buffer.unmap();
    data
}

/// A document that is plain prose but full of the keyword fallback's bait —
/// `fn`, `let`, quoted phrases, numbers, `//` — so a frame that ran the
/// keyword highlighter cannot match the plain frame.
const PLAIN_NOTES: &str = "notes from tonight\n\
    the fn keys stick and let me tell you why\n\
    \"quoted phrase\" and 42 numbers // not a comment\n\
    if while for return match struct impl use mod\n\
    plain text files have no grammar and want no colours\n";

/// The plain frame the ruling demands: what a syntax-disabled compositor
/// composes for the same state — every glyph in the theme foreground.
fn plain_oracle(gpu: &Gpu, tgt: &Target, editor: &Editor, cache: &HighlightCache) -> Vec<u8> {
    let mut plain = compositor(gpu);
    plain.set_syntax_enabled(false);
    compose(&mut plain, editor, cache, gpu, tgt);
    pixels(gpu, tgt)
}

/// The live defect, reproduced at the seam: a no-language document composed
/// through the desktop face's resolver must be byte-identical to the plain
/// frame — the built-in keyword fallback must not colour it.
#[test]
fn a_document_without_a_language_renders_the_plain_frame() {
    let gpu = gpu();
    let tgt = target(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(PLAIN_NOTES);
    let mut cache = HighlightCache::new();
    cache.refresh(&editor);

    let mut lit = compositor(&gpu);
    compose(&mut lit, &editor, &cache, &gpu, &tgt);
    let composed = pixels(&gpu, &tgt);

    let plain = plain_oracle(&gpu, &tgt, &editor, &cache);
    assert!(
        composed == plain,
        "a no-language document must render the plain frame — \
         the keyword fallback coloured text that has no language"
    );
}

/// Language set and unset at runtime, on one warm compositor: colour must
/// arrive with the language and leave with it, ending byte-identical to the
/// plain frame it started as.
#[test]
fn setting_and_clearing_a_language_recolours_and_returns_to_plain() {
    let source = "fn main() { let answer = \"forty-two\"; }\n// a comment\nfn other() {}\n";
    let gpu = gpu();
    let tgt = target(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(source);
    let mut cache = HighlightCache::new();
    cache.refresh(&editor);

    let mut warm = compositor(&gpu);
    compose(&mut warm, &editor, &cache, &gpu, &tgt);
    let unlanguaged = pixels(&gpu, &tgt);
    let plain = plain_oracle(&gpu, &tgt, &editor, &cache);
    assert!(
        unlanguaged == plain,
        "before a language is set the frame must be plain"
    );

    editor.set_language(Language::Rust);
    cache.refresh(&editor);
    compose(&mut warm, &editor, &cache, &gpu, &tgt);
    let coloured = pixels(&gpu, &tgt);
    assert!(
        coloured != unlanguaged,
        "setting a language must recolour the retained frame"
    );

    editor.state_mut().syntax.clear_language();
    cache.refresh(&editor);
    compose(&mut warm, &editor, &cache, &gpu, &tgt);
    let cleared = pixels(&gpu, &tgt);
    assert!(
        cleared == unlanguaged,
        "clearing the language must return the warm compositor to the plain frame"
    );
}
