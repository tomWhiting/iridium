//! Core editor state and cursor management.
//!
//! This module provides the main Editor type along with cursor and
//! selection handling.

mod controller;
mod core;
mod cursor;
pub mod navigation;
mod position;
mod selection;

pub use controller::{Clipboard, ControllerConfig, EditorController, MemoryClipboard};
pub use core::Editor;
pub use cursor::Cursor;
pub use position::Position;
pub use selection::Selection;
