//! Describe an exact selection using the current retained parse tree.
use super::SyntaxState;
use crate::document::{Document, Selection};

impl SyntaxState {
    /// Returns the kind of the smallest named node exactly matching a selection.
    ///
    /// Collapsed, partial, stale and parser-free selections return `None`.
    /// This never parses or copies the document, so status rendering cannot
    /// introduce a parse or label a range using an outdated tree.
    #[must_use]
    #[cfg(feature = "syntax")]
    pub fn selection_kind(
        &self,
        document: &Document,
        selection: Selection,
    ) -> Option<&'static str> {
        if selection.is_collapsed()
            || self.dirty
            || self.replaced
            || self.tracked_revision != document.revision()
        {
            return None;
        }
        let start = document.position_to_offset(selection.start())?;
        let end = document.position_to_offset(selection.end())?;
        let range = start..end;
        let node = iridium_syntax::navigate::node_at(self.tree()?.root_node(), &range)?;
        (node.byte_range() == range).then_some(node.kind())
    }

    /// Parser-free builds cannot describe a selection by syntax.
    #[must_use]
    #[cfg(not(feature = "syntax"))]
    pub const fn selection_kind(
        &self,
        _document: &Document,
        _selection: Selection,
    ) -> Option<&'static str> {
        None
    }
}

impl SyntaxState {
    /// The current non-syntax selection level, without parsing or changing state.
    #[must_use]
    pub fn navigation_label(
        &self,
        document: &Document,
        cursor: &crate::CursorState,
    ) -> Option<&'static str> {
        self.navigation.label(document, cursor)
    }
}
