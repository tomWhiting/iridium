//! The frame's syntax-highlighting cache.
//!
//! Nothing here parses. The kernel keeps one parse tree for the document and
//! refreshes it after every content change; this module reads that tree, turns
//! it into spans once per generation, and resolves those spans onto the
//! clusters of one line at a time.

use iridium_editor::Editor;
use iridium_editor::span_index::SpanIndex;
use iridium_editor::syntax::{Highlighter, Language};

use super::line::LineLayout;
use super::palette::Palette;
use crate::cell::{Color, Style};

/// The cached parse-derived state a frame is drawn from.
pub(super) struct Highlighting {
    /// The language the spans were produced for.
    language: Language,
    /// The rules, borrowed from the process-wide query cache.
    highlighter: Highlighter,
    /// The document's spans, indexed for viewport queries.
    spans: SpanIndex,
    /// The tree and document state the spans were produced from.
    generation: Generation,
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

    /// Brings the cache up to date, reusing what it can.
    ///
    /// Takes the previous cache by value and returns the next one, so a stale
    /// entry cannot be left behind by an early return.
    pub(super) fn refreshed(existing: Option<Self>, editor: &Editor) -> Option<Self> {
        let state = editor.state();
        let language = state.syntax.language()?;
        let generation = Generation {
            parses: state.syntax.full_parses() + state.syntax.incremental_parses(),
            revision: state.document.revision(),
        };

        let reusable = match existing {
            Some(existing) if existing.language == language => {
                if existing.generation == generation {
                    return Some(existing);
                }
                Some(existing.highlighter)
            },
            _ => None,
        };

        // A language whose bundled highlight query does not compile leaves the
        // face with no spans rather than no frame: unhighlighted text is a
        // degraded editor, a missing frame is no editor at all.
        let highlighter = reusable.or_else(|| Highlighter::try_new(language))?;

        let spans = state.syntax.tree().map_or_else(SpanIndex::empty, |tree| {
            SpanIndex::new(highlighter.spans_in(tree, &state.document.text()))
        });

        Some(Self {
            language,
            highlighter,
            spans,
            generation,
        })
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
