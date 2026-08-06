//! The proof that reserving space above the document moves *everything*.
//!
//! A face that draws chrome at the top of its window — the desktop face's
//! tab strip — pushes the document down by calling
//! [`FrameCompositor::set_top_inset`]. Four separate things then have to
//! agree about where the document starts: the painter, the hit test, the
//! scroll-to-caret anchor, and the scroll clamp.
//!
//! **The failure this harness exists for**: an offset applied in the painter
//! but not in the hit test agrees with the truth exactly at the top of the
//! document, where the error is under half a row, and is a full row out
//! everywhere else. Someone clicks a word and the caret lands on the line
//! above it — reliably, but only once the strip is up, so it looks like a
//! tab bug rather than a layout one.
//!
//! Headless by construction, like `retained_shaping.rs`: no surface, an
//! offscreen texture, and a missing adapter fails the run loudly rather than
//! passing over work that never happened.

use std::io::Write as _;

use iridium_editor::Editor;
use iridium_editor::render::units::{index_to_f32, pixel_to_index};
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::Color;

/// Offscreen frame width, chosen so `width * 4` is a multiple of wgpu's
/// 256-byte row alignment.
const WIDTH: u32 = 512;

/// Offscreen frame height.
const HEIGHT: u32 = 384;

/// The same height as a float, for the scroll clamp's viewport argument.
const HEIGHT_F32: f32 = 384.0;

/// The frame's dimensions as `usize`, for indexing the read-back pixels.
const WIDTH_USIZE: usize = 512;
const HEIGHT_USIZE: usize = 384;

/// The face font, so shaping exercises real glyphs and real metrics.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// Font size in pixels, matching the faces' base size.
const FONT_SIZE: f32 = 14.0;

/// The height a tab strip would claim, on top of the document's own ten
/// pixels of breathing room.
const STRIP_HEIGHT: f32 = 34.0;

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
    let _ = writeln!(std::io::stderr(), "top_inset: {message}");
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
            label: Some("Iridium Top Inset Test Device"),
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

/// A document of numbered lines, long enough to scroll.
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
fn compose(compositor: &mut FrameCompositor, editor: &Editor, scroll_y: f32, gpu: &Gpu) {
    let _ = compose_to_texture(compositor, editor, scroll_y, gpu);
}

/// The same compose, keeping the texture so its pixels can be read back.
fn compose_to_texture(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    scroll_y: f32,
    gpu: &Gpu,
) -> wgpu::Texture {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Top Inset Target"),
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
        scroll_y,
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
        label: Some("Iridium Top Inset Readback"),
        size: u64::from(row_bytes * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Iridium Top Inset Readback Encoder"),
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

/// The first row of the frame carrying ink in the column range `[left,
/// right)`, or `None` for a column that is empty top to bottom.
///
/// "Ink" is any pixel that differs from the frame's own top-left corner —
/// which is always page, since the document starts at the inset — by more
/// than a threshold well above antialiasing noise. Comparing two frames of
/// the same document under the same threshold makes the *difference* the
/// claim, so the exact threshold buys nothing and costs nothing.
fn first_inked_row(pixels: &[u8], left: f32, right: f32) -> Option<usize> {
    /// How far a channel must move from the page colour to count as ink.
    const INK: i32 = 24;

    let page = [pixels[0], pixels[1], pixels[2]];
    let first = pixel_to_index(left.max(0.0)).min(WIDTH_USIZE);
    let last = pixel_to_index(right.max(0.0)).min(WIDTH_USIZE);
    for row in 0..HEIGHT_USIZE {
        for column in first..last {
            let offset = (row * WIDTH_USIZE + column) * 4;
            let inked = (0..3).any(|channel| {
                (i32::from(pixels[offset + channel]) - i32::from(page[channel])).abs() > INK
            });
            if inked {
                return Some(row);
            }
        }
    }
    None
}

/// **The round trip.** Ask the compositor where a line is drawn, click
/// there, and get the same line back.
///
/// This is the assertion that catches a forgotten inset on either side,
/// because the two directions read it independently. It is checked at
/// several lines and at a non-zero scroll, since a constant error of less
/// than one row hides at the top of an unscrolled document.
#[test]
fn a_click_where_a_line_is_drawn_resolves_to_that_line_under_an_inset() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(10.0 + STRIP_HEIGHT);
    let editor = editor();

    for scroll_y in [0.0, 47.0, 123.0] {
        compose(&mut compositor, &editor, scroll_y, &gpu);

        for line in [0_usize, 3, 9, 14] {
            let Some((x, y)) =
                compositor.position_to_pixel(&editor, editor.fold_state(), scroll_y, line, 2)
            else {
                continue;
            };
            if y < compositor.top_inset() {
                // Scrolled above the reserved band: there is nothing to
                // click, which is the point of reserving it.
                continue;
            }
            // Half a row down from the top edge of the row, which is where
            // a click on that line's text actually lands.
            let middle = y + compositor.line_height() / 2.0;
            let (resolved, _) =
                compositor.pixel_to_position(&editor, editor.fold_state(), scroll_y, x, middle);

            assert_eq!(
                resolved, line,
                "a click at y={middle} (scroll {scroll_y}) resolved to line {resolved}, not {line}"
            );
        }
    }
}

/// The inset actually moves the document, rather than being stored and
/// ignored.
///
/// Without this the round trip above would pass on a compositor that
/// applied the inset in neither direction — two wrongs agreeing is exactly
/// the shape of failure this file is about.
#[test]
fn reserving_space_moves_the_first_line_down_by_that_much() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, 0.0, &gpu);
    let Some((_, without)) = plain.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0)
    else {
        die("the first line was not placed without an inset");
    };

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    compose(&mut inset, &editor, 0.0, &gpu);
    let Some((_, with)) = inset.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0) else {
        die("the first line was not placed with an inset");
    };

    assert!(
        (with - without - STRIP_HEIGHT).abs() < 0.5,
        "the first line moved by {} pixels, not {STRIP_HEIGHT}",
        with - without
    );
}

/// **Everything the document draws moves down together — the gutter too.**
///
/// The line numbers are a *second* text area with its own Y origin, and it was
/// written from the horizontal padding rather than from the inset. The two
/// agreed exactly while both were ten, which is every frame this editor had
/// ever drawn; the moment a face reserved a tab strip, the numbers stayed put
/// while the code they number moved down by the strip's height, and every
/// line was labelled with the number of the line above it.
///
/// This is read off the composed pixels rather than off a placement query,
/// because the gutter has no placement query — it is prepared straight into a
/// text area — and a defect that only exists in a text area's origin is
/// invisible to every other test in this file.
#[test]
fn the_gutter_moves_down_with_the_text_it_numbers() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    let plain_frame = compose_to_texture(&mut plain, &editor, 0.0, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    let inset_frame = compose_to_texture(&mut inset, &editor, 0.0, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let gutter = plain.gutter_width(60);
    assert!(gutter > 0.0, "the gutter is off; this test proves nothing");

    // The two columns that must move as one: the numbers, and the code.
    for (name, left, right) in [
        ("the gutter", 0.0, gutter),
        ("the content", gutter + 12.0, 512.0),
    ] {
        let Some(before) = first_inked_row(&plain_pixels, left, right) else {
            die(&format!("{name} column drew nothing without an inset"));
        };
        let Some(after) = first_inked_row(&inset_pixels, left, right) else {
            die(&format!("{name} column drew nothing under an inset"));
        };
        let moved = index_to_f32(after) - index_to_f32(before);
        assert!(
            (moved - STRIP_HEIGHT).abs() <= 2.0,
            "{name} moved {moved} pixels, not {STRIP_HEIGHT}"
        );
    }
}

/// **The inline blame ghost text is clipped at the band too.**
///
/// The third text area, and the one that kept the window's edge as its top
/// bound after the content and the gutter were moved onto the inset. It was
/// missed because blame is populated only from the web face, which draws no
/// chrome above the document — so the two disagreed on every frame anyone
/// had composed, and would have started disagreeing visibly the day a
/// TypeScript tab strip landed.
///
/// Blame sits on the caret's line, and the caret can be scrolled up behind
/// the reserved band; with the window edge as the bound, its ghost text is
/// then drawn *inside* a face's chrome.
///
/// The claim is made against a second frame rather than against an empty
/// band, because the band is not empty: the caret and the line-background
/// quads are geometry, not text, and a text area's bounds do not clip them
/// at all. Composing the same scroll with and without blame data isolates
/// exactly the ink this test is about.
#[test]
fn the_blame_ghost_text_stays_out_of_the_reserved_band() {
    /// A scroll that leaves the caret's line inside the band rather than
    /// above the window: `top_inset` minus this is where its text lands.
    const INTO_THE_BAND: f32 = 14.0;

    let gpu = gpu();
    let editor = editor();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut bare = compositor(&gpu);
    bare.set_top_inset(inset);
    let bare_frame = compose_to_texture(&mut bare, &editor, INTO_THE_BAND, &gpu);
    let bare_pixels = read_pixels(&gpu, &bare_frame);

    let mut blamed = compositor(&gpu);
    blamed.set_top_inset(inset);
    blamed
        .blame_data_mut()
        .insert(0, "committed by someone, a while ago".to_owned());
    let blamed_frame = compose_to_texture(&mut blamed, &editor, INTO_THE_BAND, &gpu);
    let blamed_pixels = read_pixels(&gpu, &blamed_frame);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    assert!(band > 0, "there is no band; this test proves nothing");

    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            let at = (row * WIDTH_USIZE + column) * 4;
            assert_eq!(
                bare_pixels[at..at + 3],
                blamed_pixels[at..at + 3],
                "the blame ghost text drew at ({column}, {row}), inside the {inset}-pixel \
                 band the face reserved"
            );
        }
    }
}

/// The scroll clamp grows with the inset, so the last line can still be
/// reached.
///
/// A clamp that ignored it would stop scrolling `STRIP_HEIGHT` pixels early
/// and leave the final line permanently behind the chrome — visible only to
/// someone who scrolled all the way down, which is the last thing anyone
/// tests by hand.
#[test]
fn the_scroll_clamp_still_reaches_the_last_line() {
    let gpu = gpu();
    let editor = editor();
    let height = HEIGHT_F32;

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, 0.0, &gpu);
    let without = plain.max_scroll_y(&editor, editor.fold_state(), height);

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    compose(&mut inset, &editor, 0.0, &gpu);
    let with = inset.max_scroll_y(&editor, editor.fold_state(), height);

    assert!(
        (with - without - STRIP_HEIGHT).abs() < 0.5,
        "the clamp moved by {} pixels, not {STRIP_HEIGHT}",
        with - without
    );
}

/// The default is the document's own breathing room, unchanged.
///
/// Every face that draws no chrome above the text — the web face, the
/// screenshot harness, the benches — must be pixel-identical to before this
/// existed.
#[test]
fn the_default_inset_is_the_documents_own_padding() {
    let gpu = gpu();
    let compositor = compositor(&gpu);

    assert!((compositor.top_inset() - 10.0).abs() < f32::EPSILON);
}

/// A nonsense inset is refused rather than stored.
///
/// Negative would draw the document above the top of the window, and NaN
/// would poison every comparison downstream of it. Both are rejected at the
/// setter so no consumer has to guard.
#[test]
fn a_nonsense_inset_leaves_the_previous_one_in_place() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(44.0);

    compositor.set_top_inset(-1.0);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);

    compositor.set_top_inset(f32::NAN);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);

    compositor.set_top_inset(f32::INFINITY);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);
}

/// The page colour, sampled past the end of every line at the bottom right.
///
/// **Not** the top-left, which is the corner this test is about: under a top
/// inset it is the first pixel of the reserved band, and a compositor that
/// wrongly painted there would make the band's own colour the reference and
/// hide the very defect being tested. Every line in [`editor`] is far shorter
/// than the frame is wide, so the bottom-right pixel is page on every frame
/// here.
fn page_colour(pixels: &[u8]) -> [u8; 3] {
    let offset = ((HEIGHT_USIZE - 1) * WIDTH_USIZE + (WIDTH_USIZE - 1)) * 4;
    [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
}

/// The stock dark theme paints the gutter the same colour as the page, so a
/// gutter background quad drawn inside the band would be *invisible* to a
/// pixel test. This makes it vivid, so the quad has to be where it claims.
fn loud_gutter_theme() -> iridium_editor::theme::Theme {
    let mut theme = iridium_editor::theme::Theme::dark();
    theme.editor.gutter = Color::new(0.85, 0.20, 0.65, 1.0);
    theme
}

/// Nothing the document draws reaches into the reserved band — **at a
/// scroll**, which is the case the X axis has no analogue for.
///
/// `left_inset.rs` already asserts the same sentence for the left band, and
/// it holds there because nothing scrolls sideways: every X coordinate is
/// computed once, from the inset, and either uses it or does not. The Y axis
/// is different. A row's Y is `top_inset + row - scroll`, so a row that
/// started below the band *moves into it* as the document scrolls, and the
/// question stops being "was the inset applied?" and becomes "is the band
/// clipped?".
///
/// Those are different questions with different answers. The text is clipped
/// — `TextBounds` carries `top: top_inset` on the content, the gutter and the
/// blame ghost. **`TextBounds` clips text, not geometry**, and every quad the
/// compositor emits is geometry: the gutter's background, its change bars,
/// the diff line backgrounds, the selection highlights and the caret. Each
/// one tests its own visibility against `0.0` — the window's top edge — which
/// is the same number as the band's bottom edge for exactly as long as no
/// face reserves anything.
///
/// The frame below is arranged so that all five would show: a vivid gutter
/// colour, change bars, a line background, a multi-line selection, and a
/// caret, all on lines the scroll has carried up into the band.
///
/// On the desktop face this is masked today, because the overlay paints the
/// tab strip opaquely over the band in a second pass. It is masked only for
/// as long as that chrome stays opaque and full-width — and rounded corners
/// alone would expose it.
#[test]
fn nothing_the_document_draws_reaches_into_the_reserved_band_at_a_scroll() {
    let gpu = gpu();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut compositor = compositor(&gpu);
    compositor.set_theme(loud_gutter_theme());
    compositor.set_top_inset(inset);

    // A scroll of six rows puts row five one row above the band's bottom
    // edge — inside it, not above the window, which is the only placement
    // that distinguishes a clip from the cull the builders already do.
    let line_height = compositor.line_height();
    assert!(
        line_height > 0.0 && line_height < inset,
        "the band must be more than one row deep or a row cannot sit inside it"
    );
    let scroll = 6.0 * line_height;

    let mut editor = editor();
    // Anchor above, head inside the band: the selection spans the band's
    // rows, and the caret is drawn on the head's row.
    editor.set_selection(
        iridium_editor::Position::new(3, 0),
        iridium_editor::Position::new(5, 8),
    );

    for line in 0..12 {
        compositor
            .gutter_changes_mut()
            .insert(line, Color::new(0.20, 0.85, 0.45, 1.0));
        compositor
            .line_backgrounds_mut()
            .insert(line, Color::new(0.95, 0.75, 0.10, 1.0));
    }

    let frame = compose_to_texture(&mut compositor, &editor, scroll, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page_colour(&pixels);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    assert!(band > 0, "there is no band; this test proves nothing");

    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            let offset = (row * WIDTH_USIZE + column) * 4;
            let inked = (0..3).any(|channel| {
                (i32::from(pixels[offset + channel]) - i32::from(page[channel])).abs() > 24
            });
            assert!(
                !inked,
                "the document drew at ({column}, {row}), inside the {inset}-pixel band the \
                 face reserved"
            );
        }
    }
}

/// The same claim with the gutter turned off, so the document's *own* quads
/// have to answer for themselves.
///
/// The test above fails on the first offending pixel, and the gutter's
/// background quad — which spans the full frame height from zero — is at
/// column zero, row zero. It would therefore report a leak while the
/// selection, the line backgrounds and the caret were all clipped correctly,
/// and it would keep reporting one after they were fixed. This one removes
/// the gutter entirely: anything left in the band is content.
#[test]
fn the_documents_own_quads_stay_out_of_the_reserved_band_at_a_scroll() {
    let gpu = gpu();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(inset);
    compositor.set_gutter_enabled(false);

    let line_height = compositor.line_height();
    assert!(
        line_height > 0.0 && line_height < inset,
        "the band must be more than one row deep or a row cannot sit inside it"
    );
    let scroll = 6.0 * line_height;

    let mut editor = editor();
    editor.set_selection(
        iridium_editor::Position::new(3, 0),
        iridium_editor::Position::new(5, 8),
    );
    for line in 0..12 {
        compositor
            .line_backgrounds_mut()
            .insert(line, Color::new(0.95, 0.75, 0.10, 1.0));
    }

    let frame = compose_to_texture(&mut compositor, &editor, scroll, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page_colour(&pixels);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            let offset = (row * WIDTH_USIZE + column) * 4;
            let inked = (0..3).any(|channel| {
                (i32::from(pixels[offset + channel]) - i32::from(page[channel])).abs() > 24
            });
            assert!(
                !inked,
                "the document drew at ({column}, {row}), inside the {inset}-pixel band the \
                 face reserved"
            );
        }
    }
}
