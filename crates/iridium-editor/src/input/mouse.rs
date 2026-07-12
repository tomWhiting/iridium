//! Mouse input handling.
//!
//! This module provides mouse event handling for the editor, including:
//! - Click to position cursor
//! - Drag for selection
//! - Double-click for word selection
//! - Triple-click for line selection
//! - Wheel scroll

use web_time::Instant;

use serde::{Deserialize, Serialize};

use crate::document::{CursorState, Document, Position, Selection};
use crate::history::Command;
use crate::render::Viewport;

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    /// Left mouse button (primary)
    Left,
    /// Right mouse button (context menu)
    Right,
    /// Middle mouse button (scroll wheel click)
    Middle,
    /// Additional buttons (forward/back on gaming mice)
    Other(u8),
}

/// Type of mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseEventKind {
    /// Mouse button was pressed
    Press,
    /// Mouse button was released
    Release,
    /// Mouse moved while button was held
    Drag,
    /// Mouse moved without buttons held
    Move,
    /// Scroll wheel event
    Scroll,
}

/// A mouse event.
#[derive(Debug, Clone)]
pub struct MouseEvent {
    /// Type of mouse event
    pub kind: MouseEventKind,
    /// Which button (for press/release/drag events)
    pub button: Option<MouseButton>,
    /// X position in pixels (relative to editor left edge)
    pub x: f32,
    /// Y position in pixels (relative to editor top edge)
    pub y: f32,
    /// Scroll delta X (for scroll events, positive = right)
    pub scroll_x: f32,
    /// Scroll delta Y (for scroll events, positive = down)
    pub scroll_y: f32,
    /// Whether Ctrl/Cmd is held (for multi-cursor)
    pub ctrl: bool,
    /// Whether Shift is held (for extend selection)
    pub shift: bool,
    /// Whether Alt is held
    pub alt: bool,
}

impl MouseEvent {
    /// Creates a new mouse event for a button press.
    #[must_use]
    pub const fn press(button: MouseButton, x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Press,
            button: Some(button),
            x,
            y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Creates a new mouse event for a button release.
    #[must_use]
    pub const fn release(button: MouseButton, x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Release,
            button: Some(button),
            x,
            y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Creates a new mouse event for drag.
    #[must_use]
    pub const fn drag(button: MouseButton, x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Drag,
            button: Some(button),
            x,
            y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Creates a new mouse event for scroll.
    #[must_use]
    pub const fn scroll(x: f32, y: f32, delta_x: f32, delta_y: f32) -> Self {
        Self {
            kind: MouseEventKind::Scroll,
            button: None,
            x,
            y,
            scroll_x: delta_x,
            scroll_y: delta_y,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Sets the Ctrl modifier.
    #[must_use]
    pub const fn with_ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Sets the Shift modifier.
    #[must_use]
    pub const fn with_shift(mut self) -> Self {
        self.shift = true;
        self
    }
}

/// Result of handling a mouse event.
#[derive(Debug, Clone)]
pub enum MouseResult {
    /// The event was handled, no further action needed
    Handled,

    /// The event produced a command that should be applied
    Command(Command),

    /// Scroll the viewport by the given amount
    Scroll {
        /// Horizontal scroll delta in pixels
        delta_x: f32,
        /// Vertical scroll delta in pixels
        delta_y: f32,
    },

    /// Toggle fold at the given line (T136).
    ///
    /// This result indicates that the user clicked on a fold indicator
    /// in the gutter and the fold state should be toggled.
    ToggleFold {
        /// The document line number to toggle
        line: usize,
    },

    /// Scroll to a specific line (T142: minimap click-to-navigate)
    ScrollToLine {
        /// Target line to scroll to
        target_line: usize,
    },

    /// The event was not handled (pass to next handler)
    Ignored,
}

/// Multi-click detection threshold in milliseconds.
const MULTI_CLICK_THRESHOLD_MS: u128 = 500;

/// Maximum pixel distance for multi-click detection.
const MULTI_CLICK_DISTANCE: f32 = 5.0;

/// State tracking for multi-click detection.
#[derive(Debug, Clone, Default)]
struct ClickState {
    /// Time of last click
    last_click_time: Option<Instant>,
    /// Position of last click
    last_click_pos: Option<(f32, f32)>,
    /// Number of consecutive clicks
    click_count: u32,
}

/// Mouse event handler for the editor.
///
/// This handler processes mouse input and produces commands that can be
/// applied to the document. It handles:
/// - Single click to position cursor
/// - Double-click to select word
/// - Triple-click to select line
/// - Drag to select range
/// - Ctrl+click to add cursor (T106)
/// - Shift+click to extend selection
/// - Click on fold indicator to toggle fold (T136)
#[derive(Debug, Default)]
pub struct MouseHandler {
    /// Multi-click detection state
    click_state: ClickState,
    /// Whether we're currently in a drag operation
    is_dragging: bool,
    /// Start position of drag operation
    drag_anchor: Option<Position>,
    /// Selection mode during drag (character, word, or line)
    selection_mode: SelectionMode,
    /// Gutter configuration for fold indicator click detection
    gutter_config: GutterClickConfig,
}

/// Configuration for gutter click detection (T136).
#[derive(Debug, Clone)]
pub struct GutterClickConfig {
    /// Total gutter width in pixels
    pub gutter_width: f32,
    /// Width of the fold indicator area within the gutter
    pub fold_indicator_width: f32,
    /// Whether fold indicators are enabled
    pub fold_indicators_enabled: bool,
}

impl Default for GutterClickConfig {
    fn default() -> Self {
        Self {
            gutter_width: 48.0, // Default: padding(8) + 2 digits(16) + padding(8) + fold(16)
            fold_indicator_width: 16.0,
            fold_indicators_enabled: true,
        }
    }
}

/// Selection mode during drag operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SelectionMode {
    /// Normal character-by-character selection
    #[default]
    Character,
    /// Word-by-word selection (after double-click)
    Word,
    /// Line-by-line selection (after triple-click)
    Line,
}

impl MouseHandler {
    /// Creates a new mouse handler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new mouse handler with custom gutter configuration.
    #[must_use]
    pub fn with_gutter_config(config: GutterClickConfig) -> Self {
        Self {
            gutter_config: config,
            ..Self::default()
        }
    }

    /// Updates the gutter configuration.
    ///
    /// Call this when the gutter width changes (e.g., document grows).
    pub const fn set_gutter_config(&mut self, config: GutterClickConfig) {
        self.gutter_config = config;
    }

    /// Returns a reference to the gutter configuration.
    #[must_use]
    pub const fn gutter_config(&self) -> &GutterClickConfig {
        &self.gutter_config
    }

    /// Handles a mouse event.
    ///
    /// Returns a `MouseResult` indicating what action should be taken.
    /// The caller is responsible for applying any resulting commands.
    pub fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        document: &Document,
        cursor: &CursorState,
        viewport: &Viewport,
    ) -> MouseResult {
        match event.kind {
            MouseEventKind::Press if event.button == Some(MouseButton::Left) => {
                self.handle_left_press(event, document, cursor, viewport)
            },
            MouseEventKind::Release if event.button == Some(MouseButton::Left) => {
                self.handle_left_release()
            },
            MouseEventKind::Drag if event.button == Some(MouseButton::Left) => {
                self.handle_drag(event, document, cursor, viewport)
            },
            MouseEventKind::Scroll => self.handle_scroll(event),
            _ => MouseResult::Ignored,
        }
    }

    /// Handles left mouse button press.
    fn handle_left_press(
        &mut self,
        event: &MouseEvent,
        document: &Document,
        cursor: &CursorState,
        viewport: &Viewport,
    ) -> MouseResult {
        // T136: Check for fold indicator click first
        if let Some(line) = self.hit_test_fold_indicator(event.x, event.y, viewport) {
            return MouseResult::ToggleFold { line };
        }

        // Detect multi-click
        let click_count = self.detect_multi_click(event.x, event.y);

        // Convert pixel position to document position
        let Some(position) = self.pixel_to_position(event.x, event.y, document, viewport) else {
            return MouseResult::Ignored;
        };

        // Record drag anchor
        self.is_dragging = true;
        self.drag_anchor = Some(position);

        match click_count {
            1 => self.handle_single_click(position, event, cursor),
            2 => self.handle_double_click(position, document, cursor),
            _ => self.handle_triple_click(position, document, cursor),
        }
    }

    /// Tests if a click is in the fold indicator area (T136).
    ///
    /// Returns the document line number if the click is on a fold indicator.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn hit_test_fold_indicator(&self, x: f32, y: f32, viewport: &Viewport) -> Option<usize> {
        if !self.gutter_config.fold_indicators_enabled {
            return None;
        }

        // Fold indicator is at the right edge of the gutter
        let fold_x_start =
            self.gutter_config.gutter_width - self.gutter_config.fold_indicator_width;
        let fold_x_end = self.gutter_config.gutter_width;

        // Check if x is within fold indicator area
        if x < fold_x_start || x >= fold_x_end {
            return None;
        }

        // Check if y is valid
        if y < 0.0 {
            return None;
        }

        // Calculate which line was clicked
        let line_f = (y + viewport.scroll_offset_y) / viewport.line_height;
        let screen_line = line_f.floor() as usize;

        // Convert to document line (account for scroll)
        let doc_line = screen_line + viewport.first_line;

        Some(doc_line)
    }

    /// Handles left mouse button release.
    const fn handle_left_release(&mut self) -> MouseResult {
        self.is_dragging = false;
        self.drag_anchor = None;
        self.selection_mode = SelectionMode::Character;
        MouseResult::Handled
    }

    /// Handles mouse drag.
    fn handle_drag(
        &mut self,
        event: &MouseEvent,
        document: &Document,
        cursor: &CursorState,
        viewport: &Viewport,
    ) -> MouseResult {
        if !self.is_dragging {
            return MouseResult::Ignored;
        }

        let Some(anchor) = self.drag_anchor else {
            return MouseResult::Ignored;
        };

        let Some(position) = self.pixel_to_position(event.x, event.y, document, viewport) else {
            return MouseResult::Ignored;
        };

        // Create selection from anchor to current position, respecting selection mode
        let selection = match self.selection_mode {
            SelectionMode::Character => Selection::new(anchor, position),
            SelectionMode::Word => self.create_word_selection(anchor, position, document),
            SelectionMode::Line => self.create_line_selection(anchor, position, document),
        };

        let new_cursor = CursorState::new(selection);

        if new_cursor == *cursor {
            MouseResult::Handled
        } else {
            MouseResult::Command(Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            })
        }
    }

    /// Handles scroll wheel event.
    const fn handle_scroll(&mut self, event: &MouseEvent) -> MouseResult {
        MouseResult::Scroll {
            delta_x: event.scroll_x,
            delta_y: event.scroll_y,
        }
    }

    /// Handles single click.
    fn handle_single_click(
        &mut self,
        position: Position,
        event: &MouseEvent,
        cursor: &CursorState,
    ) -> MouseResult {
        self.selection_mode = SelectionMode::Character;

        let new_cursor = if event.ctrl {
            // T106: Ctrl+Click adds a new cursor at the clicked position
            let mut new_state = cursor.clone();
            new_state.add_cursor(Selection::collapsed(position));
            new_state
        } else if event.shift {
            // Extend selection from current anchor
            let selection = Selection::new(cursor.primary.anchor, position);
            CursorState::new(selection)
        } else {
            // Position cursor (single cursor mode)
            CursorState::at(position)
        };

        if new_cursor == *cursor {
            MouseResult::Handled
        } else {
            MouseResult::Command(Command::SetSelection {
                old_state: cursor.clone(),
                new_state: new_cursor,
            })
        }
    }

    /// Handles double click (select word).
    fn handle_double_click(
        &mut self,
        position: Position,
        document: &Document,
        cursor: &CursorState,
    ) -> MouseResult {
        self.selection_mode = SelectionMode::Word;

        let selection = self.select_word_at(position, document);
        let new_cursor = CursorState::new(selection);

        // Update drag anchor to word start for proper word-wise dragging
        self.drag_anchor = Some(selection.anchor);

        MouseResult::Command(Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        })
    }

    /// Handles triple click (select line).
    fn handle_triple_click(
        &mut self,
        position: Position,
        document: &Document,
        cursor: &CursorState,
    ) -> MouseResult {
        self.selection_mode = SelectionMode::Line;

        let selection = self.select_line_at(position, document);
        let new_cursor = CursorState::new(selection);

        // Update drag anchor to line start for proper line-wise dragging
        self.drag_anchor = Some(selection.anchor);

        MouseResult::Command(Command::SetSelection {
            old_state: cursor.clone(),
            new_state: new_cursor,
        })
    }

    /// Detects multi-click and returns click count (1, 2, or 3+).
    fn detect_multi_click(&mut self, x: f32, y: f32) -> u32 {
        let now = Instant::now();

        let is_multi_click = if let (Some(last_time), Some((last_x, last_y))) = (
            self.click_state.last_click_time,
            self.click_state.last_click_pos,
        ) {
            let elapsed = now.duration_since(last_time).as_millis();
            let distance = (x - last_x).hypot(y - last_y);

            elapsed < MULTI_CLICK_THRESHOLD_MS && distance < MULTI_CLICK_DISTANCE
        } else {
            false
        };

        if is_multi_click {
            self.click_state.click_count = (self.click_state.click_count % 3) + 1;
        } else {
            self.click_state.click_count = 1;
        }

        self.click_state.last_click_time = Some(now);
        self.click_state.last_click_pos = Some((x, y));

        self.click_state.click_count
    }

    /// Converts pixel coordinates to document position.
    fn pixel_to_position(
        &self,
        x: f32,
        y: f32,
        document: &Document,
        viewport: &Viewport,
    ) -> Option<Position> {
        // Calculate line from y position
        let line_f = (y + viewport.scroll_offset_y) / viewport.line_height;
        let line = (line_f.floor() as usize).saturating_add(viewport.first_line);

        // Clamp to document bounds
        let line = line.min(document.line_count().saturating_sub(1));

        // Calculate column from x position, accounting for gutter width.
        // Uses 0.6 ratio approximation for monospace fonts (width:height).
        let text_x = (x - self.gutter_config.gutter_width).max(0.0);
        let char_width = viewport.line_height * 0.6;
        let column_f = (text_x + viewport.scroll_offset_x) / char_width;
        let column = column_f.floor().max(0.0) as usize;

        // Clamp column to line length
        let line_len = document.line_len(line).unwrap_or(0);
        let column = column.min(line_len);

        Some(Position::new(line, column))
    }

    /// Selects the word at the given position.
    fn select_word_at(&self, position: Position, document: &Document) -> Selection {
        let line_text = document.line(position.line).unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();

        if chars.is_empty() {
            return Selection::collapsed(position);
        }

        let col = position.column.min(chars.len().saturating_sub(1));

        // Determine if we're on a word character
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

        // Find word boundaries
        let mut start = col;
        let mut end = col;

        if chars.get(col).is_some_and(|c| is_word_char(*c)) {
            // On a word character - select the word
            while start > 0 && chars.get(start - 1).is_some_and(|c| is_word_char(*c)) {
                start -= 1;
            }
            while end < chars.len() && chars.get(end).is_some_and(|c| is_word_char(*c)) {
                end += 1;
            }
        } else if chars.get(col).is_some_and(|c| c.is_whitespace()) {
            // On whitespace - select whitespace block
            while start > 0 && chars.get(start - 1).is_some_and(|c| c.is_whitespace()) {
                start -= 1;
            }
            while end < chars.len() && chars.get(end).is_some_and(|c| c.is_whitespace()) {
                end += 1;
            }
        } else {
            // On punctuation - select punctuation block
            while start > 0
                && chars
                    .get(start - 1)
                    .is_some_and(|c| !is_word_char(*c) && !c.is_whitespace())
            {
                start -= 1;
            }
            while end < chars.len()
                && chars
                    .get(end)
                    .is_some_and(|c| !is_word_char(*c) && !c.is_whitespace())
            {
                end += 1;
            }
        }

        Selection::new(
            Position::new(position.line, start),
            Position::new(position.line, end),
        )
    }

    /// Selects the line at the given position.
    fn select_line_at(&self, position: Position, document: &Document) -> Selection {
        let line = position.line;
        let line_len = document.line_len(line).unwrap_or(0);

        // Select from start of line to start of next line (or end of document)
        let start = Position::new(line, 0);
        let end = if line < document.line_count().saturating_sub(1) {
            Position::new(line + 1, 0)
        } else {
            Position::new(line, line_len)
        };

        Selection::new(start, end)
    }

    /// Creates a word selection spanning from anchor word to current word.
    fn create_word_selection(
        &self,
        anchor: Position,
        current: Position,
        document: &Document,
    ) -> Selection {
        let anchor_word = self.select_word_at(anchor, document);
        let current_word = self.select_word_at(current, document);

        if anchor <= current {
            Selection::new(anchor_word.start(), current_word.end())
        } else {
            Selection::new(anchor_word.end(), current_word.start())
        }
    }

    /// Creates a line selection spanning from anchor line to current line.
    fn create_line_selection(
        &self,
        anchor: Position,
        current: Position,
        document: &Document,
    ) -> Selection {
        let anchor_line = self.select_line_at(anchor, document);
        let current_line = self.select_line_at(current, document);

        if anchor.line <= current.line {
            Selection::new(anchor_line.start(), current_line.end())
        } else {
            Selection::new(anchor_line.end(), current_line.start())
        }
    }

    /// Handles a minimap click event (T142: click-to-navigate).
    ///
    /// Returns `ScrollToLine` result if the click was on the minimap,
    /// or `Ignored` if the click was outside minimap bounds.
    ///
    /// # Arguments
    ///
    /// * `event` - The mouse press event
    /// * `minimap_renderer` - The minimap renderer to delegate to
    /// * `minimap_dimensions` - Current minimap dimensions
    /// * `viewport` - Current viewport
    #[must_use]
    pub fn handle_minimap_click(
        &mut self,
        event: &MouseEvent,
        minimap_renderer: &mut crate::render::MinimapRenderer,
        minimap_dimensions: &crate::render::MinimapDimensions,
        viewport: &Viewport,
    ) -> MouseResult {
        if event.kind != MouseEventKind::Press || event.button != Some(MouseButton::Left) {
            return MouseResult::Ignored;
        }

        if !minimap_dimensions.contains(event.x, event.y) {
            return MouseResult::Ignored;
        }

        if let Some(target_line) =
            minimap_renderer.handle_click(event.x, event.y, minimap_dimensions, viewport)
        {
            MouseResult::ScrollToLine { target_line }
        } else {
            MouseResult::Ignored
        }
    }

    /// Handles a minimap drag event (T143: drag-to-scroll).
    ///
    /// Returns `ScrollToLine` result if dragging on minimap,
    /// or `Ignored` if not currently dragging on minimap.
    ///
    /// # Arguments
    ///
    /// * `event` - The mouse drag event
    /// * `minimap_renderer` - The minimap renderer to delegate to
    /// * `minimap_dimensions` - Current minimap dimensions
    /// * `viewport` - Current viewport
    #[must_use]
    pub fn handle_minimap_drag(
        &mut self,
        event: &MouseEvent,
        minimap_renderer: &mut crate::render::MinimapRenderer,
        minimap_dimensions: &crate::render::MinimapDimensions,
        viewport: &Viewport,
    ) -> MouseResult {
        if event.kind != MouseEventKind::Drag || event.button != Some(MouseButton::Left) {
            return MouseResult::Ignored;
        }

        if !minimap_renderer.is_dragging() {
            return MouseResult::Ignored;
        }

        if let Some(target_line) =
            minimap_renderer.handle_drag(event.x, event.y, minimap_dimensions, viewport)
        {
            MouseResult::ScrollToLine { target_line }
        } else {
            MouseResult::Ignored
        }
    }

    /// Handles a minimap release event to end dragging.
    ///
    /// # Arguments
    ///
    /// * `minimap_renderer` - The minimap renderer to notify
    pub fn handle_minimap_release(
        &mut self,
        minimap_renderer: &mut crate::render::MinimapRenderer,
    ) {
        minimap_renderer.handle_release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_document() -> Document {
        Document::new("Hello World\nSecond Line\nThird Line")
    }

    fn create_test_viewport() -> Viewport {
        Viewport::new(800.0, 600.0, 20.0)
    }

    #[test]
    fn single_click_positions_cursor() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::zero());
        let mut handler = MouseHandler::new();

        // Click in the middle of the editor
        let event = MouseEvent::press(MouseButton::Left, 60.0, 10.0);
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert!(new_state.primary.is_collapsed());
            // The exact position depends on pixel calculations
            assert!(new_state.primary.head.line == 0);
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn double_click_selects_word() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::zero());
        let mut handler = MouseHandler::new();

        // First click
        let event1 = MouseEvent::press(MouseButton::Left, 30.0, 10.0);
        let _ = handler.handle_mouse(&event1, &doc, &cursor, &viewport);
        handler.handle_left_release();

        // Second click (double-click)
        let event2 = MouseEvent::press(MouseButton::Left, 30.0, 10.0);
        let result = handler.handle_mouse(&event2, &doc, &cursor, &viewport);

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should have selected a word
            assert!(!new_state.primary.is_collapsed());
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn shift_click_extends_selection() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::new(0, 5));
        let mut handler = MouseHandler::new();

        // Shift+click
        let event = MouseEvent::press(MouseButton::Left, 150.0, 10.0).with_shift();
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should extend selection from original anchor
            assert!(!new_state.primary.is_collapsed());
            assert_eq!(new_state.primary.anchor, Position::new(0, 5));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn scroll_returns_scroll_result() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::zero());
        let mut handler = MouseHandler::new();

        let event = MouseEvent::scroll(100.0, 100.0, 0.0, 30.0);
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        if let MouseResult::Scroll { delta_y, .. } = result {
            assert!((delta_y - 30.0).abs() < f32::EPSILON);
        } else {
            panic!("Expected Scroll result");
        }
    }

    #[test]
    fn word_selection() {
        let doc = create_test_document();
        let handler = MouseHandler::new();

        let selection = handler.select_word_at(Position::new(0, 2), &doc);
        assert_eq!(selection.start(), Position::new(0, 0));
        assert_eq!(selection.end(), Position::new(0, 5)); // "Hello"
    }

    #[test]
    fn line_selection() {
        let doc = create_test_document();
        let handler = MouseHandler::new();

        let selection = handler.select_line_at(Position::new(0, 2), &doc);
        assert_eq!(selection.start(), Position::new(0, 0));
        assert_eq!(selection.end(), Position::new(1, 0)); // To start of next line
    }

    #[test]
    fn ctrl_click_adds_cursor() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::new(0, 0));
        let mut handler = MouseHandler::new();

        // Ctrl+click to add a cursor
        let event = MouseEvent::press(MouseButton::Left, 100.0, 30.0).with_ctrl();
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) = result {
            // Should have 2 cursors now
            assert_eq!(new_state.cursor_count(), 2);
            // Primary cursor should still be at original position
            assert_eq!(new_state.primary.head, Position::new(0, 0));
        } else {
            panic!("Expected SetSelection command");
        }
    }

    #[test]
    fn multiple_ctrl_clicks_add_multiple_cursors() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let mut cursor = CursorState::at(Position::new(0, 0));
        let mut handler = MouseHandler::new();

        // First Ctrl+click
        let event1 = MouseEvent::press(MouseButton::Left, 100.0, 10.0).with_ctrl();
        if let MouseResult::Command(Command::SetSelection { new_state, .. }) =
            handler.handle_mouse(&event1, &doc, &cursor, &viewport)
        {
            cursor = new_state;
        }
        handler.handle_left_release();

        // Second Ctrl+click
        let event2 = MouseEvent::press(MouseButton::Left, 100.0, 30.0).with_ctrl();
        if let MouseResult::Command(Command::SetSelection { new_state, .. }) =
            handler.handle_mouse(&event2, &doc, &cursor, &viewport)
        {
            cursor = new_state;
        }

        // Should have 3 cursors now
        assert_eq!(cursor.cursor_count(), 3);
    }

    #[test]
    fn fold_indicator_click_returns_toggle_fold() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::new(0, 0));

        // Create handler with specific gutter config
        let gutter_config = GutterClickConfig {
            gutter_width: 48.0,
            fold_indicator_width: 16.0,
            fold_indicators_enabled: true,
        };
        let mut handler = MouseHandler::with_gutter_config(gutter_config);

        // Click in fold indicator area (x between 32-48, line 1)
        let event = MouseEvent::press(MouseButton::Left, 40.0, 25.0);
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        if let MouseResult::ToggleFold { line } = result {
            assert_eq!(line, 1); // y=25 at line_height=20 is line 1
        } else {
            panic!("Expected ToggleFold result, got {result:?}");
        }
    }

    #[test]
    fn click_outside_fold_indicator_positions_cursor() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::new(0, 0));

        let gutter_config = GutterClickConfig {
            gutter_width: 48.0,
            fold_indicator_width: 16.0,
            fold_indicators_enabled: true,
        };
        let mut handler = MouseHandler::with_gutter_config(gutter_config);

        // Click in text area (x=100, past the gutter)
        let event = MouseEvent::press(MouseButton::Left, 100.0, 25.0);
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        // Should be a SetSelection command, not ToggleFold
        assert!(matches!(
            result,
            MouseResult::Command(Command::SetSelection { .. })
        ));
    }

    #[test]
    fn fold_indicator_disabled_passes_through() {
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let cursor = CursorState::at(Position::new(0, 0));

        let gutter_config = GutterClickConfig {
            gutter_width: 48.0,
            fold_indicator_width: 16.0,
            fold_indicators_enabled: false, // Disabled
        };
        let mut handler = MouseHandler::with_gutter_config(gutter_config);

        // Click in what would be fold indicator area
        let event = MouseEvent::press(MouseButton::Left, 40.0, 25.0);
        let result = handler.handle_mouse(&event, &doc, &cursor, &viewport);

        // Should NOT be ToggleFold since indicators are disabled
        assert!(!matches!(result, MouseResult::ToggleFold { .. }));
    }
}
