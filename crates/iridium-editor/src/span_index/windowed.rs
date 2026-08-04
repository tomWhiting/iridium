//! The windowed span cache both native faces draw their highlights from.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change (`EditorState::refresh_syntax`,
//! which every mutating command runs); [`WindowedSpanCache`] reads that tree,
//! turns it into an indexed span set, and hands the index to a face to
//! resolve onto its own frame — coloured runs on the GPU face, per-cluster
//! styles on the terminal face.
//!
//! # The spans cover a window, not the document
//!
//! Deriving every span of a 10k-line file on every parse generation was the
//! parser tax (docs/design/PARSER-TAX-MAP.md): ~30ms of each keystroke spent
//! walking the whole tree for a viewport that paints ~90 lines, paid
//! identically by both native faces because each carried its own copy of
//! this cache — which is why it now lives here once (the map's R2 ruling).
//! The cache derives spans for the requested viewport widened by
//! [`OVERSCAN_LINES`] each way, and rebuilds when the parse generation moves
//! *or* the requested viewport escapes the covered window. Scrolling within
//! the overscan rebuilds nothing and moves nothing; scrolling past it is a
//! new answer, so it rebuilds and moves the generation like any rebuild.
//! Spans straddling the window's edges arrive whole from
//! [`Highlighter::spans_in_range`]'s boundary contract, and each face's
//! resolver clamps them to the content it paints.

use std::ops::Range;

use crate::document::Document;
use crate::editor::Editor;
use crate::span_index::SpanIndex;
use crate::syntax::{Highlighter, Language};

/// How far past the requested viewport, in document lines each direction, a
/// span rebuild derives — the parser-tax map's R1 margin.
///
/// A taste knob, not a correctness knob: spans at the widened window's edges
/// arrive whole and the faces' resolvers clamp them, so the margin only
/// decides how far a scroll can travel before the cache must re-derive. It
/// also absorbs the desktop face's wrap-unaware viewport estimate (see
/// `App::viewport_window`). The web face ships 50 lines; ±100 was ruled for
/// the native faces.
pub const OVERSCAN_LINES: usize = 100;

/// The cached parse-derived state a face's frames are resolved from.
///
/// One per session, owned by the face beside the editor.
/// [`Self::refresh_windowed`] runs before every frame and costs a generation
/// and window comparison when nothing changed; the span rebuild — over the
/// covered window only, never the whole document — happens only when the
/// kernel actually reparsed or the viewport escaped the cover.
#[derive(Debug, Default)]
pub struct WindowedSpanCache {
    /// The spans and the provenance they were produced under, when a language
    /// with a working highlighter is set.
    entry: Option<Entry>,
    /// How many times the spans have been rebuilt — the observable that
    /// proves both rebuild gates: a frame without an edit and without a
    /// window escape must not move it.
    rebuilds: u64,
    /// The answer generation a face may key retained state on: moves
    /// whenever a subsequent resolution over the index could answer
    /// differently.
    ///
    /// `rebuilds` alone is not enough — removing the language clears the
    /// spans *without* a rebuild, and the next resolution answers nothing
    /// where it answered spans. So this moves on every span rebuild *and*
    /// on every path that empties the entry while it held spans.
    generation: u64,
    /// Whether the kernel had a language set at the last refresh.
    ///
    /// It is what separates the bridge (a language is set, spans not
    /// available — a face may fall back to keyword colours) from the void
    /// (no language — content renders in the plain foreground). Deliberately
    /// about the *language*, not the entry: a language whose highlight query
    /// fails to compile still counts as set, and degrades to the bridge
    /// rather than to plain.
    language_active: bool,
}

/// One generation's worth of spans, covering one window of the document.
#[derive(Debug)]
struct Entry {
    /// The language the spans were produced for.
    language: Language,
    /// The rules, borrowed from the process-wide query cache.
    highlighter: Highlighter,
    /// The covered window's spans, indexed for viewport queries.
    index: SpanIndex,
    /// The tree and document state the spans were produced from.
    generation: Generation,
    /// The document lines (half-open) the index covers: the viewport the
    /// spans were derived for, widened by [`OVERSCAN_LINES`] each way. A
    /// requested viewport escaping this window forces a rebuild even when
    /// the generation is unmoved.
    covered_lines: Range<usize>,
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

impl WindowedSpanCache {
    /// An empty cache: no language, no spans, an index that answers nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entry: None,
            rebuilds: 0,
            generation: 0,
            language_active: false,
        }
    }

    /// Brings the cache up to date with the kernel's tree over the whole
    /// document — [`Self::refresh_windowed`] with a window covering every
    /// line.
    ///
    /// The convenience form for callers without a viewport (headless states,
    /// tests): its derive is O(document), which is exactly the cost the
    /// windowed form exists to avoid, so a per-frame path must pass its
    /// real viewport instead.
    pub fn refresh(&mut self, editor: &Editor) {
        let line_count = editor.state().document.line_count();
        self.refresh_windowed(editor, 0..line_count);
    }

    /// Brings the cache up to date with the kernel's tree, reusing everything
    /// it can, deriving spans for `viewport_lines` (document lines, half-open)
    /// widened by [`OVERSCAN_LINES`] each way.
    ///
    /// With no language set the cache empties and reports the language
    /// inactive — the face renders the plain foreground, because a file
    /// without a grammar must not wear another language's keyword colours. A
    /// language whose bundled highlight query does not compile empties the
    /// cache but keeps the language active: it degrades to a face's keyword
    /// bridge, since unhighlighted text is a degraded editor and a missing
    /// frame is no editor at all. Otherwise the spans are rebuilt only when
    /// the parse count or document revision moved *or* the requested viewport
    /// escaped the covered window — a scroll within the overscan is a
    /// comparison, not a rebuild — and the compiled highlighter survives
    /// every rebuild for the same language.
    pub fn refresh_windowed(&mut self, editor: &Editor, viewport_lines: Range<usize>) {
        let had_entry = self.entry.is_some();
        let state = editor.state();
        self.language_active = state.syntax.language().is_some();
        let Some(language) = state.syntax.language() else {
            self.entry = None;
            if had_entry {
                // The spans are gone without a rebuild — the next resolution
                // answers nothing where it answered spans, and the exposed
                // generation must say so (the `rebuilds`-is-not-enough path).
                self.generation = self.generation.wrapping_add(1);
            }
            return;
        };
        let generation = Generation {
            parses: state.syntax.full_parses() + state.syntax.incremental_parses(),
            revision: state.document.revision(),
        };
        let line_count = state.document.line_count();
        let requested = clamp_window(viewport_lines, line_count);

        let reusable = match self.entry.take() {
            Some(existing) if existing.language == language => {
                if existing.generation == generation && covers(&existing.covered_lines, &requested)
                {
                    self.entry = Some(existing);
                    return;
                }
                Some(existing.highlighter)
            },
            _ => None,
        };
        let Some(highlighter) = reusable.or_else(|| Highlighter::try_new(language)) else {
            // The entry was taken above and stays empty: a language whose
            // highlight query does not compile degrades to the fallback. If
            // spans were on screen, that is a change of answer too.
            if had_entry {
                self.generation = self.generation.wrapping_add(1);
            }
            return;
        };

        let covered_lines = widen(&requested, line_count);
        let index = state.syntax.tree().map_or_else(SpanIndex::empty, |tree| {
            let text = state.document.text();
            let window = byte_window(&state.document, &covered_lines, text.len());
            SpanIndex::new(highlighter.spans_in_range(tree, &text, window))
        });
        self.rebuilds = self.rebuilds.saturating_add(1);
        self.generation = self.generation.wrapping_add(1);
        self.entry = Some(Entry {
            language,
            highlighter,
            index,
            generation,
            covered_lines,
        });
    }

    /// How many times the spans have been rebuilt since the cache was
    /// created.
    ///
    /// Exists so both rebuild gates — the generation comparison and the
    /// window comparison — are testable from outside: refreshing without an
    /// intervening edit or window escape must leave this unchanged.
    #[must_use]
    pub const fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The answer generation a face may key retained state on — see the
    /// field documentation for what moves it.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the kernel had a language set at the last refresh — the
    /// bridge/void split, see the field documentation.
    #[must_use]
    pub const fn language_active(&self) -> bool {
        self.language_active
    }

    /// The language the cached spans were produced for, when a language with
    /// a working highlighter produced spans at the last refresh.
    #[must_use]
    pub fn language(&self) -> Option<Language> {
        self.entry.as_ref().map(|entry| entry.language)
    }

    /// The covered window's spans, indexed for viewport queries — `None`
    /// when no language (or no working highlighter) is set.
    ///
    /// Span offsets are document-absolute bytes; content outside the covered
    /// window was never derived, so the index answers nothing there and a
    /// caller must not read that silence as "unhighlighted by the grammar".
    #[must_use]
    pub fn index(&self) -> Option<&SpanIndex> {
        self.entry.as_ref().map(|entry| &entry.index)
    }
}

/// Clamps a requested viewport window to lines the document actually has.
///
/// A face's viewport estimate may run a row or two past the end of a short
/// document; unclamped, such a request could never be covered by a window
/// clamped to the document, and every frame would rebuild.
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

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use crate::Editor;
    use crate::syntax::{HighlightSpan, Language};

    use super::WindowedSpanCache;

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

    /// The byte range of one document line, without its newline.
    fn line_bytes(editor: &Editor, line: usize) -> (usize, usize) {
        let document = &editor.state().document;
        let start = document
            .line_to_byte_offset(line)
            .expect("the fixture line exists");
        let text = document.line(line).expect("the fixture line exists");
        (start, start + text.len())
    }

    /// The cached spans overlapping one document line, sorted.
    fn spans_on_line(
        cache: &WindowedSpanCache,
        editor: &Editor,
        line: usize,
    ) -> Vec<HighlightSpan> {
        let (start, end) = line_bytes(editor, line);
        let mut spans: Vec<_> = cache
            .index()
            .expect("the parsed Rust document holds an index")
            .query(start, end)
            .collect();
        spans.sort_unstable();
        spans
    }

    #[test]
    fn refreshing_without_an_edit_rebuilds_nothing() {
        let editor = rust_editor("fn main() {}\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        cache.refresh(&editor);
        cache.refresh(&editor);
        assert_eq!(
            cache.rebuilds(),
            1,
            "an unchanged document is a generation comparison, not a rebuild"
        );
    }

    #[test]
    fn an_edit_moves_the_generation_and_rebuilds_once() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1);

        editor.paste("// note\n");
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 2, "the edit's reparse invalidates once");
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 2, "and only once");
    }

    #[test]
    fn generation_is_stable_across_no_op_refreshes_and_moves_on_edits() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        let after_build = cache.generation();

        cache.refresh(&editor);
        cache.refresh(&editor);
        assert_eq!(
            cache.generation(),
            after_build,
            "an unchanged document must not move the answer generation"
        );

        editor.paste("// note\n");
        cache.refresh(&editor);
        assert_ne!(
            cache.generation(),
            after_build,
            "a rebuild is a new answer and must move the generation"
        );
    }

    /// The `rebuilds`-is-not-enough path: removing the language clears the
    /// spans without a rebuild, and the next resolution answers nothing
    /// where it answered spans — retained state keyed on the generation
    /// must notice.
    #[test]
    fn removing_the_language_moves_the_generation_without_a_rebuild() {
        let mut editor = rust_editor("fn main() {}\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1);
        let with_spans = cache.generation();

        editor.state_mut().syntax.clear_language();
        cache.refresh(&editor);
        assert_eq!(cache.rebuilds(), 1, "clearing the entry is not a rebuild");
        assert_ne!(
            cache.generation(),
            with_spans,
            "the cleared spans change the resolution answer"
        );
        assert!(
            cache.index().is_none(),
            "and the answer really did change: the cache has no spans"
        );
        assert!(
            !cache.language_active(),
            "the cleared language is reported inactive — the face renders plain"
        );

        let cleared = cache.generation();
        cache.refresh(&editor);
        assert_eq!(
            cache.generation(),
            cleared,
            "staying without a language moves nothing further"
        );
    }

    #[test]
    fn without_a_language_the_cache_stays_empty_and_inactive() {
        let mut editor = Editor::with_defaults();
        editor.set_content("plain text, no grammar\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        assert!(cache.index().is_none(), "no language means no spans");
        assert!(cache.language().is_none());
        assert!(
            !cache.language_active(),
            "and no language to bridge to — the face renders plain"
        );
        assert_eq!(cache.rebuilds(), 0);
    }

    #[test]
    fn a_set_language_is_reported_active_and_named() {
        let editor = rust_editor("fn main() {}\n");
        let mut cache = WindowedSpanCache::new();
        cache.refresh(&editor);
        assert!(cache.language_active(), "a set language is reported active");
        assert_eq!(cache.language(), Some(Language::Rust));
    }

    /// Proof obligation: scrolling within the overscan is a comparison — no
    /// rebuild, no generation move.
    #[test]
    fn scrolling_within_the_overscan_rebuilds_nothing() {
        let source = source_with_straddling_string(600, 85, 195);
        let editor = rust_editor(&source);
        let mut cache = WindowedSpanCache::new();
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
        let mut cache = WindowedSpanCache::new();
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
        let mut cache = WindowedSpanCache::new();
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
        let mut cache = WindowedSpanCache::new();
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

    /// Proof obligation: inside the covered window the windowed derive
    /// answers span-for-span as the whole-document derive — including a span
    /// entering inside the viewport and leaving past the covered end, which
    /// tree-sitter yields whole.
    #[test]
    fn inside_the_window_the_derive_agrees_with_the_whole_document() {
        let source = source_with_straddling_string(400, 85, 195);
        let editor = rust_editor(&source);

        let mut whole = WindowedSpanCache::new();
        whole.refresh(&editor);
        let mut windowed = WindowedSpanCache::new();
        // Viewport 40..90: covered 0..190, so the string (85..=195) enters
        // inside the viewport and leaves past the covered end.
        windowed.refresh_windowed(&editor, 40..90);

        for line in [45, 86, 88] {
            let from_whole = spans_on_line(&whole, &editor, line);
            let from_window = spans_on_line(&windowed, &editor, line);
            assert!(
                !from_whole.is_empty(),
                "line {line}: the fixture line carries spans"
            );
            assert_eq!(
                from_window, from_whole,
                "line {line}: the windowed derive must answer exactly as the \
                 whole-document derive inside the cover"
            );
        }
    }

    /// The "window only" observable: content past the covered window has no
    /// spans in the windowed index — which is what proves the derive was
    /// scoped — while the whole-document derive carries them.
    #[test]
    fn content_outside_the_covered_window_carries_no_spans() {
        let source = source_with_straddling_string(400, 85, 195);
        let editor = rust_editor(&source);

        let mut whole = WindowedSpanCache::new();
        whole.refresh(&editor);
        let mut windowed = WindowedSpanCache::new();
        windowed.refresh_windowed(&editor, 40..90);

        assert!(
            !spans_on_line(&whole, &editor, 300).is_empty(),
            "the whole-document derive spans line 300"
        );
        assert!(
            spans_on_line(&windowed, &editor, 300).is_empty(),
            "the windowed derive never derived spans 110 lines past its cover"
        );
    }
}
