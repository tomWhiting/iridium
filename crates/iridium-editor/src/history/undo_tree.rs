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
            // Group with current node by merging into a flat compound command.
            // The existing command is moved (never cloned), so grouping N
            // keystrokes does O(N) total work instead of O(N^2).
            if let Some(current_node) = self.nodes.get_mut(&self.current) {
                let merged = match current_node.command.take() {
                    Some(Command::Compound { mut commands }) => {
                        // Already a compound: append in place, keeping it flat.
                        commands.push(command);
                        Command::Compound { commands }
                    },
                    Some(existing_cmd) => {
                        // Single command: promote to a two-element compound.
                        Command::Compound {
                            commands: vec![existing_cmd, command],
                        }
                    },
                    None => command,
                };
                current_node.command = Some(merged);
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
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::document::{CursorState, Document, Position};

    fn insert_cmd(text: &str) -> Command {
        Command::Insert {
            position: Position::new(0, 0),
            text: text.to_string(),
        }
    }

    /// Builds an insert command at the given column on line 0.
    fn insert_at(column: usize, text: &str) -> Command {
        Command::Insert {
            position: Position::new(0, column),
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

    #[test]
    fn grouped_pushes_produce_flat_compound() {
        // Large timeout so all pushes fall inside the group window.
        let mut tree = UndoTree::with_timeout(10_000);

        tree.push(insert_at(0, "a"));
        tree.push(insert_at(1, "b"));
        tree.push(insert_at(2, "c"));
        tree.push(insert_at(3, "d"));
        tree.push(insert_at(4, "e"));

        // Grouping must not create extra nodes: root plus one edit node.
        assert_eq!(tree.get_tree_info().node_count, 2);

        // A single undo must revert all five grouped commands.
        let inverse = tree.undo().expect("undo should return inverse command");
        assert!(!tree.can_undo(), "one undo should reach the root");

        // The inverse must be a flat compound with 5 direct, non-compound
        // children — the old implementation nested one level per keystroke.
        match inverse {
            Command::Compound { commands } => {
                assert_eq!(commands.len(), 5, "expected 5 flat children");
                for child in &commands {
                    assert!(
                        !matches!(child, Command::Compound { .. }),
                        "grouped compound must not be nested"
                    );
                }
            },
            other => panic!("expected Command::Compound, got {other:?}"),
        }
    }

    #[test]
    fn undo_after_grouped_typing_restores_document() {
        let mut tree = UndoTree::with_timeout(10_000);
        let mut doc = Document::new("");
        let mut cursor = CursorState::at(Position::zero());

        // Simulate a fast typing burst: apply each keystroke and push it.
        for (column, ch) in ["h", "e", "l", "l", "o"].iter().enumerate() {
            let cmd = insert_at(column, ch);
            cmd.apply(&mut doc, &mut cursor).expect("apply keystroke");
            tree.push(cmd);
        }
        assert_eq!(doc.text(), "hello");

        // One undo must restore the original (empty) document text.
        let inverse = tree.undo().expect("undo should return inverse command");
        inverse.apply(&mut doc, &mut cursor).expect("apply inverse");
        assert_eq!(doc.text(), "");

        // Redo must reapply the full burst.
        let redo = tree.redo().expect("redo should return command");
        redo.apply(&mut doc, &mut cursor).expect("apply redo");
        assert_eq!(doc.text(), "hello");
    }

    #[test]
    fn push_after_timeout_creates_new_node() {
        // Zero timeout: elapsed time is never strictly less than the
        // timeout, so consecutive pushes must never group.
        let mut tree = UndoTree::with_timeout(0);

        tree.push(insert_at(0, "a"));
        tree.push(insert_at(1, "b"));

        // Root plus two distinct edit nodes.
        assert_eq!(tree.get_tree_info().node_count, 3);

        // Two separate undos are required to reach the root.
        assert!(tree.undo().is_some());
        assert!(tree.can_undo(), "second edit node should remain");
        assert!(tree.undo().is_some());
        assert!(!tree.can_undo());
    }

    #[test]
    fn jump_to_node_across_branch() {
        let mut tree = UndoTree::with_timeout(0);
        let mut doc = Document::new("");
        let mut cursor = CursorState::at(Position::zero());

        // Branch 1: insert "A" (node id 1).
        let cmd_a = insert_at(0, "A");
        cmd_a.apply(&mut doc, &mut cursor).expect("apply A");
        tree.push(cmd_a);

        // Undo back to root, then create branch 2: insert "B" (node id 2).
        let inv_a = tree.undo().expect("undo A");
        inv_a.apply(&mut doc, &mut cursor).expect("apply inverse A");
        let cmd_b = insert_at(0, "B");
        cmd_b.apply(&mut doc, &mut cursor).expect("apply B");
        tree.push(cmd_b);
        assert_eq!(doc.text(), "B");

        // Jump across the branch point: from node 2 to node 1.
        let commands = tree
            .jump_to_node(UndoNodeId::new(1))
            .expect("jump to sibling branch");
        assert_eq!(commands.len(), 2, "expected undo of B then redo of A");
        for cmd in &commands {
            cmd.apply(&mut doc, &mut cursor)
                .expect("apply jump command");
        }
        assert_eq!(doc.text(), "A");
        assert!(tree.can_undo());
        assert!(!tree.can_redo(), "node 1 is a leaf");
        assert_eq!(tree.branch_count(), 0);

        // Jump back across to node 2 and verify the document follows.
        let commands = tree
            .jump_to_node(UndoNodeId::new(2))
            .expect("jump back to other branch");
        assert_eq!(commands.len(), 2, "expected undo of A then redo of B");
        for cmd in &commands {
            cmd.apply(&mut doc, &mut cursor)
                .expect("apply jump command");
        }
        assert_eq!(doc.text(), "B");

        // Jumping to a nonexistent node must fail without moving.
        assert!(tree.jump_to_node(UndoNodeId::new(99)).is_none());
        assert_eq!(doc.text(), "B");
        assert!(tree.can_undo());
    }
}
