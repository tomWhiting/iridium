//! Editor events for notifying external systems of state changes.
//!
//! This module provides the event types and callback system for
//! communicating changes in the editor state to the host application.

mod types;

pub use types::{
    ContentChangedEvent, CursorMovedEvent, EditorEvent, EventCallback, EventEmitter,
    SelectionChangeReason, SelectionChangedEvent,
};
