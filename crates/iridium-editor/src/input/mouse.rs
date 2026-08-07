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
use crate::input::keyboard::motions;
use crate::render::Viewport;
use crate::render::units::pixel_to_index;

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
            MouseEventKind::Scroll => Self::handle_scroll(event),
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
        let position = self.pixel_to_position(event.x, event.y, document, viewport);

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
        &self,
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

        let position = self.pixel_to_position(event.x, event.y, document, viewport);

        // Create selection from anchor to current position, respecting selection mode
        let selection = match self.selection_mode {
            SelectionMode::Character => Selection::new(anchor, position),
            SelectionMode::Word => Self::create_word_selection(anchor, position, document),
            SelectionMode::Line => Self::create_line_selection(anchor, position, document),
        };

        // A drag owns the primary selection and nothing else. Rebuilding the
        // whole state here would drop every cursor Ctrl+click added, so the
        // added cursors are carried across unchanged; the equality guard below
        // therefore compares the full state, and a drag that moves nothing —
        // including the synthetic move a button release interposes — still
        // reports `Handled` rather than a no-op command.
        let mut new_cursor = cursor.clone();
        new_cursor.primary = selection;

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
    const fn handle_scroll(event: &MouseEvent) -> MouseResult {
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

        let selection = Self::select_word_at(position, document);
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

        let selection = Self::select_line_at(position, document);
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
    ///
    /// Every pixel maps to a position: coordinates above or left of the text
    /// area clamp to the first line and column, and coordinates past the end of
    /// the document clamp to its last line and that line's length.
    fn pixel_to_position(
        &self,
        x: f32,
        y: f32,
        document: &Document,
        viewport: &Viewport,
    ) -> Position {
        // Calculate line from y position.
        //
        // `pixel_to_index` is the explicit form of the `as usize` this used to
        // spell by hand: it truncates toward zero and clamps everything below
        // `1.0` — negatives, `NaN`, a click above the first line — to zero,
        // which is what the cast did and what the clamps below assume. It
        // therefore subsumes the `floor()` that used to precede the cast; see
        // `pixel_to_index_matches_cast`, which asserts that equivalence over
        // exactly these inputs.
        let line_f = (y + viewport.scroll_offset_y) / viewport.line_height;
        let line = pixel_to_index(line_f).saturating_add(viewport.first_line);

        // Clamp to document bounds
        let line = line.min(document.line_count().saturating_sub(1));

        // Calculate column from x position, accounting for gutter width.
        // Uses 0.6 ratio approximation for monospace fonts (width:height).
        //
        // The same conversion also subsumes the `max(0.0)` that used to guard
        // the cast: a negative quotient — reachable through a negative
        // horizontal scroll offset — resolves to column zero either way.
        let text_x = (x - self.gutter_config.gutter_width).max(0.0);
        let char_width = viewport.line_height * 0.6;
        let column_f = (text_x + viewport.scroll_offset_x) / char_width;
        let column = pixel_to_index(column_f);

        // Clamp column to line length
        let line_len = document.line_len(line).unwrap_or(0);
        let column = column.min(line_len);

        Position::new(line, column)
    }

    /// Selects the word at the given position.
    fn select_word_at(position: Position, document: &Document) -> Selection {
        let line_text = document.line(position.line).unwrap_or_default();
        let chars: Vec<char> = line_text.chars().collect();

        if chars.is_empty() {
            return Selection::collapsed(position);
        }

        let col = position.column.min(chars.len().saturating_sub(1));

        // The shared definition, not a local copy of it: a double-click must
        // select exactly the run a word motion would step over.
        let is_word_char = motions::is_word_char;

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
    fn select_line_at(position: Position, document: &Document) -> Selection {
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
        anchor: Position,
        current: Position,
        document: &Document,
    ) -> Selection {
        let anchor_word = Self::select_word_at(anchor, document);
        let current_word = Self::select_word_at(current, document);

        if anchor <= current {
            Selection::new(anchor_word.start(), current_word.end())
        } else {
            Selection::new(anchor_word.end(), current_word.start())
        }
    }

    /// Creates a line selection spanning from anchor line to current line.
    fn create_line_selection(
        anchor: Position,
        current: Position,
        document: &Document,
    ) -> Selection {
        let anchor_line = Self::select_line_at(anchor, document);
        let current_line = Self::select_line_at(current, document);

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

        minimap_renderer
            .handle_click(event.x, event.y, minimap_dimensions, viewport)
            .map_or(MouseResult::Ignored, |target_line| {
                MouseResult::ScrollToLine { target_line }
            })
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

        minimap_renderer
            .handle_drag(event.x, event.y, minimap_dimensions, viewport)
            .map_or(MouseResult::Ignored, |target_line| {
                MouseResult::ScrollToLine { target_line }
            })
    }

    /// Handles a minimap release event to end dragging.
    ///
    /// # Arguments
    ///
    /// * `minimap_renderer` - The minimap renderer to notify
    pub const fn handle_minimap_release(
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
            assert_eq!(new_state.primary.head.line, 0);
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

        let selection = MouseHandler::select_word_at(Position::new(0, 2), &doc);
        assert_eq!(selection.start(), Position::new(0, 0));
        assert_eq!(selection.end(), Position::new(0, 5)); // "Hello"
    }

    #[test]
    fn line_selection() {
        let doc = create_test_document();

        let selection = MouseHandler::select_line_at(Position::new(0, 2), &doc);
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
    fn drag_keeps_the_cursors_ctrl_click_added() {
        // A genuine Ctrl-drag — add a cursor, then move somewhere else — is
        // the gesture that grows a selection under one of several carets. It
        // must widen the primary selection without taking the other carets
        // with it.
        let doc = create_test_document();
        let viewport = create_test_viewport();
        let mut cursor = CursorState::at(Position::new(0, 0));
        let mut handler = MouseHandler::new();

        let press = MouseEvent::press(MouseButton::Left, 100.0, 10.0).with_ctrl();
        if let MouseResult::Command(Command::SetSelection { new_state, .. }) =
            handler.handle_mouse(&press, &doc, &cursor, &viewport)
        {
            cursor = new_state;
        }
        assert_eq!(cursor.cursor_count(), 2, "Ctrl+click added a cursor");

        let drag = MouseEvent::drag(MouseButton::Left, 160.0, 30.0);
        let result = handler.handle_mouse(&drag, &doc, &cursor, &viewport);

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) = result {
            assert_eq!(
                new_state.secondary, cursor.secondary,
                "a drag must leave the added cursors exactly where they were"
            );
            assert_eq!(new_state.primary.anchor, Position::new(0, 4));
            assert_eq!(new_state.primary.head, Position::new(1, 9));
        } else {
            panic!("Expected SetSelection command, got {result:?}");
        }
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

    /// The document the hit-testing oracle below resolves against.
    ///
    /// Line lengths are deliberately unequal — 5, 13, 0, 5 — so that a column
    /// clamp on one line is visibly wrong on another, and so that the empty
    /// line exercises the zero-length clamp.
    fn hit_test_document() -> Document {
        Document::new("alpha\nbravo charlie\n\ndelta")
    }

    /// A viewport whose cell metrics are exact in `f32`.
    ///
    /// `line_height` is 20.0 and the column width the handler derives from it
    /// is `20.0 * 0.6`, which rounds to exactly 12.0 in `f32` — so "one cell"
    /// is a whole number of pixels and an assertion about an exact cell
    /// boundary is an assertion about the conversion, not about float noise.
    fn hit_test_viewport() -> Viewport {
        Viewport::new(800.0, 600.0, 20.0)
    }

    /// The gutter the oracle assumes: 48 px wide, so text starts at x = 48.
    fn hit_test_handler() -> MouseHandler {
        MouseHandler::with_gutter_config(GutterClickConfig {
            gutter_width: 48.0,
            fold_indicator_width: 16.0,
            fold_indicators_enabled: true,
        })
    }

    /// Pins `pixel_to_position` at the edges of its mapping, not in its
    /// interior.
    ///
    /// An off-by-one in hit-testing compiles, lints clean and renders
    /// identically; it surfaces only as a caret landing one cell from where
    /// the user aimed. The cases below are therefore chosen to be the ones a
    /// change in the pixel-to-index conversion would move first: an exact cell
    /// boundary and the pixel below it, a negative coordinate (which the cast
    /// saturates to zero rather than wrapping), a coordinate inside the
    /// gutter, coordinates past the last column and past the last line, a
    /// negative horizontal scroll offset — where `floor` and truncation
    /// disagree on the intermediate value and must still agree on the result —
    /// and the non-finite inputs the conversion documents.
    #[test]
    fn pixel_to_position_boundaries_are_pinned() {
        let document = hit_test_document();
        let handler = hit_test_handler();

        // The cell metrics the rest of the cases are stated in must be exact,
        // or "exactly on a boundary" would not mean anything below. This is
        // the handler's own column-width expression, compared bit-for-bit
        // against 12.0 rather than within a tolerance: a tolerance would pass
        // on a width that is merely close, and every boundary case below is
        // stated in whole cells of exactly twelve pixels.
        let viewport = hit_test_viewport();
        let column_width = viewport.line_height * 0.6;
        assert_eq!(
            column_width.to_bits(),
            12.0_f32.to_bits(),
            "column width is not exactly 12.0; the boundary cases below are stated in whole cells"
        );

        // Interior control: line 1, column 3, comfortably inside both cells.
        let resolved = handler.pixel_to_position(90.0, 30.0, &document, &viewport);
        assert_eq!(resolved, Position::new(1, 3), "interior click");

        // Exactly on the line-1 boundary, and the pixel below it.
        let resolved = handler.pixel_to_position(48.0, 20.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(1, 0),
            "y exactly on the line boundary"
        );
        let resolved = handler.pixel_to_position(48.0, 19.999, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 0),
            "y just above the line boundary"
        );

        // Exactly on the column-3 boundary, and the pixel left of it.
        let resolved = handler.pixel_to_position(84.0, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 3),
            "x exactly on the column boundary"
        );
        let resolved = handler.pixel_to_position(83.999, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 2),
            "x just left of the column boundary"
        );

        // Above the top of the document: the conversion saturates at zero, so
        // the click lands on the first line rather than wrapping to the last.
        let resolved = handler.pixel_to_position(48.0, -1.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 0),
            "one pixel above the first line"
        );
        let resolved = handler.pixel_to_position(48.0, -45.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 0),
            "two cells above the first line"
        );

        // Inside the gutter, at its right edge, and left of the widget
        // entirely: all clamp to column 0 without ever going negative.
        for x in [-30.0_f32, 0.0, 10.0, 47.999, 48.0] {
            let resolved = handler.pixel_to_position(x, 0.0, &document, &viewport);
            assert_eq!(
                resolved,
                Position::new(0, 0),
                "x = {x} resolves left of the text"
            );
        }

        // Past the last column of line 0 ("alpha", 5 columns).
        let resolved = handler.pixel_to_position(528.0, 0.0, &document, &viewport);
        assert_eq!(resolved, Position::new(0, 5), "x past the end of the line");

        // Past the last line of the document (4 lines).
        let resolved = handler.pixel_to_position(48.0, 200.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(3, 0),
            "y past the end of the document"
        );

        // The empty line clamps every column to zero.
        let resolved = handler.pixel_to_position(528.0, 40.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(2, 0),
            "empty line clamps the column"
        );

        // Non-finite inputs. `f32::max` returns the non-NaN operand, so a NaN
        // x becomes 0.0 before the conversion; a NaN y reaches it directly and
        // must resolve to the first line, not to a wrapped index.
        let resolved = handler.pixel_to_position(f32::NAN, f32::NAN, &document, &viewport);
        assert_eq!(resolved, Position::new(0, 0), "NaN coordinates");
        let resolved =
            handler.pixel_to_position(f32::NEG_INFINITY, f32::NEG_INFINITY, &document, &viewport);
        assert_eq!(resolved, Position::new(0, 0), "negative infinity");
        let resolved =
            handler.pixel_to_position(f32::INFINITY, f32::INFINITY, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(3, 5),
            "positive infinity saturates to the last cell"
        );
    }

    /// Pins `pixel_to_position` where the viewport is scrolled.
    ///
    /// Scrolling is what produces the negative intermediate values: a click
    /// above a scrolled viewport gives a negative line quotient, and a
    /// negative horizontal offset gives a negative column quotient. Those are
    /// exactly the inputs on which `floor` and truncation disagree about the
    /// intermediate `f32` while having to agree about the resolved cell.
    #[test]
    fn pixel_to_position_boundaries_are_pinned_when_scrolled() {
        let document = hit_test_document();
        let handler = hit_test_handler();

        // Scrolled down to line 2: a click above the viewport still resolves
        // to the first *visible* line, not to line 0 and not past the end.
        let mut viewport = hit_test_viewport();
        viewport.first_line = 2;
        let resolved = handler.pixel_to_position(48.0, -45.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(2, 0),
            "above a viewport scrolled to line 2"
        );
        let resolved = handler.pixel_to_position(48.0, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(2, 0),
            "top of a viewport scrolled to line 2"
        );
        let resolved = handler.pixel_to_position(48.0, 20.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(3, 0),
            "second row of a viewport scrolled to line 2"
        );

        // A negative vertical offset within the first line: the quotient is
        // -0.25, which floors to -1 and truncates to -0. Both must resolve to
        // line 0.
        let mut viewport = hit_test_viewport();
        viewport.scroll_offset_y = -10.0;
        let resolved = handler.pixel_to_position(48.0, 5.0, &document, &viewport);
        assert_eq!(resolved, Position::new(0, 0), "negative vertical quotient");

        // A negative horizontal offset: the quotient is -2.5, which floors to
        // -3 and truncates to -2. Both must resolve to column 0.
        let mut viewport = hit_test_viewport();
        viewport.scroll_offset_x = -30.0;
        let resolved = handler.pixel_to_position(48.0, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 0),
            "negative horizontal quotient"
        );

        // A positive horizontal offset that lands exactly on a boundary, and
        // half a cell short of one.
        let mut viewport = hit_test_viewport();
        viewport.scroll_offset_x = 24.0;
        let resolved = handler.pixel_to_position(48.0, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 2),
            "scrolled exactly two cells right"
        );
        viewport.scroll_offset_x = 18.0;
        let resolved = handler.pixel_to_position(48.0, 0.0, &document, &viewport);
        assert_eq!(
            resolved,
            Position::new(0, 1),
            "scrolled one and a half cells right"
        );
    }
}
