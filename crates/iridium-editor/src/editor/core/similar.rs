//! Structural match discovery shares the retained parser and ordinary selections.
#[cfg(feature = "syntax")]
use super::Selection;
use super::{CursorState, Editor};
/// How a selected syntax node acts as a match template.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxSimilarity {
    /// Same grammar node kind, regardless of descendants or values.
    Kind,
    /// Same complete subtree kinds and arity, allowing different literal values.
    Shape,
}
impl Editor {
    /// Discover matches without replacing cursors or editing the document.
    /// Preserves the seed as primary; accepts normal multicursor editing afterward.
    ///
    /// # Errors
    /// Requires syntax support and an exact valid named-node selection. Oversized
    /// documents and traversal/match budget exhaustion produce no partial result.
    #[cfg(feature = "syntax")]
    pub fn similar_syntax_selections(
        &mut self,
        mode: SyntaxSimilarity,
    ) -> Result<CursorState, String> {
        let document = &self.state.document;
        if document.rope().len() > 32 * 1024 * 1024 {
            return Err("Similar-node preview accepts documents up to 32 MiB".into());
        }
        let primary = self.state.cursor.primary;
        let start = document
            .position_to_offset(primary.start())
            .ok_or("Invalid selection start")?;
        let end = document
            .position_to_offset(primary.end())
            .ok_or("Invalid selection end")?;
        let tree = self
            .state
            .syntax
            .sync(document)
            .ok_or("No syntax grammar is available for this document")?;
        let ranges = iridium_syntax::similar::ranges(
            tree.root_node(),
            start..end,
            mode == SyntaxSimilarity::Shape,
        )?;
        let mut secondary = Vec::new();
        for range in ranges {
            if range == (start..end) {
                continue;
            }
            let from = document
                .offset_to_position(range.start)
                .ok_or("Invalid syntax match start")?;
            let to = document
                .offset_to_position(range.end)
                .ok_or("Invalid syntax match end")?;
            secondary.push(if primary.is_backward() {
                Selection::new(to, from)
            } else {
                Selection::new(from, to)
            });
        }
        Ok(CursorState::from_selections(primary, secondary))
    }
    /// Parser-free builds report that structural matching is unavailable.
    ///
    /// # Errors
    /// Always returns an explicit missing-syntax error without changing state.
    #[cfg(not(feature = "syntax"))]
    pub fn similar_syntax_selections(
        &mut self,
        _mode: SyntaxSimilarity,
    ) -> Result<CursorState, String> {
        Err("This build does not include syntax matching".into())
    }
}

#[cfg(test)]
mod tests;
