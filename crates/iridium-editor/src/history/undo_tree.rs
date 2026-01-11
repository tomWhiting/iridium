//! Branching undo tree.
//!
//! The undo tree preserves all history branches, ensuring no work is ever lost.
//! When you undo and make a new change, a new branch is created instead of
//! discarding the previous redo history.

use std::time::Instant;

use super::Command;

/// Unique identifier for a node in the undo tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(usize);

impl NodeId {
    /// Returns the root node ID.
    #[must_use]
    pub const fn root() -> Self {
        Self(0)
    }

    /// Returns the internal ID value.
    #[must_use]
    pub const fn value(&self) -> usize {
        self.0
    }
}

/// Information about a node in the undo tree.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// The node's unique identifier.
    pub id: NodeId,
    /// The parent node's ID (None for root).
    pub parent: Option<NodeId>,
    /// IDs of child nodes (branches from this point).
    pub children: Vec<NodeId>,
    /// The command at this node (None for root).
    pub command: Option<Command>,
    /// When this node was created.
    pub timestamp: Instant,
    /// Index of the currently active child branch (if any).
    pub active_child_index: Option<usize>,
    /// Whether this node is on the current branch.
    pub is_on_current_branch: bool,
}

/// A node in the undo tree.
#[derive(Debug, Clone)]
struct TreeNode {
    /// The command that was executed to reach this state.
    /// None for the root node.
    command: Option<Command>,
    /// Parent node ID (None for root).
    parent: Option<NodeId>,
    /// Child node IDs (branches from this point).
    children: Vec<NodeId>,
    /// Index of the currently active child branch.
    /// When undoing, we go to the parent. When redoing, we follow this index.
    active_child: Option<usize>,
    /// When this node was created.
    timestamp: Instant,
}

impl TreeNode {
    /// Creates the root node.
    fn root() -> Self {
        Self {
            command: None,
            parent: None,
            children: Vec::new(),
            active_child: None,
            timestamp: Instant::now(),
        }
    }

    /// Creates a new node with a command.
    fn new(command: Command, parent: NodeId) -> Self {
        Self {
            command: Some(command),
            parent: Some(parent),
            children: Vec::new(),
            active_child: None,
            timestamp: Instant::now(),
        }
    }
}

/// A branching undo tree that preserves all history.
///
/// Unlike a linear undo stack, the undo tree creates branches when you
/// undo to an earlier state and make new changes. This ensures that
/// no work is ever lost - you can always navigate back to any previous
/// state in any branch.
///
/// # Example
///
/// ```
/// use iridium_editor::history::{Command, UndoTree};
///
/// let mut tree = UndoTree::new();
///
/// // Make a change
/// tree.push(Command::insert(0, "A"));
/// assert!(tree.can_undo());
///
/// // Undo and make a different change (creates a branch)
/// tree.undo();
/// tree.push(Command::insert(0, "B"));
///
/// // Go back and make another branch
/// tree.undo();
/// tree.push(Command::insert(0, "C"));
///
/// // Go to root to see all branches
/// tree.undo();
///
/// // Root now has 3 branches: A, B, C
/// assert_eq!(tree.branch_count(), 3);
/// ```
#[derive(Debug)]
pub struct UndoTree {
    /// All nodes in the tree.
    nodes: Vec<TreeNode>,
    /// The current position in the tree.
    current: NodeId,
}

impl UndoTree {
    /// Creates a new empty undo tree.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: vec![TreeNode::root()],
            current: NodeId::root(),
        }
    }

    /// Returns the current node ID.
    #[must_use]
    pub fn current(&self) -> NodeId {
        self.current
    }

    /// Returns the root node ID.
    #[must_use]
    pub fn root(&self) -> NodeId {
        NodeId::root()
    }

    /// Returns the total number of nodes in the tree.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of branches at the current node.
    #[must_use]
    pub fn branch_count(&self) -> usize {
        self.nodes[self.current.0].children.len()
    }

    /// Returns true if undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.nodes[self.current.0].parent.is_some()
    }

    /// Returns true if redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.nodes[self.current.0].children.is_empty()
    }

    /// Returns the number of undo steps available from the current position.
    #[must_use]
    pub fn undo_count(&self) -> usize {
        let mut count = 0;
        let mut node_id = self.current;
        while let Some(parent) = self.nodes[node_id.0].parent {
            count += 1;
            node_id = parent;
        }
        count
    }

    /// Returns the number of redo steps available on the active branch.
    #[must_use]
    pub fn redo_count(&self) -> usize {
        let mut count = 0;
        let mut node_id = self.current;
        while let Some(child_idx) = self.nodes[node_id.0].active_child {
            count += 1;
            node_id = self.nodes[node_id.0].children[child_idx];
        }
        count
    }

    /// Pushes a new command onto the tree.
    ///
    /// If there are existing redo branches, this creates a new branch instead
    /// of discarding them. The new branch becomes the active branch.
    pub fn push(&mut self, command: Command) {
        if command.is_empty() {
            return;
        }

        // Try to merge with the last command if we're at a leaf node
        // and the command is mergeable
        if self.nodes[self.current.0].children.is_empty() {
            if let Some(cmd) = &self.nodes[self.current.0].command {
                if let Some(merged) = cmd.try_merge(&command) {
                    self.nodes[self.current.0].command = Some(merged);
                    return;
                }
            }
        }

        // Create a new node
        let new_id = NodeId(self.nodes.len());
        let new_node = TreeNode::new(command, self.current);
        self.nodes.push(new_node);

        // Add as child of current node
        let child_index = self.nodes[self.current.0].children.len();
        self.nodes[self.current.0].children.push(new_id);
        self.nodes[self.current.0].active_child = Some(child_index);

        // Move to the new node
        self.current = new_id;
    }

    /// Undoes the current command and moves to the parent node.
    ///
    /// Returns the command that was undone, which should be unapplied to the buffer.
    /// Returns None if already at the root.
    pub fn undo(&mut self) -> Option<Command> {
        let node = &self.nodes[self.current.0];
        let parent = node.parent?;
        let command = node.command.clone();

        self.current = parent;
        command
    }

    /// Redoes along the active branch.
    ///
    /// Returns the command to apply. Returns None if no redo is available.
    pub fn redo(&mut self) -> Option<Command> {
        let node = &self.nodes[self.current.0];
        let child_index = node.active_child?;
        let child_id = node.children.get(child_index)?;
        let child = &self.nodes[child_id.0];

        self.current = *child_id;
        child.command.clone()
    }

    /// Redoes along a specific branch.
    ///
    /// Returns the command to apply. Returns None if the branch doesn't exist.
    pub fn redo_branch(&mut self, branch_index: usize) -> Option<Command> {
        let node = &mut self.nodes[self.current.0];
        let child_id = node.children.get(branch_index).copied()?;

        // Update active branch
        node.active_child = Some(branch_index);

        let child = &self.nodes[child_id.0];
        self.current = child_id;
        child.command.clone()
    }

    /// Returns information about a specific node.
    ///
    /// Returns None if the node ID is invalid.
    #[must_use]
    pub fn get_node_info(&self, id: NodeId) -> Option<NodeInfo> {
        let node = self.nodes.get(id.0)?;

        // Determine if this node is on the current branch
        let is_on_current_branch = self.is_on_current_branch(id);

        Some(NodeInfo {
            id,
            parent: node.parent,
            children: node.children.clone(),
            command: node.command.clone(),
            timestamp: node.timestamp,
            active_child_index: node.active_child,
            is_on_current_branch,
        })
    }

    /// Checks if a node is on the path from root to current position.
    fn is_on_current_branch(&self, id: NodeId) -> bool {
        // Walk from current back to root, checking if we pass through id
        let mut check = self.current;
        loop {
            if check == id {
                return true;
            }
            match self.nodes[check.0].parent {
                Some(parent) => check = parent,
                None => break,
            }
        }
        false
    }

    /// Returns the IDs of all nodes at a given depth from the root.
    #[must_use]
    pub fn nodes_at_depth(&self, depth: usize) -> Vec<NodeId> {
        if depth == 0 {
            return vec![NodeId::root()];
        }

        let mut result = Vec::new();
        self.collect_at_depth(NodeId::root(), 0, depth, &mut result);
        result
    }

    fn collect_at_depth(
        &self,
        id: NodeId,
        current_depth: usize,
        target: usize,
        out: &mut Vec<NodeId>,
    ) {
        if current_depth == target {
            out.push(id);
            return;
        }

        for &child in &self.nodes[id.0].children {
            self.collect_at_depth(child, current_depth + 1, target, out);
        }
    }

    /// Returns the path from root to the current node as a list of node IDs.
    #[must_use]
    pub fn path_to_current(&self) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut id = self.current;

        loop {
            path.push(id);
            match self.nodes[id.0].parent {
                Some(parent) => id = parent,
                None => break,
            }
        }

        path.reverse();
        path
    }

    /// Navigates directly to a specific node.
    ///
    /// This performs the necessary undo/redo operations to reach the target.
    /// Returns the commands that need to be applied, in order.
    /// Each command in the result should be either applied or unapplied
    /// depending on the direction (indicated by the boolean: true = apply, false = unapply).
    pub fn goto(&mut self, target: NodeId) -> Option<Vec<(Command, bool)>> {
        if target.0 >= self.nodes.len() {
            return None;
        }

        // Find the common ancestor
        let current_path = self.path_to_current();
        let mut target_path = Vec::new();
        let mut id = target;
        loop {
            target_path.push(id);
            match self.nodes[id.0].parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
        target_path.reverse();

        // Find common ancestor
        let mut common_depth = 0;
        for (i, (a, b)) in current_path.iter().zip(target_path.iter()).enumerate() {
            if a == b {
                common_depth = i + 1;
            } else {
                break;
            }
        }

        let mut operations = Vec::new();

        // Undo from current to common ancestor
        for i in (common_depth..current_path.len()).rev() {
            let node = &self.nodes[current_path[i].0];
            if let Some(cmd) = &node.command {
                operations.push((cmd.clone(), false)); // unapply
            }
        }

        // Redo from common ancestor to target
        for i in common_depth..target_path.len() {
            let node = &self.nodes[target_path[i].0];
            if let Some(cmd) = &node.command {
                operations.push((cmd.clone(), true)); // apply
            }

            // Update active child pointers along the way
            if i > 0 {
                let parent_id = target_path[i - 1];
                let child_id = target_path[i];
                if let Some(idx) = self.nodes[parent_id.0]
                    .children
                    .iter()
                    .position(|&c| c == child_id)
                {
                    self.nodes[parent_id.0].active_child = Some(idx);
                }
            }
        }

        self.current = target;
        Some(operations)
    }

    /// Returns all branch points (nodes with more than one child).
    #[must_use]
    pub fn branch_points(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.children.len() > 1)
            .map(|(i, _)| NodeId(i))
            .collect()
    }

    /// Returns the depth of a node (distance from root).
    #[must_use]
    pub fn depth(&self, id: NodeId) -> usize {
        let mut depth = 0;
        let mut current = id;
        while let Some(parent) = self.nodes.get(current.0).and_then(|n| n.parent) {
            depth += 1;
            current = parent;
        }
        depth
    }

    /// Clears all history, resetting to a single root node.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.nodes.push(TreeNode::root());
        self.current = NodeId::root();
    }
}

impl Default for UndoTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let tree = UndoTree::new();
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.current(), NodeId::root());
        assert!(!tree.can_undo());
        assert!(!tree.can_redo());
    }

    #[test]
    fn test_push_and_undo() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        assert_eq!(tree.node_count(), 2);
        assert!(tree.can_undo());
        assert!(!tree.can_redo());

        let cmd = tree.undo();
        assert!(cmd.is_some());
        assert!(!tree.can_undo());
        assert!(tree.can_redo());
    }

    #[test]
    fn test_redo() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.undo();

        let cmd = tree.redo();
        assert!(cmd.is_some());
        assert!(tree.can_undo());
        assert!(!tree.can_redo());
    }

    #[test]
    fn test_branching() {
        let mut tree = UndoTree::new();

        // Make initial change
        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B")); // Non-adjacent to prevent merging
        assert_eq!(tree.node_count(), 3);

        // Undo to first change
        tree.undo();
        assert_eq!(tree.node_count(), 3);

        // Make different change (creates branch)
        tree.push(Command::insert(10, "C")); // Non-adjacent to prevent merging
        assert_eq!(tree.node_count(), 4);

        // First node should now have 2 children (branch point)
        let info = tree.get_node_info(NodeId(1)).unwrap();
        assert_eq!(info.children.len(), 2);
    }

    #[test]
    fn test_branch_count() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        assert_eq!(tree.branch_count(), 0);

        // Undo and create branches
        tree.undo();
        tree.push(Command::insert(0, "B"));
        tree.undo();
        tree.push(Command::insert(0, "C"));

        // Go back to root to check branches
        tree.undo();
        assert_eq!(tree.branch_count(), 3); // A, B, C are all branches from root
    }

    #[test]
    fn test_redo_branch() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.undo();
        tree.push(Command::insert(0, "B"));
        tree.undo();

        // Root has 2 children now
        assert_eq!(tree.branch_count(), 2);

        // Redo branch 0 (first child)
        let cmd = tree.redo_branch(0);
        assert!(cmd.is_some());
    }

    #[test]
    fn test_get_node_info() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B"));

        let info = tree.get_node_info(NodeId(1)).unwrap();
        assert_eq!(info.id, NodeId(1));
        assert_eq!(info.parent, Some(NodeId::root()));
        assert_eq!(info.children.len(), 1);
        assert!(info.command.is_some());
        assert!(info.is_on_current_branch);

        // Root info
        let root_info = tree.get_node_info(NodeId::root()).unwrap();
        assert_eq!(root_info.parent, None);
        assert!(root_info.command.is_none());
    }

    #[test]
    fn test_path_to_current() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B"));
        tree.push(Command::insert(20, "C"));

        let path = tree.path_to_current();
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], NodeId::root());
    }

    #[test]
    fn test_goto() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        let node_a = tree.current();
        tree.push(Command::insert(10, "B"));
        let node_b = tree.current();
        tree.push(Command::insert(20, "C"));
        let _node_c = tree.current();

        // Go back to A
        let ops = tree.goto(node_a);
        assert!(ops.is_some());
        let ops = ops.unwrap();
        assert_eq!(ops.len(), 2); // Unapply C, unapply B
        assert_eq!(tree.current(), node_a);

        // Go forward to B
        let ops = tree.goto(node_b).unwrap();
        assert_eq!(ops.len(), 1); // Apply B
        assert_eq!(tree.current(), node_b);
    }

    #[test]
    fn test_undo_redo_count() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B"));
        tree.push(Command::insert(20, "C"));

        assert_eq!(tree.undo_count(), 3);
        assert_eq!(tree.redo_count(), 0);

        tree.undo();
        assert_eq!(tree.undo_count(), 2);
        assert_eq!(tree.redo_count(), 1);

        tree.undo();
        assert_eq!(tree.undo_count(), 1);
        assert_eq!(tree.redo_count(), 2);
    }

    #[test]
    fn test_branch_points() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        let branch_point = tree.current();
        tree.push(Command::insert(10, "B1"));
        tree.undo();
        tree.push(Command::insert(10, "B2"));

        let points = tree.branch_points();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0], branch_point);
    }

    #[test]
    fn test_depth() {
        let mut tree = UndoTree::new();

        assert_eq!(tree.depth(NodeId::root()), 0);

        tree.push(Command::insert(0, "A"));
        assert_eq!(tree.depth(tree.current()), 1);

        tree.push(Command::insert(10, "B"));
        assert_eq!(tree.depth(tree.current()), 2);
    }

    #[test]
    fn test_clear() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B"));
        assert_eq!(tree.node_count(), 3);

        tree.clear();
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.current(), NodeId::root());
    }

    #[test]
    fn test_empty_command_ignored() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, ""));
        assert_eq!(tree.node_count(), 1);
    }

    #[test]
    fn test_merge_adjacent_inserts() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "H"));
        tree.push(Command::insert(1, "e"));
        tree.push(Command::insert(2, "l"));
        tree.push(Command::insert(3, "l"));
        tree.push(Command::insert(4, "o"));

        // All should merge into one node
        assert_eq!(tree.node_count(), 2);

        let info = tree.get_node_info(tree.current()).unwrap();
        if let Some(Command::Insert { text, .. }) = info.command {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Insert command");
        }
    }

    #[test]
    fn test_nodes_at_depth() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        tree.undo();
        tree.push(Command::insert(0, "B"));
        tree.undo();
        tree.push(Command::insert(0, "C"));

        let depth_0 = tree.nodes_at_depth(0);
        assert_eq!(depth_0.len(), 1);

        let depth_1 = tree.nodes_at_depth(1);
        assert_eq!(depth_1.len(), 3); // A, B, C
    }

    #[test]
    fn test_is_on_current_branch() {
        let mut tree = UndoTree::new();

        tree.push(Command::insert(0, "A"));
        let a = tree.current();
        tree.push(Command::insert(10, "B"));
        let b = tree.current();

        // Both A and B are on current branch
        assert!(tree.get_node_info(a).unwrap().is_on_current_branch);
        assert!(tree.get_node_info(b).unwrap().is_on_current_branch);

        // Create a branch
        tree.undo();
        tree.push(Command::insert(10, "C"));
        let c = tree.current();

        // A and C are on current branch, B is not
        assert!(tree.get_node_info(a).unwrap().is_on_current_branch);
        assert!(!tree.get_node_info(b).unwrap().is_on_current_branch);
        assert!(tree.get_node_info(c).unwrap().is_on_current_branch);
    }

    #[test]
    fn test_navigate_back_to_original_branch() {
        let mut tree = UndoTree::new();

        // Create original content: "AB"
        tree.push(Command::insert(0, "A"));
        tree.push(Command::insert(10, "B"));
        let original_branch = tree.current();

        // Undo to mid-point and create new branch
        tree.undo(); // Undo B
        tree.push(Command::insert(10, "C")); // Insert C instead

        // Verify we can navigate back to original branch
        let ops = tree.goto(original_branch);
        assert!(ops.is_some());
        assert_eq!(tree.current(), original_branch);

        // Verify the original content would be recovered
        let info = tree.get_node_info(original_branch).unwrap();
        assert!(info.is_on_current_branch);
    }
}
