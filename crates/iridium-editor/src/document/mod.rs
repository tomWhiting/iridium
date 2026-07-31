//! Document model and text manipulation.
//!
//! This module contains the core document representation including:
//! - [`Document`] - The text buffer backed by a rope data structure
//! - [`Position`] - A location in the document (line, column)
//! - [`Range`] - A span of text between two positions
//! - [`Selection`] - A selection with anchor and head
//! - [`CursorState`] - Multiple cursor support

mod buffer;
mod cursor;
mod edit_span;
mod position;

pub use buffer::Document;
pub use cursor::{CursorState, Selection};
pub use edit_span::{EditSpan, EditSpanError, byte_point, compute_edit_span};
pub use position::{Position, Range};
