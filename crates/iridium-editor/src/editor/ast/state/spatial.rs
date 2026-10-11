//! Selection-preserving spatial movement uses the retained syntax tree.
use super::{SyntaxState, SyntaxTree};
use crate::{CursorState, Document};
impl SyntaxState {
    pub(in crate::editor) fn retarget_horizontal(
        &mut self,
        document: &Document,
        before: &CursorState,
        forward: bool,
        count: u32,
    ) -> CursorState {
        self.sync(document);
        self.expand.clear();
        let tree = self.tree.as_ref().and_then(SyntaxTree::tree);
        self.navigation
            .horizontal(document, tree, before, forward, count)
    }

    pub(in crate::editor) fn retarget_vertical(
        &mut self,
        document: &Document,
        before: &CursorState,
        moved: &CursorState,
        down: bool,
    ) -> CursorState {
        self.sync(document);
        self.expand.clear();
        let tree = self.tree.as_ref().and_then(SyntaxTree::tree);
        self.navigation
            .retarget(document, tree, before, moved, down)
    }
}
