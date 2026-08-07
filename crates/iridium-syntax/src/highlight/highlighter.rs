//! The highlighter: a compiled query, and the walk that runs it.
//!
//! The rules come from the bundled query files (.scm) vendored from the Zed
//! editor; the tree comes from `crate::SyntaxTree`, which this module reads
//! and never owns.

use std::ops::Range;

use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use super::capture::HighlightType;
use super::span::HighlightSpan;
use crate::SyntaxError;
use crate::query::{self, QueryKind};

/// Maps a parse tree's nodes to highlight spans, for one language.
///
/// The highlighter owns no parser and no tree. It holds the rules — a compiled
/// `highlights.scm` and the capture-name mapping — and reads a [`Tree`] someone
/// else parsed, so there is no second copy of the document's structure that
/// could disagree with the first. See [`crate::SyntaxTree`] for the owner.
///
/// The query itself is borrowed from the process-wide cache in [`crate::query`]:
/// `highlights.scm` runs to thousands of patterns, and every open buffer of a
/// language compiling its own copy put that cost on the path that opens a file.
pub struct Highlighter {
    language: crate::Language,
    query: &'static Query,
    capture_names: Vec<Option<HighlightType>>,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("language", &self.language)
            .field("captures", &self.capture_names.len())
            .finish_non_exhaustive()
    }
}

impl Highlighter {
    /// Creates a highlighter for the given language.
    ///
    /// # Errors
    ///
    /// Returns an error if the language's bundled `highlights.scm` does not
    /// compile against its grammar. Every supported language ships one, so this
    /// is never routine — it means a dependency moved underneath a vendored
    /// file.
    pub fn new(language: crate::Language) -> Result<Self, SyntaxError> {
        let query = query::compiled(language, QueryKind::Highlights)?.ok_or_else(|| {
            SyntaxError::QueryError {
                message: format!("No highlights query for language: {}", language.id()),
            }
        })?;

        // Pre-compute capture name mappings for performance
        let capture_names = query
            .capture_names()
            .iter()
            .map(|name| HighlightType::from_capture_name(name))
            .collect();

        Ok(Self {
            language,
            query,
            capture_names,
        })
    }

    /// Creates a highlighter that will work without crashing, even if the
    /// language isn't fully supported. Returns None for unsupported languages.
    #[must_use]
    pub fn try_new(language: crate::Language) -> Option<Self> {
        Self::new(language).ok()
    }

    /// Returns the language this highlighter is configured for.
    #[must_use]
    pub const fn language(&self) -> crate::Language {
        self.language
    }

    /// Returns the highlight spans for `tree`, sorted by position.
    ///
    /// `source` must be the exact text `tree` was parsed from: every span is a
    /// byte range into it, and a mismatch would colour the wrong characters.
    ///
    /// Spans may overlap where one capture nests inside another; the renderer
    /// decides precedence.
    ///
    /// # Cost
    ///
    /// This walks the *entire* tree — O(document), tens of milliseconds on a
    /// 10k-line file at Rust token density — so it has no place on a
    /// per-keystroke path. A caller answering for a viewport wants
    /// [`Self::spans_in_range`], which restricts the walk to the bytes it
    /// will actually paint; this whole-document form is for callers that
    /// genuinely need every span at once.
    #[must_use]
    pub fn spans_in(&self, tree: &Tree, source: &str) -> Vec<HighlightSpan> {
        self.spans_with(QueryCursor::new(), tree, source)
    }

    /// Returns the highlight spans for the part of `tree` overlapping
    /// `range`, sorted by position — [`Self::spans_in`] restricted to a byte
    /// window via tree-sitter's `QueryCursor::set_byte_range`.
    ///
    /// `source` must still be the *whole* text `tree` was parsed from: node
    /// offsets are document-absolute and query predicates read source bytes
    /// wherever their captures land, so a window of the text would colour the
    /// wrong characters or mis-evaluate predicates.
    ///
    /// # Boundary contract
    ///
    /// Tree-sitter's own contract for `ts_query_cursor_set_byte_range`
    /// (vendored 0.26.3): the cursor returns every match that *intersects*
    /// the byte range. Three consequences, stated so callers need not guess:
    ///
    /// - A captured node straddling a range edge is yielded **whole**, with
    ///   its full extents unclamped — a multi-line string crossing the window
    ///   edge arrives with its real `start` and `end`, possibly both outside
    ///   the range. Nothing here clamps; the faces' resolvers already clamp
    ///   spans to the content they paint.
    /// - A match that only partially overlaps the range may carry captures
    ///   lying **entirely outside** it — the whole match is returned once any
    ///   part of it touches the range.
    /// - Spans wholly inside the range are exactly the spans
    ///   [`Self::spans_in`] would produce for them: same extents, same types,
    ///   same ordering.
    ///
    /// An empty `range` (`start >= end`) yields no spans. That case is
    /// answered here rather than forwarded: tree-sitter treats an end byte of
    /// `0` as "unbounded" and leaves the cursor unrestricted on an inverted
    /// range, either of which would silently degrade to the whole-document
    /// walk this method exists to avoid.
    #[must_use]
    pub fn spans_in_range(
        &self,
        tree: &Tree,
        source: &str,
        range: Range<usize>,
    ) -> Vec<HighlightSpan> {
        if range.start >= range.end {
            return Vec::new();
        }
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range);
        self.spans_with(cursor, tree, source)
    }

    /// The walk shared by [`Self::spans_in`] and [`Self::spans_in_range`]:
    /// runs the compiled query through `cursor` — already restricted, or not
    /// — then maps, sorts and dedups the captures.
    ///
    /// # One highlight per byte range
    ///
    /// A `highlights.scm` routinely captures one node from several patterns,
    /// so the same byte range arrives carrying different highlights: in a
    /// single line of TSX the `<` is captured as both a bracket and an
    /// operator, and `main` in `fn main()` is captured as both a function
    /// definition and a variable. Fifteen such ranges appear across five
    /// one-line sources — see `no_byte_range_carries_two_different_highlights`.
    ///
    /// Emitting both is not harmless redundancy, because **nothing
    /// downstream can break the tie.** [`HighlightSpan`]'s `Ord` compares
    /// `(start, end)` and ignores the highlight, so the pair compares equal;
    /// the desktop resolver then sorts with `sort_unstable` and takes the
    /// first claim on each byte. `sort_unstable` is under no obligation to
    /// preserve the order of equal elements, and above roughly twenty
    /// elements it genuinely does not — which is to say, on any ordinary line
    /// of code. The colour would depend on how many spans happened to share
    /// the screen.
    ///
    /// The tie is broken here instead, by keeping the **first span emitted**
    /// for each range. `sort` is stable, so that is the query's own match
    /// order, and tree-sitter yields the specific capture before the general
    /// fallback — which is why the right colour is on screen today. This
    /// change makes that a guarantee rather than a coincidence, and by
    /// construction it alters no colour that is currently stable.
    fn spans_with(&self, mut cursor: QueryCursor, tree: &Tree, source: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        // Note: tree-sitter 0.26 uses StreamingIterator instead of Iterator
        let mut matches = cursor.matches(self.query, tree.root_node(), source.as_bytes());

        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                let capture_idx = capture.index as usize;

                // Skip captures that don't map to a highlight type
                let Some(Some(highlight_type)) = self.capture_names.get(capture_idx).copied()
                else {
                    continue;
                };

                let node = capture.node;
                let start = node.start_byte();
                let end = node.end_byte();

                // Skip empty spans
                if start >= end {
                    continue;
                }

                spans.push(HighlightSpan::new(start, end, highlight_type));
            }
        }

        // Stable, and load-bearing: `sort` preserves emission order among
        // spans that compare equal, which is the query's own match order.
        spans.sort();

        // One highlight per byte range. `dedup()` would remove only exact
        // triples and leave the same-range-different-highlight pairs behind —
        // the whole point of this being `dedup_by` is that those pairs are
        // what carries the ambiguity.
        spans.dedup_by(|later, earlier| later.start == earlier.start && later.end == earlier.end);

        spans
    }
}
