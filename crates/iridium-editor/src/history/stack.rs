//! Command history stack for undo/redo.
//!
//! Provides a stack-based history that tracks commands for undo and redo
//! operations.

use super::Command;

/// A history of commands supporting undo and redo.
///
/// The history maintains two stacks: one for undo (past commands) and
/// one for redo (undone commands). When a new command is pushed, the
/// redo stack is cleared.
#[derive(Debug, Default)]
pub struct History {
    /// Commands that can be undone (most recent last).
    undo_stack: Vec<Command>,
    /// Commands that can be redone (most recent last).
    redo_stack: Vec<Command>,
    /// Maximum number of commands to keep in history.
    max_size: Option<usize>,
}

impl History {
    /// Creates a new empty history.
    #[must_use]
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_size: None,
        }
    }

    /// Creates a new history with a maximum size limit.
    ///
    /// When the limit is reached, the oldest commands are discarded.
    #[must_use]
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_size: Some(max_size),
        }
    }

    /// Pushes a new command onto the history.
    ///
    /// This clears the redo stack since the history has diverged.
    /// Empty commands are ignored.
    pub fn push(&mut self, command: Command) {
        if command.is_empty() {
            return;
        }

        // Clear redo stack since we're making a new edit
        self.redo_stack.clear();

        // Try to merge with the previous command
        if let Some(last) = self.undo_stack.last() {
            if let Some(merged) = last.try_merge(&command) {
                self.undo_stack.pop();
                self.undo_stack.push(merged);
                return;
            }
        }

        self.undo_stack.push(command);

        // Enforce max size if set
        if let Some(max) = self.max_size {
            while self.undo_stack.len() > max {
                self.undo_stack.remove(0);
            }
        }
    }

    /// Pops the most recent command for undoing.
    ///
    /// The command is moved to the redo stack.
    /// Returns None if there are no commands to undo.
    pub fn undo(&mut self) -> Option<Command> {
        let command = self.undo_stack.pop()?;
        self.redo_stack.push(command.clone());
        Some(command)
    }

    /// Pops the most recently undone command for redoing.
    ///
    /// The command is moved back to the undo stack.
    /// Returns None if there are no commands to redo.
    pub fn redo(&mut self) -> Option<Command> {
        let command = self.redo_stack.pop()?;
        self.undo_stack.push(command.clone());
        Some(command)
    }

    /// Returns true if there are commands available for undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns true if there are commands available for redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns the number of commands in the undo stack.
    #[must_use]
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns the number of commands in the redo stack.
    #[must_use]
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clears all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Marks a checkpoint in the history.
    ///
    /// This can be used to group multiple commands together by pushing
    /// a compound command later.
    pub fn checkpoint(&mut self) -> usize {
        self.undo_stack.len()
    }

    /// Groups all commands since the checkpoint into a single compound command.
    ///
    /// Returns true if commands were grouped.
    pub fn group_since(&mut self, checkpoint: usize) -> bool {
        if checkpoint >= self.undo_stack.len() {
            return false;
        }

        let commands: Vec<Command> = self.undo_stack.drain(checkpoint..).collect();
        if commands.is_empty() {
            return false;
        }

        if commands.len() == 1 {
            // No need to wrap a single command
            self.undo_stack.extend(commands);
        } else {
            self.undo_stack.push(Command::compound(commands));
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let history = History::new();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 0);
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn test_push_and_undo() {
        let mut history = History::new();
        history.push(Command::insert(0, "Hello"));

        assert!(history.can_undo());
        assert!(!history.can_redo());

        let cmd = history.undo();
        assert!(cmd.is_some());
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn test_redo() {
        let mut history = History::new();
        history.push(Command::insert(0, "Hello"));

        history.undo();
        assert!(history.can_redo());

        let cmd = history.redo();
        assert!(cmd.is_some());
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_push_clears_redo() {
        let mut history = History::new();
        history.push(Command::insert(0, "A"));
        history.push(Command::insert(1, "B"));

        history.undo(); // Undo B
        assert!(history.can_redo());

        history.push(Command::insert(1, "C")); // New command
        assert!(!history.can_redo()); // Redo stack cleared
    }

    #[test]
    fn test_empty_command_ignored() {
        let mut history = History::new();
        history.push(Command::insert(0, "")); // Empty
        assert!(!history.can_undo());
    }

    #[test]
    fn test_max_size() {
        let mut history = History::with_max_size(3);
        // Use non-adjacent positions to prevent merging
        history.push(Command::insert(0, "A"));
        history.push(Command::insert(10, "B"));
        history.push(Command::insert(20, "C"));
        history.push(Command::insert(30, "D"));

        assert_eq!(history.undo_count(), 3);

        // The oldest (A) should be gone
        history.undo(); // D
        history.undo(); // C
        history.undo(); // B
        assert!(!history.can_undo()); // A is gone
    }

    #[test]
    fn test_merge_adjacent_inserts() {
        let mut history = History::new();
        history.push(Command::insert(0, "H"));
        history.push(Command::insert(1, "e"));
        history.push(Command::insert(2, "l"));
        history.push(Command::insert(3, "l"));
        history.push(Command::insert(4, "o"));

        // All should be merged into one
        assert_eq!(history.undo_count(), 1);

        let cmd = history.undo().unwrap();
        if let Command::Insert { text, .. } = cmd {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Insert command");
        }
    }

    #[test]
    fn test_no_merge_non_adjacent() {
        let mut history = History::new();
        history.push(Command::insert(0, "A"));
        history.push(Command::insert(5, "B")); // Gap

        assert_eq!(history.undo_count(), 2);
    }

    #[test]
    fn test_clear() {
        let mut history = History::new();
        history.push(Command::insert(0, "A"));
        history.push(Command::insert(1, "B"));
        history.undo();

        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_checkpoint_and_group() {
        let mut history = History::new();
        history.push(Command::insert(0, "A"));

        let checkpoint = history.checkpoint();
        history.push(Command::insert(10, "B"));
        history.push(Command::insert(20, "C"));

        history.group_since(checkpoint);

        // Should have 2 commands: A and the group (B, C)
        assert_eq!(history.undo_count(), 2);

        // Undoing the group should undo both B and C
        let cmd = history.undo().unwrap();
        assert!(matches!(cmd, Command::Compound { commands } if commands.len() == 2));
    }

    #[test]
    fn test_checkpoint_single_command() {
        let mut history = History::new();
        let checkpoint = history.checkpoint();
        history.push(Command::insert(0, "A"));

        history.group_since(checkpoint);

        // Single command should not be wrapped
        assert_eq!(history.undo_count(), 1);
        let cmd = history.undo().unwrap();
        assert!(matches!(cmd, Command::Insert { .. }));
    }

    #[test]
    fn test_checkpoint_no_commands() {
        let mut history = History::new();
        let checkpoint = history.checkpoint();

        let grouped = history.group_since(checkpoint);
        assert!(!grouped);
    }
}
