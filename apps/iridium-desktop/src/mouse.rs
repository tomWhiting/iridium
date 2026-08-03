//! Translation from winit mouse events to the kernel's mouse machinery.
//!
//! The kernel already owns every selection semantic a pointer can express —
//! [`MouseHandler`] does click-count detection, drag anchoring, word and line
//! selection modes, shift-extend and ctrl/cmd-add-cursor, and hands back
//! reversible [`Command`]s — so this face implements none of it. What the
//! handler does *not* own honestly is the pixel mapping: its internal
//! `pixel_to_position` resolves a coordinate against a fixed
//! `line_height × 0.6` character width on an unwrapped, unfolded grid, while
//! the compositor's `pixel_to_position` resolves against the measured font
//! and the wrap/fold state of the last composed frame.
//!
//! # The synthesized grid
//!
//! This module reconciles the two by splitting the event in half. The
//! *position* is resolved first, by the compositor, from the real physical
//! pixel — wrap-aware, fold-aware, measured-font-accurate. The *semantics*
//! are then driven through the kernel's handler with a **synthesized**
//! coordinate: the center of the resolved `(line, column)` cell on the
//! handler's own idealized grid ([`cell_pixel`]). The handler's internal
//! mapping decodes that coordinate back to exactly the position the
//! compositor resolved — the half-cell offset keeps floating point on the
//! right side of every `floor` — so its click counting, drag modes and
//! selection commands all operate on honest positions without the kernel
//! changing at all. A test below proves the round trip against the real
//! handler rather than assuming it.
//!
//! Two consequences are accepted and stated: multi-click distance is judged
//! in cells rather than raw pixels (two clicks in one character cell always
//! count as a double-click, which is if anything more forgiving than the
//! 5-pixel rule on a Retina display), and the handler's gutter fold-indicator
//! hit test never fires, because a synthesized coordinate is always inside
//! the text column — a gutter click resolves to column zero of its line and
//! places the caret there. Fold toggling by mouse is a later slice's overlay
//! concern, not silently dropped: the key chords fold today.
//!
//! # Wheel scrolling stays on the face
//!
//! The handler's scroll arm only echoes the deltas back, and `scroll_y` is
//! the face's to own (the compositor seam says so), so wheel deltas are
//! converted here ([`wheel_delta`]) and applied to `scroll_y` under the same
//! clamp every other scroll write uses. winit's sign convention — positive
//! means the content moves down — is the opposite of the kernel's "positive
//! scrolls down the document", so the conversion negates; the tests pin both
//! variants so the sign cannot silently flip.

use iridium_editor::render::Viewport;
use iridium_editor::{
    CursorState, Document, Modifiers, MouseButton, MouseEvent, MouseEventKind, MouseHandler,
    MouseResult,
};
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

use crate::units::{index_to_f32, pixel_from_f64};

/// The character-width ratio the kernel's handler resolves columns with.
///
/// This must equal the ratio inside `MouseHandler::pixel_to_position`
/// (`line_height * 0.6`); the synthesized grid is built with it so the
/// handler decodes every synthesized coordinate back to the intended cell.
/// The round-trip test at the bottom of this module is what notices if the
/// kernel's ratio ever changes.
const CHAR_WIDTH_RATIO: f32 = 0.6;

/// The window geometry a mouse event is resolved against, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    /// The compositor's line height.
    pub line_height: f32,
    /// Surface width.
    pub width: f32,
    /// Surface height.
    pub height: f32,
}

/// The pointer's face-side state: the kernel's handler plus what winit
/// reports piecemeal — the last cursor position and whether the primary
/// button is down — so a `CursorMoved` can become a drag.
#[derive(Debug, Default)]
pub struct Pointer {
    /// The kernel's mouse machinery: click counting, drag modes, selection.
    handler: MouseHandler,
    /// The last cursor position winit reported, in physical pixels.
    position: (f32, f32),
    /// Whether the primary button is currently held.
    left_down: bool,
}

impl Pointer {
    /// A pointer that has not moved or pressed anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the cursor position winit last reported.
    pub const fn set_position(&mut self, x: f32, y: f32) {
        self.position = (x, y);
    }

    /// The last recorded cursor position, in physical pixels.
    #[must_use]
    pub const fn position(&self) -> (f32, f32) {
        self.position
    }

    /// Whether the primary button is held — a cursor move is a drag while it
    /// is.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.left_down
    }

    /// Handles a primary-button press at an already-resolved document
    /// position.
    ///
    /// `line` and `column` come from the compositor's hit test; the kernel's
    /// handler receives their synthesized cell center and owns everything
    /// after that — click counting included, so a second press on the same
    /// cell selects the word and a third the line.
    pub fn press(
        &mut self,
        line: usize,
        column: usize,
        modifiers: Modifiers,
        grid: Grid,
        document: &Document,
        cursor: &CursorState,
    ) -> MouseResult {
        self.left_down = true;
        let event = self.cell_event(MouseEventKind::Press, line, column, modifiers, grid);
        self.handler
            .handle_mouse(&event, document, cursor, &hit_viewport(grid))
    }

    /// Handles a cursor move while the primary button is held.
    pub fn drag(
        &mut self,
        line: usize,
        column: usize,
        modifiers: Modifiers,
        grid: Grid,
        document: &Document,
        cursor: &CursorState,
    ) -> MouseResult {
        if !self.left_down {
            return MouseResult::Ignored;
        }
        let event = self.cell_event(MouseEventKind::Drag, line, column, modifiers, grid);
        self.handler
            .handle_mouse(&event, document, cursor, &hit_viewport(grid))
    }

    /// Handles the primary button's release, ending any drag.
    pub fn release(
        &mut self,
        grid: Grid,
        document: &Document,
        cursor: &CursorState,
    ) -> MouseResult {
        self.left_down = false;
        let (x, y) = self.position;
        let event = MouseEvent::release(MouseButton::Left, x, y);
        self.handler
            .handle_mouse(&event, document, cursor, &hit_viewport(grid))
    }

    /// Builds the kernel mouse event for a resolved cell.
    fn cell_event(
        &self,
        kind: MouseEventKind,
        line: usize,
        column: usize,
        modifiers: Modifiers,
        grid: Grid,
    ) -> MouseEvent {
        let (x, y) = cell_pixel(
            line,
            column,
            grid.line_height,
            self.handler.gutter_config().gutter_width,
        );
        MouseEvent {
            kind,
            button: Some(MouseButton::Left),
            x,
            y,
            scroll_x: 0.0,
            scroll_y: 0.0,
            // The kernel documents the bit as "Ctrl/Cmd held (for
            // multi-cursor)", and ⌘-click is the mac spelling of add-cursor.
            ctrl: modifiers.ctrl || modifiers.meta,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }
}

/// The viewport the handler's own pixel mapping runs against: origin at the
/// document's first line, nothing scrolled, so the synthesized grid is an
/// identity mapping.
fn hit_viewport(grid: Grid) -> Viewport {
    Viewport::new(grid.width, grid.height, grid.line_height)
}

/// The synthesized coordinate for a document cell: its center on the
/// handler's idealized grid.
///
/// The half-cell offset is the correctness margin — a coordinate on a cell
/// *boundary* could `floor` to either neighbour once floating point rounds,
/// while a center is half a cell away from both.
pub fn cell_pixel(line: usize, column: usize, line_height: f32, gutter_width: f32) -> (f32, f32) {
    let char_width = line_height * CHAR_WIDTH_RATIO;
    let x = (index_to_f32(column) + 0.5).mul_add(char_width, gutter_width);
    let y = (index_to_f32(line) + 0.5) * line_height;
    (x, y)
}

/// Converts a winit wheel delta to the kernel's scroll convention, in
/// physical pixels: positive scrolls the document down (`scroll_y` grows).
///
/// Line deltas are scaled by the compositor's line height; pixel deltas —
/// what macOS trackpads report — are already physical and pass through with
/// only the sign flipped.
#[must_use]
pub fn wheel_delta(delta: &MouseScrollDelta, line_height: f32) -> (f32, f32) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-x * line_height, -y * line_height),
        MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => {
            (-pixel_signed(*x), -pixel_signed(*y))
        },
    }
}

/// A signed physical pixel delta from winit's `f64`, preserving the sign that
/// [`pixel_from_f64`] clamps away.
fn pixel_signed(value: f64) -> f32 {
    if value < 0.0 {
        -pixel_from_f64(-value)
    } else {
        pixel_from_f64(value)
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::history::Command;
    use iridium_editor::{CursorState, Document, Position};

    use super::*;

    /// The grid a 14px-font session composes at.
    const GRID: Grid = Grid {
        line_height: 19.6,
        width: 800.0,
        height: 600.0,
    };

    fn document() -> Document {
        Document::new("Hello World desktop\nSecond Line here\nThird Line\nfourth")
    }

    /// Where a `SetSelection` left the primary caret.
    fn head_of(result: MouseResult) -> Position {
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => new_state.primary.head,
            other => panic!("expected a selection command, got {other:?}"),
        }
    }

    #[test]
    fn the_synthesized_grid_round_trips_through_the_kernel_handler() {
        // The load-bearing claim of this module: for every cell the
        // compositor can resolve, the kernel handler decodes the synthesized
        // center back to that exact cell. Runs against the real handler so a
        // change to its internal ratio or flooring breaks here, not in a
        // window.
        let doc = document();
        for line in 0..4 {
            for column in 0..=6 {
                let mut pointer = Pointer::new();
                // Parked outside the pressed range, so every press is a
                // change and produces a command rather than `Handled`.
                let cursor = CursorState::at(Position::new(0, 15));
                let result = pointer.press(line, column, Modifiers::none(), GRID, &doc, &cursor);
                assert_eq!(
                    head_of(result),
                    Position::new(line, column),
                    "cell ({line}, {column}) did not survive the round trip"
                );
            }
        }
    }

    #[test]
    fn a_click_collapses_the_selection_to_the_cell() {
        let doc = document();
        let cursor = CursorState::at(Position::new(0, 0));
        let mut pointer = Pointer::new();
        let result = pointer.press(1, 3, Modifiers::none(), GRID, &doc, &cursor);
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => {
                assert!(new_state.primary.is_collapsed());
                assert_eq!(new_state.primary.head, Position::new(1, 3));
            },
            other => panic!("expected a selection command, got {other:?}"),
        }
    }

    #[test]
    fn a_shift_click_extends_from_the_existing_anchor() {
        let doc = document();
        let cursor = CursorState::at(Position::new(0, 2));
        let mut pointer = Pointer::new();
        let result = pointer.press(1, 5, Modifiers::shift(), GRID, &doc, &cursor);
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => {
                assert_eq!(new_state.primary.anchor, Position::new(0, 2));
                assert_eq!(new_state.primary.head, Position::new(1, 5));
            },
            other => panic!("expected a selection command, got {other:?}"),
        }
    }

    #[test]
    fn a_double_click_selects_the_word() {
        let doc = document();
        let mut cursor = CursorState::at(Position::new(0, 0));
        let mut pointer = Pointer::new();

        // First click on "World", release, second click on the same cell.
        if let MouseResult::Command(Command::SetSelection { new_state, .. }) =
            pointer.press(0, 7, Modifiers::none(), GRID, &doc, &cursor)
        {
            cursor = new_state;
        }
        let _ = pointer.release(GRID, &doc, &cursor);
        let result = pointer.press(0, 7, Modifiers::none(), GRID, &doc, &cursor);
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => {
                assert_eq!(new_state.primary.start(), Position::new(0, 6));
                assert_eq!(new_state.primary.end(), Position::new(0, 11), "\"World\"");
            },
            other => panic!("expected a word selection, got {other:?}"),
        }
    }

    #[test]
    fn a_drag_selects_from_press_to_the_dragged_cell() {
        let doc = document();
        let mut cursor = CursorState::at(Position::new(0, 0));
        let mut pointer = Pointer::new();

        if let MouseResult::Command(Command::SetSelection { new_state, .. }) =
            pointer.press(0, 2, Modifiers::none(), GRID, &doc, &cursor)
        {
            cursor = new_state;
        }
        assert!(pointer.is_dragging());
        let result = pointer.drag(2, 4, Modifiers::none(), GRID, &doc, &cursor);
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => {
                assert_eq!(new_state.primary.anchor, Position::new(0, 2));
                assert_eq!(new_state.primary.head, Position::new(2, 4));
            },
            other => panic!("expected a drag selection, got {other:?}"),
        }

        let _ = pointer.release(GRID, &doc, &cursor);
        assert!(!pointer.is_dragging());
        assert!(matches!(
            pointer.drag(2, 5, Modifiers::none(), GRID, &doc, &cursor),
            MouseResult::Ignored
        ));
    }

    #[test]
    fn a_cmd_click_reads_as_the_kernels_add_cursor_bit() {
        let doc = document();
        let cursor = CursorState::at(Position::new(0, 0));
        let mut pointer = Pointer::new();
        let meta = Modifiers {
            meta: true,
            ..Modifiers::none()
        };
        let result = pointer.press(1, 2, meta, GRID, &doc, &cursor);
        match result {
            MouseResult::Command(Command::SetSelection { new_state, .. }) => {
                assert_eq!(new_state.cursor_count(), 2, "cmd-click adds a cursor");
            },
            other => panic!("expected an added cursor, got {other:?}"),
        }
    }

    #[test]
    fn line_wheel_deltas_scale_by_line_height_and_flip_sign() {
        // Scrolling the wheel toward the user (winit y = -1: content moves
        // up) must grow scroll_y — the document scrolls down.
        let (dx, dy) = wheel_delta(&MouseScrollDelta::LineDelta(0.0, -1.0), 20.0);
        // `-0.0` is an acceptable zero: only the magnitude is asserted.
        assert!(dx.abs() < f32::EPSILON, "no horizontal delta was given");
        assert_eq!(dy.to_bits(), 20.0_f32.to_bits());

        let (_, up) = wheel_delta(&MouseScrollDelta::LineDelta(0.0, 2.0), 20.0);
        assert_eq!(up.to_bits(), (-40.0_f32).to_bits());
    }

    #[test]
    fn pixel_wheel_deltas_pass_through_with_the_sign_flipped() {
        let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(4.0, -12.5));
        let (dx, dy) = wheel_delta(&delta, 20.0);
        assert_eq!(dx.to_bits(), (-4.0_f32).to_bits());
        assert_eq!(dy.to_bits(), 12.5_f32.to_bits());
    }
}
