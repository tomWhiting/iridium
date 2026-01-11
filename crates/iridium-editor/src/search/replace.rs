//! Replace functionality.
//!
//! This module provides replace operations that work with the search module
//! to replace matched text. All replacements are performed through commands,
//! enabling proper undo/redo support.
//!
//! # Example
//!
//! ```
//! use iridium_editor::search::{SearchOptions, SearchState, ReplaceResult};
//! use iridium_editor::Document;
//!
//! let mut doc = Document::new("foo bar foo");
//! let mut state = SearchState::new();
//! state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
//!
//! // Replace operations are done through the Editor or via commands
//! ```

use crate::document::{CursorState, Document, Position, Range, Selection};
use crate::history::Command;

use super::SearchState;

/// Result of a replace operation.
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// Number of replacements made
    pub count: usize,
    /// Text that was replaced
    pub replaced_text: Vec<String>,
}

impl ReplaceResult {
    /// Creates a new replace result.
    #[must_use]
    pub const fn new(count: usize, replaced_text: Vec<String>) -> Self {
        Self {
            count,
            replaced_text,
        }
    }

    /// Creates an empty result (no replacements).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            count: 0,
            replaced_text: Vec::new(),
        }
    }
}

/// Creates a command to replace the current match with the replacement text.
///
/// This function creates a compound command that:
/// 1. Deletes the current match
/// 2. Inserts the replacement text
/// 3. Updates the cursor position
///
/// The caller is responsible for applying the command and updating the search state.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `replacement` - The text to replace the match with
/// * `document` - The document (for reading the text to be replaced)
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some(Command)` if there is a current match to replace,
/// or `None` if there are no matches or no current match.
#[must_use]
pub fn replace_current(
    search_state: &SearchState,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<Command> {
    let range = search_state.current_range()?;

    let old_text = document.slice(*range);
    let new_pos = compute_position_after_replace(range.start, replacement);
    let new_cursor = CursorState::at(new_pos);

    let commands = vec![
        Command::Replace {
            range: *range,
            old_text,
            new_text: replacement.to_string(),
        },
        Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        },
    ];

    Some(Command::Compound { commands })
}

/// Creates a command to replace all matches with the replacement text.
///
/// This function creates a single compound command containing all replacements,
/// which means the entire replace-all operation can be undone in one step
/// (per FR-030).
///
/// Replacements are applied from the end of the document to the beginning
/// to ensure that byte offsets remain valid throughout the operation.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `replacement` - The text to replace each match with
/// * `document` - The document (for reading the text to be replaced)
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some((Command, ReplaceResult))` if there are matches to replace,
/// or `None` if there are no matches.
#[must_use]
pub fn replace_all(
    search_state: &SearchState,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<(Command, ReplaceResult)> {
    if search_state.matches.is_empty() {
        return None;
    }

    let mut commands = Vec::new();
    let mut replaced_text = Vec::new();

    // Process matches in reverse order (end to start) so positions remain valid
    for range in search_state.matches.iter().rev() {
        let old_text = document.slice(*range);
        replaced_text.push(old_text.clone());

        commands.push(Command::Replace {
            range: *range,
            old_text,
            new_text: replacement.to_string(),
        });
    }

    // Reverse replaced_text to match original order
    replaced_text.reverse();

    // Move cursor to end of first replacement
    let first_match = &search_state.matches[0];
    let new_pos = compute_position_after_replace(first_match.start, replacement);
    let new_cursor = CursorState::at(new_pos);

    commands.push(Command::SetSelection {
        old_state: cursor.clone(),
        new_state: new_cursor,
    });

    let count = search_state.matches.len();
    let result = ReplaceResult::new(count, replaced_text);

    Some((Command::Compound { commands }, result))
}

/// Creates commands to replace a specific match by index.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `index` - The index of the match to replace
/// * `replacement` - The text to replace the match with
/// * `document` - The document
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some(Command)` if the index is valid, or `None` otherwise.
#[must_use]
pub fn replace_at_index(
    search_state: &SearchState,
    index: usize,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<Command> {
    let range = search_state.matches.get(index)?;

    let old_text = document.slice(*range);
    let new_pos = compute_position_after_replace(range.start, replacement);
    let new_cursor = CursorState::at(new_pos);

    let commands = vec![
        Command::Replace {
            range: *range,
            old_text,
            new_text: replacement.to_string(),
        },
        Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        },
    ];

    Some(Command::Compound { commands })
}

/// Creates a command to replace all matches in a selection with replacement text.
///
/// Only replaces matches whose ranges are fully contained within the selection.
///
/// # Arguments
///
/// * `search_state` - The current search state with matches
/// * `selection` - The selection range to restrict replacements to
/// * `replacement` - The text to replace each match with
/// * `document` - The document
/// * `cursor` - The current cursor state
///
/// # Returns
///
/// Returns `Some((Command, ReplaceResult))` if there are matches in the selection,
/// or `None` if there are no matches in the selection.
#[must_use]
pub fn replace_in_selection(
    search_state: &SearchState,
    selection: &Selection,
    replacement: &str,
    document: &Document,
    cursor: &CursorState,
) -> Option<(Command, ReplaceResult)> {
    let sel_range = selection.range();

    // Filter matches that are fully within the selection
    let matches_in_selection: Vec<&Range> = search_state
        .matches
        .iter()
        .filter(|r| r.start >= sel_range.start && r.end <= sel_range.end)
        .collect();

    if matches_in_selection.is_empty() {
        return None;
    }

    let mut commands = Vec::new();
    let mut replaced_text = Vec::new();

    // Process matches in reverse order (end to start)
    for range in matches_in_selection.iter().rev() {
        let old_text = document.slice(**range);
        replaced_text.push(old_text.clone());

        commands.push(Command::Replace {
            range: **range,
            old_text,
            new_text: replacement.to_string(),
        });
    }

    // Reverse replaced_text to match original order
    replaced_text.reverse();

    // Cursor stays at its current position
    commands.push(Command::SetSelection {
        old_state: cursor.clone(),
        new_state: cursor.clone(),
    });

    let count = matches_in_selection.len();
    let result = ReplaceResult::new(count, replaced_text);

    Some((Command::Compound { commands }, result))
}

/// Computes the cursor position after a replacement.
fn compute_position_after_replace(start: Position, replacement: &str) -> Position {
    let mut line = start.line;
    let mut column = start.column;

    for ch in replacement.chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else if ch == '\r' {
            // Skip CR in CRLF
        } else {
            column += 1;
        }
    }

    Position::new(line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchOptions;

    fn setup_search(doc: &Document, query: &str) -> SearchState {
        let mut state = SearchState::new();
        state
            .find_all(query, &SearchOptions::default(), doc)
            .unwrap();
        state
    }

    #[test]
    fn replace_current_basic() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "baz", &doc, &cursor);
        assert!(cmd.is_some());

        let cmd = cmd.unwrap();
        if let Command::Compound { commands } = cmd {
            assert_eq!(commands.len(), 2);
            // First should be Replace
            assert!(matches!(&commands[0], Command::Replace { .. }));
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn replace_current_no_matches() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "xyz");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "baz", &doc, &cursor);
        assert!(cmd.is_none());
    }

    #[test]
    fn replace_all_basic() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let result = replace_all(&search, "baz", &doc, &cursor);
        assert!(result.is_some());

        let (cmd, replace_result) = result.unwrap();
        assert_eq!(replace_result.count, 2);
        assert_eq!(replace_result.replaced_text.len(), 2);

        if let Command::Compound { commands } = cmd {
            // 2 Replace commands + 1 SetSelection
            assert_eq!(commands.len(), 3);
        } else {
            panic!("Expected Compound command");
        }
    }

    #[test]
    fn replace_all_no_matches() {
        let doc = Document::new("foo bar");
        let search = setup_search(&doc, "xyz");
        let cursor = CursorState::at(Position::zero());

        let result = replace_all(&search, "baz", &doc, &cursor);
        assert!(result.is_none());
    }

    #[test]
    fn replace_all_single_undo() {
        // Verify that replace_all creates a single compound command
        let doc = Document::new("a a a");
        let search = setup_search(&doc, "a");
        let cursor = CursorState::at(Position::zero());

        let (cmd, _) = replace_all(&search, "b", &doc, &cursor).unwrap();

        // The command should be a single Compound, not nested
        assert!(matches!(cmd, Command::Compound { .. }));

        // Apply the command
        let mut new_doc = doc.clone();
        let mut new_cursor = cursor.clone();
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();

        assert_eq!(new_doc.text(), "b b b");

        // Apply inverse (undo)
        cmd.inverse().apply(&mut new_doc, &mut new_cursor).unwrap();
        assert_eq!(new_doc.text(), "a a a");
    }

    #[test]
    fn replace_at_index_valid() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_at_index(&search, 1, "baz", &doc, &cursor);
        assert!(cmd.is_some());
    }

    #[test]
    fn replace_at_index_invalid() {
        let doc = Document::new("foo bar foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_at_index(&search, 10, "baz", &doc, &cursor);
        assert!(cmd.is_none());
    }

    #[test]
    fn replace_in_selection_filters_correctly() {
        let doc = Document::new("foo bar foo baz foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        // Select only the middle part: "bar foo baz"
        let selection = Selection::new(Position::new(0, 4), Position::new(0, 15));

        let result = replace_in_selection(&search, &selection, "xxx", &doc, &cursor);
        assert!(result.is_some());

        let (_, replace_result) = result.unwrap();
        // Only the middle "foo" should be replaced
        assert_eq!(replace_result.count, 1);
    }

    #[test]
    fn replace_preserves_order() {
        // Verify replacements happen in correct order for undo
        let doc = Document::new("aaa");
        let search = setup_search(&doc, "a");
        let cursor = CursorState::at(Position::zero());

        let (cmd, result) = replace_all(&search, "bb", &doc, &cursor).unwrap();

        assert_eq!(result.count, 3);
        assert_eq!(result.replaced_text, vec!["a", "a", "a"]);

        // Apply
        let mut new_doc = doc.clone();
        let mut new_cursor = cursor.clone();
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();
        assert_eq!(new_doc.text(), "bbbbbb");
    }

    #[test]
    fn multiline_replacement() {
        let doc = Document::new("foo");
        let search = setup_search(&doc, "foo");
        let cursor = CursorState::at(Position::zero());

        let cmd = replace_current(&search, "bar\nbaz", &doc, &cursor).unwrap();

        let mut new_doc = doc.clone();
        let mut new_cursor = cursor.clone();
        cmd.apply(&mut new_doc, &mut new_cursor).unwrap();

        assert_eq!(new_doc.text(), "bar\nbaz");
        assert_eq!(new_doc.line_count(), 2);

        // Cursor should be at end of replacement
        assert_eq!(new_cursor.primary.head, Position::new(1, 3));
    }
}
