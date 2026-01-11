//! Core editor state and cursor management.
//!
//! This module provides the main Editor type along with cursor and
//! selection handling.

mod core;
mod cursor;
mod position;
mod selection;

pub use core::Editor;
pub use cursor::Cursor;
pub use position::Position;
pub use selection::Selection;
