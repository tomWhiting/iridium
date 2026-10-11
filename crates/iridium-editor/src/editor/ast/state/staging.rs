//! Independent syntax state for commands that have not committed.
use super::SyntaxState;

impl SyntaxState {
    pub(in crate::editor) fn fork(&self) -> Result<Self, String> {
        Ok(Self {
            tree: self
                .tree
                .as_ref()
                .map(super::SyntaxTree::fork)
                .transpose()
                .map_err(|error| error.to_string())?,
            highlighter: self.highlighter.clone(),
            language: self.language,
            tracked_revision: self.tracked_revision,
            dirty: self.dirty,
            replaced: self.replaced,
            full_parses: self.full_parses,
            incremental_parses: self.incremental_parses,
            expand: self.expand.clone(),
            navigation: self.navigation.clone(),
            pending_edits: self.pending_edits.clone(),
            pending_delta: self.pending_delta.clone(),
        })
    }

    pub(in crate::editor) fn commit_expansion(&mut self, staged: Self) {
        self.expand = staged.expand;
        self.navigation = staged.navigation;
    }
}
