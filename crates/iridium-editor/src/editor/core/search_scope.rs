//! Selected-text search is shared state, not a host-side result filter.
use super::{Editor, EditorEvent, EditorKeyResult, SearchAction};

impl Editor {
    /// Handles a search action from keyboard input.
    pub(super) fn handle_search_action(&mut self, action: SearchAction) -> EditorKeyResult {
        match action {
            SearchAction::SelectAllMatches => {
                if let Err(message) = self.select_search_matches() {
                    self.emit(&EditorEvent::Error {
                        message,
                        code: "SEARCH_FAILED".to_owned(),
                    });
                }
                EditorKeyResult::None
            },
            SearchAction::OpenInSelection => match self.search_in_selections() {
                Ok(()) => EditorKeyResult::Search(SearchAction::OpenSearch),
                Err(message) => {
                    self.emit(&EditorEvent::Error {
                        message,
                        code: "SEARCH_SCOPE_FAILED".to_owned(),
                    });
                    EditorKeyResult::None
                },
            },
            SearchAction::ClearScope => {
                self.clear_search_scope();
                EditorKeyResult::None
            },
            SearchAction::OpenSearch => EditorKeyResult::Search(action),
            SearchAction::CloseSearch => {
                self.close_search();
                EditorKeyResult::Search(action)
            },
            SearchAction::NextMatch => {
                self.goto_next_match();
                EditorKeyResult::None
            },
            SearchAction::PreviousMatch => {
                self.goto_previous_match();
                EditorKeyResult::None
            },
        }
    }

    /// Restricts the current and subsequent query to all nonempty selections.
    /// Matches keep full-document regex context and must fit entirely in a scope.
    /// Capturing a scope ends the typing group, so undo can restore its baseline.
    pub fn search_in_selections(&mut self) -> Result<(), String> {
        self.state.search.capture_scope(
            self.state.cursor.all_selections().copied(),
            &self.state.document,
            self.state.history.current_node_id(),
        )?;
        self.state.history.finish_group();
        self.state.refresh_search();
        self.emit_search_updated();
        Ok(())
    }

    /// Explicitly returns the current query to the whole document.
    pub fn clear_search_scope(&mut self) {
        self.state.search.clear_scope();
        self.state.refresh_search();
        self.emit_search_updated();
    }
}

#[cfg(test)]
mod tests;
