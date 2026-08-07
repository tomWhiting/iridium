//! The windowed derive (docs/design/PARSER-TAX-MAP.md stage 1): what the
//! covered window holds, and what a span straddling its edge renders as.

use std::collections::HashMap;
use std::fmt::Write as _;

use iridium_editor::Editor;
use iridium_editor::render::{HighlightContext, HighlightSource, ViewportConfig};
use iridium_editor::theme::{Color, Theme};

use super::super::HighlightCache;
use super::rust_editor;

/// A Rust document of `total` lines carrying one multi-line string from
/// line `string_start` through `string_end` inclusive — the span shape
/// every boundary question below is asked about.
fn source_with_straddling_string(total: usize, string_start: usize, string_end: usize) -> String {
    let mut out = String::new();
    for line in 0..total {
        if line == string_start {
            out.push_str("const LONG: &str = \"string start\n");
        } else if line > string_start && line < string_end {
            out.push_str("string interior\n");
        } else if line == string_end {
            out.push_str("string end\";\n");
        } else {
            writeln!(out, "fn f{line}() {{ let value = {line}; }}")
                .expect("writing to a String cannot fail");
        }
    }
    out
}

/// Resolves one whole document line against `cache` and returns its runs.
fn runs_for_line<'a>(
    cache: &HighlightCache,
    source: &'a str,
    line_start_byte: usize,
    line_end_byte: usize,
    theme: &Theme,
    syntax_theme: &'a HashMap<String, Color>,
    viewport: &'a ViewportConfig,
) -> Vec<(&'a str, Color)> {
    let context = HighlightContext {
        content: &source[line_start_byte..line_end_byte],
        content_start_byte: line_start_byte,
        content_width: 800.0,
        line_height: 20.0,
        viewport_config: viewport,
        syntax_theme,
        foreground: theme.editor.foreground,
    };
    cache
        .resolver(&theme.syntax)
        .resolve(&context)
        .expect("a parsed Rust document resolves spans")
}

/// The byte range of one document line, without its newline.
fn line_bytes(editor: &Editor, line: usize) -> (usize, usize) {
    let document = &editor.state().document;
    let start = document
        .line_to_byte_offset(line)
        .expect("the fixture line exists");
    let text = document.line(line).expect("the fixture line exists");
    (start, start + text.len())
}

/// Proof obligation: a span straddling the covered window's edge renders
/// at the window edge exactly as the whole-document derive renders it.
/// The string here starts inside the viewport and runs more than
/// [`super::OVERSCAN_LINES`] past it, so its node crosses the covered
/// window's end; tree-sitter yields it whole and the resolver clamps it.
#[test]
fn a_span_straddling_the_covered_window_edge_resolves_like_the_whole_document() {
    let string_start = 85;
    let string_end = 195;
    let source = source_with_straddling_string(400, string_start, string_end);
    let editor = rust_editor(&source);

    let mut whole = HighlightCache::new();
    whole.refresh(&editor);
    let mut windowed = HighlightCache::new();
    // Viewport 40..90: covered 0..190, so the string (85..=195) enters
    // inside the viewport and leaves past the covered end.
    windowed.refresh_windowed(&editor, 40..90);

    let theme = Theme::default();
    let syntax_theme = HashMap::new();
    let viewport = ViewportConfig::default();
    for line in [86, 88] {
        let (start, end) = line_bytes(&editor, line);
        let from_whole = runs_for_line(
            &whole,
            &source,
            start,
            end,
            &theme,
            &syntax_theme,
            &viewport,
        );
        let from_window = runs_for_line(
            &windowed,
            &source,
            start,
            end,
            &theme,
            &syntax_theme,
            &viewport,
        );
        assert_eq!(
            from_window, from_whole,
            "line {line}: the windowed derive must answer exactly as the \
             whole-document derive at the window edge"
        );
        assert!(
            from_window
                .iter()
                .any(|(_, colour)| *colour == theme.syntax.string),
            "line {line}: the straddling string really does paint as a string"
        );
    }
}

/// The "window only" observable: content past the covered window has no
/// spans in the windowed index — which is what proves the derive was
/// scoped — while the whole-document derive colours it.
#[test]
fn content_outside_the_covered_window_carries_no_spans() {
    let source = source_with_straddling_string(400, 85, 195);
    let editor = rust_editor(&source);

    let mut whole = HighlightCache::new();
    whole.refresh(&editor);
    let mut windowed = HighlightCache::new();
    windowed.refresh_windowed(&editor, 40..90);

    let theme = Theme::default();
    let syntax_theme = HashMap::new();
    let viewport = ViewportConfig::default();
    let (start, end) = line_bytes(&editor, 300);
    let from_whole = runs_for_line(
        &whole,
        &source,
        start,
        end,
        &theme,
        &syntax_theme,
        &viewport,
    );
    let from_window = runs_for_line(
        &windowed,
        &source,
        start,
        end,
        &theme,
        &syntax_theme,
        &viewport,
    );
    assert!(
        from_whole
            .iter()
            .any(|(_, colour)| *colour != theme.editor.foreground),
        "the whole-document derive colours line 300"
    );
    assert!(
        from_window
            .iter()
            .all(|(_, colour)| *colour == theme.editor.foreground),
        "the windowed derive never derived spans 110 lines past its cover"
    );
}

/// Proof obligation: scrolling within the overscan is a comparison — no
/// rebuild, no generation move.
#[test]
fn scrolling_within_the_overscan_rebuilds_nothing() {
    let source = source_with_straddling_string(600, 85, 195);
    let editor = rust_editor(&source);
    let mut cache = HighlightCache::new();
    cache.refresh_windowed(&editor, 200..250);
    assert_eq!(cache.rebuilds(), 1);
    let generation = cache.generation();

    // Covered window: 100..350. Both requests stay inside it.
    cache.refresh_windowed(&editor, 230..280);
    cache.refresh_windowed(&editor, 120..170);
    assert_eq!(
        cache.rebuilds(),
        1,
        "a scroll within the overscan is a window comparison, not a rebuild"
    );
    assert_eq!(
        cache.generation(),
        generation,
        "and the resolution answer has not changed, so the generation holds"
    );
}

/// Proof obligation: scrolling past the cover re-derives, and the new
/// cover is a new answer — the generation moves like any rebuild.
#[test]
fn scrolling_past_the_cover_rebuilds_and_moves_the_generation() {
    let source = source_with_straddling_string(600, 85, 195);
    let editor = rust_editor(&source);
    let mut cache = HighlightCache::new();
    cache.refresh_windowed(&editor, 200..250);
    assert_eq!(cache.rebuilds(), 1);
    let generation = cache.generation();

    // Covered window: 100..350. One line past the cover escapes it.
    cache.refresh_windowed(&editor, 301..351);
    assert_eq!(
        cache.rebuilds(),
        2,
        "a viewport escaping the covered window forces the re-derive"
    );
    assert_ne!(
        cache.generation(),
        generation,
        "content outside the old cover now answers differently"
    );

    // And the new cover holds for the viewport that produced it.
    cache.refresh_windowed(&editor, 301..351);
    assert_eq!(
        cache.rebuilds(),
        2,
        "the new cover answers the new viewport"
    );
}

/// Proof obligation: a generation move inside the window rebuilds — the
/// parse-gate half of the trigger is untouched by the windowing.
#[test]
fn an_edit_inside_the_window_rebuilds_the_window() {
    let source = source_with_straddling_string(400, 85, 195);
    let mut editor = rust_editor(&source);
    let mut cache = HighlightCache::new();
    cache.refresh_windowed(&editor, 40..90);
    assert_eq!(cache.rebuilds(), 1);
    let generation = cache.generation();

    editor.paste("// note\n");
    cache.refresh_windowed(&editor, 40..90);
    assert_eq!(
        cache.rebuilds(),
        2,
        "the edit's reparse rebuilds the window once"
    );
    assert_ne!(cache.generation(), generation);
    cache.refresh_windowed(&editor, 40..90);
    assert_eq!(cache.rebuilds(), 2, "and only once");
}

/// A short document's viewport estimate may run past its last line; the
/// clamp keeps such a request answerable, so it must not rebuild forever.
#[test]
fn a_viewport_past_the_document_end_is_clamped_not_rebuilt_every_frame() {
    let editor = rust_editor("fn main() {}\n");
    let mut cache = HighlightCache::new();
    cache.refresh_windowed(&editor, 0..500);
    assert_eq!(cache.rebuilds(), 1);
    cache.refresh_windowed(&editor, 0..500);
    cache.refresh_windowed(&editor, 0..500);
    assert_eq!(
        cache.rebuilds(),
        1,
        "an over-long viewport clamps to the document and stays covered"
    );
}
