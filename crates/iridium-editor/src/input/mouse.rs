//! Mouse event handling.
//!
//! This module handles mouse events including clicks, drags, and multi-click
//! gestures for word and line selection.

use std::time::{Duration, Instant};

use super::types::{InputAction, InputResult};
use crate::editor::Position;

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseButton {
    /// Primary button (usually left).
    #[default]
    Primary,
    /// Secondary button (usually right).
    Secondary,
    /// Middle button (usually scroll wheel click).
    Middle,
    /// Other buttons.
    Other(u8),
}

/// Mouse event types.
#[derive(Debug, Clone)]
pub enum MouseEventKind {
    /// Button was pressed.
    Down,
    /// Button was released.
    Up,
    /// Mouse moved (with button held).
    Move,
    /// Mouse scrolled.
    Scroll { delta_x: f32, delta_y: f32 },
}

/// A mouse event.
#[derive(Debug, Clone)]
pub struct MouseEvent {
    /// The type of mouse event.
    pub kind: MouseEventKind,
    /// The mouse button involved (for click events).
    pub button: MouseButton,
    /// X coordinate in pixels.
    pub x: f32,
    /// Y coordinate in pixels.
    pub y: f32,
    /// Whether shift is held.
    pub shift: bool,
    /// Whether ctrl/cmd is held.
    pub ctrl: bool,
    /// Whether alt/option is held.
    pub alt: bool,
}

impl MouseEvent {
    /// Creates a new mouse down event.
    #[must_use]
    pub fn down(button: MouseButton, x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Down,
            button,
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// Creates a new mouse up event.
    #[must_use]
    pub fn up(button: MouseButton, x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Up,
            button,
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// Creates a new mouse move event.
    #[must_use]
    pub fn move_event(x: f32, y: f32) -> Self {
        Self {
            kind: MouseEventKind::Move,
            button: MouseButton::Primary,
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// Creates a new scroll event.
    #[must_use]
    pub fn scroll(delta_x: f32, delta_y: f32) -> Self {
        Self {
            kind: MouseEventKind::Scroll { delta_x, delta_y },
            button: MouseButton::Primary,
            x: 0.0,
            y: 0.0,
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    /// Sets the shift modifier.
    #[must_use]
    pub fn with_shift(mut self, shift: bool) -> Self {
        self.shift = shift;
        self
    }

    /// Sets the ctrl modifier.
    #[must_use]
    pub fn with_ctrl(mut self, ctrl: bool) -> Self {
        self.ctrl = ctrl;
        self
    }

    /// Sets the alt modifier.
    #[must_use]
    pub fn with_alt(mut self, alt: bool) -> Self {
        self.alt = alt;
        self
    }
}

/// Tracks multi-click state (double-click, triple-click).
#[derive(Debug, Clone)]
pub struct ClickState {
    /// Last click position.
    last_position: Option<(f32, f32)>,
    /// Last click time.
    last_time: Option<Instant>,
    /// Number of consecutive clicks.
    click_count: u8,
    /// Maximum time between clicks for multi-click (in milliseconds).
    multi_click_interval: Duration,
    /// Maximum distance between clicks for multi-click (in pixels).
    multi_click_tolerance: f32,
}

impl Default for ClickState {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickState {
    /// Creates a new click state tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_position: None,
            last_time: None,
            click_count: 0,
            multi_click_interval: Duration::from_millis(500),
            multi_click_tolerance: 5.0,
        }
    }

    /// Creates click state with custom settings.
    #[must_use]
    pub fn with_settings(interval_ms: u64, tolerance: f32) -> Self {
        Self {
            last_position: None,
            last_time: None,
            click_count: 0,
            multi_click_interval: Duration::from_millis(interval_ms),
            multi_click_tolerance: tolerance,
        }
    }

    /// Records a click and returns the click count (1, 2, or 3).
    pub fn record_click(&mut self, x: f32, y: f32) -> u8 {
        let now = Instant::now();

        // Check if this could be a multi-click
        let is_multi_click = match (self.last_position, self.last_time) {
            (Some((lx, ly)), Some(last_time)) => {
                let distance = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
                let elapsed = now.duration_since(last_time);

                distance <= self.multi_click_tolerance && elapsed <= self.multi_click_interval
            }
            _ => false,
        };

        if is_multi_click {
            self.click_count = (self.click_count % 3) + 1;
        } else {
            self.click_count = 1;
        }

        self.last_position = Some((x, y));
        self.last_time = Some(now);

        self.click_count
    }

    /// Resets the click state.
    pub fn reset(&mut self) {
        self.last_position = None;
        self.last_time = None;
        self.click_count = 0;
    }

    /// Returns the current click count.
    #[must_use]
    pub fn click_count(&self) -> u8 {
        self.click_count
    }
}

/// Tracks drag state.
#[derive(Debug, Clone, Default)]
struct DragState {
    /// Whether we're currently dragging.
    dragging: bool,
    /// Starting position of the drag.
    start_position: Option<Position>,
}

/// Handles mouse input and translates to editor actions.
#[derive(Debug)]
pub struct MouseHandler {
    /// Click state for multi-click detection.
    click_state: ClickState,
    /// Drag state.
    drag_state: DragState,
    /// Font metrics for position calculation.
    char_width: f32,
    line_height: f32,
    /// Editor content offset (for scrolling).
    scroll_x: f32,
    scroll_y: f32,
    /// Editor padding.
    padding_left: f32,
    padding_top: f32,
}

impl Default for MouseHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseHandler {
    /// Creates a new mouse handler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            click_state: ClickState::new(),
            drag_state: DragState::default(),
            char_width: 8.0,
            line_height: 20.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding_left: 0.0,
            padding_top: 0.0,
        }
    }

    /// Sets font metrics for position calculation.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
    }

    /// Sets scroll offset.
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        self.scroll_x = x;
        self.scroll_y = y;
    }

    /// Sets editor padding.
    pub fn set_padding(&mut self, left: f32, top: f32) {
        self.padding_left = left;
        self.padding_top = top;
    }

    /// Returns whether a drag is in progress.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag_state.dragging
    }

    /// Converts pixel coordinates to a buffer position.
    #[must_use]
    pub fn pixel_to_position(&self, x: f32, y: f32) -> Position {
        // Adjust for scroll and padding
        let adjusted_x = x - self.padding_left + self.scroll_x;
        let adjusted_y = y - self.padding_top + self.scroll_y;

        // Calculate line (clamp to 0 minimum)
        let line = if adjusted_y < 0.0 {
            0
        } else {
            (adjusted_y / self.line_height).floor() as usize
        };

        // Calculate column (clamp to 0 minimum)
        let column = if adjusted_x < 0.0 {
            0
        } else {
            // Round to nearest character position
            ((adjusted_x + self.char_width / 2.0) / self.char_width).floor() as usize
        };

        Position::new(line, column)
    }

    /// Handles a mouse event and returns the resulting action.
    #[must_use]
    pub fn handle_mouse(&mut self, event: &MouseEvent) -> InputResult {
        match event.kind {
            MouseEventKind::Down => self.handle_mouse_down(event),
            MouseEventKind::Up => self.handle_mouse_up(event),
            MouseEventKind::Move => self.handle_mouse_move(event),
            MouseEventKind::Scroll { .. } => {
                // Scrolling is handled by the viewport, not as an input action
                InputResult::not_handled()
            }
        }
    }

    /// Handles mouse down events.
    fn handle_mouse_down(&mut self, event: &MouseEvent) -> InputResult {
        // Only handle primary button clicks
        if event.button != MouseButton::Primary {
            return InputResult::not_handled();
        }

        let position = self.pixel_to_position(event.x, event.y);

        // Record click for multi-click detection
        let click_count = self.click_state.record_click(event.x, event.y);

        // Start drag
        self.drag_state.dragging = true;
        self.drag_state.start_position = Some(position);

        // Determine action based on click count and modifiers
        match click_count {
            1 => {
                if event.shift {
                    InputResult::action(InputAction::ShiftClick(position))
                } else {
                    InputResult::action(InputAction::Click(position))
                }
            }
            2 => InputResult::action(InputAction::DoubleClick(position)),
            3 => InputResult::action(InputAction::TripleClick(position)),
            _ => InputResult::action(InputAction::Click(position)),
        }
    }

    /// Handles mouse up events.
    fn handle_mouse_up(&mut self, event: &MouseEvent) -> InputResult {
        if event.button != MouseButton::Primary {
            return InputResult::not_handled();
        }

        if self.drag_state.dragging {
            self.drag_state.dragging = false;
            self.drag_state.start_position = None;
            InputResult::action(InputAction::DragEnd)
        } else {
            InputResult::consumed()
        }
    }

    /// Handles mouse move events.
    fn handle_mouse_move(&mut self, event: &MouseEvent) -> InputResult {
        if !self.drag_state.dragging {
            return InputResult::not_handled();
        }

        let position = self.pixel_to_position(event.x, event.y);
        InputResult::action(InputAction::DragMove(position))
    }

    /// Resets the handler state.
    pub fn reset(&mut self) {
        self.click_state.reset();
        self.drag_state.dragging = false;
        self.drag_state.start_position = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_click_state_single_click() {
        let mut state = ClickState::new();
        let count = state.record_click(10.0, 10.0);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_click_state_double_click() {
        let mut state = ClickState::new();
        state.record_click(10.0, 10.0);
        let count = state.record_click(10.0, 10.0);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_click_state_triple_click() {
        let mut state = ClickState::new();
        state.record_click(10.0, 10.0);
        state.record_click(10.0, 10.0);
        let count = state.record_click(10.0, 10.0);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_click_state_wraps_after_triple() {
        let mut state = ClickState::new();
        state.record_click(10.0, 10.0);
        state.record_click(10.0, 10.0);
        state.record_click(10.0, 10.0);
        let count = state.record_click(10.0, 10.0);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_click_state_resets_on_move() {
        let mut state = ClickState::new();
        state.record_click(10.0, 10.0);
        // Click far away
        let count = state.record_click(100.0, 100.0);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_mouse_handler_pixel_to_position() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        // First character of first line
        let pos = handler.pixel_to_position(0.0, 0.0);
        assert_eq!(pos, Position::new(0, 0));

        // Somewhere in the middle
        let pos = handler.pixel_to_position(25.0, 45.0);
        assert_eq!(pos, Position::new(2, 3)); // line 2, column 3

        // With rounding
        let pos = handler.pixel_to_position(14.0, 10.0);
        assert_eq!(pos, Position::new(0, 1)); // Should round to column 1
    }

    #[test]
    fn test_mouse_handler_with_scroll() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);
        handler.set_scroll(100.0, 200.0);

        // Position should account for scroll
        let pos = handler.pixel_to_position(0.0, 0.0);
        assert_eq!(pos, Position::new(10, 10)); // (200/20, 100/10)
    }

    #[test]
    fn test_mouse_handler_with_padding() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);
        handler.set_padding(50.0, 40.0);

        // Position should subtract padding
        let pos = handler.pixel_to_position(50.0, 40.0);
        assert_eq!(pos, Position::new(0, 0));

        let pos = handler.pixel_to_position(60.0, 60.0);
        assert_eq!(pos, Position::new(1, 1));
    }

    #[test]
    fn test_mouse_handler_negative_coords() {
        let handler = MouseHandler::new();

        // Should clamp to 0
        let pos = handler.pixel_to_position(-100.0, -100.0);
        assert_eq!(pos, Position::new(0, 0));
    }

    #[test]
    fn test_mouse_handler_click() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0);
        let result = handler.handle_mouse(&event);

        assert!(matches!(result.actions[0], InputAction::Click(_)));
    }

    #[test]
    fn test_mouse_handler_shift_click() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0).with_shift(true);
        let result = handler.handle_mouse(&event);

        assert!(matches!(result.actions[0], InputAction::ShiftClick(_)));
    }

    #[test]
    fn test_mouse_handler_double_click() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        // First click
        let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0);
        let _ = handler.handle_mouse(&event);
        let event = MouseEvent::up(MouseButton::Primary, 25.0, 15.0);
        let _ = handler.handle_mouse(&event);

        // Second click (double-click)
        let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0);
        let result = handler.handle_mouse(&event);

        assert!(matches!(result.actions[0], InputAction::DoubleClick(_)));
    }

    #[test]
    fn test_mouse_handler_triple_click() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        // Three clicks
        for _ in 0..2 {
            let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0);
            let _ = handler.handle_mouse(&event);
            let event = MouseEvent::up(MouseButton::Primary, 25.0, 15.0);
            let _ = handler.handle_mouse(&event);
        }

        // Third click (triple-click)
        let event = MouseEvent::down(MouseButton::Primary, 25.0, 15.0);
        let result = handler.handle_mouse(&event);

        assert!(matches!(result.actions[0], InputAction::TripleClick(_)));
    }

    #[test]
    fn test_mouse_handler_drag() {
        let mut handler = MouseHandler::new();
        handler.set_font_metrics(10.0, 20.0);

        // Start drag
        let event = MouseEvent::down(MouseButton::Primary, 0.0, 0.0);
        let _ = handler.handle_mouse(&event);
        assert!(handler.is_dragging());

        // Move during drag
        let event = MouseEvent::move_event(50.0, 30.0);
        let result = handler.handle_mouse(&event);
        assert!(matches!(result.actions[0], InputAction::DragMove(_)));

        // End drag
        let event = MouseEvent::up(MouseButton::Primary, 50.0, 30.0);
        let result = handler.handle_mouse(&event);
        assert!(matches!(result.actions[0], InputAction::DragEnd));
        assert!(!handler.is_dragging());
    }

    #[test]
    fn test_mouse_handler_secondary_button_ignored() {
        let mut handler = MouseHandler::new();

        let event = MouseEvent::down(MouseButton::Secondary, 25.0, 15.0);
        let result = handler.handle_mouse(&event);

        assert!(!result.consumed);
    }
}
