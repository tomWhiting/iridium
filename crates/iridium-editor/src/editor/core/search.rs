//! Editor search commands and their observable outcomes.
use super::{Editor, EditorEvent};
use crate::document::CursorState;
use crate::search::{SearchOptions, SearchState, replace_all, replace_current};

impl Editor {
    // ==================== Search Methods (T121-T126) ====================

    /// Starts a search with the given query and options.
    ///
    /// This finds all matches in the document and updates the search state.
    /// The current match is set to the first match found (if any).
    ///
    /// # Arguments
    ///
    /// * `query` - The search query (plain text or regex pattern)
    /// * `options` - Search options controlling matching behavior
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(String)` if the regex is invalid.
    pub fn find(&mut self, query: &str, options: &SearchOptions) -> Result<(), String> {
        let result = self
            .state
            .search
            .find_all(query, options, &self.state.document);
        if result.is_err() {
            self.emit_search_updated();
            return result;
        }

        // Move to nearest match from current cursor position
        if self.state.search.has_matches() {
            self.state
                .search
                .goto_nearest_match(self.state.cursor.primary.head);

            // Move cursor to current match
            if let Some(range) = self.state.search.current_range() {
                self.keyboard_handler.reset_vertical_state();
                self.keyboard_handler.invalidate_cursor_order();
                self.state.cursor = CursorState::at(range.start);
                self.emit_selection_changed();
            }
        }

        self.emit_search_updated();
        Ok(())
    }

    /// Updates the search query incrementally.
    ///
    /// This is useful for live search where the query is updated as the user types.
    ///
    /// # Arguments
    ///
    /// * `query` - The new search query
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the matches changed, `Ok(false)` otherwise,
    /// or `Err(String)` if the regex is invalid.
    pub fn update_search(&mut self, query: &str) -> Result<bool, String> {
        let result = self.state.search.update_query(query, &self.state.document);
        self.emit_search_updated();
        result
    }

    /// Sets search options and re-runs the search.
    pub fn set_search_options(&mut self, options: SearchOptions) -> Result<(), String> {
        let result = self.state.search.set_options(options, &self.state.document);
        self.emit_search_updated();
        result
    }

    /// Goes to the next search match.
    ///
    /// Wraps around to the first match when at the end.
    pub fn goto_next_match(&mut self) {
        self.state.search.next_match();

        if let Some(range) = self.state.search.current_range() {
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Goes to the previous search match.
    ///
    /// Wraps around to the last match when at the beginning.
    pub fn goto_previous_match(&mut self) {
        self.state.search.previous_match();

        if let Some(range) = self.state.search.current_range() {
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.state.cursor = CursorState::at(range.start);
            self.emit_selection_changed();
        }

        self.emit_search_updated();
    }

    /// Replaces the current match with the replacement text.
    ///
    /// After replacement, moves to the next match.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace the current match with
    ///
    /// # Returns
    ///
    /// Returns `true` if a replacement was made, `false` otherwise.
    pub fn replace_current_match(&mut self, replacement: &str) -> bool {
        if self.state.search.refresh_if_stale(&self.state.document) {
            self.emit_search_updated();
        }
        if self.state.read_only {
            return false;
        }

        let cmd = replace_current(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some(cmd) = cmd {
            self.state.history.finish_group();
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.apply_command_internal(cmd);
            self.state.history.finish_group();

            // Applying the edit already refreshed the search. Restart its
            // navigation at the first result without scanning a second time.
            self.state.search.current_match = None;

            // Move to next match
            self.goto_next_match();
            true
        } else {
            false
        }
    }

    /// Replaces all matches with the replacement text.
    ///
    /// This is a single undoable operation per FR-030.
    ///
    /// # Arguments
    ///
    /// * `replacement` - The text to replace each match with
    ///
    /// # Returns
    ///
    /// Returns the number of replacements made.
    pub fn replace_all_matches(&mut self, replacement: &str) -> usize {
        if self.state.search.refresh_if_stale(&self.state.document) {
            self.emit_search_updated();
        }
        if self.state.read_only {
            return 0;
        }

        let result = replace_all(
            &self.state.search,
            replacement,
            &self.state.document,
            &self.state.cursor,
        );

        if let Some((cmd, replace_result)) = result {
            self.state.history.finish_group();
            // The replace command carries a SetSelection that moves the
            // cursor without going through handle_key.
            self.keyboard_handler.reset_vertical_state();
            self.keyboard_handler.invalidate_cursor_order();
            self.apply_command_internal(cmd);
            self.state.history.finish_group();

            // Clear search since all matches are replaced
            if self.state.search.scope_range_count().is_none() {
                self.close_search();
            }

            replace_result.count
        } else {
            0
        }
    }

    /// Closes/clears the current search.
    pub fn close_search(&mut self) {
        self.state.search.clear();
        self.emit_search_updated();
    }

    /// Returns true if a search is currently active.
    #[must_use]
    pub const fn is_searching(&self) -> bool {
        self.state.search.is_active
    }

    /// Returns the current search state.
    #[must_use]
    pub const fn search_state(&self) -> &SearchState {
        &self.state.search
    }

    /// Returns the number of search matches.
    #[must_use]
    pub fn search_match_count(&self) -> usize {
        self.state.search.match_count()
    }

    /// Returns the current match index (0-based), if any.
    #[must_use]
    pub const fn current_match_index(&self) -> Option<usize> {
        self.state.search.current_match
    }

    /// Emits a search updated event (T125).
    pub(super) fn emit_search_updated(&self) {
        self.emit(&EditorEvent::SearchUpdated {
            match_count: self.state.search.match_count(),
            current_index: self.state.search.current_match,
            error: self.state.search.error().map(str::to_owned),
            scope_ranges: self.state.search.scope_range_count(),
        });
    }
}

#[cfg(test)]
mod tests;
