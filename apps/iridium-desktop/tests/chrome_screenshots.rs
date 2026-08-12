//! Headless screenshot harness for the desktop chrome — the review artifact
//! for the rounded-panel reskin, and now for the three candidate light
//! themes, neither of which any CPU test can judge by eye.
//!
//! Ignored by default: it needs a GPU adapter and writes multi-megabyte
//! images. Run it explicitly with
//!
//! ```sh
//! IRIDIUM_CHROME_SHOT_DIR=/some/dir \
//!     cargo test -p iridium-desktop --test chrome_screenshots -- --ignored
//! ```
//!
//! into the named directory (the system temp directory when the variable is
//! unset). What it writes:
//!
//! - **the dark control**: `chrome-palette.png`, `chrome-search.png`,
//!   `chrome-history.png`, `chrome-tabs.png`, plus the context menu as
//!   `chrome-menu.png` and `chrome-menu-read-only.png` (the frame that shows
//!   the greyed verbs);
//! - **six frames per candidate light variant** of
//!   `docs/design/LIGHT-THEME-MAP.md` §2.3, named `light-<variant>-<state>.png`
//!   — `editor`, `selection`, `palette`, `search`, `history`, `bridge`.
//!
//! # Which syntax palette a frame is showing
//!
//! This matters more than anything else here, because two different light
//! code palettes ship in the same binary (the light-theme map's fact 2):
//!
//! - Every frame but `bridge` sets a Rust grammar and resolves the kernel's
//!   real tree-sitter spans through [`HighlightCache`], coloured from
//!   `theme.syntax` — **the variant's own palette**, which is what the
//!   variants are being chosen on.
//! - The `bridge` frame plays a language'd document whose spans never arrive
//!   ([`NoHighlights`]), which is the one legitimate case for the
//!   compositor's built-in keyword fallback. That fallback keeps its own
//!   palette, selected on `theme.is_dark` alone, and never consults
//!   `theme.syntax` — so the `bridge` frame shows **the same colours in all
//!   three variants** and is not a judgement of any of them. It is here as
//!   the map asks (§4.1 state 2): the proof of what the divergence costs
//!   while D-8 is unruled.
//!
//! # Headless, by construction
//!
//! No window ever opens: the adapter is requested with no compatible
//! surface — the same construction as the kernel's `compose_frame` bench —
//! and every frame is composed onto an offscreen texture, then copied to a
//! mappable buffer and encoded through the `png` crate.
//!
//! # What the frames cost, and why the encoder is borrowed
//!
//! This harness used to hand-roll its encoder to avoid an image dependency,
//! emitting a zlib stream of deflate "stored" blocks. That is a valid PNG and
//! it is also no compression at all: the measured ratio was 1.00008, every
//! frame 23,760,386 bytes, every run 546 MB for 23 shots.
//!
//! The run is now 8,734,042 bytes for the same 23 frames — 62.6x, each frame
//! between 331,984 and 436,267 bytes.
//!
//! Both halves of the replacement are chosen from measurement on this
//! harness's own frames rather than from the usual rules of thumb:
//!
//! - **Filter [`png::Filter::NoFilter`]**, not the crate's adaptive default
//!   and not the `Sub`/`Up` that flat UI images are supposed to prefer. On
//!   these frames every filter *loses*, because the long horizontal runs of
//!   identical background bytes are the thing deflate matches best and any
//!   filter breaks them up. Whole-run totals: none 8,734,042 bytes, adaptive
//!   10,550,488 — filtering costs 21% here. Per frame, on the raw scanlines
//!   of `light-paper-editor`: none 373,773; adaptive 469,369; Paeth 496,390;
//!   Up 502,252; Sub 520,071.
//! - **Deflate level 9**, worth about 4% over the default level 6 on an
//!   artifact written once and read by eye.
//!
//! Note that the two are set independently rather than through
//! `Encoder::set_compression`, which overwrites the filter — see [`png_bytes`].
//!
//! # The encoder has an oracle
//!
//! The uncompressed stream survived because nothing ever read a frame back.
//! Every shot is now decoded again by [`verify_png`] — the `png` crate's
//! decoder, which did not write it — and checked for its dimensions, its
//! colour type and depth, and every byte of its pixels against what the GPU
//! handed over. [`png_round_trip_is_compressed_and_decodable`] makes the same
//! check without a GPU and asserts the compression ratio outright, so the
//! claim in this doc comment is falsifiable by `cargo test` rather than
//! merely stated.
//!
//! What that GPU-free test does *not* police is the filter choice: a small
//! synthetic frame is regular enough vertically that `Adaptive` beats
//! `NoFilter` on it, the opposite of the real frames. The filter is held by
//! [`MAX_RUN_BYTES`] instead, which is measured on the real set.

use std::path::{Path, PathBuf};

use iridium_config::test_support::classic_light_faces;
use iridium_desktop::command_palette::CommandPalette;
use iridium_desktop::context_menu::ContextMenu;
use iridium_desktop::highlight::HighlightCache;
use iridium_desktop::history_overlay::HistoryPanel;
use iridium_desktop::overlay::{OverlayPainter, PanelContent, PanelFit, StripContent};
use iridium_desktop::search::SearchOverlay;
use iridium_desktop::tab_strip::{TabItem, TabStripContent};
use iridium_desktop::units::u32_to_f32;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::render::{
    FrameCompositor, FrameTarget, HighlightContext, HighlightSource, RunStyle,
};
use iridium_editor::theme::{Color, Theme};
use iridium_editor::{Editor, KeyCode, KeyEvent, Language, Modifiers, Position};

/// Frame width in physical pixels — a 2× desktop window.
const WIDTH: u32 = 3024;

/// Frame height in physical pixels.
const HEIGHT: u32 = 1964;

/// The window scale factor the shots simulate.
const SCALE: f32 = 2.0;

/// The faces' base font size at that scale.
const FONT_SIZE: f32 = 14.0 * SCALE;

/// The same face font the desktop shell embeds.
static FONT: &[u8] = include_bytes!("../../../examples/web/public/fonts/JetBrainsMono-Regular.ttf");

/// A highlight source playing a language'd document whose spans never
/// arrive — the bridge case, selecting the compositor's built-in keyword
/// fallback: realistic colour without a tree-sitter worker.
struct NoHighlights;

impl HighlightSource for NoHighlights {
    fn resolve<'a>(&mut self, _context: &HighlightContext<'a>) -> Option<Vec<(&'a str, RunStyle)>> {
        None
    }

    fn language_active(&self) -> bool {
        true
    }

    fn generation(&self) -> u64 {
        0
    }
}

/// The headless GPU objects every shot composes onto.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Brings up a device with no surface anywhere near it and an offscreen
/// render target.
fn headless_gpu() -> Result<Gpu, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        // No surface: this is what keeps the harness provably windowless.
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|error| format!("no headless GPU adapter is available: {error}"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Iridium Chrome Shot Device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(|error| format!("the GPU device request failed: {error}"))?;

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Iridium Chrome Shot Target"),
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

    Ok(Gpu {
        device,
        queue,
        texture,
        view,
    })
}

/// A realistic Rust document: enough repeated request handlers to fill the
/// viewport with highlighted code, the word `entry` recurring so the search
/// shot has matches to count.
///
/// Every block carries one of each token class a light theme is judged on —
/// a doc comment, an attribute, a keyword, a type, a function, a string
/// literal, a numeric literal and punctuation — because a variant whose
/// string colour never appears on screen cannot be chosen from a picture.
fn rust_document() -> String {
    use std::fmt::Write as _;
    let mut source = String::new();
    for block in 0..40_u32 {
        let _ = write!(
            source,
            "/// Applies request {block} against the shared store.\n\
             #[instrument(skip(store))]\n\
             pub fn handle_request_{block}(store: &mut Store) -> Result<Response, Error> {{\n\
             \x20   const LABEL: &str = \"request {block}: entry lookup\";\n\
             \x20   let key = EntryKey::new({block});\n\
             \x20   let entry = store.entry(&key).ok_or(Error::Missing)?;\n\
             \x20   if entry.revision > {block} {{\n\
             \x20       return Err(Error::Stale);\n\
             \x20   }}\n\
             \x20   let response = Response::from(entry.value.clone(), LABEL);\n\
             \x20   Ok(response)\n\
             }}\n"
        );
    }
    source
}

/// A plain key press.
const fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A key press with modifiers held.
const fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

/// Which syntax palette a shot exercises — the distinction the module doc
/// opens on, made explicit at every call site rather than implied by a bare
/// `bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Palette {
    /// A Rust grammar, real tree-sitter spans, coloured from `theme.syntax`:
    /// the theme's own code colours.
    ThemeSyntax,
    /// A language'd document whose spans never arrive: the compositor's
    /// built-in keyword fallback, whose palette the theme cannot influence.
    KeywordBridge,
}

/// An editor over the document under `theme`, carrying this face's commands
/// and keymap — the same seeding the desktop shell performs at startup.
///
/// [`Palette::ThemeSyntax`] sets the Rust grammar, which is what gives the
/// kernel a parse tree for [`HighlightCache`] to read.
fn editor_with_document(theme: &Theme, palette: Palette) -> Result<Editor, String> {
    let mut editor = Editor::with_defaults();
    editor.set_theme(theme.clone());
    editor.set_content(&rust_document());
    if palette == Palette::ThemeSyntax {
        editor.set_language(Language::Rust);
    }
    for meta in iridium_desktop::commands::command_metas() {
        editor
            .register_command(meta)
            .map_err(|error| format!("the kernel refused a face command: {error}"))?;
    }
    editor
        .push_keymap(iridium_desktop::commands::keymap())
        .map_err(|error| format!("the face keymap failed validation: {error}"))?;
    editor.set_cursor(Position::new(4, 8));
    Ok(editor)
}

/// A compositor already holding `theme`, built fresh for one shot.
///
/// Fresh per shot on purpose: the compositor retains shaped buffers keyed on
/// its theme generation and the highlight source's generation, and two shots
/// of the same document under the same theme but different palettes (the
/// grammar path and the bridge) can present the same generations. A new
/// compositor cannot serve one shot's colours to another.
fn shot_compositor(gpu: &Gpu, theme: &Theme) -> Result<FrameCompositor, String> {
    let mut compositor = FrameCompositor::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Bgra8Unorm,
        WIDTH,
        HEIGHT,
    )
    .map_err(|error| format!("the compositor could not be created: {error}"))?;
    compositor.set_font_size(FONT_SIZE);
    assert!(
        compositor.load_font(FONT.to_vec()),
        "the vendored test font holds no readable face"
    );
    compositor.set_theme(theme.clone());
    Ok(compositor)
}

/// What one shot paints over the composed document — the two docked strips
/// and the floating panels, together, because no caller ever varies one alone.
#[derive(Debug, Clone, Copy)]
struct Chrome<'a> {
    /// The tab strip along the top edge, when the state has one.
    tabs: Option<&'a TabStripContent>,
    /// The docked strip, when the state has one.
    strip: Option<&'a StripContent>,
    /// The floating panels, back to front.
    panels: &'a [&'a PanelContent],
}

impl Chrome<'_> {
    /// No overlay at all: the bare document.
    const NONE: Self = Self {
        tabs: None,
        strip: None,
        panels: &[],
    };
}

/// Composes the document and paints the given overlays over it, headlessly.
fn render_shot(
    gpu: &Gpu,
    compositor: &mut FrameCompositor,
    overlay: &mut OverlayPainter,
    editor: &Editor,
    highlights: &mut dyn HighlightSource,
    chrome: Chrome<'_>,
) -> Result<(), String> {
    compositor
        .compose(
            editor,
            editor.fold_state(),
            0.0,
            highlights,
            FrameTarget {
                view: &gpu.view,
                device: &gpu.device,
                queue: &gpu.queue,
                width: WIDTH,
                height: HEIGHT,
            },
        )
        .map_err(|error| format!("compose failed: {error}"))?;
    overlay
        .paint(
            FrameTarget {
                view: &gpu.view,
                device: &gpu.device,
                queue: &gpu.queue,
                width: WIDTH,
                height: HEIGHT,
            },
            chrome.tabs,
            chrome.strip,
            chrome.panels,
            &editor.state().theme,
        )
        .map_err(|error| format!("the overlay pass failed: {error}"))?;
    Ok(())
}

/// Copies the rendered texture into CPU memory as tightly packed RGBA rows.
fn read_pixels(gpu: &Gpu) -> Result<Vec<u8>, String> {
    let bytes_per_pixel: u32 = 4;
    let unpadded = WIDTH * bytes_per_pixel;
    // Buffer copies require 256-byte row alignment.
    let padded = unpadded.div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Iridium Chrome Shot Readback"),
        size: u64::from(padded) * u64::from(HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Iridium Chrome Shot Copy"),
        });
    encoder.copy_texture_to_buffer(
        gpu.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
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

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("waiting for the GPU failed: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("the map callback never answered: {error}"))?
        .map_err(|error| format!("mapping the readback buffer failed: {error}"))?;

    let data = slice.get_mapped_range();
    let unpadded_usize =
        usize::try_from(unpadded).map_err(|_| "row width exceeds memory".to_string())?;
    let padded_usize =
        usize::try_from(padded).map_err(|_| "padded row exceeds memory".to_string())?;
    let height_usize = usize::try_from(HEIGHT).map_err(|_| "height exceeds memory".to_string())?;
    let mut pixels = Vec::with_capacity(unpadded_usize * height_usize);
    for row in 0..height_usize {
        let start = row * padded_usize;
        let line = &data[start..start + unpadded_usize];
        // The texture is Bgra8Unorm; PNG wants RGBA.
        for pixel in line.chunks_exact(4) {
            pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    drop(data);
    buffer.unmap();
    Ok(pixels)
}

/// The byte count of one RGBA frame of the given dimensions, refusing any
/// pair that cannot be addressed on this host.
fn frame_byte_count(width: u32, height: u32) -> Result<usize, String> {
    let width_usize = usize::try_from(width).map_err(|_| "width exceeds memory".to_string())?;
    let height_usize = usize::try_from(height).map_err(|_| "height exceeds memory".to_string())?;
    width_usize
        .checked_mul(4)
        .and_then(|row| row.checked_mul(height_usize))
        .ok_or_else(|| "the frame exceeds addressable memory".to_string())
}

/// Encodes tightly packed RGBA rows as a compressed PNG.
///
/// The filter and compression settings are measured, not assumed — see the
/// module doc for the numbers they were chosen on.
fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if rgba.len() != frame_byte_count(width, height)? {
        return Err("pixel buffer does not match the declared dimensions".to_string());
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Deliberately not `set_compression`: that convenience setter also
    // *overwrites the filter*, mapping every level except `NoCompression` back
    // onto `Filter::Adaptive`. Calling it after `set_filter` silently discards
    // the filter choice — which is exactly what happened here, and cost 25% of
    // the frame until a decoder read the filter bytes back. Setting the two
    // options independently is immune to the ordering.
    encoder.set_deflate_compression(png::DeflateCompression::Level(9));
    encoder.set_filter(png::Filter::NoFilter);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("the PNG header could not be written: {error}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|error| format!("the PNG pixels could not be written: {error}"))?;
    writer
        .finish()
        .map_err(|error| format!("the PNG stream could not be closed: {error}"))?;
    Ok(out)
}

/// Reads a written frame back with the `png` crate's decoder and proves it is
/// the frame the GPU produced: the declared dimensions, RGBA at eight bits,
/// and every pixel byte identical.
///
/// This is the harness's oracle, and it exists because its absence is what
/// let an uncompressed encoder ship unnoticed. The decoder is deliberately
/// not ours — an encoder checked only by its own author's arithmetic proves
/// that the arithmetic is self-consistent, not that the file is a PNG.
fn verify_png(path: &Path, width: u32, height: u32, expected: &[u8]) -> Result<(), String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot reopen {}: {error}", path.display()))?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .map_err(|error| format!("{} is not a readable PNG: {error}", path.display()))?;
    let capacity = reader
        .output_buffer_size()
        .ok_or_else(|| format!("{}: the decoded frame exceeds memory", path.display()))?;
    let mut decoded = vec![0_u8; capacity];
    let info = reader
        .next_frame(&mut decoded)
        .map_err(|error| format!("{} did not decode: {error}", path.display()))?;

    if info.width != width || info.height != height {
        return Err(format!(
            "{}: decodes {}x{}, but the frame is {width}x{height}",
            path.display(),
            info.width,
            info.height
        ));
    }
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{}: decodes as {:?} at {:?}, not eight-bit RGBA",
            path.display(),
            info.color_type,
            info.bit_depth
        ));
    }
    let pixels = decoded
        .get(..info.buffer_size())
        .ok_or_else(|| format!("{}: the decoder under-filled its buffer", path.display()))?;
    if pixels != expected {
        return Err(format!(
            "{}: the decoded pixels differ from the ones composed",
            path.display()
        ));
    }
    Ok(())
}

/// Renders one shot, writes it as a PNG, reads it back to prove it decodes
/// to what was composed, and hands back its pixels so a caller can assert on
/// them.
fn capture(
    gpu: &Gpu,
    compositor: &mut FrameCompositor,
    overlay: &mut OverlayPainter,
    editor: &Editor,
    highlights: &mut dyn HighlightSource,
    chrome: Chrome<'_>,
    path: &Path,
) -> Result<Vec<u8>, String> {
    render_shot(gpu, compositor, overlay, editor, highlights, chrome)?;
    let pixels = read_pixels(gpu)?;
    let encoded = png_bytes(WIDTH, HEIGHT, &pixels)?;
    std::fs::write(path, &encoded)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    verify_png(path, WIDTH, HEIGHT, &pixels)?;
    Ok(pixels)
}

/// Composes one shot under `theme` with the palette the caller names, writes
/// the PNG, and returns its pixels.
///
/// This is where the two highlight sources are chosen between, and the only
/// place either is constructed.
fn shoot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    editor: &Editor,
    palette: Palette,
    chrome: Chrome<'_>,
    path: &Path,
) -> Result<Vec<u8>, String> {
    let mut compositor = shot_compositor(gpu, theme)?;
    // What the face does wherever the strip's height becomes known: a frame
    // that painted the band without reserving room for it would show the tab
    // strip lying over the document's first two lines, and the shot would be
    // a picture of a layout nothing ships.
    if chrome.tabs.is_some() {
        compositor.set_top_inset(
            FrameCompositor::DOCUMENT_TOP_PADDING + overlay.tab_strip_height(u32_to_f32(HEIGHT)),
        );
    }
    match palette {
        Palette::ThemeSyntax => {
            let mut cache = HighlightCache::new();
            cache.refresh(editor);
            let mut highlights = cache.resolver(&editor.state().theme);
            capture(
                gpu,
                &mut compositor,
                overlay,
                editor,
                &mut highlights,
                chrome,
                path,
            )
        },
        Palette::KeywordBridge => {
            let mut highlights = NoHighlights;
            capture(
                gpu,
                &mut compositor,
                overlay,
                editor,
                &mut highlights,
                chrome,
                path,
            )
        },
    }
}

/// The tolerance a channel comparison needs when an `f32` colour has been
/// quantised to eight bits and back — one step, generously.
const CHANNEL_TOLERANCE: f32 = 1.5 / 255.0;

/// Proves the frame really reached the GPU wearing the theme it was given:
/// the bottom-right pixel is past every glyph, every band and the gutter, so
/// it is the compositor's clear colour and nothing else — and the clear
/// colour is `theme.editor.background`, verbatim.
///
/// Cheap, and it is what stops a variant silently rendering a colour nobody
/// chose.
fn check_clear_colour(pixels: &[u8], expected: Color, label: &str) -> Result<(), String> {
    let corner = pixels
        .len()
        .checked_sub(4)
        .ok_or_else(|| format!("{label}: the frame has no pixels"))?;
    let actual = &pixels[corner..];
    let channels = [
        ("red", f32::from(actual[0]), expected.r),
        ("green", f32::from(actual[1]), expected.g),
        ("blue", f32::from(actual[2]), expected.b),
        ("alpha", f32::from(actual[3]), expected.a),
    ];
    for (channel, read, wanted) in channels {
        if (read / 255.0 - wanted).abs() > CHANNEL_TOLERANCE {
            return Err(format!(
                "{label}: the page renders {channel} {read}/255, but the \
                 theme states {:.0}/255 — the frame is not wearing the \
                 theme it was given",
                wanted * 255.0
            ));
        }
    }
    Ok(())
}

/// Where the shots land: `IRIDIUM_CHROME_SHOT_DIR`, or the temp directory.
fn shot_dir() -> PathBuf {
    std::env::var_os("IRIDIUM_CHROME_SHOT_DIR").map_or_else(std::env::temp_dir, PathBuf::from)
}

/// The bare document: the surface, the gutter, the current-line band under
/// the caret, and the code in the theme's own syntax colours.
///
/// The only shot that asserts a pixel, because it is the only one with an
/// uncovered page: [`check_clear_colour`] reads its corner.
fn document_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let editor = editor_with_document(theme, palette)?;
    let pixels = shoot(gpu, overlay, theme, &editor, palette, Chrome::NONE, path)?;
    check_clear_colour(
        &pixels,
        theme.editor.background,
        &format!("{} ({})", path.display(), theme.name),
    )
}

/// A live multi-line selection over the page — `selection` and
/// `selection_inactive`'s hue judged against the code it covers.
///
/// Not a current-line shot, and deliberately not named one: the compositor's
/// only line-background pass is fed by `line_backgrounds_mut`, which nothing
/// in this face populates, so `theme.editor.current_line` never reaches the
/// document surface. It reaches pixels through the overlay's panel and strip
/// derivation instead, which is what the `palette`, `search` and `history`
/// frames show.
fn selection_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for _ in 0..3 {
        let _ = editor.handle_key(&chord(KeyCode::Down, Modifiers::shift()));
    }
    let _ = editor.handle_key(&chord(KeyCode::Right, Modifiers::shift()));
    shoot(gpu, overlay, theme, &editor, palette, Chrome::NONE, path)?;
    Ok(())
}

/// The command palette: a query typed, the selection moved off the first row,
/// over a document with a visible text selection.
fn palette_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for _ in 0..2 {
        let _ = editor.handle_key(&chord(KeyCode::Down, Modifiers::shift()));
    }
    let mut command_palette = CommandPalette::new();
    command_palette.open();
    let mru = CommandMru::default();
    for character in "se".chars() {
        let _ = command_palette.handle_key(&press(KeyCode::Char(character)), &editor, &mru);
    }
    let _ = command_palette.handle_key(&press(KeyCode::Down), &editor, &mru);
    let content = command_palette.content(&editor, &mru, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[&content],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The context menu hung from a click in the document — the frame that
/// answers what no CPU test can: row height and padding against the
/// palette's, the separator rules, and (in the read-only shot) the exact
/// alpha a greyed verb is drawn at.
fn menu_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    read_only: bool,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    // A selection under the click, which is what a real right-press finds.
    for _ in 0..2 {
        let _ = editor.handle_key(&chord(KeyCode::Right, Modifiers::shift()));
    }
    editor.state_mut().read_only = read_only;
    let menu = ContextMenu::open(&editor, 0.34 * u32_to_f32(WIDTH), 0.30 * u32_to_f32(HEIGHT));
    let content = menu.content(&editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[&content],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The search panel above the strip, a query with live matches — where
/// `search_match` and `search_match_current` are judged against real text.
fn search_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    let mut search = SearchOverlay::new();
    search.open(&mut editor);
    for character in "entry".chars() {
        let _ = search.handle_key(&press(KeyCode::Char(character)), &mut editor);
    }
    let strip = StripContent {
        text: "chrome preview — the strip stands on an honest background".to_string(),
        caret_column: None,
        is_error: false,
    };
    let content = search.content(&editor, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: Some(&strip),
        panels: &[&content],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The undo tree, with a real branch to badge — the panel's dim rows are the
/// `line_number`-class grey.
fn history_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let mut editor = editor_with_document(theme, palette)?;
    for character in "abc".chars() {
        let _ = editor.handle_key(&press(KeyCode::Char(character)));
    }
    let _ = editor.handle_key(&chord(KeyCode::Char('z'), Modifiers::ctrl()));
    for character in "xy".chars() {
        let _ = editor.handle_key(&press(KeyCode::Char(character)));
    }
    let mut history = HistoryPanel::new();
    history.open();
    let content = history.content(&editor, &editor.state().theme, fit);
    let chrome = Chrome {
        tabs: None,
        strip: None,
        panels: &[&content],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// The tab strip along the top edge, with the document pushed down under it.
///
/// The frame that answers what no CPU test can about the strip: whether the
/// active tab's card reads against the band, whether an inactive label is
/// legible at its alpha, whether the dirty dot is findable, and — the thing
/// the round-trip test proves arithmetically and nobody has yet *seen* —
/// whether the document really does start below the band rather than under
/// it.
fn tab_strip_shot(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    theme: &Theme,
    palette: Palette,
    path: &Path,
) -> Result<(), String> {
    let editor = editor_with_document(theme, palette)?;
    let tab = |label: &str, is_active: bool, is_dirty: bool| TabItem {
        label: label.to_owned(),
        is_active,
        is_dirty,
    };
    let tabs = TabStripContent {
        tabs: vec![
            tab("main.rs", false, false),
            tab("compositor.rs", true, true),
            tab("Cargo.toml", false, false),
            // Longer than the budget: the frame that shows where the cut
            // falls and what the ellipsis looks like beside a real name.
            tab("a-rather-long-file-name.json", false, true),
            tab("untitled", false, false),
        ],
    };
    let chrome = Chrome {
        tabs: Some(&tabs),
        strip: None,
        panels: &[],
    };
    shoot(gpu, overlay, theme, &editor, palette, chrome, path)?;
    Ok(())
}

/// One candidate light variant's six frames, per the light-theme map §4.1.
///
/// Five of them wear the variant's own syntax palette; the sixth is the
/// bridge, which wears nobody's — see the module doc.
fn variant_shots(
    gpu: &Gpu,
    overlay: &mut OverlayPainter,
    fit: PanelFit,
    slug: &str,
    theme: &Theme,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let named = |state: &str| out_dir.join(format!("light-{slug}-{state}.png"));

    let editor_path = named("editor");
    document_shot(gpu, overlay, theme, Palette::ThemeSyntax, &editor_path)?;

    let selection_path = named("selection");
    selection_shot(gpu, overlay, theme, Palette::ThemeSyntax, &selection_path)?;

    let palette_path = named("palette");
    palette_shot(
        gpu,
        overlay,
        fit,
        theme,
        Palette::ThemeSyntax,
        &palette_path,
    )?;

    let search_path = named("search");
    search_shot(gpu, overlay, fit, theme, Palette::ThemeSyntax, &search_path)?;

    let history_path = named("history");
    history_shot(
        gpu,
        overlay,
        fit,
        theme,
        Palette::ThemeSyntax,
        &history_path,
    )?;

    let bridge_path = named("bridge");
    document_shot(gpu, overlay, theme, Palette::KeywordBridge, &bridge_path)?;

    Ok(vec![
        editor_path,
        selection_path,
        palette_path,
        search_path,
        history_path,
        bridge_path,
    ])
}

/// The whole harness: the dark control's three frames, then six per candidate
/// light variant.
fn run() -> Result<Vec<PathBuf>, String> {
    let out_dir = shot_dir();
    std::fs::create_dir_all(&out_dir)
        .map_err(|error| format!("cannot create {}: {error}", out_dir.display()))?;
    let gpu = headless_gpu()?;

    let mut overlay = OverlayPainter::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Bgra8Unorm,
        WIDTH,
        HEIGHT,
    )
    .map_err(|error| format!("the overlay painter could not be created: {error}"))?;
    assert!(
        overlay.set_font(FONT_SIZE, FONT.to_vec()),
        "the vendored test font holds no readable face"
    );
    overlay.set_scale(SCALE);

    let fit = overlay
        .panel_fit(WIDTH, HEIGHT)
        .ok_or_else(|| "the shot window cannot fit a panel".to_string())?;
    let mut written = Vec::new();

    // The dark control: the three frames this harness has always produced,
    // under the same theme and the same keyword-bridge source, under the same
    // names — so the light set is judged against an unchanged reference.
    let dark = Theme::dark();
    let palette_path = out_dir.join("chrome-palette.png");
    palette_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &palette_path,
    )?;
    written.push(palette_path);

    let search_path = out_dir.join("chrome-search.png");
    search_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &search_path,
    )?;
    written.push(search_path);

    let menu_path = out_dir.join("chrome-menu.png");
    menu_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        false,
        &menu_path,
    )?;
    written.push(menu_path);

    // The same menu over a read-only document, where the two mutating verbs
    // are greyed — the only frame that shows the disabled alpha.
    let menu_read_only_path = out_dir.join("chrome-menu-read-only.png");
    menu_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        true,
        &menu_read_only_path,
    )?;
    written.push(menu_read_only_path);

    let history_path = out_dir.join("chrome-history.png");
    history_shot(
        &gpu,
        &mut overlay,
        fit,
        &dark,
        Palette::KeywordBridge,
        &history_path,
    )?;
    written.push(history_path);

    let tabs_path = out_dir.join("chrome-tabs.png");
    tab_strip_shot(
        &gpu,
        &mut overlay,
        &dark,
        Palette::KeywordBridge,
        &tabs_path,
    )?;
    written.push(tabs_path);

    // The map's three light faces: the ruled preset, plus the two that ship
    // as files under `themes/` and are read from disk here exactly as a user's
    // `--theme` reads them.
    let faces = classic_light_faces()
        .map_err(|error| format!("the shipped light themes under `themes/` must load: {error}"))?;
    for (slug, theme) in faces {
        written.extend(variant_shots(
            &gpu,
            &mut overlay,
            fit,
            slug,
            &theme,
            &out_dir,
        )?);
    }

    Ok(written)
}

/// How many frames the dark control contributes: the palette, the search
/// panel, the context menu in both its editable and read-only states, the
/// history tree, and the tab strip.
///
/// Named rather than written into the assertion as a literal because it was a
/// literal, it said three, and it stayed saying three when the two menu shots
/// landed — so the harness failed its own count on every run.
const DARK_CONTROL_SHOTS: usize = 6;

/// How many frames each candidate light variant contributes, per the
/// light-theme map §4.1.
const VARIANT_SHOTS: usize = 6;

/// How many light faces the map designed: Platinum from the binary, Paper and
/// Monochrome from `themes/`.
///
/// A literal rather than `classic_light_faces().len()` so that the expected
/// count and the produced count come from different places — a list that
/// silently lost an entry would otherwise agree with itself.
const CLASSIC_LIGHT_FACES: usize = 3;

/// A generous ceiling on one encoded frame.
///
/// The frames measure 331,984 to 436,267 bytes; the uncompressed encoder this
/// harness used to carry produced 23,760,386 bytes for every one of them.
/// Anything between the two is a compression regression, and this is where it
/// stops.
const MAX_FRAME_BYTES: u64 = 1024 * 1024;

/// A budget on the whole run, which is the number the harness is actually
/// judged on — 23 frames onto someone's disk.
///
/// Tighter than [`MAX_FRAME_BYTES`] on purpose, because it is the only
/// assertion that can see the filter choice. Measured totals: 8,734,042 bytes
/// with [`png::Filter::NoFilter`], 10,550,488 with `Adaptive`, 546,488,878
/// with the stored-block encoder this replaced. The ceiling sits above the
/// first and below the second, so silently losing the filter setting — which
/// `Encoder::set_compression` will do for you — fails here.
const MAX_RUN_BYTES: u64 = 9_500_000;

/// A frame shaped like the ones the harness really writes — a flat page, a
/// band across it, and a scatter of glyph-like marks — without a GPU.
fn synthetic_frame(width: u32, height: u32) -> Vec<u8> {
    // A capacity hint only; the loop below is what fixes the real length.
    let mut pixels = Vec::with_capacity(frame_byte_count(width, height).unwrap_or_default());
    for y in 0..height {
        for x in 0..width {
            let band = y % 64 < 3;
            let mark = !band && x % 11 < 4 && y % 19 < 12;
            let pixel = match (band, mark) {
                (true, _) => [0x2A, 0x2E, 0x36, 0xFF],
                (_, true) => [0xD4, 0xC8, 0x9A, 0xFF],
                _ => [0x1E, 0x21, 0x27, 0xFF],
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    pixels
}

/// The encoder's oracle, without a GPU: encode a frame, decode it with the
/// `png` crate's decoder, and hold the result to both claims the module doc
/// makes — that the bytes are a real RGBA PNG of the stated size carrying the
/// stated pixels, and that they are actually compressed.
///
/// The second assertion is the one that matters. The defect this replaced was
/// a valid PNG in every respect except that its compression ratio was
/// 1.00008, and no test in the tree could tell.
///
/// The threshold is deliberately loose. This frame is far more compressible
/// than a real one — and it is the wrong shape to judge the *filter* on, so
/// it does not try; see the module doc.
#[test]
fn png_round_trip_is_compressed_and_decodable() {
    let width = 512_u32;
    let height = 384_u32;
    let pixels = synthetic_frame(width, height);
    let encoded = match png_bytes(width, height, &pixels) {
        Ok(bytes) => bytes,
        Err(message) => panic!("{message}"),
    };

    // Stated as integers rather than a ratio so no cast is needed: at least
    // tenfold. The measured frames manage sixtyfold; the stored-block encoder
    // this replaced managed 1.00008 and would fail here.
    assert!(
        encoded.len() * 10 < pixels.len(),
        "the encoder turned {} bytes into {} — under tenfold, which is not \
         the compression this harness depends on",
        pixels.len(),
        encoded.len()
    );

    let mut reader = match png::Decoder::new(std::io::Cursor::new(&encoded)).read_info() {
        Ok(reader) => reader,
        Err(error) => panic!("the encoded bytes are not a readable PNG: {error}"),
    };
    let capacity = reader
        .output_buffer_size()
        .expect("the decoder reports an unrepresentable frame size");
    let mut decoded = vec![0_u8; capacity];
    let info = match reader.next_frame(&mut decoded) {
        Ok(info) => info,
        Err(error) => panic!("the encoded bytes did not decode: {error}"),
    };

    assert_eq!(info.width, width, "the decoded width");
    assert_eq!(info.height, height, "the decoded height");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "the decoded colour type"
    );
    assert_eq!(
        info.bit_depth,
        png::BitDepth::Eight,
        "the decoded bit depth"
    );
    assert_eq!(
        &decoded[..info.buffer_size()],
        &pixels[..],
        "the decoded pixels"
    );

    // Two named pixels, so the whole-buffer comparison above cannot pass by
    // both sides being wrong the same way: the band that crosses the origin,
    // and the page colour on the first row below it.
    let row_bytes =
        usize::try_from(width).expect("the test frame width does not fit this host") * 4;
    assert_eq!(
        &decoded[..4],
        &[0x2A, 0x2E, 0x36, 0xFF],
        "the band at the origin"
    );
    let page = 3 * row_bytes + 5 * 4;
    assert_eq!(
        &decoded[page..page + 4],
        &[0x1E, 0x21, 0x27, 0xFF],
        "the page below the band"
    );
}

/// Rejects a pixel buffer that does not match the dimensions it is encoded
/// under, rather than writing a frame that decodes to something else.
#[test]
fn png_bytes_refuses_a_mismatched_buffer() {
    let short = vec![0_u8; 4 * 4 * 4];
    assert!(png_bytes(8, 8, &short).is_err(), "an undersized buffer");
    assert!(png_bytes(0, 0, &short).is_err(), "an oversized buffer");
}

/// The review artifact: the dark control plus the three candidate light
/// variants, produced headlessly.
#[test]
#[ignore = "needs a GPU and writes multi-megabyte artifacts; run with --ignored"]
fn chrome_screenshots_render_headlessly() {
    match run() {
        Ok(paths) => {
            assert_eq!(
                paths.len(),
                DARK_CONTROL_SHOTS + VARIANT_SHOTS * CLASSIC_LIGHT_FACES,
                "the shot set is the dark control plus six frames per variant"
            );
            let mut run_bytes = 0_u64;
            for path in paths {
                let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
                assert!(size > 0, "{} is empty", path.display());
                assert!(
                    size <= MAX_FRAME_BYTES,
                    "{} is {size} bytes, past the {MAX_FRAME_BYTES}-byte ceiling \
                     — the frames are no longer being compressed",
                    path.display()
                );
                run_bytes += size;
            }
            assert!(
                run_bytes <= MAX_RUN_BYTES,
                "the run wrote {run_bytes} bytes, past the {MAX_RUN_BYTES}-byte \
                 budget — either the frames grew, or the encoder lost its \
                 filter or compression settings"
            );
        },
        Err(message) => panic!("{message}"),
    }
}
