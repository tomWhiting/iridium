//! Search and replace functionality.
//!
//! This module provides comprehensive search and replace functionality for the editor:
//!
//! - **Plain text search** with case-insensitive matching by default
//! - **Regular expression search** with full regex syntax support
//! - **Case-sensitive mode** for exact matching
//! - **Whole-word matching** to avoid partial matches
//! - **Match navigation** (next/previous with wrapping)
//! - **Replace operations** including single replacement and replace-all
//!
//! All replace operations generate reversible commands, enabling proper undo/redo support.
//! Replace-all creates a single compound command that can be undone in one step.
//!
//! # Example
//!
//! ```
//! use iridium_editor::search::{SearchOptions, SearchState, replace_all};
//! use iridium_editor::{Document, CursorState, Position};
//!
//! let doc = Document::new("foo bar foo baz");
//! let cursor = CursorState::at(Position::zero());
//!
//! // Find all matches
//! let mut state = SearchState::new();
//! state.find_all("foo", &SearchOptions::default(), &doc).unwrap();
//! assert_eq!(state.match_count(), 2);
//!
//! // Navigate between matches
//! state.next_match();
//! assert_eq!(state.current_match, Some(1));
//!
//! // Replace all matches (returns a single command for undo)
//! if let Some((cmd, result)) = replace_all(&state, "xxx", &doc, &cursor) {
//!     assert_eq!(result.count, 2);
//! }
//! ```

mod find;
mod replace;

pub use find::{SearchOptions, SearchState};
pub use replace::{
    ReplaceResult, replace_all, replace_at_index, replace_current, replace_in_selection,
};
