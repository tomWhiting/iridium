//! The retained-shaping proof harness for [`FrameCompositor::compose`].
//!
//! Two obligations, from `docs/design/RETAINED-SHAPING-MAP.md` §5:
//!
//! - **The staleness matrix.** For every input of the shape key (§2 of the
//!   map): mutating it makes the next compose a rebuild, observable through
//!   `shape_rebuilds()`; and the inputs deliberately *outside* the key — an
//!   identical frame, a sub-line scroll, blink phase, the per-frame
//!   presentation maps — produce hits.
//! - **Pixel identity, hot vs cold.** The cache may only change *when* work
//!   happens, never *what* is produced: a hit frame is byte-identical to
//!   the cold frame before it, and every warm-after-invalidation frame is
//!   byte-identical to a fresh compositor composing the same state cold.
//!   Determinism is arranged, not hoped for — the face font is loaded from
//!   bytes, the metrics are fixed, and the blink is pinned by
//!   `reset_blink()` immediately before every compared compose.
//!
//! Headless by construction, exactly like `benches/compose_frame.rs`: no
//! surface anywhere, an offscreen texture per test, and a missing GPU
//! adapter fails the test loudly rather than passing over work that did
//! not happen.

use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::mpsc;

use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::{Color, Theme};
use iridium_editor::{Editor, KeyCode, KeyEvent, Language, Modifiers, Position};

/// Offscreen frame width in physical pixels. Chosen so `width * 4` is a
/// multiple of wgpu's 256-byte row alignment, which keeps the readback a
/// straight copy with no row padding to strip.
const WIDTH: u32 = 512;

/// Offscreen frame height in physical pixels.
const HEIGHT: u32 = 384;

/// The wider target the resize row composes onto (also 256-byte aligned).
const WIDE_WIDTH: u32 = 768;

/// The same face font the desktop shell, the web demo and the compose bench
/// load, so shaping exercises real glyphs.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the faces' base size.
const FONT_SIZE: f32 = 14.0;

/// A highlight source with a language but no spans, ever — the bridge case:
/// the compositor's built-in keyword fallback colors the content, and the
/// generation is honestly constant because the answer is `None` forever.
struct NoHighlights;

impl HighlightSource for NoHighlights {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        None
    }

    fn language_active(&self) -> bool {
        true
    }

    fn generation(&self) -> u64 {
        0
    }
}

/// A controllable highlight source for the highlight-generation row: `None`
/// selects the fallback, `Some(color)` paints the whole content one color,
/// and the test moves `generation` by hand exactly when it changes `tint` —
/// the contract a real face implements.
struct TintHighlights {
    /// The color painted over the whole content, or `None` for fallback.
    tint: Option<Color>,
    /// The reported generation.
    generation: u64,
}

impl HighlightSource for TintHighlights {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        self.tint.map(|color| vec![(context.content, color)])
    }

    fn language_active(&self) -> bool {
        true
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// A source standing in for a face whose language is set or cleared at
/// runtime: `active` is the [`HighlightSource::language_active`] answer,
/// spans never arrive, and the generation is honestly constant — the
/// compositor keys the language answer directly, so the flip alone must
/// carry the invalidation.
struct ToggleLanguage {
    /// Whether a language is currently set.
    active: bool,
}

impl HighlightSource for ToggleLanguage {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        None
    }

    fn language_active(&self) -> bool {
        self.active
    }

    fn generation(&self) -> u64 {
        0
    }
}

/// The headless GPU objects one test composes with.
struct Gpu {
    /// The device frames are composed with.
    device: wgpu::Device,
    /// The queue frames are submitted to.
    queue: wgpu::Queue,
}

/// One offscreen render target, standing in for a swapchain frame.
struct Target {
    /// The texture, held so the view stays valid and the readback can copy
    /// from it.
    texture: wgpu::Texture,
    /// The render target view handed to the compositor.
    view: wgpu::TextureView,
    /// Width in physical pixels.
    width: u32,
    /// Height in physical pixels.
    height: u32,
}

/// Reports a failure the harness cannot proceed past and exits non-zero —
/// loud by exit status, exactly the compose bench's pattern. A missing GPU
/// must fail the run rather than silently pass over work that did not
/// happen.
fn die(message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "retained_shaping: {message}");
    std::process::exit(1)
}

/// Brings up a device with no surface anywhere near it. Fails loudly when
/// no adapter or device can be had — a silently green test of nothing would
/// be worse than a red one.
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
            label: Some("Iridium Retained Shaping Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(error) => die(&format!("the GPU device request failed: {error}")),
        };
    Gpu { device, queue }
}

/// An offscreen render target of the given pixel size.
fn target(gpu: &Gpu, width: u32, height: u32) -> Target {
    assert_eq!(
        (width * 4) % 256,
        0,
        "target widths are chosen so readback rows need no padding"
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Retained Shaping Target"),
        size: wgpu::Extent3d {
            width,
            height,
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
    Target {
        texture,
        view,
        width,
        height,
    }
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

/// One compose with the blink pinned, plus the wait for its GPU work.
fn compose(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    scroll_y: f32,
    highlights: &mut dyn HighlightSource,
    gpu: &Gpu,
    tgt: &Target,
) {
    compositor.reset_blink();
    if let Err(error) = compositor.compose(
        editor,
        editor.fold_state(),
        scroll_y,
        highlights,
        FrameTarget {
            view: &tgt.view,
            device: &gpu.device,
            queue: &gpu.queue,
            width: tgt.width,
            height: tgt.height,
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
    let bytes_per_row = tgt.width * 4;
    let size = u64::from(bytes_per_row) * u64::from(tgt.height);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Iridium Retained Shaping Readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Iridium Retained Shaping Readback Encoder"),
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
            width: tgt.width,
            height: tgt.height,
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

/// Composes the given editor state cold on a *fresh* compositor (configured
/// by `configure` after the standard font setup) and returns its pixels —
/// the oracle every warm-after-invalidation frame is compared against.
fn cold_pixels(
    gpu: &Gpu,
    tgt: &Target,
    editor: &Editor,
    scroll_y: f32,
    highlights: &mut dyn HighlightSource,
    configure: impl FnOnce(&mut FrameCompositor),
) -> Vec<u8> {
    let mut fresh = compositor(gpu);
    configure(&mut fresh);
    compose(&mut fresh, editor, scroll_y, highlights, gpu, tgt);
    assert_eq!(
        fresh.shape_rebuilds(),
        1,
        "a fresh compositor composes cold"
    );
    pixels(gpu, tgt)
}

/// A deterministic document of numbered single-line functions.
fn document(lines: usize) -> String {
    let mut source = String::with_capacity(lines * 40);
    for line in 0..lines {
        let _ = writeln!(source, "fn item_{line}() {{ let value = {line}; }}");
    }
    source
}

/// An editor over [`document`] of the given line count, cursor at the
/// origin.
fn editor_over(lines: usize) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(&document(lines));
    editor.set_cursor(Position::new(0, 0));
    editor
}

/// A plain key press, the kernel's normal edit path.
const fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

// =============================================================================
// The hit side of the matrix
// =============================================================================

/// An identical frame, a sub-line scroll, a blink-phase frame and every
/// per-frame presentation input are hits — and the hit frame's pixels are
/// byte-identical to the cold frame before it (§5.3a).
#[test]
fn steady_frames_hit_and_reproduce_the_cold_frame_exactly() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(compositor.shape_rebuilds(), 1, "the first frame is cold");
    let cold = pixels(&gpu, &tgt);

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        compositor.shape_rebuilds(),
        1,
        "an identical frame is a hit"
    );
    let hot = pixels(&gpu, &tgt);
    assert!(
        hot == cold,
        "the hit frame must be byte-identical to the cold one"
    );

    // A sub-line scroll: the viewport line range is unchanged, only the
    // remainder applied at the text area's top edge moves.
    let line_height = compositor.line_height();
    compose(
        &mut compositor,
        &editor,
        0.4 * line_height,
        &mut highlights,
        &gpu,
        &tgt,
    );
    assert_eq!(compositor.shape_rebuilds(), 1, "a sub-line scroll is a hit");

    // Presentation inputs feed per-frame quads and blame, never the shaped
    // buffers — they must not invalidate.
    compositor
        .line_backgrounds_mut()
        .insert(2, Color::new(0.2, 0.4, 0.2, 1.0));
    compositor
        .gutter_changes_mut()
        .insert(3, Color::new(0.8, 0.6, 0.1, 1.0));
    compositor
        .blame_data_mut()
        .insert(0, "author, yesterday".to_string());
    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        compositor.shape_rebuilds(),
        1,
        "presentation inputs must not invalidate the shapes"
    );

    // A frame later in the blink cycle: blink is compose-internal state,
    // not a shaping input.
    std::thread::sleep(std::time::Duration::from_millis(600));
    compositor
        .compose(
            &editor,
            editor.fold_state(),
            0.0,
            &mut highlights,
            FrameTarget {
                view: &tgt.view,
                device: &gpu.device,
                queue: &gpu.queue,
                width: tgt.width,
                height: tgt.height,
            },
        )
        .expect("the blink frame must compose");
    assert_eq!(compositor.shape_rebuilds(), 1, "a blink frame is a hit");
    assert_eq!(
        compositor.lines_reshaped(),
        0,
        "no frame here went through the diff path"
    );
}

/// The composed frame stands on the theme's own background: the dark
/// preset must present opaque `#1a1a1a` — the pixels the web demo shows
/// behind its canvas — on a surface with no compositing behind it.
#[test]
fn the_dark_frame_background_is_opaque_1a1a1a() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let frame = pixels(&gpu, &tgt);
    // The bottom-right corner pixel: right of the gutter column and every
    // glyph, under no quad — the clear color alone.
    let corner = (((HEIGHT - 1) * WIDTH + (WIDTH - 1)) * 4) as usize;
    assert_eq!(
        &frame[corner..corner + 4],
        &[26, 26, 26, 255],
        "the dark background must present as opaque #1a1a1a (Bgra8Unorm bytes)"
    );
}

/// The empty document is the degenerate content path (one empty buffer
/// line): it must compose, hit, and reproduce itself exactly.
#[test]
fn an_empty_document_composes_and_hits() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content("");
    let mut highlights = NoHighlights;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let cold = pixels(&gpu, &tgt);
    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(compositor.shape_rebuilds(), 1, "the empty frame hits too");
    let hot = pixels(&gpu, &tgt);
    assert!(hot == cold, "the empty hit frame must be byte-identical");
}

// =============================================================================
// The miss side: stage 2a, the typing path
// =============================================================================

/// A one-character keystroke is a miss served by per-line diffing: exactly
/// one line reshapes, and the diffed frame is byte-identical to a fresh
/// compositor composing the edited document cold (§5.4, §5.3b).
#[test]
fn a_one_character_edit_reshapes_one_line_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    assert_eq!(warm.lines_reshaped(), 0, "the cold frame is a full build");

    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "an edit is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        1,
        "a one-character edit reshapes exactly one line"
    );
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the edit must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "the diffed frame must be byte-identical to a cold compose of the same state"
    );

    // A second consecutive keystroke diffs against a diffed buffer — the
    // construction must be self-consistent, not merely consistent with the
    // full build once.
    let _ = editor.handle_key(&press(KeyCode::Char('y')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        warm.lines_reshaped(),
        2,
        "the second keystroke reshapes one more"
    );
    let second = pixels(&gpu, &tgt);
    let second_cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        second == second_cold,
        "diff-after-diff must stay byte-identical"
    );
}

/// The plain-text arm (syntax highlighting off) diffs through
/// `set_text_diffed`, whose construction must match `Buffer::set_text`'s.
#[test]
fn a_plain_text_edit_diffs_one_line_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    warm.set_syntax_enabled(false);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the edit misses");
    assert_eq!(warm.lines_reshaped(), 1, "and diffs exactly one line");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_syntax_enabled(false);
    });
    assert!(
        after == cold,
        "the plain-text diffed frame must be byte-identical"
    );
}

/// Multibyte content takes cosmic-text's full bidi analysis path when the
/// diffed setter splits lines — the construction must stay byte-identical
/// there too, not only for the ASCII fast path.
#[test]
fn a_multibyte_edit_diffs_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    let mut content = document(40);
    content.push_str("fn greet() { let s = \"héllo 🦀 ẑ\"; }\n");
    content.push_str(&document(40));
    editor.set_content(&content);
    editor.set_cursor(Position::new(40, 13));
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the edit misses");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "the multibyte diffed frame must be byte-identical"
    );
}

/// An edit that inserts a line shifts everything below it in the buffer —
/// the diff must extend/truncate correctly and stay pixel-identical.
#[test]
fn an_edit_that_adds_a_line_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 0));
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Enter));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the newline is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "the line-inserting diff must be byte-identical"
    );
}

/// An edit that makes a line wrap changes the wrap counts and with them the
/// gutter's continuation rows — the diffed gutter must keep up.
#[test]
fn an_edit_that_changes_wrapping_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    editor.paste("wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap");
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the paste is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "the wrap-changing diff must be byte-identical"
    );
}

// =============================================================================
// The miss side: every other key input, each against the cold oracle
// =============================================================================

/// A scroll that crosses a line boundary changes the viewport range: a full
/// miss (stage 2b is out of scope), never the diff path.
#[test]
fn a_line_scroll_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;
    let line_height = warm.line_height();
    // Mid-line anchored so `floor(scroll / line_height)` is robust against
    // f32 rounding on both sides of the one-line hop.
    let base = 50.25 * line_height;

    compose(&mut warm, &editor, base, &mut highlights, &gpu, &tgt);
    compose(
        &mut warm,
        &editor,
        base + line_height,
        &mut highlights,
        &gpu,
        &tgt,
    );
    assert_eq!(warm.shape_rebuilds(), 2, "a one-line scroll is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        0,
        "a viewport shift is a full rebuild, not a diff"
    );
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        base + line_height,
        &mut NoHighlights,
        |_| {},
    );
    assert!(after == cold, "the scrolled frame must be byte-identical");
}

/// A surface resize changes the wrap width.
#[test]
fn a_resize_misses_and_recomposes_identically() {
    let gpu = gpu();
    let narrow = target(&gpu, WIDTH, HEIGHT);
    let wide = target(&gpu, WIDE_WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &narrow);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &wide);
    assert_eq!(warm.shape_rebuilds(), 2, "a resize is a miss");
    let after = pixels(&gpu, &wide);

    let cold = cold_pixels(&gpu, &wide, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(after == cold, "the resized frame must be byte-identical");
}

/// Loading font data can change how `Family::Monospace` resolves.
#[test]
fn loading_a_font_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    warm.load_font(FONT.to_vec());
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a font load is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.load_font(FONT.to_vec());
    });
    assert!(after == cold, "the post-load frame must be byte-identical");
}

/// The font size is a shaping metric.
#[test]
fn a_font_size_change_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_font_size(16.0);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a font size change is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the size change must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_font_size(16.0);
    });
    assert!(
        after == cold,
        "the resized-font frame must be byte-identical"
    );
}

/// The theme feeds text colors and the fallback highlighter's palette.
#[test]
fn a_theme_flip_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_dark_theme(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a theme flip is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the theme flip must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_dark_theme(false);
    });
    assert!(
        after == cold,
        "the light-theme frame must be byte-identical"
    );
}

/// The staleness row for `set_theme`, the arbitrary-theme mutator beside
/// `set_dark_theme`: replacing the theme must miss — a `set_theme` that
/// skipped the generation bump would serve stale-colored retained frames —
/// and the frame must both present the new background and be byte-identical
/// to a cold compositor holding the same theme.
#[test]
fn a_set_theme_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;
    let mut theme = Theme::dark();
    // #336699: every channel an exact multiple of 1/255, so the readback
    // bytes are exact.
    theme.editor.background = Color::new(0.2, 0.4, 0.6, 1.0);

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_theme(theme.clone());
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a theme replacement is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the new background must be visible");

    // The clear color follows the set theme, not a frozen preset.
    let corner = (((HEIGHT - 1) * WIDTH + (WIDTH - 1)) * 4) as usize;
    assert_eq!(
        &after[corner..corner + 4],
        &[153, 102, 51, 255],
        "the frame must stand on the set theme's background (Bgra8Unorm bytes)"
    );

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_theme(theme.clone());
    });
    assert!(after == cold, "the set-theme frame must be byte-identical");
}

/// Toggling syntax highlighting switches the whole fill path.
#[test]
fn a_syntax_toggle_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_syntax_enabled(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a syntax toggle is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "losing the keyword colors must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_syntax_enabled(false);
    });
    assert!(after == cold, "the plain frame must be byte-identical");
}

/// The no-language ruling at the compositor's own seam: a source that
/// reports no language and no spans composes the plain frame — byte-identical
/// to the syntax-disabled fill path, and visibly not the keyword fallback.
#[test]
fn a_language_less_source_composes_the_plain_frame() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut void = ToggleLanguage { active: false };

    compose(&mut compositor, &editor, 0.0, &mut void, &gpu, &tgt);
    let composed = pixels(&gpu, &tgt);

    let plain = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_syntax_enabled(false);
    });
    assert!(
        composed == plain,
        "no language must render the plain frame, never the keyword fallback"
    );

    let bridged = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        composed != bridged,
        "and the comparison discriminates: the bridge frame is coloured"
    );
}

/// The bridge half of the ruling, pinned: a language set with spans absent
/// this frame still gets the built-in keyword fallback's colors.
#[test]
fn a_set_language_without_spans_keeps_the_keyword_fallback() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut bridge = ToggleLanguage { active: true };

    compose(&mut compositor, &editor, 0.0, &mut bridge, &gpu, &tgt);
    let bridged = pixels(&gpu, &tgt);

    let fallback = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        bridged == fallback,
        "a language-active source with no spans is exactly the fallback frame"
    );

    let plain = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_syntax_enabled(false);
    });
    assert!(
        bridged != plain,
        "and the fallback really coloured: the bridge frame is not the plain one"
    );
}

/// The staleness-matrix row for the language answer: a language set or
/// unset at runtime — with every other input, the generation included,
/// unchanged — must miss, and both directions must recompose exactly what a
/// cold compositor produces for the same state.
#[test]
fn a_language_set_or_unset_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut source = ToggleLanguage { active: true };

    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 1, "the first frame is cold");
    let bridged = pixels(&gpu, &tgt);

    source.active = false;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "unsetting the language is a miss");
    let unlanguaged = pixels(&gpu, &tgt);
    assert!(
        unlanguaged != bridged,
        "losing the language must lose the keyword colors"
    );
    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ToggleLanguage { active: false },
        |_| {},
    );
    assert!(
        unlanguaged == cold,
        "the language-less frame must be byte-identical to a cold compose"
    );

    source.active = true;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 3, "setting it back is a miss too");
    let rebridged = pixels(&gpu, &tgt);
    assert!(
        rebridged == bridged,
        "the returned language must reproduce the fallback frame exactly"
    );
}

/// The face's highlight generation is a key member: bumping it (with a
/// genuinely different answer) recolors, and the recolored frame matches a
/// cold compose against the same source.
#[test]
fn a_highlight_generation_bump_misses_and_recolors_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let tint = Color::new(0.1, 0.8, 0.3, 1.0);
    let mut source = TintHighlights {
        tint: None,
        generation: 0,
    };

    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    source.tint = Some(tint);
    source.generation = 1;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a generation bump is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the new spans must be visible");

    let mut cold_source = TintHighlights {
        tint: Some(tint),
        generation: 7,
    };
    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut cold_source, |_| {});
    assert!(after == cold, "the recolored frame must be byte-identical");
}

/// The raw `syntax_theme_mut` accessor bumps its generation on every
/// mutable borrow — the borrow is the change signal.
#[test]
fn a_syntax_theme_borrow_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = warm.syntax_theme_mut();
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        warm.shape_rebuilds(),
        2,
        "a mutable borrow of the theme map is a miss by contract"
    );
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "an unchanged map still recomposes identically"
    );
}

/// The sharpest trap in the input set: folding hides lines without moving
/// the document revision. The fold generation must carry the miss, and the
/// folded frame must match a cold compose of the folded state.
#[test]
fn a_fold_toggle_misses_despite_an_unchanged_document_revision() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(
        "fn folded() {\n    one();\n    two();\n    three();\n}\nfn after() {\n    tail();\n}\n",
    );
    editor.set_language(Language::Rust);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    let revision_before = editor.state().document.revision();
    assert!(editor.fold_at(0), "the function must be foldable");
    assert_eq!(
        editor.state().document.revision(),
        revision_before,
        "folding must not touch the document revision — that is the trap"
    );

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the fold is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        0,
        "a fold change is a full rebuild — the line↔buffer correspondence moved"
    );
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the fold must be visible");

    let mut cold_editor = Editor::with_defaults();
    cold_editor.set_content(
        "fn folded() {\n    one();\n    two();\n    three();\n}\nfn after() {\n    tail();\n}\n",
    );
    cold_editor.set_language(Language::Rust);
    assert!(cold_editor.fold_at(0));
    let cold = cold_pixels(&gpu, &tgt, &cold_editor, 0.0, &mut NoHighlights, |_| {});
    assert!(after == cold, "the folded frame must be byte-identical");
}

/// Custom gutter text changes the gutter's text and (through its measured
/// width) the content column.
#[test]
fn custom_gutter_lines_miss_and_recompose_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;
    let custom: Vec<String> = (0..201).map(|line| format!("+{line}")).collect();

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_custom_gutter_lines(Some(custom.clone()));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "custom gutter text is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the custom gutter must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_custom_gutter_lines(Some(custom.clone()));
    });
    assert!(
        after == cold,
        "the custom-gutter frame must be byte-identical"
    );
}

/// Toggling the gutter changes both buffers' geometry.
#[test]
fn a_gutter_toggle_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_gutter_enabled(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a gutter toggle is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the missing gutter must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |fresh| {
        fresh.set_gutter_enabled(false);
    });
    assert!(after == cold, "the gutterless frame must be byte-identical");
}

/// The 999→1000 digit rollover widens the gutter through an ordinary edit —
/// the whole chain (revision, gutter width, content width) must land on the
/// same pixels a cold compose produces.
#[test]
fn the_digit_rollover_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(998); // 998 lines + the trailing line = 999
    let mut highlights = NoHighlights;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    // Append a line: 999 → 1000 total lines, three digits become four.
    let last_line = editor.state().document.line_count() - 1;
    editor.set_cursor(Position::new(last_line, 0));
    let _ = editor.handle_key(&press(KeyCode::Enter));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the rollover edit is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the widened gutter must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut NoHighlights, |_| {});
    assert!(
        after == cold,
        "the rolled-over frame must be byte-identical"
    );
}
