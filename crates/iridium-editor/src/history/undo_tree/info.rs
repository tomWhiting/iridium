//! Describing the undo tree to callers outside the kernel.
//!
//! The tree itself deals in node identifiers and borrowed internals; a host —
//! an undo-tree panel, the wasm surface, a persisted session — needs plain
//! owned data it can serialize. These are the types and queries that translate
//! between the two, kept apart from the traversal logic so neither obscures
//! the other.

use serde::{Deserialize, Serialize};

use super::{UndoNodeId, UndoTree};
use std::time::Duration;

/// A single node of the undo tree, described for a caller outside the kernel.
///
/// Identifiers are rendered as decimal strings rather than integers so a
/// JavaScript host cannot silently lose precision on a 64-bit value; pair with
/// [`UndoNodeId::as_u64`] when working in Rust.
///
/// Serialized in camelCase, matching the palette's wire shape: these types
/// exist to cross a language boundary, and one convention over that boundary
/// is worth more than matching the Rust field names on the other side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoNodeInfo {
    /// Unique node identifier
    pub id: String,
    /// Parent node ID (`None` for the root)
    pub parent_id: Option<String>,
    /// Child node IDs, in creation order — index *i* is the branch reached by
    /// [`UndoTree::redo_branch`] with `branch_index == i`
    pub child_ids: Vec<String>,
    /// The child on this node's active path, the one a plain
    /// [`UndoTree::redo`] would take (`None` on a leaf, or on a node no
    /// traversal has descended from)
    pub preferred_child_id: Option<String>,
    /// Age of this edit in milliseconds, measured from the creation of the
    /// tree.
    ///
    /// This is a monotonic offset, not a wall-clock timestamp: the kernel
    /// records edits with an [`Instant`](std::time::Instant), which has no epoch,
    /// and a monotonic
    /// clock is the correct choice because it cannot run backwards when the
    /// system clock is adjusted mid-session.
    pub elapsed_ms: u64,
    /// Human-readable label for this edit, if one was recorded
    pub description: Option<String>,
    /// Whether this node is the tree's current position
    pub is_current: bool,
}

/// Information about the undo tree structure.
///
/// Serialized in camelCase, for the same reason as [`UndoNodeInfo`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// The whole tree at one instant, for a view that draws it.
///
/// [`UndoTree::branches`] answers "where can I go from here?" and
/// [`UndoTree::node_info`] answers "what is that node?"; neither lets a panel
/// draw the shape, because walking it one `node_info` call at a time means
/// N round trips across the wasm boundary for a tree that changes on every
/// keystroke. This is one call, one allocation, one serialization.
///
/// Every node carries `parent_id` and `preferred_child_id`, so the *drawing*
/// needs no further kernel queries: depth comes from following parents, and
/// the active path comes from following preferred children down from the root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoTreeSnapshot {
    /// Every node, in ascending id order — which is creation order, so a
    /// consumer that ignores the links still gets a chronological list.
    pub nodes: Vec<UndoNodeInfo>,
    /// The same summary [`UndoTree::get_tree_info`] returns, so a panel needs
    /// exactly one call rather than two that could disagree.
    pub info: UndoTreeInfo,
}

impl UndoTree {
    /// Describes the whole tree in one call.
    ///
    /// Ordering is by node id, which is assignment order, so the vector is
    /// chronological regardless of the tree's shape. That is deliberate: a
    /// panel wanting the drawing order derives it from the links, and a panel
    /// wanting "what did I do, in order" gets it for free.
    #[must_use]
    pub fn snapshot(&self) -> UndoTreeSnapshot {
        // Sorted by the underlying integer rather than by an `Ord` on
        // `UndoNodeId`: the type is deliberately opaque, and ordering it would
        // assert an ordering *semantic* on identifiers when what is wanted here
        // is only a stable, chronological enumeration.
        let mut ids: Vec<UndoNodeId> = self.nodes.keys().copied().collect();
        ids.sort_unstable_by_key(|id| id.as_u64());
        UndoTreeSnapshot {
            nodes: ids.iter().filter_map(|id| self.node_info(*id)).collect(),
            info: self.get_tree_info(),
        }
    }

    /// Returns the tree's current position.
    ///
    /// Pair with [`UndoTree::jump_to_node`] to return here after exploring
    /// elsewhere in the tree.
    #[must_use]
    pub const fn current_node_id(&self) -> UndoNodeId {
        self.current
    }

    /// Returns the tree's root, the state the document was opened in.
    #[must_use]
    pub const fn root_node_id(&self) -> UndoNodeId {
        self.root
    }

    /// Describes a single node, or `None` if no such node exists.
    #[must_use]
    pub fn node_info(&self, node_id: UndoNodeId) -> Option<UndoNodeInfo> {
        let node = self.nodes.get(&node_id)?;
        // The root's timestamp is the tree's creation time, so it needs no
        // separate field and cannot drift from it.
        let elapsed = self
            .nodes
            .get(&self.root)
            .map_or(Duration::ZERO, |root| {
                node.timestamp.saturating_duration_since(root.timestamp)
            })
            .as_millis();

        Some(UndoNodeInfo {
            id: node.id.as_u64().to_string(),
            parent_id: node.parent.map(|id| id.as_u64().to_string()),
            child_ids: node
                .children
                .iter()
                .map(|id| id.as_u64().to_string())
                .collect(),
            preferred_child_id: node
                .preferred_child
                .filter(|id| node.children.contains(id))
                .map(|id| id.as_u64().to_string()),
            // A tree that outlives 585 million years of monotonic time is not
            // a case worth a fallible signature; saturate instead.
            elapsed_ms: u64::try_from(elapsed).unwrap_or(u64::MAX),
            description: node.description.clone(),
            is_current: node_id == self.current,
        })
    }

    /// Describes every branch available from the current node, in creation
    /// order, so index *i* is the branch [`UndoTree::redo_branch`] takes for
    /// `branch_index == i`.
    #[must_use]
    pub fn branches(&self) -> Vec<UndoNodeInfo> {
        self.nodes
            .get(&self.current)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|id| self.node_info(*id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sets the human-readable label reported for a node.
    ///
    /// Returns `false` if no such node exists. Labels are what an undo-tree
    /// view shows in place of a bare node id.
    pub fn set_description(&mut self, node_id: UndoNodeId, description: Option<String>) -> bool {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.description = description;
            true
        } else {
            false
        }
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
}
