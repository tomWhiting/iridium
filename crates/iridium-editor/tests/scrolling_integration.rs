//! Integration tests for scrolling and viewport functionality.
//!
//! Tests the complete scrolling workflow including:
//! - Viewport scrolling and visible range calculation
//! - Mouse wheel and trackpad handling
//! - Page Up/Down navigation
//! - Go to Line command
//! - Viewport culling
//! - Large file performance
//! - ScrollChanged event emission

use iridium_editor::{
    EditorController, EditorView, InputAction, MouseButton, MouseEvent, MouseEventKind, Position,
    Viewport,
};

/// Helper to create a buffer with the specified number of lines.
fn create_large_content(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            if i < lines - 1 {
                format!("Line {:05}: This is line content for testing\n", i)
            } else {
                format!("Line {:05}: This is line content for testing", i)
            }
        })
        .collect()
}

mod viewport_tests {
    use super::*;

    #[test]
    fn test_viewport_visible_range_at_top() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(1000);

        let (first, last) = viewport.visible_line_range();
        assert_eq!(first, 0);
        // 600 / 20 = 30 lines visible
        assert_eq!(last, 29);
    }

    #[test]
    fn test_viewport_visible_range_after_scroll() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(1000);

        // Scroll to line 100
        viewport.scroll_to_line(100);

        let (first, last) = viewport.visible_line_range();
        // Verify scrolling moved the viewport and line 100 is visible
        assert!(first <= 100 && last >= 100, "Line 100 should be visible");
        // Should still show approximately 30 lines (600px / 20px)
        // May vary slightly due to subpixel scrolling
        let visible_count = last - first + 1;
        assert!(visible_count >= 29 && visible_count <= 32, "Expected ~30 lines visible, got {}", visible_count);
    }

    #[test]
    fn test_viewport_scroll_clamps_to_bounds() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(50);

        // Try to scroll past end
        viewport.scroll_to_line(100);

        // Should clamp to max valid scroll position
        let (first, _) = viewport.visible_line_range();
        assert!(first < 50);
    }

    #[test]
    fn test_viewport_ensure_cursor_visible_scrolls_down() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(1000);

        // Cursor at line 100 should scroll viewport
        viewport.ensure_cursor_visible(Position::new(100, 0));

        let (first, last) = viewport.visible_line_range();
        assert!(first <= 100 && last >= 100);
    }

    #[test]
    fn test_viewport_ensure_cursor_visible_scrolls_up() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(1000);

        // Scroll to line 500
        viewport.scroll_to_line(500);

        // Cursor at line 10 should scroll back up
        viewport.ensure_cursor_visible(Position::new(10, 0));

        let (first, last) = viewport.visible_line_range();
        assert!(first <= 10 && last >= 10);
    }

    #[test]
    fn test_viewport_page_navigation() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(1000);

        // Page down
        viewport.scroll_page_down();
        let (first1, _) = viewport.visible_line_range();
        assert!(first1 > 0);

        // Page up should go back
        viewport.scroll_page_up();
        let (first2, _) = viewport.visible_line_range();
        assert_eq!(first2, 0);
    }
}

mod scroll_input_tests {
    use super::*;

    #[test]
    fn test_mouse_wheel_scroll_emits_action() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(100));
        view.set_font_metrics(8.0, 20.0);

        let event = MouseEvent {
            kind: MouseEventKind::Scroll {
                delta_x: 0.0,
                delta_y: 60.0, // 3 lines worth
            },
            button: MouseButton::Primary,
            x: 100.0,
            y: 100.0,
            shift: false,
            ctrl: false,
            alt: false,
        };

        let result = view.controller_mut().mouse_handler_mut().handle_mouse(&event);
        assert!(result.consumed);
        assert!(!result.actions.is_empty());

        match &result.actions[0] {
            InputAction::Scroll { delta_y, .. } => {
                assert!(*delta_y > 0.0);
            }
            _ => panic!("Expected Scroll action"),
        }
    }

    #[test]
    fn test_page_up_down_keyboard_handling() {
        let mut controller = EditorController::with_content(&create_large_content(1000));

        // Move cursor to middle
        controller.execute_action(InputAction::GoToLine(500));
        let pos1 = controller.editor().cursor_position();
        assert_eq!(pos1.line, 499); // 1-indexed to 0-indexed

        // Page down
        controller.execute_action(InputAction::PageDown);
        let pos2 = controller.editor().cursor_position();
        assert!(pos2.line > pos1.line);

        // Page up
        controller.execute_action(InputAction::PageUp);
        let pos3 = controller.editor().cursor_position();
        assert!(pos3.line < pos2.line);
    }

    #[test]
    fn test_go_to_line_command() {
        let mut controller = EditorController::with_content(&create_large_content(1000));

        // Go to line 500 (1-indexed)
        controller.execute_action(InputAction::GoToLine(500));
        assert_eq!(controller.editor().cursor_position().line, 499);

        // Go to line 1
        controller.execute_action(InputAction::GoToLine(1));
        assert_eq!(controller.editor().cursor_position().line, 0);

        // Go to last line
        controller.execute_action(InputAction::GoToLine(1000));
        assert_eq!(controller.editor().cursor_position().line, 999);

        // Go past end should clamp
        controller.execute_action(InputAction::GoToLine(5000));
        assert_eq!(controller.editor().cursor_position().line, 999);
    }
}

mod viewport_culling_tests {
    use super::*;

    #[test]
    fn test_visible_text_returns_only_visible_lines() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(1000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        let visible_text = view.visible_text();

        // Should only have ~30 lines worth of text (600px / 20px per line)
        let line_count = visible_text.lines().count();
        assert!(line_count <= 35); // Allow some buffer
        assert!(line_count >= 25);

        // First line should be line 0
        assert!(visible_text.starts_with("Line 00000"));
    }

    #[test]
    fn test_visible_text_after_scrolling() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(1000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        // Scroll to line 100
        view.viewport_mut().scroll_to_line(100);
        view.update();

        let visible_text = view.visible_text();
        let (first, _) = view.visible_line_range();

        // First visible line in text should match viewport's first line
        let expected_line = format!("Line {:05}", first);
        assert!(
            visible_text.contains(&expected_line),
            "Expected to find '{}' in visible text starting with '{}'",
            expected_line,
            visible_text.lines().next().unwrap_or("")
        );
    }

    #[test]
    fn test_visible_line_range_matches_visible_text() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(1000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        let (first, last) = view.visible_line_range();
        let visible_text = view.visible_text();
        let lines: Vec<&str> = visible_text.lines().collect();

        // Line count should match range (approximately - last line may be partial)
        assert!(lines.len() <= (last - first + 2));
    }
}

mod scroll_event_tests {
    use super::*;

    #[test]
    fn test_scroll_changed_returns_true_after_scroll() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(1000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        // Scroll to line 100
        view.viewport_mut().scroll_to_line(100);

        // take_scroll_changed should return true
        let changed = view.viewport_mut().take_scroll_changed();
        assert!(changed);

        // Calling again should return false (already consumed)
        let changed_again = view.viewport_mut().take_scroll_changed();
        assert!(!changed_again);
    }

    #[test]
    fn test_no_scroll_change_without_scroll() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(1000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        // No scroll, should return false
        let changed = view.viewport_mut().take_scroll_changed();
        assert!(!changed);
    }
}

mod gutter_rendering_tests {
    use super::*;

    #[test]
    fn test_gutter_generates_line_numbers() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(100));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        let gutter = view.gutter_renderer();
        let cursor_line = view.controller().editor().cursor_position().line;
        let (first, last) = view.visible_line_range();
        let line_numbers = gutter.generate_line_numbers(first, last, cursor_line, 0.0);

        // Should have line numbers for visible lines
        assert!(!line_numbers.is_empty());

        // Check first line number (1-indexed for display)
        let first_entry = &line_numbers[0];
        assert_eq!(first_entry.number, 1);
    }

    #[test]
    fn test_gutter_width_scales_with_line_count() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(10));
        view.set_font_metrics(8.0, 20.0);
        view.update();
        let width1 = view.gutter_width();

        let mut view2 = EditorView::with_content(800.0, 600.0, &create_large_content(10000));
        view2.set_font_metrics(8.0, 20.0);
        view2.update();
        let width2 = view2.gutter_width();

        // More lines = wider gutter for more digits
        assert!(width2 > width1);
    }
}

mod momentum_scroll_tests {
    use super::*;

    #[test]
    fn test_momentum_scroll_decays() {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(10000);

        // Start trackpad scroll with velocity (is_end = false to initiate momentum)
        viewport.handle_trackpad_scroll(0.0, 100.0, false);

        let initial_y = viewport.scroll_y;

        // Update momentum several times
        for _ in 0..10 {
            viewport.update_momentum();
        }

        // Should have scrolled further due to momentum
        assert!(viewport.scroll_y > initial_y);

        // Eventually momentum should stop
        for _ in 0..100 {
            viewport.update_momentum();
        }

        // Record position and verify it's stable
        let stable_y = viewport.scroll_y;
        viewport.update_momentum();
        assert!((viewport.scroll_y - stable_y).abs() < 0.1);
    }
}

mod line_cache_integration_tests {
    use super::*;

    #[test]
    fn test_line_cache_updates_with_scroll() {
        let mut view = EditorView::with_content(800.0, 600.0, &create_large_content(10000));
        view.set_font_metrics(8.0, 20.0);
        view.update();

        // Scroll to different positions and verify highlight_rects works
        for line in [0, 100, 500, 1000, 5000, 9000] {
            view.viewport_mut().scroll_to_line(line);
            view.update();

            // This should work without panic (uses line cache internally)
            let _ = view.highlight_rects();
        }
    }

    #[test]
    fn test_large_file_visible_text_performance() {
        // Create a 100k line file
        let content = create_large_content(100_000);
        let mut view = EditorView::with_content(800.0, 600.0, &content);
        view.set_font_metrics(8.0, 20.0);
        view.update();

        // Scroll to various positions and measure that visible_text is fast
        let positions = [0, 1000, 10000, 50000, 99000];

        for &line in &positions {
            view.viewport_mut().scroll_to_line(line);
            view.update();

            // visible_text should only return ~30 lines, not 100k
            let visible = view.visible_text();
            let line_count = visible.lines().count();
            assert!(line_count < 50, "Too many lines returned: {}", line_count);
        }
    }
}
