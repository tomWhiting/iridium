//! Event type definitions.
//!
//! Defines the events that the editor can emit to notify external systems
//! of state changes.

use crate::editor::{Position, Selection};

/// A change in the editor's text content.
#[derive(Debug, Clone)]
pub struct ContentChangedEvent {
    /// The start position of the change (before the change).
    pub start: Position,
    /// The end position of the change (before the change).
    pub end: Position,
    /// The text that was removed (empty if insertion only).
    pub removed_text: String,
    /// The text that was inserted (empty if deletion only).
    pub inserted_text: String,
    /// The new cursor position after the change.
    pub cursor_position: Position,
    /// Whether this change can be merged with the previous change for undo.
    pub is_mergeable: bool,
}

impl ContentChangedEvent {
    /// Creates a new content changed event.
    #[must_use]
    pub fn new(
        start: Position,
        end: Position,
        removed_text: String,
        inserted_text: String,
        cursor_position: Position,
    ) -> Self {
        Self {
            start,
            end,
            removed_text,
            inserted_text,
            cursor_position,
            is_mergeable: true,
        }
    }

    /// Creates an insertion event.
    #[must_use]
    pub fn insertion(position: Position, text: String, cursor_position: Position) -> Self {
        Self::new(position, position, String::new(), text, cursor_position)
    }

    /// Creates a deletion event.
    #[must_use]
    pub fn deletion(start: Position, end: Position, removed_text: String) -> Self {
        Self::new(start, end, removed_text, String::new(), start)
    }

    /// Creates a replacement event.
    #[must_use]
    pub fn replacement(
        start: Position,
        end: Position,
        removed_text: String,
        inserted_text: String,
        cursor_position: Position,
    ) -> Self {
        Self::new(start, end, removed_text, inserted_text, cursor_position)
    }

    /// Marks this event as not mergeable with previous changes.
    #[must_use]
    pub fn not_mergeable(mut self) -> Self {
        self.is_mergeable = false;
        self
    }

    /// Returns true if this is an insertion (no text removed).
    #[must_use]
    pub fn is_insertion(&self) -> bool {
        self.removed_text.is_empty() && !self.inserted_text.is_empty()
    }

    /// Returns true if this is a deletion (no text inserted).
    #[must_use]
    pub fn is_deletion(&self) -> bool {
        !self.removed_text.is_empty() && self.inserted_text.is_empty()
    }

    /// Returns true if this is a replacement (text both removed and inserted).
    #[must_use]
    pub fn is_replacement(&self) -> bool {
        !self.removed_text.is_empty() && !self.inserted_text.is_empty()
    }

    /// Returns the net change in character count.
    pub fn char_delta(&self) -> isize {
        self.inserted_text.chars().count() as isize - self.removed_text.chars().count() as isize
    }
}

/// A change in the selection state.
#[derive(Debug, Clone)]
pub struct SelectionChangedEvent {
    /// The previous selection (if any).
    pub previous: Option<Selection>,
    /// The current selection.
    pub current: Selection,
    /// The reason for the selection change.
    pub reason: SelectionChangeReason,
}

/// Reasons for a selection change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionChangeReason {
    /// Selection changed due to cursor movement.
    CursorMove,
    /// Selection changed due to mouse click.
    MouseClick,
    /// Selection changed due to drag operation.
    MouseDrag,
    /// Selection changed due to double-click (word selection).
    WordSelect,
    /// Selection changed due to triple-click (line selection).
    LineSelect,
    /// Selection changed due to keyboard selection (shift+arrows).
    KeyboardSelect,
    /// Selection changed due to select all.
    SelectAll,
    /// Selection changed due to text insertion/deletion.
    ContentChange,
    /// Selection changed programmatically.
    Programmatic,
}

impl SelectionChangedEvent {
    /// Creates a new selection changed event.
    #[must_use]
    pub fn new(
        previous: Option<Selection>,
        current: Selection,
        reason: SelectionChangeReason,
    ) -> Self {
        Self {
            previous,
            current,
            reason,
        }
    }

    /// Returns true if the selection now has content (is not a cursor).
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.current.is_selection()
    }

    /// Returns true if the selection was cleared (went from selection to cursor).
    #[must_use]
    pub fn selection_cleared(&self) -> bool {
        matches!(&self.previous, Some(sel) if sel.is_selection()) && self.current.is_cursor()
    }

    /// Returns true if a selection was created (went from cursor to selection).
    #[must_use]
    pub fn selection_created(&self) -> bool {
        (self.previous.is_none() || matches!(&self.previous, Some(sel) if sel.is_cursor()))
            && self.current.is_selection()
    }
}

/// A cursor position change event.
#[derive(Debug, Clone)]
pub struct CursorMovedEvent {
    /// The previous cursor position.
    pub previous: Position,
    /// The current cursor position.
    pub current: Position,
}

impl CursorMovedEvent {
    /// Creates a new cursor moved event.
    #[must_use]
    pub fn new(previous: Position, current: Position) -> Self {
        Self { previous, current }
    }

    /// Returns true if the cursor moved to a different line.
    #[must_use]
    pub fn line_changed(&self) -> bool {
        self.previous.line != self.current.line
    }

    /// Returns true if the cursor moved to a different column.
    #[must_use]
    pub fn column_changed(&self) -> bool {
        self.previous.column != self.current.column
    }
}

/// Union of all editor events.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The content has changed.
    ContentChanged(ContentChangedEvent),
    /// The selection has changed.
    SelectionChanged(SelectionChangedEvent),
    /// The cursor has moved.
    CursorMoved(CursorMovedEvent),
}

impl From<ContentChangedEvent> for EditorEvent {
    fn from(event: ContentChangedEvent) -> Self {
        EditorEvent::ContentChanged(event)
    }
}

impl From<SelectionChangedEvent> for EditorEvent {
    fn from(event: SelectionChangedEvent) -> Self {
        EditorEvent::SelectionChanged(event)
    }
}

impl From<CursorMovedEvent> for EditorEvent {
    fn from(event: CursorMovedEvent) -> Self {
        EditorEvent::CursorMoved(event)
    }
}

/// Type alias for event callback functions.
pub type EventCallback = Box<dyn Fn(&EditorEvent) + Send + Sync>;

/// Manages event emission and callbacks.
#[derive(Default)]
pub struct EventEmitter {
    /// Registered callbacks.
    callbacks: Vec<EventCallback>,
    /// Whether event emission is enabled.
    enabled: bool,
    /// Pending events (for batching).
    pending: Vec<EditorEvent>,
    /// Whether we're in a batch operation.
    batching: bool,
}

impl EventEmitter {
    /// Creates a new event emitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
            enabled: true,
            pending: Vec::new(),
            batching: false,
        }
    }

    /// Registers a callback for events.
    pub fn on_event(&mut self, callback: EventCallback) {
        self.callbacks.push(callback);
    }

    /// Enables or disables event emission.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns whether event emission is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Emits an event to all registered callbacks.
    pub fn emit(&mut self, event: EditorEvent) {
        if !self.enabled {
            return;
        }

        if self.batching {
            self.pending.push(event);
        } else {
            for callback in &self.callbacks {
                callback(&event);
            }
        }
    }

    /// Emits a content changed event.
    pub fn emit_content_changed(&mut self, event: ContentChangedEvent) {
        self.emit(EditorEvent::ContentChanged(event));
    }

    /// Emits a selection changed event.
    pub fn emit_selection_changed(&mut self, event: SelectionChangedEvent) {
        self.emit(EditorEvent::SelectionChanged(event));
    }

    /// Emits a cursor moved event.
    pub fn emit_cursor_moved(&mut self, event: CursorMovedEvent) {
        self.emit(EditorEvent::CursorMoved(event));
    }

    /// Begins batching events.
    ///
    /// Events will be collected until `end_batch` is called.
    pub fn begin_batch(&mut self) {
        self.batching = true;
    }

    /// Ends batching and emits all collected events.
    pub fn end_batch(&mut self) {
        self.batching = false;
        let events: Vec<_> = self.pending.drain(..).collect();

        if self.enabled {
            for event in events {
                for callback in &self.callbacks {
                    callback(&event);
                }
            }
        }
    }

    /// Clears all pending events without emitting them.
    pub fn cancel_batch(&mut self) {
        self.batching = false;
        self.pending.clear();
    }

    /// Returns the number of registered callbacks.
    #[must_use]
    pub fn callback_count(&self) -> usize {
        self.callbacks.len()
    }

    /// Clears all registered callbacks.
    pub fn clear_callbacks(&mut self) {
        self.callbacks.clear();
    }
}

impl std::fmt::Debug for EventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventEmitter")
            .field("enabled", &self.enabled)
            .field("callback_count", &self.callbacks.len())
            .field("pending_count", &self.pending.len())
            .field("batching", &self.batching)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_changed_insertion() {
        let event = ContentChangedEvent::insertion(
            Position::new(0, 5),
            "hello".to_string(),
            Position::new(0, 10),
        );

        assert!(event.is_insertion());
        assert!(!event.is_deletion());
        assert!(!event.is_replacement());
        assert_eq!(event.char_delta(), 5);
    }

    #[test]
    fn test_content_changed_deletion() {
        let event = ContentChangedEvent::deletion(
            Position::new(0, 5),
            Position::new(0, 10),
            "hello".to_string(),
        );

        assert!(!event.is_insertion());
        assert!(event.is_deletion());
        assert!(!event.is_replacement());
        assert_eq!(event.char_delta(), -5);
    }

    #[test]
    fn test_content_changed_replacement() {
        let event = ContentChangedEvent::replacement(
            Position::new(0, 5),
            Position::new(0, 10),
            "hello".to_string(),
            "world!".to_string(),
            Position::new(0, 11),
        );

        assert!(!event.is_insertion());
        assert!(!event.is_deletion());
        assert!(event.is_replacement());
        assert_eq!(event.char_delta(), 1); // "world!" (6) - "hello" (5) = 1
    }

    #[test]
    fn test_selection_changed_event() {
        let event = SelectionChangedEvent::new(
            None,
            Selection::new(Position::new(0, 0), Position::new(0, 10)),
            SelectionChangeReason::MouseClick,
        );

        assert!(event.has_selection());
        assert!(event.selection_created());
        assert!(!event.selection_cleared());
    }

    #[test]
    fn test_selection_cleared() {
        let event = SelectionChangedEvent::new(
            Some(Selection::new(Position::new(0, 0), Position::new(0, 10))),
            Selection::cursor(Position::new(0, 0)),
            SelectionChangeReason::CursorMove,
        );

        assert!(!event.has_selection());
        assert!(!event.selection_created());
        assert!(event.selection_cleared());
    }

    #[test]
    fn test_cursor_moved_event() {
        let event = CursorMovedEvent::new(Position::new(0, 5), Position::new(1, 0));

        assert!(event.line_changed());
        assert!(event.column_changed());
    }

    #[test]
    fn test_cursor_moved_same_line() {
        let event = CursorMovedEvent::new(Position::new(0, 5), Position::new(0, 10));

        assert!(!event.line_changed());
        assert!(event.column_changed());
    }

    #[test]
    fn test_event_emitter() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut emitter = EventEmitter::new();
        emitter.on_event(Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));

        emitter.emit_content_changed(ContentChangedEvent::insertion(
            Position::origin(),
            "test".to_string(),
            Position::new(0, 4),
        ));

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_event_emitter_disabled() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut emitter = EventEmitter::new();
        emitter.on_event(Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));

        emitter.set_enabled(false);
        emitter.emit_content_changed(ContentChangedEvent::insertion(
            Position::origin(),
            "test".to_string(),
            Position::new(0, 4),
        ));

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_event_emitter_batching() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut emitter = EventEmitter::new();
        emitter.on_event(Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));

        emitter.begin_batch();

        emitter.emit_content_changed(ContentChangedEvent::insertion(
            Position::origin(),
            "a".to_string(),
            Position::new(0, 1),
        ));
        emitter.emit_content_changed(ContentChangedEvent::insertion(
            Position::new(0, 1),
            "b".to_string(),
            Position::new(0, 2),
        ));

        // Not emitted yet
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        emitter.end_batch();

        // Now both should be emitted
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_event_emitter_cancel_batch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut emitter = EventEmitter::new();
        emitter.on_event(Box::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));

        emitter.begin_batch();
        emitter.emit_content_changed(ContentChangedEvent::insertion(
            Position::origin(),
            "test".to_string(),
            Position::new(0, 4),
        ));
        emitter.cancel_batch();

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}
