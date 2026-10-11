use super::*;
mod unicode;

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
    let resolved = handler.pixel_to_position(f32::INFINITY, f32::INFINITY, &document, &viewport);
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
