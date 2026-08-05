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
//! - **the dark control**, unchanged: `chrome-palette.png`,
//!   `chrome-search.png`, `chrome-history.png`;
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
//! mappable buffer and encoded as an uncompressed PNG with no image
//! dependency (zlib "stored" blocks are still a valid PNG stream).

use std::path::{Path, PathBuf};

use iridium_desktop::command_palette::CommandPalette;
use iridium_desktop::highlight::HighlightCache;
use iridium_desktop::history_overlay::HistoryPanel;
use iridium_desktop::overlay::{OverlayPainter, PanelContent, PanelFit, StripContent};
use iridium_desktop::search::SearchOverlay;
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::render::{FrameCompositor, FrameTarget, HighlightContext, HighlightSource};
use iridium_editor::theme::{ClassicVariant, Color, Theme};
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
    compositor.load_font(FONT.to_vec());
    compositor.set_theme(theme.clone());
    Ok(compositor)
}

/// What one shot paints over the composed document — the docked strip and the
/// floating panels, together, because no caller ever varies one alone.
#[derive(Debug, Clone, Copy)]
struct Chrome<'a> {
    /// The docked strip, when the state has one.
    strip: Option<&'a StripContent>,
    /// The floating panels, back to front.
    panels: &'a [&'a PanelContent],
}

impl Chrome<'_> {
    /// No overlay at all: the bare document.
    const NONE: Self = Self {
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

/// CRC-32 (IEEE), as PNG chunks require.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32, as the zlib stream's trailer requires.
fn adler32(bytes: &[u8]) -> u32 {
    let mut low: u32 = 1;
    let mut high: u32 = 0;
    // 5552 is the largest run before the sums can overflow u32.
    for chunk in bytes.chunks(5552) {
        for &byte in chunk {
            low += u32::from(byte);
            high += low;
        }
        low %= 65_521;
        high %= 65_521;
    }
    (high << 16) | low
}

/// Appends one PNG chunk: length, type, data, CRC over type-plus-data.
fn push_chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Result<(), String> {
    let length = u32::try_from(data.len()).map_err(|_| "chunk too large".to_string())?;
    out.extend_from_slice(&length.to_be_bytes());
    let crc_start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[crc_start..]);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

/// Encodes tightly packed RGBA rows as an uncompressed PNG — a zlib stream
/// of "stored" deflate blocks, valid everywhere, no encoder dependency.
fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let width_usize = usize::try_from(width).map_err(|_| "width exceeds memory".to_string())?;
    let height_usize = usize::try_from(height).map_err(|_| "height exceeds memory".to_string())?;
    let row_bytes = width_usize * 4;
    if rgba.len() != row_bytes * height_usize {
        return Err("pixel buffer does not match the declared dimensions".to_string());
    }

    // The filtered stream: every scanline prefixed with filter type 0.
    let mut raw = Vec::with_capacity((row_bytes + 1) * height_usize);
    for row in rgba.chunks_exact(row_bytes) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    // The zlib wrapper: header, stored blocks, Adler-32.
    let mut idat = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 16);
    idat.extend_from_slice(&[0x78, 0x01]);
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        let last = u8::from(blocks.peek().is_none());
        let length = u16::try_from(block.len()).map_err(|_| "oversized block".to_string())?;
        idat.push(last);
        idat.extend_from_slice(&length.to_le_bytes());
        idat.extend_from_slice(&(!length).to_le_bytes());
        idat.extend_from_slice(block);
    }
    idat.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    // Bit depth 8, colour type 6 (RGBA), deflate, filter 0, no interlace.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut out = Vec::with_capacity(idat.len() + 128);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    push_chunk(&mut out, *b"IHDR", &ihdr)?;
    push_chunk(&mut out, *b"IDAT", &idat)?;
    push_chunk(&mut out, *b"IEND", &[])?;
    Ok(out)
}

/// Renders one shot, writes it as a PNG, and hands back its pixels so a
/// caller can assert on them.
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
    match palette {
        Palette::ThemeSyntax => {
            let mut cache = HighlightCache::new();
            cache.refresh(editor);
            let mut highlights = cache.resolver(&editor.state().theme.syntax);
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
        strip: None,
        panels: &[&content],
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
    variant: ClassicVariant,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let theme = variant.theme();
    let slug = variant.slug();
    let named = |state: &str| out_dir.join(format!("light-{slug}-{state}.png"));

    let editor_path = named("editor");
    document_shot(gpu, overlay, &theme, Palette::ThemeSyntax, &editor_path)?;

    let selection_path = named("selection");
    selection_shot(gpu, overlay, &theme, Palette::ThemeSyntax, &selection_path)?;

    let palette_path = named("palette");
    palette_shot(
        gpu,
        overlay,
        fit,
        &theme,
        Palette::ThemeSyntax,
        &palette_path,
    )?;

    let search_path = named("search");
    search_shot(
        gpu,
        overlay,
        fit,
        &theme,
        Palette::ThemeSyntax,
        &search_path,
    )?;

    let history_path = named("history");
    history_shot(
        gpu,
        overlay,
        fit,
        &theme,
        Palette::ThemeSyntax,
        &history_path,
    )?;

    let bridge_path = named("bridge");
    document_shot(gpu, overlay, &theme, Palette::KeywordBridge, &bridge_path)?;

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
    overlay.set_font(FONT_SIZE, FONT.to_vec());
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

    // The three candidates.
    for variant in ClassicVariant::ALL {
        written.extend(variant_shots(&gpu, &mut overlay, fit, variant, &out_dir)?);
    }

    Ok(written)
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
                3 + 6 * ClassicVariant::ALL.len(),
                "the shot set is the dark control plus six frames per variant"
            );
            for path in paths {
                let size = std::fs::metadata(&path).map_or(0, |meta| meta.len());
                assert!(size > 0, "{} is empty", path.display());
            }
        },
        Err(message) => panic!("{message}"),
    }
}
