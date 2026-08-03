//! Frame-time benchmarks for [`FrameCompositor::compose`] on a 10k-line
//! document — the compose-path half of the desktop shell plan's step 3
//! (`docs/DESKTOP-SHELL-PLAN.md`), where the 120fps / sub-8ms claim gets
//! measured rather than asserted.
//!
//! # Headless, by construction
//!
//! No window ever opens. The adapter is requested with no compatible
//! surface (`Backends::PRIMARY` answers with Metal headlessly on macOS)
//! and every frame is composed onto an offscreen `Bgra8Unorm` texture of a
//! realistic window's dimensions, handed to the compositor through the same
//! [`FrameTarget`] a surface's swapchain view would fill. When no adapter
//! or device can be had, the bench exits loudly with a failure status —
//! a silently green benchmark of nothing would be worse than a red one.
//!
//! # What one iteration spans
//!
//! Each timed iteration runs `compose` — viewport extraction, shaping, the
//! wrap readback, quad building, glyph preparation, the render pass and its
//! queue submit — and then blocks until the GPU has finished the submitted
//! work, so a sample is the whole cost of producing the frame's pixels, not
//! just of encoding the commands. Presentation is a swapchain's business
//! and has no headless equivalent; it is the one step outside the number.
//!
//! # The three cases
//!
//! - **`steady_state`**: compose repeatedly with nothing changed — the pure
//!   redraw cost, what a caret blink or an overlay repaint pays.
//! - **`after_mid_file_edit`**: each iteration types one character at the
//!   middle of the document through the kernel's normal key path and then
//!   composes; the compensating backspace runs outside the timed region
//!   (via `iter_custom`, this bench's equivalent of `iter_batched`'s
//!   untimed setup), so the document is the same 10k lines at every
//!   iteration's start.
//! - **`after_scroll_change`**: each iteration composes at a scroll offset
//!   a screenful away from the previous one, forcing the viewport
//!   extraction and visual-line-map rebuild a real scroll pays.
//!
//! # Highlighting is the built-in fallback
//!
//! The [`HighlightSource`] resolves `None` every frame, so the compositor's
//! own keyword highlighter colors the content — the same path a face
//! without a tree-sitter worker gets. No language is set on the editor:
//! tree-sitter's parse cost is `benches/syntax.rs`'s subject, and mixing it
//! in here would blur whose number this is.

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::Color;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers, Position};

/// Offscreen frame width in physical pixels — a realistic window, not a
/// thumbnail.
const WIDTH: u32 = 1512;

/// Offscreen frame height in physical pixels.
const HEIGHT: u32 = 982;

/// The same face font the desktop shell and the web demo load, so glyph
/// shaping measures real glyphs.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the faces' base size.
const FONT_SIZE: f32 = 14.0;

/// How many ten-line blocks the generated document repeats.
const BLOCKS: usize = 1_000;

/// The document line the mid-file edit lands on.
const MID_LINE: usize = 5_000;

/// Reports a failure this bench cannot proceed past and exits non-zero —
/// loud by exit status, never a silent pass over work that did not happen.
fn die(message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "compose_frame bench: {message}");
    std::process::exit(1)
}

/// A highlight source with nothing to offer, selecting the compositor's
/// built-in keyword fallback — see the module documentation.
struct NoHighlights;

impl HighlightSource for NoHighlights {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        None
    }
}

/// The headless GPU objects every case composes onto.
struct Gpu {
    /// The device frames are composed with.
    device: wgpu::Device,
    /// The queue frames are submitted to.
    queue: wgpu::Queue,
    /// The offscreen texture standing in for a swapchain frame. Held so the
    /// view below stays valid for the whole run.
    _texture: wgpu::Texture,
    /// The render target view handed to the compositor.
    view: wgpu::TextureView,
}

/// Brings up a device with no surface anywhere near it and an offscreen
/// render target of a realistic window's size.
fn headless_gpu() -> Gpu {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        // No surface: this is what keeps the bench provably windowless.
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Ok(adapter) => adapter,
        Err(error) => die(&format!("no headless GPU adapter is available: {error}")),
    };

    let (device, queue) =
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Iridium Compose Bench Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(error) => die(&format!("the GPU device request failed: {error}")),
        };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Compose Bench Target"),
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

    Gpu {
        device,
        queue,
        _texture: texture,
        view,
    }
}

/// A deterministic 10,000-line Rust-like document: [`BLOCKS`] repetitions
/// of a ten-line function, varied only by an index, so every run shapes and
/// highlights identical content.
fn ten_thousand_lines() -> String {
    let mut source = String::with_capacity(BLOCKS * 320);
    for block in 0..BLOCKS {
        let _ = write!(
            source,
            "/// Applies request {block} against the shared store.\n\
             pub fn handle_request_{block}(store: &mut Store) -> Result<Response, Error> {{\n\
             \x20   let key = EntryKey::new({block});\n\
             \x20   let entry = store.entry(&key).ok_or(Error::Missing)?;\n\
             \x20   if entry.revision > {block} {{\n\
             \x20       return Err(Error::Stale);\n\
             \x20   }}\n\
             \x20   let response = Response::from(entry.value.clone());\n\
             \x20   Ok(response)\n\
             }}\n"
        );
    }
    source
}

/// One compose call plus the wait for its GPU work — the timed unit every
/// case shares.
fn compose_once(compositor: &mut FrameCompositor, editor: &Editor, scroll_y: f32, gpu: &Gpu) {
    let mut highlights = NoHighlights;
    if let Err(error) = compositor.compose(
        editor,
        editor.fold_state(),
        scroll_y,
        &mut highlights,
        FrameTarget {
            view: &gpu.view,
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

/// A plain key press, the kernel's normal edit path.
const fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

fn compose_benchmark(c: &mut Criterion) {
    let gpu = headless_gpu();

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

    let mut editor = Editor::with_defaults();
    editor.set_content(&ten_thousand_lines());
    editor.set_cursor(Position::new(MID_LINE, 0));

    // A screen-centered scroll offset over the mid-file cursor, so every
    // case composes a fully populated viewport with the caret in it.
    let line_height = compositor.line_height();
    let mid_scroll = 5_000.0_f32.mul_add(line_height, -491.0).max(0.0);
    // A whole viewport's hop, what a PageDown-sized scroll change costs.
    let scroll_hop = 982.0_f32;

    let mut group = c.benchmark_group("compose_frame");

    // (a) The steady-state redraw: nothing changed since the last frame.
    group.bench_function("steady_state", |b| {
        b.iter(|| compose_once(&mut compositor, &editor, mid_scroll, &gpu));
    });

    // (b) The frame after a one-character edit at the middle of the file.
    // The timed region is the keystroke plus its frame; the compensating
    // backspace runs off the clock so every iteration edits the same
    // document.
    group.bench_function("after_mid_file_edit", |b| {
        b.iter_custom(|iterations| {
            let mut total = Duration::ZERO;
            for _ in 0..iterations {
                let start = Instant::now();
                let _ = editor.handle_key(&press(KeyCode::Char('x')));
                compose_once(&mut compositor, &editor, mid_scroll, &gpu);
                total += start.elapsed();
                // Untimed: put the document back for the next iteration.
                let _ = editor.handle_key(&press(KeyCode::Backspace));
            }
            total
        });
    });

    // (c) The frame after a scroll change: every iteration lands a
    // screenful away from the last, so the viewport extraction and the
    // visual line map rebuild against fresh lines each time.
    group.bench_function("after_scroll_change", |b| {
        let mut hopped = false;
        b.iter(|| {
            hopped = !hopped;
            let scroll_y = if hopped {
                mid_scroll + scroll_hop
            } else {
                mid_scroll
            };
            compose_once(&mut compositor, &editor, scroll_y, &gpu);
        });
    });

    group.finish();
}

criterion_group!(benches, compose_benchmark);
criterion_main!(benches);
