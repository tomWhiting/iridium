//! The fixtures every row of the matrix composes against.
//!
//! Determinism is arranged here, not hoped for: one font loaded from bytes,
//! one set of metrics, one compositor configuration, and a blink pinned
//! immediately before every compared compose. Two comparands built by two
//! slightly different paths would make every pixel comparison in this binary
//! meaningless, so there is exactly one path and it lives in this file.

use std::fmt::Write as _;

use iridium_editor::render::{FrameCompositor, HighlightContext, HighlightSource};
use iridium_editor::theme::Color;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers, Position};

use crate::support;
use crate::support::frame::Target;
use crate::support::gpu::Gpu;

/// The name this harness reports failures and labels GPU objects under.
const HARNESS: &str = "retained_shaping";

/// The wider target the resize row composes onto (also 256-byte aligned).
pub const WIDE_WIDTH: u32 = 768;

/// A highlight source with a language but no spans, ever — the bridge case:
/// the compositor's built-in keyword fallback colors the content, and the
/// generation is honestly constant because the answer is `None` forever.
///
/// **Deliberately not `support::scene::ActiveLanguageNoSpans`**, which answers `false`
/// to `language_active`. That difference is the whole subject of several tests
/// below, so the two must stay separate types — and this one is named for what
/// it does rather than sharing a name with something that behaves differently.
pub struct ActiveLanguageNoSpans;

impl HighlightSource for ActiveLanguageNoSpans {
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
pub struct TintHighlights {
    /// The color painted over the whole content, or `None` for fallback.
    pub tint: Option<Color>,
    /// The reported generation.
    pub generation: u64,
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
pub struct ToggleLanguage {
    /// Whether a language is currently set.
    pub active: bool,
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

/// The headless device this harness composes on.
pub fn gpu() -> Gpu {
    support::gpu::gpu(HARNESS)
}

/// An offscreen render target of the given pixel size.
///
/// The size is explicit here, unlike in the inset harnesses, because the
/// resize row composes the same document onto [`WIDE_WIDTH`] as well and the
/// whole point of that test is that the two differ.
pub fn target(gpu: &Gpu, width: u32, height: u32) -> Target {
    support::frame::target_sized(gpu, width, height)
}

/// A compositor configured exactly as every comparand in these tests is:
/// same format, same font size, same font bytes, in the same order.
pub fn compositor(gpu: &Gpu) -> FrameCompositor {
    support::gpu::compositor(gpu)
}

/// One compose with the blink pinned, plus the wait for its GPU work.
pub fn compose(
    compositor: &mut FrameCompositor,
    editor: &Editor,
    scroll_y: f32,
    highlights: &mut dyn HighlightSource,
    gpu: &Gpu,
    tgt: &Target,
) {
    support::frame::compose(compositor, editor, scroll_y, highlights, gpu, tgt);
}

/// Reads the target's pixels back as bytes.
pub fn pixels(gpu: &Gpu, tgt: &Target) -> Vec<u8> {
    support::frame::read_pixels(gpu, tgt)
}

/// Composes the given editor state cold on a *fresh* compositor (configured
/// by `configure` after the standard font setup) and returns its pixels —
/// the oracle every warm-after-invalidation frame is compared against.
pub fn cold_pixels(
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
pub fn document(lines: usize) -> String {
    let mut source = String::with_capacity(lines * 40);
    for line in 0..lines {
        let _ = writeln!(source, "fn item_{line}() {{ let value = {line}; }}");
    }
    source
}

/// An editor over [`document`] of the given line count, cursor at the
/// origin.
pub fn editor_over(lines: usize) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(&document(lines));
    editor.set_cursor(Position::new(0, 0));
    editor
}

/// A plain key press, the kernel's normal edit path.
pub const fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}
