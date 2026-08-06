//! The proof that reserving space beside the document moves *everything*.
//!
//! A face that draws chrome down the left of its window — a sidebar, a file
//! tree — pushes the document across by calling
//! [`FrameCompositor::set_left_inset`]. Six separate things then have to
//! agree about where the left edge is: the gutter's background, the line
//! numbers, the change bars, the content column, and both directions of
//! hit-testing.
//!
//! **The failure this harness exists for** is the one the vertical axis
//! already suffered. The line numbers were written from the horizontal
//! padding while the code was written from the vertical inset; the two were
//! the same number on every frame this editor had ever drawn, so nothing
//! caught it until a tab strip moved one and not the other. Every X-axis
//! measure in the compositor started life at "the window's left edge" and
//! agrees with "the reserved edge" for exactly as long as nothing is
//! reserved.
//!
//! So the load-bearing test here is not a query — it is a pixel. The gutter
//! and the change bars have no placement query at all; they are quads and a
//! text area, and a defect that lives only in an origin is invisible to
//! every assertion that does not look at the frame.
//!
//! Headless by construction, like `top_inset.rs`: no surface, an offscreen
//! texture, and a missing adapter fails the run loudly rather than passing
//! over work that never happened.

use std::io::Write as _;

use iridium_editor::Editor;
use iridium_editor::render::units::{index_to_f32, pixel_to_index};
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::{Color, Theme};

/// Offscreen frame width, chosen so `width * 4` is a multiple of wgpu's
/// 256-byte row alignment.
const WIDTH: u32 = 512;

/// Offscreen frame height.
const HEIGHT: u32 = 384;

/// The frame's dimensions as `usize`, for indexing the read-back pixels.
const WIDTH_USIZE: usize = 512;
const HEIGHT_USIZE: usize = 384;

/// The face font, so shaping exercises real glyphs and real metrics.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the faces' base size.
const FONT_SIZE: f32 = 14.0;

/// The width a sidebar would claim down the left of the window.
///
/// Wide enough that a missed inset is a whole character cell out rather than
/// a rounding argument, and narrow enough to leave the document a usable
/// column in a 512-pixel frame.
const SIDEBAR_WIDTH: f32 = 96.0;

/// A highlight source that never has spans — the colour of the text is not
/// what is under test.
struct NoHighlights;

impl HighlightSource for NoHighlights {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, Color)>> {
        None
    }

    fn language_active(&self) -> bool {
        false
    }

    fn generation(&self) -> u64 {
        0
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

/// A silently green test of nothing is worse than a red one.
fn die(message: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "left_inset: {message}");
    std::process::exit(1)
}

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
            label: Some("Iridium Left Inset Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(error) => die(&format!("the GPU device request failed: {error}")),
        };
    Gpu { device, queue }
}

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

/// The stock dark theme paints the gutter the same colour as the page, so a
/// gutter background left at the window's edge would be *invisible* to a
/// pixel test. This makes it vivid, so the quad has to be where it claims.
fn loud_gutter_theme() -> Theme {
    let mut theme = Theme::dark();
    theme.editor.gutter = Color::new(0.85, 0.20, 0.65, 1.0);
    theme
}

/// A document of numbered lines, long enough to scroll and short enough that
/// no line reaches the right of the frame.
fn editor() -> Editor {
    let mut editor = Editor::with_defaults();
    let text = (0..60)
        .map(|line| format!("line {line} of the document"))
        .collect::<Vec<_>>()
        .join("\n");
    editor.set_content(&text);
    editor
}

/// One compose onto an offscreen texture, with the GPU work awaited so the
/// between-frames caches this test reads are populated.
fn compose(compositor: &mut FrameCompositor, editor: &Editor, gpu: &Gpu) {
    let _ = compose_to_texture(compositor, editor, gpu);
}

/// The same compose, keeping the texture so its pixels can be read back.
fn compose_to_texture(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    gpu: &Gpu,
) -> wgpu::Texture {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Left Inset Target"),
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
    let mut highlights = NoHighlights;
    if let Err(error) = compositor.compose(
        editor,
        editor.fold_state(),
        0.0,
        &mut highlights,
        FrameTarget {
            view: &view,
            device: &gpu.device,
            queue: &gpu.queue,
            width: WIDTH,
            height: HEIGHT,
        },
    ) {
        die(&format!("the frame did not compose: {error}"));
    }
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        die(&format!("the frame's GPU work did not complete: {error}"));
    }
    texture
}

/// The composed frame's pixels, row-major BGRA.
///
/// [`WIDTH`] is chosen so `WIDTH * 4` is already a multiple of wgpu's 256-byte
/// copy alignment, so the rows come back tightly packed and need no unpadding.
fn read_pixels(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<u8> {
    let row_bytes = WIDTH * 4;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Iridium Left Inset Readback"),
        size: u64::from(row_bytes * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Iridium Left Inset Readback Encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));

    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    if let Err(error) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
        die(&format!("the readback did not complete: {error}"));
    }
    let pixels = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    pixels
}

/// The page colour, sampled from the frame's bottom-right corner.
///
/// Not the top-left, which is the corner this file is about: under a left
/// inset it is the first pixel of the reserved band, and a compositor that
/// wrongly painted the gutter there would make the band's own colour the
/// reference and hide the defect. The bottom-right is past the end of every
/// line in [`editor`] and is page on every frame here.
fn page(pixels: &[u8]) -> [u8; 3] {
    let offset = ((HEIGHT_USIZE - 1) * WIDTH_USIZE + (WIDTH_USIZE - 1)) * 4;
    [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
}

/// How far a channel must move from the page colour to count as ink.
const INK: i32 = 24;

/// Whether the pixel at `(column, row)` differs from the page.
fn inked(pixels: &[u8], page: [u8; 3], column: usize, row: usize) -> bool {
    let offset = (row * WIDTH_USIZE + column) * 4;
    (0..3)
        .any(|channel| (i32::from(pixels[offset + channel]) - i32::from(page[channel])).abs() > INK)
}

/// The first column of the frame carrying ink anywhere down its height, or
/// `None` for a frame that drew nothing at all.
fn first_inked_column(pixels: &[u8]) -> Option<usize> {
    let page = page(pixels);
    (0..WIDTH_USIZE).find(|&column| (0..HEIGHT_USIZE).any(|row| inked(pixels, page, column, row)))
}

/// **The round trip.** Ask the compositor where a column is drawn, click
/// there, and get the same column back.
///
/// This catches a forgotten inset on either side of the hit test, because
/// the two directions read it independently. Checked at several columns,
/// since an error of less than half a character hides at column zero.
#[test]
fn a_click_where_a_column_is_drawn_resolves_to_that_column_under_an_inset() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_left_inset(SIDEBAR_WIDTH);
    let editor = editor();
    compose(&mut compositor, &editor, &gpu);

    for column in [0_usize, 1, 7, 14, 20] {
        let Some((x, y)) =
            compositor.position_to_pixel(&editor, editor.fold_state(), 0.0, 3, column)
        else {
            die("line 3 was not placed under an inset");
        };
        let middle = y + compositor.line_height() / 2.0;
        let (line, resolved) =
            compositor.pixel_to_position(&editor, editor.fold_state(), 0.0, x, middle);

        assert_eq!(line, 3, "a click at x={x} resolved to line {line}, not 3");
        assert_eq!(
            resolved, column,
            "a click at x={x} resolved to column {resolved}, not {column}"
        );
    }
}

/// The inset actually moves the content column, rather than being stored and
/// ignored.
///
/// Without this the round trip above would pass on a compositor that applied
/// the inset in neither direction — two wrongs agreeing is the shape of
/// failure this file is about.
#[test]
fn reserving_space_moves_the_content_column_across_by_that_much() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, &gpu);
    let Some((without, _)) = plain.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0)
    else {
        die("the first column was not placed without an inset");
    };

    let mut inset = compositor(&gpu);
    inset.set_left_inset(SIDEBAR_WIDTH);
    compose(&mut inset, &editor, &gpu);
    let Some((with, _)) = inset.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0) else {
        die("the first column was not placed under an inset");
    };

    assert!(
        (with - without - SIDEBAR_WIDTH).abs() < 0.5,
        "the content column moved by {} pixels, not {SIDEBAR_WIDTH}",
        with - without
    );
}

/// **Nothing the document draws reaches into the reserved band.**
///
/// The load-bearing test. Six X-axis measures in the compositor were written
/// from the window's left edge — the gutter's background quad, the line
/// numbers' origin and both of their clip bounds, the change bars, and the
/// content text area's left bound. Every one of them agrees with the
/// reserved edge while nothing is reserved, which is every frame drawn
/// before this existed.
///
/// So the frame is arranged so that each of them would *show*: a vivid
/// gutter colour, because the stock dark theme paints the gutter the same
/// colour as the page and an unmoved quad would be invisible; and a change
/// bar, because the bars are three pixels wide at a hardcoded offset and
/// nothing else would reveal them. Then the claim is one sentence — the
/// reserved band is empty — and it holds against all six at once.
#[test]
fn nothing_the_document_draws_reaches_into_the_reserved_band() {
    let gpu = gpu();
    let editor = editor();

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    for line in 0..12 {
        inset
            .gutter_changes_mut()
            .insert(line, Color::new(0.20, 0.85, 0.45, 1.0));
    }
    let frame = compose_to_texture(&mut inset, &editor, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page(&pixels);

    let band = pixel_to_index(SIDEBAR_WIDTH).min(WIDTH_USIZE);
    for column in 0..band {
        for row in 0..HEIGHT_USIZE {
            assert!(
                !inked(&pixels, page, column, row),
                "the document drew at ({column}, {row}), inside the {SIDEBAR_WIDTH}-pixel band \
                 the face reserved"
            );
        }
    }
}

/// The band is empty because the document moved, not because it stopped
/// drawing.
///
/// The test above is satisfied by a compositor that draws nothing at all.
/// This one pins the other side: the leftmost ink in the frame is exactly a
/// sidebar's width further right than it was without one.
#[test]
fn the_gutter_moves_across_with_the_content_it_sits_beside() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    plain.set_theme(loud_gutter_theme());
    let plain_frame = compose_to_texture(&mut plain, &editor, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    let inset_frame = compose_to_texture(&mut inset, &editor, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let Some(before) = first_inked_column(&plain_pixels) else {
        die("the frame drew nothing without an inset");
    };
    let Some(after) = first_inked_column(&inset_pixels) else {
        die("the frame drew nothing under an inset");
    };

    let moved = index_to_f32(after) - index_to_f32(before);
    assert!(
        (moved - SIDEBAR_WIDTH).abs() <= 2.0,
        "the leftmost thing the document draws moved {moved} pixels, not {SIDEBAR_WIDTH}"
    );
}

/// **Everything right of the band is the same pixels it was, moved across.**
///
/// A pure translation is a fair claim here and only here: the lines in
/// [`editor`] are far shorter than the content column at either inset, so
/// nothing wraps differently, and the inset is a whole number of pixels, so
/// nothing rasterizes differently either. Comparing the frames outright is
/// then stricter than any eye, and covers every glyph rather than the ones
/// someone thought to look at.
///
/// **What it deliberately does not claim** is that the content's left clip
/// is in the right place. Both frames clip relative to their own content
/// column, so a bound set uniformly too far right shaves both by the same
/// amount and the translation survives intact — two wrongs agreeing, the
/// same shape of failure this whole file is about. Verified: a twelve-pixel
/// over-tight bound passes this test untouched. The absolute claim is
/// [`the_content_starts_where_the_compositor_says_it_does`], which is why
/// that test exists separately.
#[test]
fn the_content_column_is_the_same_pixels_a_sidebars_width_across() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    plain.set_theme(loud_gutter_theme());
    let plain_frame = compose_to_texture(&mut plain, &editor, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    let inset_frame = compose_to_texture(&mut inset, &editor, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let shift = pixel_to_index(SIDEBAR_WIDTH);
    let start = pixel_to_index(plain.content_left_edge(60));
    if start + shift >= WIDTH_USIZE {
        die("the frame is too narrow for this comparison to mean anything");
    }

    let page = page(&plain_pixels);
    let mut compared = 0_usize;
    for row in 0..HEIGHT_USIZE {
        for column in start..(WIDTH_USIZE - shift) {
            let here = (row * WIDTH_USIZE + column) * 4;
            let there = (row * WIDTH_USIZE + column + shift) * 4;
            assert_eq!(
                plain_pixels[here..here + 3],
                inset_pixels[there..there + 3],
                "the content pixel at ({column}, {row}) is not the one that \
                 landed {SIDEBAR_WIDTH} pixels further right under an inset"
            );
            if inked(&plain_pixels, page, column, row) {
                compared += 1;
            }
        }
    }

    assert!(
        compared > 1_000,
        "only {compared} inked pixels were compared; this test proves nothing"
    );
}

/// **The content's first glyph is where the compositor says the column
/// begins**, under an inset and without one.
///
/// The absolute claim the translation test cannot make. The content text
/// area's left bound moved from the window's edge to the gutter's, and a
/// bound set too far right slices the first column of glyphs — invisible to
/// every relative assertion, because the ink that survives has still moved
/// by exactly the right amount.
///
/// The tolerance is one character cell, and it is a real limit rather than
/// a hedge. A glyph's ink starts a small bearing inside its cell, and that
/// bearing is a fact about the font, not about this compositor; a bound
/// wrong by less than it removes nothing at all, so there is nothing there
/// to catch. A cell is enough to catch the mistake anyone would actually
/// make — clipping at the padding, or at the content column plus it.
///
/// **It reads one line's rows, not the whole frame**, and that is the whole
/// difficulty. The caret is a *quad*, and quads are not subject to a text
/// area's bounds at all; it is drawn at the content column's left edge, so
/// a frame whose text had been clipped away entirely still had ink exactly
/// where this test looks for it. Scanning the full height measured the
/// caret and called it the text — a proxy agreeing with its target on
/// everything examined. Line three has glyphs and no caret.
#[test]
fn the_content_starts_where_the_compositor_says_it_does() {
    /// A line far enough down to be clear of the caret's row, and well
    /// inside the composed viewport.
    const PROBE_LINE: usize = 3;

    let gpu = gpu();
    let editor = editor();

    for inset in [0.0, SIDEBAR_WIDTH] {
        let mut compositor = compositor(&gpu);
        compositor.set_theme(loud_gutter_theme());
        compositor.set_left_inset(inset);
        let frame = compose_to_texture(&mut compositor, &editor, &gpu);
        let pixels = read_pixels(&gpu, &frame);
        let page = page(&pixels);

        let Some((_, top)) =
            compositor.position_to_pixel(&editor, editor.fold_state(), 0.0, PROBE_LINE, 0)
        else {
            die("the probe line was not placed");
        };
        let rows =
            pixel_to_index(top)..pixel_to_index(top + compositor.line_height()).min(HEIGHT_USIZE);
        if rows.is_empty() {
            die("the probe line's rows fall outside the frame");
        }

        let edge = compositor.content_left_edge(60);
        let first = pixel_to_index(edge).min(WIDTH_USIZE);
        let cell = compositor.char_width();

        // Not `die` here: a probe line that draws nothing is the defect this
        // test is for — a clip that swallowed the whole line — and it has to
        // be reported as a failing assertion rather than as the harness
        // giving up, or the run reports an aborted process instead of a
        // named test.
        let ink = (first..WIDTH_USIZE)
            .find(|&column| rows.clone().any(|row| inked(&pixels, page, column, row)));
        assert!(
            ink.is_some(),
            "under an inset of {inset} the content's line {PROBE_LINE} drew nothing at all \
             right of {edge}"
        );
        let Some(ink) = ink else { return };

        let gap = index_to_f32(ink) - edge;
        assert!(
            gap < cell,
            "under an inset of {inset} the content's first ink is at {ink}, {gap} pixels \
             right of the column edge the compositor reports at {edge} — more than the \
             {cell}-pixel cell it should start inside"
        );
    }
}

/// The default reserves nothing.
///
/// Every face that draws no chrome beside the text — the web face, the
/// desktop face today, the screenshot harness, the benches — must be
/// pixel-identical to before this existed.
#[test]
fn the_default_left_inset_reserves_nothing() {
    let gpu = gpu();
    let compositor = compositor(&gpu);

    assert!(compositor.left_inset().abs() < f32::EPSILON);
}

/// A nonsense inset is refused rather than stored.
///
/// Negative would draw the gutter off the left of the window, and NaN would
/// poison every comparison downstream of it. Both are rejected at the setter
/// so no consumer has to guard.
#[test]
fn a_nonsense_left_inset_leaves_the_previous_one_in_place() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_left_inset(SIDEBAR_WIDTH);

    compositor.set_left_inset(-1.0);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);

    compositor.set_left_inset(f32::NAN);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);

    compositor.set_left_inset(f32::INFINITY);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);
}
