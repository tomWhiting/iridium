//! Reversible command definitions.

use serde::{Deserialize, Serialize};

use crate::document::{CursorState, Document, Position, Range};
use crate::editor::IridiumError;

/// A command that can be applied to the document.
///
/// All commands are reversible via [`Command::inverse()`], which enables
/// the undo tree functionality.
///
/// # Example
///
/// ```
/// use iridium_editor::{Command, Position};
///
/// let cmd = Command::Insert {
///     position: Position::new(0, 5),
///     text: "Hello".to_string(),
/// };
///
/// let inverse = cmd.inverse();
/// // inverse is a Delete command that removes "Hello"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Insert text at a position.
    Insert {
        /// Position to insert at
        position: Position,
        /// Text to insert
        text: String,
    },

    /// Delete text in a range.
    Delete {
        /// Range to delete
        range: Range,
        /// The deleted text (stored for undo)
        deleted_text: String,
    },

    /// Replace text in a range.
    Replace {
        /// Range to replace
        range: Range,
        /// Original text (stored for undo)
        old_text: String,
        /// New text
        new_text: String,
    },

    /// Change cursor/selection state.
    SetSelection {
        /// Previous cursor state
        old_state: CursorState,
        /// New cursor state
        new_state: CursorState,
    },

    /// A group of commands executed atomically.
    Compound {
        /// The commands in this group
        commands: Vec<Command>,
    },
}

impl Command {
    /// Applies this command to the document and cursor state.
    ///
    /// This method mutates the document according to the command type:
    /// - `Insert`: Inserts text at the specified position
    /// - `Delete`: Deletes text in the specified range
    /// - `Replace`: Replaces text in the specified range with new text
    /// - `SetSelection`: Updates the cursor/selection state
    /// - `Compound`: Applies all sub-commands in order
    ///
    /// # Errors
    ///
    /// Returns an error if the command references invalid positions or ranges
    /// in the document.
    ///
    /// # Example
    ///
    /// ```
    /// use iridium_editor::{Command, Document, CursorState, Position};
    ///
    /// let mut doc = Document::new("Hello World");
    /// let mut cursor = CursorState::at(Position::zero());
    ///
    /// let cmd = Command::Insert {
    ///     position: Position::new(0, 5),
    ///     text: ",".to_string(),
    /// };
    ///
    /// cmd.apply(&mut doc, &mut cursor).unwrap();
    /// assert_eq!(doc.text(), "Hello, World");
    /// ```
    pub fn apply(&self, document: &mut Document, cursor: &mut CursorState) -> Result<(), IridiumError> {
        match self {
            Self::Insert { position, text } => {
                document.insert(*position, text)?;
            }

            Self::Delete { range, .. } => {
                document.delete(*range)?;
            }

            Self::Replace { range, new_text, .. } => {
                document.replace(*range, new_text)?;
            }

            Self::SetSelection { new_state, .. } => {
                *cursor = new_state.clone();
            }

            Self::Compound { commands } => {
                for cmd in commands {
                    cmd.apply(document, cursor)?;
                }
            }
        }

        Ok(())
    }

    /// Returns the inverse of this command.
    ///
    /// Applying the inverse undoes the effect of this command.
    #[must_use]
    pub fn inverse(&self) -> Self {
        match self {
            Self::Insert { position, text } => {
                let end = Self::compute_end_position(*position, text);
                Self::Delete {
                    range: Range::new(*position, end),
                    deleted_text: text.clone(),
                }
            }

            Self::Delete { range, deleted_text } => Self::Insert {
                position: range.start,
                text: deleted_text.clone(),
            },

            Self::Replace { range, old_text, new_text } => {
                let new_end = Self::compute_end_position(range.start, new_text);
                Self::Replace {
                    range: Range::new(range.start, new_end),
                    old_text: new_text.clone(),
                    new_text: old_text.clone(),
                }
            }

            Self::SetSelection { old_state, new_state } => Self::SetSelection {
                old_state: new_state.clone(),
                new_state: old_state.clone(),
            },

            Self::Compound { commands } => Self::Compound {
                commands: commands.iter().rev().map(Self::inverse).collect(),
            },
        }
    }

    /// Returns true if this command has no effect.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Insert { text, .. } => text.is_empty(),
            Self::Delete { deleted_text, .. } => deleted_text.is_empty(),
            Self::Replace { old_text, new_text, .. } => old_text == new_text,
            Self::SetSelection { old_state, new_state } => old_state == new_state,
            Self::Compound { commands } => commands.is_empty() || commands.iter().all(Self::is_empty),
        }
    }

    /// Computes the end position after inserting text at a position.
    fn compute_end_position(start: Position, text: &str) -> Position {
        let mut line = start.line;
        let mut column = start.column;

        for ch in text.chars() {
            if ch == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }

        Position::new(line, column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Selection;

    #[test]
    fn insert_inverse() {
        let cmd = Command::Insert {
            position: Position::new(0, 0),
            text: "Hello".to_string(),
        };

        let inverse = cmd.inverse();

        match inverse {
            Command::Delete { range, deleted_text } => {
                assert_eq!(range.start, Position::new(0, 0));
                assert_eq!(range.end, Position::new(0, 5));
                assert_eq!(deleted_text, "Hello");
            }
            _ => panic!("Expected Delete command"),
        }
    }

    #[test]
    fn delete_inverse() {
        let cmd = Command::Delete {
            range: Range::new(Position::new(0, 0), Position::new(0, 5)),
            deleted_text: "Hello".to_string(),
        };

        let inverse = cmd.inverse();

        match inverse {
            Command::Insert { position, text } => {
                assert_eq!(position, Position::new(0, 0));
                assert_eq!(text, "Hello");
            }
            _ => panic!("Expected Insert command"),
        }
    }

    #[test]
    fn selection_inverse() {
        let old = CursorState::at(Position::new(0, 0));
        let new = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 5)));

        let cmd = Command::SetSelection { old_state: old.clone(), new_state: new.clone() };

        let inverse = cmd.inverse();

        match inverse {
            Command::SetSelection { old_state, new_state } => {
                assert_eq!(old_state, new);
                assert_eq!(new_state, old);
            }
            _ => panic!("Expected SetSelection command"),
        }
    }

    #[test]
    fn empty_commands() {
        assert!(Command::Insert { position: Position::zero(), text: String::new() }.is_empty());

        assert!(
            Command::Delete { range: Range::empty(Position::zero()), deleted_text: String::new() }.is_empty()
        );

        assert!(Command::Compound { commands: vec![] }.is_empty());
    }

    #[test]
    fn apply_insert() {
        let mut doc = Document::new("Hello World");
        let mut cursor = CursorState::at(Position::zero());

        let cmd = Command::Insert {
            position: Position::new(0, 5),
            text: ",".to_string(),
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Hello, World");
    }

    #[test]
    fn apply_delete() {
        let mut doc = Document::new("Hello, World");
        let mut cursor = CursorState::at(Position::zero());

        let cmd = Command::Delete {
            range: Range::new(Position::new(0, 5), Position::new(0, 6)),
            deleted_text: ",".to_string(),
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Hello World");
    }

    #[test]
    fn apply_replace() {
        let mut doc = Document::new("Hello World");
        let mut cursor = CursorState::at(Position::zero());

        let cmd = Command::Replace {
            range: Range::new(Position::new(0, 6), Position::new(0, 11)),
            old_text: "World".to_string(),
            new_text: "Rust".to_string(),
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Hello Rust");
    }

    #[test]
    fn apply_set_selection() {
        let mut doc = Document::new("Hello");
        let mut cursor = CursorState::at(Position::zero());

        let new_state = CursorState::new(Selection::new(Position::new(0, 0), Position::new(0, 5)));

        let cmd = Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_state.clone(),
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(cursor, new_state);
    }

    #[test]
    fn apply_compound() {
        let mut doc = Document::new("Hello");
        let mut cursor = CursorState::at(Position::zero());

        let cmd = Command::Compound {
            commands: vec![
                Command::Insert {
                    position: Position::new(0, 5),
                    text: " World".to_string(),
                },
                Command::Insert {
                    position: Position::new(0, 0),
                    text: "Say: ".to_string(),
                },
            ],
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Say: Hello World");
    }

    #[test]
    fn apply_and_inverse_roundtrip() {
        let mut doc = Document::new("Hello");
        let mut cursor = CursorState::at(Position::zero());

        let cmd = Command::Insert {
            position: Position::new(0, 5),
            text: " World".to_string(),
        };

        cmd.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Hello World");

        let inverse = cmd.inverse();
        inverse.apply(&mut doc, &mut cursor).unwrap();
        assert_eq!(doc.text(), "Hello");
    }
}
