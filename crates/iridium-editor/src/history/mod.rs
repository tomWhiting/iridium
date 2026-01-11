//! Undo/redo command history.
//!
//! This module provides command-based history management for undo/redo
//! functionality. Each edit operation is recorded as a command that can
//! be applied and unapplied.
//!
//! Two history implementations are provided:
//! - `History`: A simple linear stack (discards redo history on new edits)
//! - `UndoTree`: A branching tree that preserves all history branches

mod commands;
mod stack;
mod tree;

pub use commands::Command;
pub use stack::History;
pub use tree::{NodeId, NodeInfo, UndoTree};
