//! Edit commands for undo/redo.
//!
//! Commands represent atomic edit operations that can be applied and
//! unapplied (undone). Each command stores enough information to reverse
//! the operation.

use crate::buffer::Buffer;

/// An atomic edit command that can be applied and unapplied.
///
/// Commands are the fundamental unit of the undo/redo system. Each command
/// represents a single edit operation and stores sufficient information
/// to both redo (apply) and undo (unapply) the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Insert text at a position.
    ///
    /// When applied, inserts the text at the position.
    /// When unapplied, removes the text starting at the position.
    Insert {
        /// The character position where text was inserted.
        position: usize,
        /// The text that was inserted.
        text: String,
    },

    /// Delete text at a position.
    ///
    /// When applied, removes the text starting at the position.
    /// When unapplied, re-inserts the text at the position.
    Delete {
        /// The character position where text was deleted.
        position: usize,
        /// The text that was deleted (stored for undo).
        text: String,
    },

    /// Replace text at a position.
    ///
    /// Combines a delete and insert into a single command, useful for
    /// selection replacement operations.
    Replace {
        /// The character position where replacement occurred.
        position: usize,
        /// The original text that was replaced (stored for undo).
        old_text: String,
        /// The new text that replaced the original.
        new_text: String,
    },

    /// A group of commands that should be undone/redone together.
    ///
    /// Compound commands are treated as a single unit for undo/redo.
    /// This is useful for operations like "find and replace all" that
    /// make multiple edits.
    Compound {
        /// The commands in this group, in order of application.
        commands: Vec<Command>,
    },
}

impl Command {
    /// Creates a new insert command.
    #[must_use]
    pub fn insert(position: usize, text: impl Into<String>) -> Self {
        Self::Insert {
            position,
            text: text.into(),
        }
    }

    /// Creates a new delete command.
    #[must_use]
    pub fn delete(position: usize, text: impl Into<String>) -> Self {
        Self::Delete {
            position,
            text: text.into(),
        }
    }

    /// Creates a new replace command.
    #[must_use]
    pub fn replace(
        position: usize,
        old_text: impl Into<String>,
        new_text: impl Into<String>,
    ) -> Self {
        Self::Replace {
            position,
            old_text: old_text.into(),
            new_text: new_text.into(),
        }
    }

    /// Creates a compound command from multiple commands.
    #[must_use]
    pub fn compound(commands: Vec<Command>) -> Self {
        Self::Compound { commands }
    }

    /// Applies this command to the buffer (redo).
    ///
    /// This performs the edit operation described by the command.
    pub fn apply(&self, buffer: &mut Buffer) {
        match self {
            Command::Insert { position, text } => {
                buffer.insert(*position, text);
            }
            Command::Delete { position, text } => {
                let end = *position + text.chars().count();
                buffer.remove(*position, end);
            }
            Command::Replace {
                position,
                old_text,
                new_text,
            } => {
                let end = *position + old_text.chars().count();
                buffer.remove(*position, end);
                buffer.insert(*position, new_text);
            }
            Command::Compound { commands } => {
                // Apply commands in order
                for cmd in commands {
                    cmd.apply(buffer);
                }
            }
        }
    }

    /// Unapplies this command from the buffer (undo).
    ///
    /// This reverses the edit operation described by the command.
    pub fn unapply(&self, buffer: &mut Buffer) {
        match self {
            Command::Insert { position, text } => {
                // To undo an insert, we delete the inserted text
                let end = *position + text.chars().count();
                buffer.remove(*position, end);
            }
            Command::Delete { position, text } => {
                // To undo a delete, we re-insert the deleted text
                buffer.insert(*position, text);
            }
            Command::Replace {
                position,
                old_text,
                new_text,
            } => {
                // To undo a replace, we replace the new text with the old text
                let end = *position + new_text.chars().count();
                buffer.remove(*position, end);
                buffer.insert(*position, old_text);
            }
            Command::Compound { commands } => {
                // Unapply commands in reverse order
                for cmd in commands.iter().rev() {
                    cmd.unapply(buffer);
                }
            }
        }
    }

    /// Returns true if this command has no effect.
    ///
    /// Empty commands can be skipped during application.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Command::Insert { text, .. } => text.is_empty(),
            Command::Delete { text, .. } => text.is_empty(),
            Command::Replace {
                old_text, new_text, ..
            } => old_text.is_empty() && new_text.is_empty(),
            Command::Compound { commands } => {
                commands.is_empty() || commands.iter().all(|c| c.is_empty())
            }
        }
    }

    /// Returns the inverted command (for implementing undo via redo).
    ///
    /// The inverted command, when applied, has the same effect as
    /// unapplying the original command.
    #[must_use]
    pub fn inverted(&self) -> Self {
        match self {
            Command::Insert { position, text } => Command::Delete {
                position: *position,
                text: text.clone(),
            },
            Command::Delete { position, text } => Command::Insert {
                position: *position,
                text: text.clone(),
            },
            Command::Replace {
                position,
                old_text,
                new_text,
            } => Command::Replace {
                position: *position,
                old_text: new_text.clone(),
                new_text: old_text.clone(),
            },
            Command::Compound { commands } => Command::Compound {
                commands: commands.iter().rev().map(|c| c.inverted()).collect(),
            },
        }
    }

    /// Returns the starting position of this command's effect in the buffer.
    #[must_use]
    pub fn position(&self) -> usize {
        match self {
            Command::Insert { position, .. }
            | Command::Delete { position, .. }
            | Command::Replace { position, .. } => *position,
            Command::Compound { commands } => commands.first().map(|c| c.position()).unwrap_or(0),
        }
    }

    /// Attempts to merge this command with another command.
    ///
    /// Returns Some with the merged command if the commands can be combined,
    /// or None if they cannot be merged.
    ///
    /// Commands can be merged if they are the same type and adjacent.
    /// This is used for grouping rapid keystrokes into a single undo operation.
    #[must_use]
    pub fn try_merge(&self, other: &Command) -> Option<Command> {
        match (self, other) {
            // Merge adjacent inserts
            (
                Command::Insert {
                    position: p1,
                    text: t1,
                },
                Command::Insert {
                    position: p2,
                    text: t2,
                },
            ) => {
                // Check if second insert is immediately after first
                if *p2 == *p1 + t1.chars().count() {
                    Some(Command::Insert {
                        position: *p1,
                        text: format!("{t1}{t2}"),
                    })
                } else {
                    None
                }
            }
            // Merge adjacent deletes (backwards, like backspace)
            (
                Command::Delete {
                    position: p1,
                    text: t1,
                },
                Command::Delete {
                    position: p2,
                    text: t2,
                },
            ) => {
                // Check if second delete is immediately before first (backspace pattern)
                if *p1 == *p2 + t2.chars().count() {
                    Some(Command::Delete {
                        position: *p2,
                        text: format!("{t2}{t1}"),
                    })
                }
                // Check if second delete is at same position (delete key pattern)
                else if *p1 == *p2 {
                    Some(Command::Delete {
                        position: *p1,
                        text: format!("{t1}{t2}"),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_apply() {
        let mut buffer = Buffer::from("Hello");
        let cmd = Command::insert(5, ", world!");
        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, world!");
    }

    #[test]
    fn test_insert_unapply() {
        let mut buffer = Buffer::from("Hello, world!");
        let cmd = Command::insert(5, ", world");
        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello!");
    }

    #[test]
    fn test_delete_apply() {
        let mut buffer = Buffer::from("Hello, world!");
        let cmd = Command::delete(5, ", world");
        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello!");
    }

    #[test]
    fn test_delete_unapply() {
        let mut buffer = Buffer::from("Hello!");
        let cmd = Command::delete(5, ", world");
        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, world!");
    }

    #[test]
    fn test_replace_apply() {
        let mut buffer = Buffer::from("Hello, world!");
        let cmd = Command::replace(7, "world", "Rust");
        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, Rust!");
    }

    #[test]
    fn test_replace_unapply() {
        let mut buffer = Buffer::from("Hello, Rust!");
        let cmd = Command::replace(7, "world", "Rust");
        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, world!");
    }

    #[test]
    fn test_compound_apply() {
        let mut buffer = Buffer::from("Hello");
        let cmd = Command::compound(vec![Command::insert(5, "!"), Command::insert(0, "Say: ")]);
        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Say: Hello!");
    }

    #[test]
    fn test_compound_unapply() {
        let mut buffer = Buffer::from("Say: Hello!");
        let cmd = Command::compound(vec![Command::insert(5, "!"), Command::insert(0, "Say: ")]);
        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello");
    }

    #[test]
    fn test_is_empty() {
        assert!(Command::insert(0, "").is_empty());
        assert!(Command::delete(0, "").is_empty());
        assert!(Command::replace(0, "", "").is_empty());
        assert!(Command::compound(vec![]).is_empty());
        assert!(Command::compound(vec![Command::insert(0, "")]).is_empty());

        assert!(!Command::insert(0, "x").is_empty());
        assert!(!Command::delete(0, "x").is_empty());
    }

    #[test]
    fn test_inverted() {
        let insert = Command::insert(5, "text");
        let delete = Command::delete(5, "text");

        assert_eq!(insert.inverted(), delete);
        assert_eq!(delete.inverted(), insert);

        let replace = Command::replace(0, "old", "new");
        let replace_inv = replace.inverted();
        assert!(
            matches!(replace_inv, Command::Replace { old_text, new_text, .. }
            if old_text == "new" && new_text == "old")
        );
    }

    #[test]
    fn test_inverted_compound() {
        let compound = Command::compound(vec![Command::insert(0, "A"), Command::insert(1, "B")]);
        let inverted = compound.inverted();

        // Should be reversed order of inverted commands
        if let Command::Compound { commands } = inverted {
            assert_eq!(commands.len(), 2);
            assert!(matches!(&commands[0], Command::Delete { position: 1, text } if text == "B"));
            assert!(matches!(&commands[1], Command::Delete { position: 0, text } if text == "A"));
        } else {
            panic!("Expected Compound");
        }
    }

    #[test]
    fn test_try_merge_adjacent_inserts() {
        let cmd1 = Command::insert(0, "Hello");
        let cmd2 = Command::insert(5, ", world!");

        let merged = cmd1.try_merge(&cmd2);
        assert!(merged.is_some());
        let merged = merged.unwrap();
        assert!(matches!(merged, Command::Insert { position: 0, text } if text == "Hello, world!"));
    }

    #[test]
    fn test_try_merge_non_adjacent_inserts() {
        let cmd1 = Command::insert(0, "Hello");
        let cmd2 = Command::insert(10, "World"); // Gap

        let merged = cmd1.try_merge(&cmd2);
        assert!(merged.is_none());
    }

    #[test]
    fn test_try_merge_backspace_deletes() {
        let cmd1 = Command::delete(5, "o"); // Deleted 'o' at position 5
        let cmd2 = Command::delete(4, "l"); // Deleted 'l' at position 4 (backspace)

        let merged = cmd1.try_merge(&cmd2);
        assert!(merged.is_some());
        let merged = merged.unwrap();
        assert!(matches!(merged, Command::Delete { position: 4, text } if text == "lo"));
    }

    #[test]
    fn test_try_merge_delete_key_deletes() {
        let cmd1 = Command::delete(5, "w");
        let cmd2 = Command::delete(5, "o"); // Same position (delete key)

        let merged = cmd1.try_merge(&cmd2);
        assert!(merged.is_some());
        let merged = merged.unwrap();
        assert!(matches!(merged, Command::Delete { position: 5, text } if text == "wo"));
    }

    #[test]
    fn test_try_merge_different_types() {
        let insert = Command::insert(0, "text");
        let delete = Command::delete(0, "text");

        assert!(insert.try_merge(&delete).is_none());
    }

    #[test]
    fn test_position() {
        assert_eq!(Command::insert(5, "x").position(), 5);
        assert_eq!(Command::delete(10, "x").position(), 10);
        assert_eq!(Command::replace(15, "a", "b").position(), 15);

        let compound = Command::compound(vec![Command::insert(5, "x"), Command::insert(10, "y")]);
        assert_eq!(compound.position(), 5);

        let empty_compound = Command::compound(vec![]);
        assert_eq!(empty_compound.position(), 0);
    }

    #[test]
    fn test_roundtrip_insert() {
        let original = "Hello";
        let mut buffer = Buffer::from(original);
        let cmd = Command::insert(5, ", world!");

        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, world!");

        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), original);
    }

    #[test]
    fn test_roundtrip_delete() {
        let original = "Hello, world!";
        let mut buffer = Buffer::from(original);
        let cmd = Command::delete(5, ", world");

        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello!");

        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), original);
    }

    #[test]
    fn test_roundtrip_replace() {
        let original = "Hello, world!";
        let mut buffer = Buffer::from(original);
        let cmd = Command::replace(7, "world", "Rust");

        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello, Rust!");

        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), original);
    }

    #[test]
    fn test_unicode_handling() {
        let mut buffer = Buffer::from("Hello 世界");
        let cmd = Command::insert(6, "美丽的");
        cmd.apply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello 美丽的世界");

        cmd.unapply(&mut buffer);
        assert_eq!(buffer.to_string(), "Hello 世界");
    }
}
