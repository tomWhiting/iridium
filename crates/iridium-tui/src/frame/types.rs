//! Frame cache ownership and the legacy line-paint placement.

use iridium_editor::Document;
use ropey::{Rope, extra::esoterica::ropes_are_instances};

use super::highlight::Highlighting;

/// Renders editor state into a cell buffer.
///
/// The renderer is stateful only as a cache: it holds the syntax highlighter
/// and the span index so that a frame costs a viewport query rather than a
/// whole-document highlight. Rendering itself takes the editor by shared
/// reference and mutates nothing in it.
#[derive(Default)]
pub struct Frame {
    /// The highlighter and spans, once a language has been set.
    pub(super) highlighting: Option<Highlighting>,
    /// Actual immutable text source of those spans, retained across paints.
    pub(super) highlight_source: Option<HighlightSource>,
}

/// A retained rope keeps its allocation alive, excluding pointer-reuse cache
/// hits. Id/revision alone cannot distinguish divergent Document clones.
///
/// Ropey's comparison is O(1): unmodified clones match, different contents
/// never do. Other results may vary, causing only a safe cache miss. Retaining
/// the shared snapshot makes a later edit copy its affected rope path; R4 must
/// measure that cost alongside frame preparation and repeated painting.
pub(super) struct HighlightSource {
    document_id: u64,
    text: Rope,
}

impl HighlightSource {
    pub(super) fn new(document: &Document) -> Self {
        Self {
            document_id: document.id(),
            text: document.rope().clone(),
        }
    }

    pub(super) fn matches(&self, document: &Document) -> bool {
        self.document_id == document.id() && ropes_are_instances(&self.text, document.rope())
    }
}

/// Which document line one `paint_line` call draws, and where.
///
/// The three travel together — a line is painted at a row, and whether it is
/// active decides its background — so they are one parameter rather than
/// three positional `usize`/`bool`s that are easy to transpose.
#[derive(Debug, Clone, Copy)]
pub(super) struct PaintedLine {
    /// The document line being drawn.
    pub(super) document_line: usize,
    /// The grid row it lands on.
    pub(super) row: usize,
    /// Whether a caret sits on this line.
    pub(super) is_active: bool,
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame")
            .field(
                "language",
                &self.highlighting.as_ref().map(Highlighting::language),
            )
            .finish_non_exhaustive()
    }
}
