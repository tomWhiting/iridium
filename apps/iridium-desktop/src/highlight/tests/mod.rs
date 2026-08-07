//! The module's suite, split on the same seam the code is.
//!
//! [`frame`] asks what one frame resolves to; [`window`] asks what the windowed
//! derive covers. The two helpers below are shared rather than duplicated,
//! because every test in either half must build its context the way the
//! compositor does.

mod frame;
mod window;

use std::collections::HashMap;

use iridium_editor::render::{HighlightContext, ViewportConfig};
use iridium_editor::theme::Color;
use iridium_editor::{Editor, Language};

/// A context over `content` as an unfolded document starting at byte 0,
/// which is what the compositor hands the resolver for a short unscrolled
/// document.
fn context_over<'a>(
    content: &'a str,
    syntax_theme: &'a HashMap<String, Color>,
    viewport: &'a ViewportConfig,
    foreground: Color,
) -> HighlightContext<'a> {
    HighlightContext {
        content,
        content_start_byte: 0,
        content_width: 800.0,
        line_height: 20.0,
        viewport_config: viewport,
        syntax_theme,
        foreground,
    }
}

/// An editor holding `source` as a Rust document, parsed.
fn rust_editor(source: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(source);
    editor.set_language(Language::Rust);
    editor
}
