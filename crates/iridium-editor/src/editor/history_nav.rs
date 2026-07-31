//! Undo-tree navigation at the [`Editor`] level.
//!
//! [`crate::UndoTree`] has always been able to enter a chosen branch
//! ([`crate::UndoTree::redo_branch`]) or jump to an arbitrary node
//! ([`crate::UndoTree::jump_to_node`]), but both hand the caller a command —
//! or a list of them — that the caller must apply itself. Reached through
//! [`Editor::state_mut`] that is a footgun: the tree pointer moves, the
//! document does not, and every later undo replays against a state that never
//! existed.
//!
//! These methods close that gap. They apply the returned commands and then run
//! the same post-replay step as [`Editor::undo`] and [`Editor::redo`], so
//! cursor restoration, multi-cursor invalidation, search refresh and event
//! emission behave identically however the tree was traversed.

use super::core::{Editor, EditorEvent};
use crate::history::{Command, UndoNodeId, UndoNodeInfo, UndoTreeSnapshot};
use crate::input::HistoryRequest;

impl Editor {
    /// Performs one [`HistoryRequest`], reporting whether anything changed.
    ///
    /// The single funnel every history *command* goes through, so `Ctrl+Z`, the
    /// palette's *Undo* entry, and a host calling `run_command("history.undo")`
    /// are one code path rather than three. Branch cycling changes only which
    /// fork redo would take, so it reports `true` on a successful cycle without
    /// touching the document.
    pub fn perform_history_request(&mut self, request: HistoryRequest) -> bool {
        match request {
            HistoryRequest::Undo => self.undo(),
            HistoryRequest::Redo => self.redo(),
            HistoryRequest::RedoBranch(index) => self.redo_branch(index),
            HistoryRequest::NextBranch => self.cycle_history_branch(true),
            HistoryRequest::PreviousBranch => self.cycle_history_branch(false),
        }
    }

    /// Points redo at the next (or previous) branch of the current node.
    ///
    /// Nothing is applied to the document: this changes only which fork a
    /// subsequent redo takes. Returns `false` when there is nothing to cycle
    /// through — a leaf, or a node with a single branch.
    pub fn cycle_history_branch(&mut self, forward: bool) -> bool {
        let changed = self.state.history.cycle_branch(forward).is_some();
        if changed {
            // No document change and no cursor change, but a panel showing the
            // branches must repaint, and this is the only signal it has.
            self.emit(&EditorEvent::HistoryBranchChanged);
        }
        changed
    }

    /// Describes the whole undo tree in one call, for a view that draws it.
    #[must_use]
    pub fn history_snapshot(&self) -> UndoTreeSnapshot {
        self.state().history.snapshot()
    }

    /// Returns the tree's current position.
    #[must_use]
    pub const fn current_history_node(&self) -> UndoNodeId {
        self.state().history.current_node_id()
    }

    /// Describes every branch leaving the current node, in creation order.
    ///
    /// Index *i* is the branch [`Editor::redo_branch`] enters for
    /// `branch_index == i`. An empty result means the current node is a leaf:
    /// there is nothing to redo into.
    #[must_use]
    pub fn history_branches(&self) -> Vec<UndoNodeInfo> {
        self.state().history.branches()
    }

    /// Describes a single node of the undo tree, or `None` if no such node
    /// exists.
    #[must_use]
    pub fn history_node(&self, node_id: UndoNodeId) -> Option<UndoNodeInfo> {
        self.state().history.node_info(node_id)
    }

    /// Redoes into a specific branch of the current node.
    ///
    /// This is how a caller descends a fork the plain [`Editor::redo`] would
    /// not take: `redo` follows the branch last travelled, while this chooses
    /// explicitly and makes that choice the new active path.
    ///
    /// Returns `false` — leaving the document and the tree pointer untouched —
    /// when `branch_index` names no branch.
    pub fn redo_branch(&mut self, branch_index: usize) -> bool {
        let Some(cmd) = self.state.history.redo_branch(branch_index) else {
            return false;
        };

        self.apply_replayed(&cmd, "REDO_BRANCH_FAILED")
    }

    /// Moves to an arbitrary node of the undo tree, replaying the document to
    /// that state.
    ///
    /// The tree walks up to the common ancestor of the current node and the
    /// target and back down, so this reaches states no sequence of
    /// undo/redo could reach without first abandoning a branch.
    ///
    /// Returns `false` — leaving the document and the tree pointer untouched —
    /// when `node_id` names no node in this tree.
    pub fn jump_to_history_node(&mut self, node_id: UndoNodeId) -> bool {
        let Some(commands) = self.state.history.jump_to_node(node_id) else {
            return false;
        };

        // A jump to the node already occupied is a no-op, not a failure: there
        // is nothing to replay and nothing about the cursor to restore.
        let Some((last, leading)) = commands.split_last() else {
            return true;
        };

        for cmd in leading {
            if !self.apply_replayed_silently(cmd, "HISTORY_JUMP_FAILED") {
                return false;
            }
        }

        self.apply_replayed(last, "HISTORY_JUMP_FAILED")
    }

    /// Applies one replayed command and runs the shared post-replay step.
    ///
    /// On failure the error is emitted and `false` returned. The tree pointer
    /// has already moved by this point, which matches the pre-existing
    /// behaviour of [`Editor::undo`] and [`Editor::redo`]: a command that
    /// fails to apply to the document it was recorded against indicates the
    /// document was mutated outside the history, and no local recovery can
    /// make the two agree again.
    fn apply_replayed(&mut self, cmd: &Command, code: &str) -> bool {
        if let Err(e) = cmd.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: code.to_string(),
            });
            return false;
        }

        self.finish_history_replay(cmd);
        true
    }

    /// Applies one command of a multi-step replay without the post-replay
    /// step, which only the final command should trigger.
    ///
    /// A jump crosses several nodes; running cursor restoration and firing a
    /// content-changed event per edge would make hosts observe intermediate
    /// document states that were never a destination.
    fn apply_replayed_silently(&mut self, cmd: &Command, code: &str) -> bool {
        if let Err(e) = cmd.apply(&mut self.state.document, &mut self.state.cursor) {
            self.emit(&EditorEvent::Error {
                message: e.to_string(),
                code: code.to_string(),
            });
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests;
