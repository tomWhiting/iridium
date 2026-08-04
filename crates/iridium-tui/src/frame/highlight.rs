//! The frame's syntax-highlighting cache.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change; this module reads that tree, turns
//! it into spans once per generation, and resolves those spans onto the
//! clusters of one line at a time.
//!
//! # The spans cover a window, not the document
//!
//! Deriving every span of a 10k-line file on every parse generation was the
//! parser tax (docs/design/PARSER-TAX-MAP.md) — this face paid it on every
//! keystroke with a language set, exactly as the desktop face did, because
//! the desktop cache is this design carried to that seam. The cache therefore
//! derives spans for the requested viewport widened by [`OVERSCAN_LINES`]
//! each way, and rebuilds when the parse generation moves *or* the requested
//! viewport escapes the covered window. Scrolling within the overscan
//! rebuilds nothing. Spans straddling the window's edges arrive whole from
//! the kernel ([`Highlighter::spans_in_range`]'s boundary contract) and
//! [`cluster_styles`] clamps them to the clusters it paints, as it always
//! has.

use std::ops::Range;

use iridium_editor::Editor;
use iridium_editor::document::Document;
use iridium_editor::span_index::SpanIndex;
use iridium_editor::syntax::{Highlighter, Language};

use super::line::LineLayout;
use super::palette::Palette;
use crate::cell::{Color, Style};

/// How far past the requested viewport, in document lines each direction, a
/// span rebuild derives — the parser-tax map's R1 margin.
///
/// A taste knob, not a correctness knob: spans at the widened window's edges
/// arrive whole and the per-cluster resolution clamps them, so the margin
/// only decides how far a scroll can travel before the cache must re-derive.
/// ±100 was ruled for both native faces.
pub(super) const OVERSCAN_LINES: usize = 100;

/// The cached parse-derived state a frame is drawn from.
pub(super) struct Highlighting {
    /// The language the spans were produced for.
    language: Language,
    /// The rules, borrowed from the process-wide query cache.
    highlighter: Highlighter,
    /// The covered window's spans, indexed for per-line queries.
    spans: SpanIndex,
    /// The tree and document state the spans were produced from.
    generation: Generation,
    /// The document lines (half-open) the index covers: the viewport the
    /// spans were derived for, widened by [`OVERSCAN_LINES`] each way. A
    /// requested viewport escaping this window forces a rebuild even when
    /// the generation is unmoved.
    covered_lines: Range<usize>,
    /// How many times the spans have been rebuilt — the observable that
    /// proves both rebuild gates: a frame without an edit and without a
    /// window escape must not move it.
    rebuilds: u64,
}

/// How current a set of highlight spans is.
///
/// The parse count moves whenever the kernel reparses, so it is what actually
/// invalidates the cache in every path measured so far.
///
/// The revision is carried as well, on the reasoning that a mutation reaching
/// the document without reaching the tree must not leave last parse's colours
/// on screen. **That half is unproven.** Probing it found `Document::revision`
/// reporting `0` both before and after a whole-content replacement that moved
/// the parse count from 2 to 3, so no test here discriminates on it: breaking
/// the revision comparison alone changes nothing observable. It is kept as
/// defence in depth rather than deleted, and this comment says so rather than
/// implying a guarantee the tests do not back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Generation {
    /// Full plus incremental parses the kernel has run.
    parses: u64,
    /// The document revision.
    revision: u64,
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
        self.rebuilds
    }

    /// Brings the cache up to date, reusing what it can, deriving spans for
    /// `viewport_lines` (document lines, half-open) widened by
    /// [`OVERSCAN_LINES`] each way.
    ///
    /// Takes the previous cache by value and returns the next one, so a stale
    /// entry cannot be left behind by an early return. The spans are rebuilt
    /// only when the parse count or document revision moved *or* the
    /// requested viewport escaped the covered window — a scroll within the
    /// overscan is a comparison, not a rebuild.
    pub(super) fn refreshed(
        existing: Option<Self>,
        editor: &Editor,
        viewport_lines: Range<usize>,
    ) -> Option<Self> {
        let state = editor.state();
        let language = state.syntax.language()?;
        let generation = Generation {
            parses: state.syntax.full_parses() + state.syntax.incremental_parses(),
            revision: state.document.revision(),
        };
        let line_count = state.document.line_count();
        let requested = clamp_window(viewport_lines, line_count);

        let (reusable, rebuilds) = match existing {
            Some(existing) if existing.language == language => {
                if existing.generation == generation && covers(&existing.covered_lines, &requested)
                {
                    return Some(existing);
                }
                (Some(existing.highlighter), existing.rebuilds)
            },
            _ => (None, 0),
        };

        // A language whose bundled highlight query does not compile leaves the
        // face with no spans rather than no frame: unhighlighted text is a
        // degraded editor, a missing frame is no editor at all.
        let highlighter = reusable.or_else(|| Highlighter::try_new(language))?;

        let covered_lines = widen(&requested, line_count);
        let spans = state.syntax.tree().map_or_else(SpanIndex::empty, |tree| {
            let text = state.document.text();
            let window = byte_window(&state.document, &covered_lines, text.len());
            SpanIndex::new(highlighter.spans_in_range(tree, &text, window))
        });

        Some(Self {
            language,
            highlighter,
            spans,
            generation,
            covered_lines,
            rebuilds: rebuilds.saturating_add(1),
        })
    }
}

/// Clamps a requested viewport window to lines the document actually has.
///
/// A viewport may run a row past the end of a short document; unclamped,
/// such a request could never be covered by a window clamped to the
/// document, and every frame would rebuild.
fn clamp_window(window: Range<usize>, line_count: usize) -> Range<usize> {
    window.start.min(line_count)..window.end.min(line_count)
}

/// Whether the covered window answers for every line the request names.
const fn covers(covered: &Range<usize>, requested: &Range<usize>) -> bool {
    covered.start <= requested.start && requested.end <= covered.end
}

/// The requested window widened by [`OVERSCAN_LINES`] each way, clamped to
/// the document.
fn widen(requested: &Range<usize>, line_count: usize) -> Range<usize> {
    let start = requested.start.saturating_sub(OVERSCAN_LINES);
    let end = requested.end.saturating_add(OVERSCAN_LINES).min(line_count);
    start..end
}

/// The byte range of a covered line window, for the kernel's range derive.
///
/// `text_len` is the length of the document's materialised text, so the end
/// of the last line needs no extra rope lookup. Line indices at or past the
/// line count land on the document's end.
fn byte_window(document: &Document, lines: &Range<usize>, text_len: usize) -> Range<usize> {
    let start = document
        .line_to_byte_offset(lines.start)
        .unwrap_or(text_len);
    let end = document.line_to_byte_offset(lines.end).unwrap_or(text_len);
    start..end
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
    let Some(highlighting) = highlighting else {
        return styles;
    };

    let line_end = line_start + layout.byte_count();
    let mut spans: Vec<_> = highlighting.spans.query(line_start, line_end).collect();
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
    /// The string starts inside the viewport and runs more than
    /// [`super::OVERSCAN_LINES`] past it, so its node crosses the covered
    /// window's end; tree-sitter yields it whole and the per-cluster
    /// resolution clamps it.
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
