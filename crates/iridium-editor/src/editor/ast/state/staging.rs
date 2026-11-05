//! Independent syntax state for commands that have not committed.
use super::{SyntaxParser, SyntaxState};

impl SyntaxState {
    /// A copy that parses inline, for staging a command sequence.
    ///
    /// The background worker is not shared: a staged step needs the tree
    /// current before it reads scopes, and an answer from a thread fed by the
    /// live document would describe the wrong text. The copy gets a parser of
    /// its own and the tree as it stands, which tree-sitter shares rather than
    /// copies.
    ///
    /// The tree counts as edited only while a parse is still owed for it.
    /// With the parse in the background, `dirty` stays set after a parse
    /// lands, and copying it would make the first staged step reparse a tree
    /// that is already current.
    pub(in crate::editor) fn fork(&self) -> Result<Self, String> {
        let parser = match (self.parser.as_ref(), self.language) {
            (Some(_), Some(language)) => {
                Some(SyntaxParser::new(language).map_err(|error| error.to_string())?)
            },
            _ => None,
        };
        Ok(Self {
            parser,
            tree: self.tree.clone(),
            highlighter: self.highlighter.clone(),
            language: self.language,
            tracked_revision: self.tracked_revision,
            dirty: self.is_behind(),
            replaced: self.replaced,
            full_parses: self.full_parses,
            incremental_parses: self.incremental_parses,
            expand: self.expand.clone(),
            navigation: self.navigation.clone(),
            pending_delta: self.pending_delta.clone(),
            landed_folds: None,
            background: None,
        })
    }

    pub(in crate::editor) fn commit_expansion(&mut self, mut staged: Self) {
        // Taken rather than moved out: a state with a background worker drops
        // its tree there, so the type has a destructor and cannot be split.
        self.expand = std::mem::take(&mut staged.expand);
        self.navigation = std::mem::take(&mut staged.navigation);
    }
}
