//! IME (Input Method Editor) composition handling.
//!
//! This module handles IME input for CJK (Chinese, Japanese, Korean) and other
//! languages that use input method composition. IME input works differently from
//! direct keyboard input:
//!
//! 1. User starts composing text (e.g., types pinyin for Chinese)
//! 2. Composition text is displayed with special styling (usually underlined)
//! 3. User selects final characters from candidates
//! 4. Composition is committed to the document
//!
//! The editor displays composition text inline but does not commit it to the
//! document until the IME signals completion.

use serde::{Deserialize, Serialize};

use crate::document::{CursorState, Document, Position};
use crate::history::Command;

/// IME composition state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ImeState {
    /// No IME composition is active
    #[default]
    Inactive,
    /// IME composition is in progress
    Composing {
        /// The composition text being entered
        text: String,
        /// Cursor position within composition (for multi-stage input)
        cursor: usize,
    },
    /// Composition is complete and ready to be committed
    Committed {
        /// The final text to insert
        text: String,
    },
}

/// An IME event from the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImeEvent {
    /// IME composition has started
    Enabled,

    /// IME preedit text has changed (composition in progress)
    Preedit {
        /// Current composition text
        text: String,
        /// Cursor position within the composition (if any)
        cursor: Option<usize>,
    },

    /// IME composition has been committed
    Commit {
        /// Final text to insert
        text: String,
    },

    /// IME composition was cancelled
    Disabled,
}

/// Result of handling an IME event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeResult {
    /// The event was handled, no document change needed
    Handled,

    /// The event produced a command that should be applied
    Command(Command),

    /// Composition state changed (for rendering updates)
    StateChanged(ImeState),

    /// The event was not handled
    Ignored,
}

/// IME composition handler for the editor.
///
/// This handler manages the IME composition lifecycle:
/// - Tracks composition state (inactive, composing, committed)
/// - Provides composition text for inline rendering
/// - Produces commands when composition is committed
///
/// # Platform Integration
///
/// The host platform is responsible for:
/// - Capturing raw IME events from the OS
/// - Converting them to [`ImeEvent`] and calling [`ImeHandler::handle_ime`]
/// - Rendering the composition text with appropriate styling
///
/// # Example
///
/// ```ignore
/// let mut ime = ImeHandler::new();
///
/// // User types pinyin "ni" for Chinese character 你
/// let result = ime.handle_ime(&ImeEvent::Preedit {
///     text: "ni".to_string(),
///     cursor: Some(2),
/// }, &document, &cursor);
///
/// // Composition text "ni" should be displayed inline (underlined)
///
/// // User selects the character 你
/// let result = ime.handle_ime(&ImeEvent::Commit {
///     text: "你".to_string(),
/// }, &document, &cursor);
///
/// // Result contains a Command to insert "你"
/// ```
#[derive(Debug, Default)]
pub struct ImeHandler {
    /// Current IME state
    state: ImeState,
    /// Position where composition started
    composition_start: Option<Position>,
}

impl ImeHandler {
    /// Creates a new IME handler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: ImeState::Inactive,
            composition_start: None,
        }
    }

    /// Returns the current IME state.
    #[must_use]
    pub const fn state(&self) -> &ImeState {
        &self.state
    }

    /// Returns true if IME composition is active.
    #[must_use]
    pub fn is_composing(&self) -> bool {
        matches!(self.state, ImeState::Composing { .. })
    }

    /// Returns the current composition text, if any.
    #[must_use]
    pub fn composition_text(&self) -> Option<&str> {
        match &self.state {
            ImeState::Composing { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Returns the cursor position within composition, if any.
    #[must_use]
    pub fn composition_cursor(&self) -> Option<usize> {
        match &self.state {
            ImeState::Composing { cursor, .. } => Some(*cursor),
            _ => None,
        }
    }

    /// Handles an IME event.
    ///
    /// Returns an [`ImeResult`] indicating what action should be taken.
    /// The caller is responsible for:
    /// - Applying any resulting commands
    /// - Updating the display to show composition text
    /// - Re-rendering the cursor position appropriately
    pub fn handle_ime(
        &mut self,
        event: &ImeEvent,
        document: &Document,
        cursor: &CursorState,
    ) -> ImeResult {
        match event {
            ImeEvent::Enabled => self.handle_enabled(cursor),
            ImeEvent::Preedit {
                text,
                cursor: ime_cursor,
            } => self.handle_preedit(text, *ime_cursor),
            ImeEvent::Commit { text } => self.handle_commit(text, document, cursor),
            ImeEvent::Disabled => self.handle_disabled(),
        }
    }

    /// Handles IME enabled event.
    fn handle_enabled(&mut self, cursor: &CursorState) -> ImeResult {
        self.composition_start = Some(cursor.primary.head);
        self.state = ImeState::Composing {
            text: String::new(),
            cursor: 0,
        };
        ImeResult::StateChanged(self.state.clone())
    }

    /// Handles preedit text update.
    fn handle_preedit(&mut self, text: &str, cursor: Option<usize>) -> ImeResult {
        let ime_cursor = cursor.unwrap_or(text.len());

        self.state = ImeState::Composing {
            text: text.to_string(),
            cursor: ime_cursor,
        };

        ImeResult::StateChanged(self.state.clone())
    }

    /// Handles IME commit.
    fn handle_commit(
        &mut self,
        text: &str,
        document: &Document,
        cursor: &CursorState,
    ) -> ImeResult {
        // Get position to insert at
        let insert_pos = self.composition_start.unwrap_or(cursor.primary.head);

        // Reset state
        self.state = ImeState::Inactive;
        self.composition_start = None;

        // If there's a selection, delete it first
        if !cursor.primary.is_collapsed() {
            let range = cursor.primary.range();
            let deleted = document.slice(range);

            let new_cursor_pos = compute_position_after_insert(range.start, text);
            let new_cursor = CursorState::at(new_cursor_pos);

            let commands = vec![
                Command::Delete {
                    range,
                    deleted_text: deleted,
                },
                Command::Insert {
                    position: range.start,
                    text: text.to_string(),
                },
                Command::SetSelection {
                    old_state: cursor.clone(),
                    new_state: new_cursor,
                },
            ];

            return ImeResult::Command(Command::Compound { commands });
        }

        // Simple insert at cursor position
        let new_cursor_pos = compute_position_after_insert(insert_pos, text);
        let new_cursor = CursorState::at(new_cursor_pos);

        let commands = vec![
            Command::Insert {
                position: insert_pos,
                text: text.to_string(),
            },
            Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            },
        ];

        ImeResult::Command(Command::Compound { commands })
    }

    /// Handles IME disabled (cancelled).
    fn handle_disabled(&mut self) -> ImeResult {
        self.state = ImeState::Inactive;
        self.composition_start = None;
        ImeResult::StateChanged(ImeState::Inactive)
    }

    /// Resets the IME handler state.
    ///
    /// This should be called when the editor loses focus or when
    /// a non-IME action is taken that should cancel composition.
    pub fn reset(&mut self) {
        self.state = ImeState::Inactive;
        self.composition_start = None;
    }
}

/// Computes cursor position after inserting text.
fn compute_position_after_insert(start: Position, text: &str) -> Position {
    let mut line = start.line;
    let mut column = start.column;

    for ch in text.chars() {
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

    #[test]
    fn ime_inactive_by_default() {
        let handler = ImeHandler::new();
        assert!(!handler.is_composing());
        assert_eq!(handler.composition_text(), None);
    }

    #[test]
    fn ime_preedit() {
        let mut handler = ImeHandler::new();
        let doc = Document::new("Hello");
        let cursor = CursorState::at(Position::new(0, 5));

        // Start composition
        let _ = handler.handle_ime(&ImeEvent::Enabled, &doc, &cursor);
        assert!(handler.is_composing());

        // Update preedit text
        let result = handler.handle_ime(
            &ImeEvent::Preedit {
                text: "ni".to_string(),
                cursor: Some(2),
            },
            &doc,
            &cursor,
        );

        assert!(handler.is_composing());
        assert_eq!(handler.composition_text(), Some("ni"));
        assert_eq!(handler.composition_cursor(), Some(2));

        if let ImeResult::StateChanged(ImeState::Composing { text, cursor }) = result {
            assert_eq!(text, "ni");
            assert_eq!(cursor, 2);
        } else {
            panic!("Expected StateChanged result");
        }
    }

    #[test]
    fn ime_commit() {
        let mut handler = ImeHandler::new();
        let doc = Document::new("Hello");
        let cursor = CursorState::at(Position::new(0, 5));

        // Start and preedit
        let _ = handler.handle_ime(&ImeEvent::Enabled, &doc, &cursor);
        let _ = handler.handle_ime(
            &ImeEvent::Preedit {
                text: "ni".to_string(),
                cursor: Some(2),
            },
            &doc,
            &cursor,
        );

        // Commit
        let result = handler.handle_ime(
            &ImeEvent::Commit {
                text: "你".to_string(),
            },
            &doc,
            &cursor,
        );

        assert!(!handler.is_composing());

        if let ImeResult::Command(Command::Compound { commands }) = result {
            assert!(
                commands
                    .iter()
                    .any(|c| matches!(c, Command::Insert { text, .. } if text == "你"))
            );
        } else {
            panic!("Expected Command result");
        }
    }

    #[test]
    fn ime_cancel() {
        let mut handler = ImeHandler::new();
        let doc = Document::new("Hello");
        let cursor = CursorState::at(Position::new(0, 5));

        // Start and preedit
        let _ = handler.handle_ime(&ImeEvent::Enabled, &doc, &cursor);
        let _ = handler.handle_ime(
            &ImeEvent::Preedit {
                text: "ni".to_string(),
                cursor: Some(2),
            },
            &doc,
            &cursor,
        );

        // Cancel
        let result = handler.handle_ime(&ImeEvent::Disabled, &doc, &cursor);

        assert!(!handler.is_composing());
        if let ImeResult::StateChanged(ImeState::Inactive) = result {
            // Good
        } else {
            panic!("Expected StateChanged to Inactive");
        }
    }
}
