//! The frame's syntax-highlighting cache.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change; the kernel's
//! [`WindowedSpanCache`] reads that tree and turns it into spans for a
//! viewport-plus-overscan window, and this module resolves those spans onto
//! the clusters of one line at a time.
//!
//! # The spans cover a window, not the document
//!
//! Deriving every span of a 10k-line file on every parse generation was the
//! parser tax (docs/design/PARSER-TAX-MAP.md) — this face paid it on every
//! keystroke with a language set, exactly as the desktop face did, because
//! the desktop cache was this design carried to that seam. The windowed
//! derive that fixed it — viewport widened by
//! [`OVERSCAN_LINES`](iridium_editor::span_index::OVERSCAN_LINES) each way,
//! rebuild only when the parse generation moves *or* the requested viewport
//! escapes the covered window — now lives in the kernel, shared with the
//! desktop face (the parser-tax map's R2 ruling); [`Highlighting`] is this
//! face's thin handle on it. Spans straddling the window's edges arrive
//! whole from the kernel
//! ([`Highlighter::spans_in_range`](iridium_editor::syntax::Highlighter::spans_in_range)'s
//! boundary contract) and [`cluster_styles`] clamps them to the clusters it
//! paints, as it always has.

use std::ops::Range;

use iridium_editor::Editor;
use iridium_editor::span_index::WindowedSpanCache;
use iridium_editor::syntax::Language;

use super::line::LineLayout;
use super::palette::Palette;
use crate::cell::{Color, Style};

/// The cached parse-derived state a frame is drawn from: this face's handle
/// on the kernel's [`WindowedSpanCache`].
///
/// Exists only while a language with a working highlighter has produced
/// spans — the frame holds `Option<Highlighting>` and this type's
/// [`Self::refreshed`] hand-off keeps that invariant, so [`cluster_styles`]
/// never has to ask whether the cache is populated and the frame's language
/// answer stays one field read.
pub(super) struct Highlighting {
    /// The language the spans were produced for.
    language: Language,
    /// The hoisted cache: spans, covered window, generation gates.
    cache: WindowedSpanCache,
}

impl Highlighting {
    /// The language these spans were produced for.
    pub(super) const fn language(&self) -> Language {
        self.language
    }

    /// How many times the spans have been rebuilt across the cache's
    /// hand-offs through [`Self::refreshed`].
    ///
    /// Exists so both rebuild gates — the generation comparison and the
    /// window comparison — are testable from outside.
    #[cfg(test)]
    pub(super) const fn rebuilds(&self) -> u64 {
        self.cache.rebuilds()
    }

    /// Brings the cache up to date, reusing what it can, deriving spans for
    /// `viewport_lines` (document lines, half-open) widened by the kernel's
    /// [`OVERSCAN_LINES`](iridium_editor::span_index::OVERSCAN_LINES) each
    /// way.
    ///
    /// Takes the previous cache by value and returns the next one, so a stale
    /// entry cannot be left behind by an early return. The spans are rebuilt
    /// only when the parse count or document revision moved *or* the
    /// requested viewport escaped the covered window — a scroll within the
    /// overscan is a comparison, not a rebuild; see
    /// [`WindowedSpanCache::refresh_windowed`].
    ///
    /// `None` when no language is set, and when the set language's bundled
    /// highlight query does not compile — no spans rather than no frame:
    /// unhighlighted text is a degraded editor, a missing frame is no editor
    /// at all.
    pub(super) fn refreshed(
        existing: Option<Self>,
        editor: &Editor,
        viewport_lines: Range<usize>,
    ) -> Option<Self> {
        let mut cache = existing.map_or_else(WindowedSpanCache::new, |previous| previous.cache);
        cache.refresh_windowed(editor, viewport_lines);
        let language = cache.language()?;
        Some(Self { language, cache })
    }
}

/// The style of every cluster on one line.
///
/// Highlight spans are byte ranges into the whole document, so each cluster is
/// resolved by its own absolute byte offset. Overlapping spans — one capture
/// nested inside another — are painted outermost first, so the innermost wins,
/// which is why the query result is sorted by start ascending and end
/// *descending*.
///
/// With no cache — a language with no grammar, or one whose query would not
/// compile — every cluster takes the default style, so the line still paints.
pub(super) fn cluster_styles(
    highlighting: Option<&Highlighting>,
    line_start: usize,
    layout: &LineLayout,
    palette: &Palette,
    background: Color,
) -> Vec<Style> {
    let default = palette.text().with_background(background);
    let mut styles = vec![default; layout.clusters().len()];
    let Some(index) = highlighting.and_then(|highlighting| highlighting.cache.index()) else {
        return styles;
    };

    let line_end = line_start + layout.byte_count();
    let mut spans: Vec<_> = index.query(line_start, line_end).collect();
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| right.end.cmp(&left.end))
    });

    let clusters = layout.clusters();
    for span in spans {
        let style = palette.highlighted(span.highlight, background);
        let first =
            clusters.partition_point(|cluster| line_start + cluster.byte_index() < span.start);
        for (index, cluster) in clusters.iter().enumerate().skip(first) {
            if line_start + cluster.byte_index() >= span.end {
                break;
            }
            if let Some(slot) = styles.get_mut(index) {
                *slot = style;
            }
        }
    }
    styles
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use iridium_editor::Editor;
    use iridium_editor::syntax::Language;

    use super::super::line::LineLayout;
    use super::super::palette::Palette;
    use super::{Highlighting, cluster_styles};

    /// An editor holding `source` as a Rust document, parsed.
    fn rust_editor(source: &str) -> Editor {
        let mut editor = Editor::with_defaults();
        editor.set_content(source);
        editor.set_language(Language::Rust);
        editor
    }

    /// A Rust document of `total` lines carrying one multi-line string from
    /// line `string_start` through `string_end` inclusive — the span shape
    /// every boundary question below is asked about.
    fn source_with_straddling_string(
        total: usize,
        string_start: usize,
        string_end: usize,
    ) -> String {
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

    /// The styles one document line resolves to under `highlighting`.
    fn styles_for_line(
        highlighting: Option<&Highlighting>,
        editor: &Editor,
        line: usize,
        palette: &Palette,
    ) -> Vec<crate::cell::Style> {
        let state = editor.state();
        let text = state.document.line(line).expect("the fixture line exists");
        let layout = LineLayout::new(&text, state.config.tab_width);
        let line_start = state
            .document
            .line_to_byte_offset(line)
            .expect("the fixture line exists");
        cluster_styles(
            highlighting,
            line_start,
            &layout,
            palette,
            palette.text().background,
        )
    }

    /// Proof obligation: a span straddling the covered window's edge paints
    /// at the window edge exactly as the whole-document derive paints it.
    /// The string starts inside the viewport and runs more than the kernel's
    /// [`iridium_editor::span_index::OVERSCAN_LINES`] past it, so its node
    /// crosses the covered window's end; tree-sitter yields it whole and the
    /// per-cluster resolution clamps it.
    #[test]
    fn a_span_straddling_the_covered_window_edge_paints_like_the_whole_document() {
        let source = source_with_straddling_string(400, 85, 195);
        let editor = rust_editor(&source);
        let palette = Palette::from_theme(editor.get_theme());
        let line_count = editor.state().document.line_count();

        let whole = Highlighting::refreshed(None, &editor, 0..line_count)
            .expect("a parsed Rust document builds the cache");
        // Viewport 40..90: covered 0..190, so the string (85..=195) enters
        // inside the viewport and leaves past the covered end.
        let windowed = Highlighting::refreshed(None, &editor, 40..90)
            .expect("a parsed Rust document builds the cache");

        for line in [86, 88] {
            let from_whole = styles_for_line(Some(&whole), &editor, line, &palette);
            let from_window = styles_for_line(Some(&windowed), &editor, line, &palette);
            assert_eq!(
                from_window, from_whole,
                "line {line}: the windowed derive must paint exactly as the \
                 whole-document derive at the window edge"
            );
            assert_ne!(
                from_window,
                styles_for_line(None, &editor, line, &palette),
                "line {line}: the straddling string really is coloured"
            );
        }
    }

    /// Proof obligation: scrolling within the overscan is a comparison — no
    /// rebuild; scrolling past the cover re-derives.
    #[test]
    fn the_window_gate_rebuilds_on_escape_and_never_within_the_overscan() {
        let source = source_with_straddling_string(600, 85, 195);
        let editor = rust_editor(&source);

        let cache = Highlighting::refreshed(None, &editor, 200..250)
            .expect("a parsed Rust document builds the cache");
        assert_eq!(cache.rebuilds(), 1);

        // Covered window: 100..350. Both requests stay inside it.
        let cache = Highlighting::refreshed(Some(cache), &editor, 230..280)
            .expect("the cache survives a covered scroll");
        let cache = Highlighting::refreshed(Some(cache), &editor, 120..170)
            .expect("the cache survives a covered scroll");
        assert_eq!(
            cache.rebuilds(),
            1,
            "a scroll within the overscan is a window comparison, not a rebuild"
        );

        // One line past the cover escapes it.
        let cache = Highlighting::refreshed(Some(cache), &editor, 301..351)
            .expect("the escaping viewport rebuilds the cache");
        assert_eq!(
            cache.rebuilds(),
            2,
            "a viewport escaping the covered window forces the re-derive"
        );
    }

    /// Proof obligation: a generation move inside the window rebuilds — the
    /// parse-gate half of the trigger is untouched by the windowing.
    #[test]
    fn an_edit_inside_the_window_rebuilds_the_window() {
        let source = source_with_straddling_string(400, 85, 195);
        let mut editor = rust_editor(&source);

        let cache = Highlighting::refreshed(None, &editor, 40..90)
            .expect("a parsed Rust document builds the cache");
        assert_eq!(cache.rebuilds(), 1);

        editor.paste("// note\n");
        let cache = Highlighting::refreshed(Some(cache), &editor, 40..90)
            .expect("the edited document rebuilds the cache");
        assert_eq!(
            cache.rebuilds(),
            2,
            "the edit's reparse rebuilds the window once"
        );
        let cache = Highlighting::refreshed(Some(cache), &editor, 40..90)
            .expect("the unchanged document keeps the cache");
        assert_eq!(cache.rebuilds(), 2, "and only once");
    }

    /// A viewport past the end of a short document clamps to the document,
    /// so it stays covered rather than rebuilding every frame.
    #[test]
    fn a_viewport_past_the_document_end_is_clamped_not_rebuilt_every_frame() {
        let editor = rust_editor("fn main() {}\n");
        let cache = Highlighting::refreshed(None, &editor, 0..500)
            .expect("a parsed Rust document builds the cache");
        assert_eq!(cache.rebuilds(), 1);
        let cache = Highlighting::refreshed(Some(cache), &editor, 0..500)
            .expect("the clamped viewport stays covered");
        assert_eq!(
            cache.rebuilds(),
            1,
            "an over-long viewport clamps to the document and stays covered"
        );
    }
}
