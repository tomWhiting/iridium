//! Tree-structured undo history.

use std::collections::HashMap;
use web_time::Instant;

use serde::{Deserialize, Serialize};

use super::commands::Command;

/// Opaque identifier for undo nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UndoNodeId(u64);

impl UndoNodeId {
    /// Creates a new node ID.
    const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw ID value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// A node in the undo tree representing a document state.
#[derive(Debug, Clone)]
struct UndoNode {
    /// Unique identifier (used for debugging and future serialization)
    #[allow(dead_code)]
    id: UndoNodeId,
    /// Parent node (None for root)
    parent: Option<UndoNodeId>,
    /// Child nodes (branches)
    children: Vec<UndoNodeId>,
    /// Command that was applied to reach this state from parent
    command: Option<Command>,
    /// When this edit was made (used for edit grouping and future features)
    #[allow(dead_code)]
    timestamp: Instant,
    /// Optional description for this edit (reserved for future features)
    #[allow(dead_code)]
    description: Option<String>,
}

/// Information about an undo node for external use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct UndoNodeInfo {
    /// Unique node identifier
    pub id: String,
    /// Parent node ID (null for root)
    pub parent_id: Option<String>,
    /// Child node IDs
    pub child_ids: Vec<String>,
    /// When this edit was made (milliseconds since epoch)
    pub timestamp: u64,
    /// Optional description
    pub description: Option<String>,
}

/// Information about the undo tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoTreeInfo {
    /// Current node ID
    pub current_id: String,
    /// Root node ID
    pub root_id: String,
    /// Total number of nodes
    pub node_count: usize,
    /// Can undo from current position
    pub can_undo: bool,
    /// Can redo from current position
    pub can_redo: bool,
    /// Number of branches at current node
    pub branch_count: usize,
}

/// Tree-structured undo history.
///
/// Unlike a traditional undo stack, the undo tree preserves all branches.
/// When you undo several changes and then make new edits, the old branch
/// is preserved and can be navigated back to later.
///
/// # Example
///
/// ```
/// use iridium_editor::{Command, Position, UndoTree};
///
/// let mut tree = UndoTree::new();
///
/// // Make some edits
/// tree.push(Command::Insert {
///     position: Position::new(0, 0),
///     text: "Hello".to_string(),
/// });
///
/// assert!(tree.can_undo());
/// let undo_cmd = tree.undo();
/// assert!(undo_cmd.is_some());
/// ```
#[derive(Debug)]
pub struct UndoTree {
    /// All nodes in the tree
    nodes: HashMap<UndoNodeId, UndoNode>,
    /// Root node (initial document state)
    root: UndoNodeId,
    /// Current position in the tree
    current: UndoNodeId,
    /// Counter for generating node IDs
    next_id: u64,
    /// Time of last edit (for grouping)
    last_edit_time: Option<Instant>,
    /// Grouping timeout in milliseconds
    group_timeout_ms: u64,
}

impl Default for UndoTree {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoTree {
    /// Creates a new undo tree.
    #[must_use]
    pub fn new() -> Self {
        let root_id = UndoNodeId::new(0);
        let root_node = UndoNode {
            id: root_id,
            parent: None,
            children: Vec::new(),
            command: None,
            timestamp: Instant::now(),
            description: None,
        };

        let mut nodes = HashMap::new();
        nodes.insert(root_id, root_node);

        Self {
            nodes,
            root: root_id,
            current: root_id,
            next_id: 1,
            last_edit_time: None,
            group_timeout_ms: 500,
        }
    }

    /// Creates an undo tree with a custom grouping timeout.
    #[must_use]
    pub fn with_timeout(group_timeout_ms: u64) -> Self {
        Self {
            group_timeout_ms,
            ..Self::new()
        }
    }

    /// Pushes a new command onto the tree.
    ///
    /// The command is added as a child of the current node, and the
    /// current position moves to the new node.
    pub fn push(&mut self, command: Command) {
        if command.is_empty() {
            return;
        }

        let now = Instant::now();
        let should_group = self.should_group(now);

        if should_group {
            // Group with current node by replacing its command
            if let Some(current_node) = self.nodes.get_mut(&self.current) {
                let new_command = if let Some(existing_cmd) = &current_node.command {
                    // Combine into compound command
                    Command::Compound {
                        commands: vec![existing_cmd.clone(), command],
                    }
                } else {
                    command
                };
                current_node.command = Some(new_command);
                current_node.timestamp = now;
            }
        } else {
            // Create new node
            let new_id = UndoNodeId::new(self.next_id);
            self.next_id += 1;

            let new_node = UndoNode {
                id: new_id,
                parent: Some(self.current),
                children: Vec::new(),
                command: Some(command),
                timestamp: now,
                description: None,
            };

            // Add as child of current
            if let Some(current_node) = self.nodes.get_mut(&self.current) {
                current_node.children.push(new_id);
            }

            self.nodes.insert(new_id, new_node);
            self.current = new_id;
        }

        self.last_edit_time = Some(now);
    }

    /// Undoes the current node's command and moves to parent.
    ///
    /// Returns the inverse command that was applied, or `None` if at root.
    pub fn undo(&mut self) -> Option<Command> {
        let current_node = self.nodes.get(&self.current)?;
        let parent_id = current_node.parent?;
        let command = current_node.command.clone()?;

        self.current = parent_id;
        self.last_edit_time = None;

        Some(command.inverse())
    }

    /// Redoes by moving to the first child.
    ///
    /// Returns the command to apply, or `None` if no children.
    pub fn redo(&mut self) -> Option<Command> {
        self.redo_branch(0)
    }

    /// Redoes by moving to a specific child branch.
    ///
    /// Returns the command to apply, or `None` if branch doesn't exist.
    pub fn redo_branch(&mut self, branch_index: usize) -> Option<Command> {
        let current_node = self.nodes.get(&self.current)?;
        let child_id = current_node.children.get(branch_index).copied()?;

        let child_node = self.nodes.get(&child_id)?;
        let command = child_node.command.clone()?;

        self.current = child_id;
        self.last_edit_time = None;

        Some(command)
    }

    /// Jumps to a specific node in the tree.
    ///
    /// Returns the sequence of commands needed to reach that node.
    pub fn jump_to_node(&mut self, target_id: UndoNodeId) -> Option<Vec<Command>> {
        if !self.nodes.contains_key(&target_id) {
            return None;
        }

        // Find path from current to target via common ancestor
        let path_to_root_current = self.path_to_root(self.current);
        let path_to_root_target = self.path_to_root(target_id);

        // Find common ancestor
        let current_set: std::collections::HashSet<_> = path_to_root_current.iter().collect();
        let common_ancestor = path_to_root_target
            .iter()
            .find(|id| current_set.contains(id))
            .copied()?;

        let mut commands = Vec::new();

        // Undo from current to common ancestor
        let mut node = self.current;
        while node != common_ancestor {
            if let Some(current_node) = self.nodes.get(&node) {
                if let Some(cmd) = &current_node.command {
                    commands.push(cmd.inverse());
                }
                node = current_node.parent?;
            } else {
                break;
            }
        }

        // Redo from common ancestor to target
        let redo_path: Vec<_> = path_to_root_target
            .into_iter()
            .take_while(|id| *id != common_ancestor)
            .collect();

        for node_id in redo_path.into_iter().rev() {
            if let Some(node_data) = self.nodes.get(&node_id) {
                if let Some(cmd) = &node_data.command {
                    commands.push(cmd.clone());
                }
            }
        }

        self.current = target_id;
        self.last_edit_time = None;

        Some(commands)
    }

    /// Returns true if undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.nodes
            .get(&self.current)
            .is_some_and(|n| n.parent.is_some())
    }

    /// Returns true if redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.nodes
            .get(&self.current)
            .is_some_and(|n| !n.children.is_empty())
    }

    /// Returns the number of redo branches at the current node.
    #[must_use]
    pub fn branch_count(&self) -> usize {
        self.nodes
            .get(&self.current)
            .map_or(0, |n| n.children.len())
    }

    /// Returns information about the undo tree structure.
    #[must_use]
    pub fn get_tree_info(&self) -> UndoTreeInfo {
        UndoTreeInfo {
            current_id: self.current.as_u64().to_string(),
            root_id: self.root.as_u64().to_string(),
            node_count: self.nodes.len(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
            branch_count: self.branch_count(),
        }
    }

    /// Returns the path from a node to the root.
    fn path_to_root(&self, mut node_id: UndoNodeId) -> Vec<UndoNodeId> {
        let mut path = Vec::new();
        while let Some(node) = self.nodes.get(&node_id) {
            path.push(node_id);
            if let Some(parent) = node.parent {
                node_id = parent;
            } else {
                break;
            }
        }
        path
    }

    /// Determines if the new edit should be grouped with the current one.
    fn should_group(&self, now: Instant) -> bool {
        if let Some(last_time) = self.last_edit_time {
            let elapsed = now.duration_since(last_time);
            if elapsed.as_millis() < u128::from(self.group_timeout_ms) {
                // Only group if current node has a command (not root)
                return self
                    .nodes
                    .get(&self.current)
                    .is_some_and(|n| n.command.is_some());
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Position;

    fn insert_cmd(text: &str) -> Command {
        Command::Insert {
            position: Position::new(0, 0),
            text: text.to_string(),
        }
    }

    #[test]
    fn new_tree() {
        let tree = UndoTree::new();
        assert!(!tree.can_undo());
        assert!(!tree.can_redo());
        assert_eq!(tree.branch_count(), 0);
    }

    #[test]
    fn push_and_undo() {
        let mut tree = UndoTree::with_timeout(0); // Disable grouping
        tree.push(insert_cmd("Hello"));

        assert!(tree.can_undo());
        assert!(!tree.can_redo());

        let undo_cmd = tree.undo();
        assert!(undo_cmd.is_some());
        assert!(!tree.can_undo());
        assert!(tree.can_redo());
    }

    #[test]
    fn undo_redo() {
        let mut tree = UndoTree::with_timeout(0);
        tree.push(insert_cmd("Hello"));

        tree.undo();
        assert!(tree.can_redo());

        let redo_cmd = tree.redo();
        assert!(redo_cmd.is_some());
        assert!(!tree.can_redo());
    }

    #[test]
    fn branching() {
        let mut tree = UndoTree::with_timeout(0);

        // Initial edit
        tree.push(insert_cmd("A"));
        tree.undo();

        // Create branch
        tree.push(insert_cmd("B"));

        // Should have 2 branches at root
        tree.undo();
        assert_eq!(tree.branch_count(), 2);
    }
}
