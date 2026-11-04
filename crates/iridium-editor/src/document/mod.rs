//! Document model and text manipulation.
//!
//! This module contains the core document representation including:
//! - [`Document`] - The text buffer backed by a rope data structure
//! - [`Position`] - A location in the document (line, column)
//! - [`Range`] - A span of text between two positions
//! - [`Selection`] - A selection with anchor and head
//! - [`CursorState`] - Multiple cursor support
//! - [`TextSnapshot`] - The text at one revision, shared rather than copied
//! - [`Divergence`] - Whether the text still matches a reference, kept current

mod buffer;
mod changes;
mod cursor;
mod divergence;
mod edit_span;
mod position;
mod snapshot;

pub use buffer::Document;
pub use changes::Unchanged;
pub use cursor::{CursorState, Selection};
pub use divergence::Divergence;
pub use edit_span::{EditSpan, EditSpanError, byte_point, compute_edit_span};
pub use position::{Position, Range};
pub use snapshot::TextSnapshot;
