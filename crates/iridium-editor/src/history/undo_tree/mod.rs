//! Tree-structured undo history.

use std::collections::HashMap;
use web_time::Instant;

use serde::{Deserialize, Serialize};

use super::commands::Command;

mod info;

pub use info::{UndoNodeInfo, UndoTreeInfo, UndoTreeSnapshot};

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

    /// Rebuilds an identifier from its raw value.
    ///
    /// Round-trips [`UndoNodeId::as_u64`], so a host that persisted or
    /// serialized an id can navigate back to it. The value is not validated
    /// here — an id belonging to no node is simply refused by
    /// [`UndoTree::jump_to_node`] and [`UndoTree::node_info`].
    #[must_use]
    pub const fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

/// A node in the undo tree representing a document state.
#[derive(Debug, Clone)]
struct UndoNode {
    /// Unique identifier, also this node's key in [`UndoTree::nodes`]
    id: UndoNodeId,
    /// Parent node (None for root)
    parent: Option<UndoNodeId>,
    /// Child nodes (branches), in creation order
    children: Vec<UndoNodeId>,
    /// The child on the currently active path (Vim's `curhead`).
    ///
    /// Set when a branch is departed via [`UndoTree::undo`], entered via
    /// [`UndoTree::redo_branch`], or created by [`UndoTree::push`]. A plain
    /// [`UndoTree::redo`] returns to this child, so redo always retraces the
    /// path the caller last travelled rather than picking a branch by index.
    /// `None` on a leaf, or on a node reached without traversing any child.
    preferred_child: Option<UndoNodeId>,
    /// Command that was applied to reach this state from parent
    command: Option<Command>,
    /// When this edit was made, used for edit grouping and reported as
    /// [`UndoNodeInfo::elapsed_ms`]
    timestamp: Instant,
    /// Human-readable label for this edit, shown by an undo-tree view
    description: Option<String>,
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
            preferred_child: None,
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

    /// Sets how long consecutive edits keep merging into one undo step.
    ///
    /// Zero disables grouping. Only affects subsequent pushes: nodes already
    /// merged stay merged, because splitting a compound command back into its
    /// parts would invent undo steps the user never saw.
    pub const fn set_group_timeout_ms(&mut self, timeout_ms: u64) {
        self.group_timeout_ms = timeout_ms;
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
                preferred_child: None,
                command: Some(command),
                timestamp: now,
                description: None,
            };

            // Add as child of current. The new branch becomes the active path,
            // so a later undo/redo round-trip returns here rather than to a
            // sibling that was abandoned earlier.
            if let Some(current_node) = self.nodes.get_mut(&self.current) {
                current_node.children.push(new_id);
                current_node.preferred_child = Some(new_id);
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

        // Remember which branch we came from so `redo` retraces this exact
        // step instead of re-entering a sibling that was abandoned earlier.
        let departed = self.current;
        if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
            parent_node.preferred_child = Some(departed);
        }

        self.current = parent_id;
        self.last_edit_time = None;

        Some(command.inverse())
    }

    /// Redoes by moving back down the active path.
    ///
    /// Returns to the child recorded by the most recent [`UndoTree::undo`],
    /// [`UndoTree::redo_branch`], or [`UndoTree::push`] at this node, so redo
    /// always reverses the traversal that led here. When no such child is
    /// recorded, the most recently created branch is taken — never the oldest,
    /// which would silently resurrect abandoned work.
    ///
    /// Returns the command to apply, or `None` if no children.
    pub fn redo(&mut self) -> Option<Command> {
        let current_node = self.nodes.get(&self.current)?;

        // Fall back to the newest branch; `preferred_child` is authoritative
        // whenever it still names a live child of this node.
        let child_id = current_node
            .preferred_child
            .filter(|id| current_node.children.contains(id))
            .or_else(|| current_node.children.last().copied())?;

        self.redo_into(child_id)
    }

    /// Redoes by moving to a specific child branch.
    ///
    /// `branch_index` indexes [`UndoTree::branch_count`] in creation order.
    /// The chosen branch becomes the active path, so a subsequent undo/redo
    /// round-trip returns to it.
    ///
    /// Returns the command to apply, or `None` if branch doesn't exist.
    pub fn redo_branch(&mut self, branch_index: usize) -> Option<Command> {
        let current_node = self.nodes.get(&self.current)?;
        let child_id = current_node.children.get(branch_index).copied()?;

        self.redo_into(child_id)
    }

    /// Moves down into `child_id`, recording it as the active path.
    ///
    /// Returns the command to apply, or `None` if the child carries none.
    fn redo_into(&mut self, child_id: UndoNodeId) -> Option<Command> {
        let command = self.nodes.get(&child_id)?.command.clone()?;

        if let Some(current_node) = self.nodes.get_mut(&self.current) {
            current_node.preferred_child = Some(child_id);
        }

        self.current = child_id;
        self.last_edit_time = None;

        Some(command)
    }

    /// Jumps to a specific node in the tree.
    ///
    /// Returns the sequence of commands needed to reach that node, which the
    /// caller MUST apply to the document — this method moves the tree pointer
    /// but does not touch document state.
    ///
    /// Every edge traversed is recorded as the active path, so a subsequent
    /// [`UndoTree::redo`] retraces the jump rather than re-entering a branch
    /// abandoned earlier. Where the upward and downward walks meet, the
    /// downward step wins, because that is the branch the caller ends up on.
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
        // (parent, child) edges traversed, applied only once the whole walk
        // succeeds so a failed jump leaves no partial preference behind.
        let mut traversed = Vec::new();

        // Undo from current to common ancestor
        let mut node = self.current;
        while node != common_ancestor {
            if let Some(current_node) = self.nodes.get(&node) {
                if let Some(cmd) = &current_node.command {
                    commands.push(cmd.inverse());
                }
                let parent = current_node.parent?;
                traversed.push((parent, node));
                node = parent;
            } else {
                break;
            }
        }

        // Redo from common ancestor to target
        let redo_path: Vec<_> = path_to_root_target
            .into_iter()
            .take_while(|id| *id != common_ancestor)
            .collect();

        let mut parent = common_ancestor;
        for node_id in redo_path.into_iter().rev() {
            if let Some(node_data) = self.nodes.get(&node_id) {
                if let Some(cmd) = &node_data.command {
                    commands.push(cmd.clone());
                }
            }
            traversed.push((parent, node_id));
            parent = node_id;
        }

        for (parent_id, child_id) in traversed {
            if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
                parent_node.preferred_child = Some(child_id);
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

    /// Which child of the current node a plain [`UndoTree::redo`] would take,
    /// as an index into [`UndoTree::branches`].
    ///
    /// `None` on a leaf. This is the *resolved* answer, not the raw
    /// `preferred_child`: it applies the same fallback `redo` does, so a caller
    /// showing "branch 2 of 3" and a caller pressing redo can never disagree.
    #[must_use]
    pub fn active_branch_index(&self) -> Option<usize> {
        let node = self.nodes.get(&self.current)?;
        if node.children.is_empty() {
            return None;
        }
        node.preferred_child
            .and_then(|id| node.children.iter().position(|child| *child == id))
            .or_else(|| node.children.len().checked_sub(1))
    }

    /// Points redo at a different branch of the current node, without moving.
    ///
    /// This is the "which fork am I about to take?" verb. It changes only the
    /// active path, so nothing is applied to the document and nothing needs
    /// undoing — which is exactly why it is safe to hold down while looking at
    /// a panel.
    ///
    /// Wraps in both directions: with three branches, stepping forward from the
    /// last returns to the first. Wrapping is right here where it is wrong for
    /// a list cursor, because the branches are a *ring* of alternatives with no
    /// natural first or last, and there is no risk of overshoot — nothing moves.
    ///
    /// Returns the new active index, or `None` when the current node is a leaf
    /// or has only one branch (nothing to cycle).
    pub fn cycle_branch(&mut self, forward: bool) -> Option<usize> {
        let node = self.nodes.get(&self.current)?;
        let count = node.children.len();
        if count < 2 {
            return None;
        }
        let current_index = self.active_branch_index()?;
        let next_index = if forward {
            (current_index + 1) % count
        } else {
            (current_index + count - 1) % count
        };

        let child_id = self
            .nodes
            .get(&self.current)?
            .children
            .get(next_index)
            .copied()?;
        self.nodes.get_mut(&self.current)?.preferred_child = Some(child_id);
        Some(next_index)
    }

    /// Returns the number of redo branches at the current node.
    #[must_use]
    pub fn branch_count(&self) -> usize {
        self.nodes
            .get(&self.current)
            .map_or(0, |n| n.children.len())
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
mod branch_tests;
#[cfg(test)]
mod tests;
