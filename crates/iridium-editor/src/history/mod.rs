//! Undo/redo command history.
//!
//! This module provides command-based history management for undo/redo
//! functionality. Each edit operation is recorded as a command that can
//! be applied and unapplied.

mod commands;
mod stack;

pub use commands::Command;
pub use stack::History;
