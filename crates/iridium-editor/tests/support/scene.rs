//! What gets composed: the document, the theme, and a highlighter that has
//! nothing to say.

use iridium_editor::Editor;
use iridium_editor::render::{HighlightContext, HighlightSource};
use iridium_editor::theme::{Color, Theme};

/// A highlight source that never has spans.
///
/// The colour of the text is not what any of these tests are about, and a real
/// highlighter would make the frame depend on a grammar as well as on the
/// geometry under test.
pub struct NoHighlights;

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

/// The number of lines [`numbered_document`] produces.
///
/// Long enough to scroll well past a 384-pixel frame.
pub const DOCUMENT_LINES: usize = 60;

/// A document of numbered lines.
///
/// Every line is far shorter than a 512-pixel frame is wide, which is what
/// makes the bottom-right pixel reliably page — see [`super::pixels::page`].
#[must_use]
pub fn numbered_document() -> String {
    (0..DOCUMENT_LINES)
        .map(|line| format!("line {line} of the document"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An editor over [`numbered_document`].
#[must_use]
pub fn editor() -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(&numbered_document());
    editor
}

/// The dark theme with a gutter nobody could miss.
///
/// The stock dark theme paints the gutter the same colour as the page, so a
/// gutter background drawn in the wrong place would be *invisible* to a pixel
/// test. This makes it vivid, so the quad has to be where it claims to be.
#[must_use]
pub fn loud_gutter_theme() -> Theme {
    let mut theme = Theme::dark();
    theme.editor.gutter = Color::new(0.85, 0.20, 0.65, 1.0);
    theme
}
