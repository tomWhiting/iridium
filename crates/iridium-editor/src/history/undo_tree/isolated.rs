//! Explicit cell gestures form one node and fence adjacent automatic grouping.

use super::UndoTree;
use crate::history::Command;

impl UndoTree {
    pub(crate) fn push_isolated(&mut self, command: Command) {
        if command.is_empty() {
            return;
        }
        self.last_edit_time = None;
        self.push(command);
        self.last_edit_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CursorState, Position};

    fn insert(character: &str) -> Command {
        Command::Insert {
            position: Position::zero(),
            text: character.to_owned(),
        }
    }

    #[test]
    fn cell_transaction_group_barriers_do_not_change_timeout_or_branches() {
        let mut tree = UndoTree::with_timeout(u64::MAX);
        tree.push(insert("a"));
        let first = tree.current;
        assert!(tree.should_group(std::time::Instant::now()));
        tree.push_isolated(insert("b"));
        let isolated = tree.current;
        assert_ne!(isolated, first);
        assert_eq!(tree.last_edit_time, None);
        assert_eq!(tree.group_timeout_ms, u64::MAX);
        tree.push(insert("c"));
        assert_ne!(tree.current, isolated);
        assert!(tree.undo().is_some());
        assert_eq!(tree.current, isolated);
        tree.push_isolated(insert("d"));
        assert!(tree.undo().is_some());
        assert_eq!(tree.branch_count(), 2);
        assert_eq!(tree.group_timeout_ms, u64::MAX);
    }

    #[test]
    fn cell_transaction_empty_command_preserves_grouping_but_selection_is_isolated() {
        let mut tree = UndoTree::with_timeout(u64::MAX);
        tree.push(insert("a"));
        let before = tree.current;
        let last_edit = tree.last_edit_time;
        tree.push_isolated(Command::Compound {
            commands: Vec::new(),
        });
        assert_eq!(tree.current, before);
        assert_eq!(tree.last_edit_time, last_edit);
        tree.push_isolated(Command::SetSelection {
            old_state: CursorState::at(Position::zero()),
            new_state: CursorState::at(Position::new(0, 1)),
        });
        assert_ne!(tree.current, before);
        assert_eq!(tree.last_edit_time, None);
    }
}
