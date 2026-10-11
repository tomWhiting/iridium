//! Turning the active search into ordinary editable selections.

use super::{Editor, EditorEvent};
use crate::{Command, CursorState, Selection};

impl Editor {
    /// Selects every match of the current query, preserving its search options.
    ///
    /// Re-runs the query against the current document before using any ranges.
    /// The current match remains primary when it still exists. No matches leave
    /// the existing cursors alone. Selection is allowed in read-only documents
    /// and does not add an undo entry; the next edit records these selections.
    ///
    /// Returns the number of resulting cursors. Zero-width matches become
    /// carets and overlapping/touching carets follow the usual cursor rules.
    ///
    /// # Errors
    /// Returns the search engine's diagnostic for an invalid query. The search
    /// results are cleared and reported, but text, cursors and history remain.
    pub fn select_search_matches(&mut self) -> Result<usize, String> {
        let search = &self.state.search;
        if !search.is_active || search.query.is_empty() {
            return Ok(0);
        }
        let previous = search.current_range().copied();
        let query = search.query.clone();
        let options = search.options.clone();
        let result = self
            .state
            .search
            .find_all(&query, &options, &self.state.document);
        let caret = self.cursor();
        self.state.search.goto_nearest_match(caret);
        if let Some(index) = self
            .state
            .search
            .matches
            .iter()
            .position(|range| Some(*range) == previous)
        {
            self.state.search.current_match = Some(index);
        }
        self.emit(&EditorEvent::SearchUpdated {
            match_count: self.state.search.match_count(),
            current_index: self.state.search.current_match,
            error: self.state.search.error().map(str::to_owned),
            scope_ranges: self.state.search.scope_range_count(),
        });
        result?;

        let search = &self.state.search;
        let Some(primary) = search.current_range() else {
            return Ok(0);
        };
        let primary = Selection::new(primary.start, primary.end);
        let secondary = search
            .matches
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != search.current_match)
            .map(|(_, range)| Selection::new(range.start, range.end))
            .collect();
        let new_state = CursorState::from_selections(primary, secondary);
        let count = new_state.cursor_count();
        self.apply_command(Command::SetSelection {
            old_state: self.state.cursor.clone(),
            new_state,
        });
        Ok(count)
    }
}
