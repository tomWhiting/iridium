//! Undo/redo history with tree structure.
//!
//! This module implements a tree-structured undo history where branches
//! are preserved when edits are made after undo operations. This ensures
//! users never lose work.

mod commands;
mod undo_tree;

pub use commands::Command;
pub use undo_tree::{UndoNodeId, UndoNodeInfo, UndoTree, UndoTreeInfo};
