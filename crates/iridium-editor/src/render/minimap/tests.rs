//! Tests for minimap rendering.

use super::*;
use crate::document::Document;
use crate::render::Viewport;
use crate::theme::Theme;

fn create_test_document() -> Document {
    let content = (0..100)
        .map(|i| format!("Line {} with some content here\n", i))
        .collect::<String>();
    Document::new(&content)
}

fn create_test_viewport() -> Viewport {
    Viewport::new(800.0, 600.0, 20.0)
}

#[test]
fn minimap_config_default() {
    let config = MinimapConfig::default();
    assert!((config.width - DEFAULT_MINIMAP_WIDTH).abs() < f32::EPSILON);
    assert!(config.enabled);
    assert_eq!(config.position, MinimapPosition::Right);
}

#[test]
fn dimensions_calculate() {
    let config = MinimapConfig::default();
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    assert!((dims.width - DEFAULT_MINIMAP_WIDTH).abs() < f32::EPSILON);
    assert_eq!(dims.total_lines, 101); // 100 lines + trailing newline handling
    assert!(dims.line_height > 0.0);
}

#[test]
fn dimensions_y_to_line() {
    let config = MinimapConfig::default();
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    assert_eq!(dims.y_to_line(0.0), dims.first_visible_line);
    assert_eq!(
        dims.y_to_line(dims.line_height * 5.0),
        dims.first_visible_line + 5
    );
}

#[test]
fn dimensions_line_to_y() {
    let config = MinimapConfig::default();
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    let y = dims.line_to_y(dims.first_visible_line + 10);
    let expected = 10.0 * dims.line_height;
    assert!((y - expected).abs() < f32::EPSILON);
}

#[test]
fn dimensions_contains() {
    let config = MinimapConfig::default();
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    // Inside bounds
    assert!(dims.contains(dims.x + 10.0, dims.y + 10.0));

    // Outside bounds
    assert!(!dims.contains(0.0, 0.0)); // Too far left
    assert!(!dims.contains(dims.x + dims.width + 10.0, 50.0)); // Too far right
}

#[test]
fn viewport_indicator_calculate() {
    let config = MinimapConfig::default();
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);
    let indicator =
        ViewportIndicator::calculate(&dims, &viewport, 0.15, Color::rgb(1.0, 1.0, 1.0));

    assert!(indicator.height > 0.0);
    assert!((indicator.color.a - 0.15).abs() < 0.01);
}

#[test]
fn minimap_line_from_text() {
    let line = MinimapLine::from_text(0, "Hello World", Color::rgb(1.0, 1.0, 1.0));

    assert_eq!(line.line_number, 0);
    assert_eq!(line.segments.len(), 1);
    assert_eq!(line.segments[0].start, 0);
    assert_eq!(line.segments[0].end, 11);
}

#[test]
fn minimap_line_empty() {
    let line = MinimapLine::from_text(5, "", Color::rgb(1.0, 1.0, 1.0));

    assert_eq!(line.line_number, 5);
    assert!(line.segments.is_empty());
}

#[test]
fn renderer_new() {
    let renderer = MinimapRenderer::new();
    assert!(renderer.is_enabled());
    assert!(!renderer.is_dragging());
}

#[test]
fn renderer_set_enabled() {
    let mut renderer = MinimapRenderer::new();

    renderer.set_enabled(false);
    assert!(!renderer.is_enabled());
    assert!((renderer.width() - 0.0).abs() < f32::EPSILON);

    renderer.set_enabled(true);
    assert!(renderer.is_enabled());
    assert!(renderer.width() > 0.0);
}

#[test]
fn renderer_prepare_lines() {
    let mut renderer = MinimapRenderer::new();
    let document = create_test_document();
    let viewport = create_test_viewport();
    let theme = Theme::dark();

    let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
    renderer.prepare_lines(&document, &dims, &theme, None);

    assert!(!renderer.lines().is_empty());
}

#[test]
fn renderer_generate_rects() {
    let mut renderer = MinimapRenderer::new();
    let document = create_test_document();
    let viewport = create_test_viewport();
    let theme = Theme::dark();

    let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
    renderer.prepare_lines(&document, &dims, &theme, None);

    let rects = renderer.generate_rects(&dims);
    assert!(!rects.is_empty());

    // Verify rects are within bounds
    for (rect, _color) in &rects {
        assert!(rect.x >= dims.x);
        assert!(rect.x + rect.width <= dims.x + dims.width + 1.0);
        assert!(rect.height > 0.0);
    }
}

#[test]
fn renderer_handle_click() {
    let mut renderer = MinimapRenderer::new();
    let document = create_test_document();
    let viewport = create_test_viewport();

    let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);

    // Click in the middle of the minimap
    let click_x = dims.x + dims.width / 2.0;
    let click_y = dims.height / 2.0;

    let result = renderer.handle_click(click_x, click_y, &dims, &viewport);
    assert!(result.is_some());

    // Should start dragging
    assert!(renderer.is_dragging());
}

#[test]
fn renderer_handle_click_outside() {
    let mut renderer = MinimapRenderer::new();
    let document = create_test_document();
    let viewport = create_test_viewport();

    let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);

    // Click outside the minimap
    let result = renderer.handle_click(0.0, 50.0, &dims, &viewport);
    assert!(result.is_none());
    assert!(!renderer.is_dragging());
}

#[test]
fn drag_state_start_and_end() {
    let mut state = MinimapDragState::new();

    assert!(!state.is_dragging);

    state.start(50, 10, 30);
    assert!(state.is_dragging);
    assert_eq!(state.start_line, Some(50));

    state.end();
    assert!(!state.is_dragging);
    assert!(state.start_line.is_none());
}

#[test]
fn drag_state_update() {
    let mut state = MinimapDragState::new();
    state.start(50, 10, 30);

    let target = state.update(60, 30, 100);

    // Target should be somewhere near the dragged line
    assert!(target <= 100);
}

#[test]
fn renderer_cache_invalidation() {
    let mut renderer = MinimapRenderer::new();
    let document = create_test_document();
    let viewport = create_test_viewport();
    let theme = Theme::dark();

    let dims = renderer.calculate_dimensions(&viewport, &document, 20.0);
    renderer.prepare_lines(&document, &dims, &theme, None);

    let line_count = renderer.lines().len();
    assert!(line_count > 0);

    // Invalidate and check
    renderer.invalidate_cache();
    assert!(renderer.lines().is_empty());
}

#[test]
fn minimap_position_left() {
    let config = MinimapConfig {
        position: MinimapPosition::Left,
        ..Default::default()
    };
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    assert!((dims.x - 0.0).abs() < f32::EPSILON);
}

#[test]
fn minimap_position_right() {
    let config = MinimapConfig {
        position: MinimapPosition::Right,
        ..Default::default()
    };
    let viewport = create_test_viewport();
    let document = create_test_document();

    let dims = MinimapDimensions::calculate(&config, &viewport, &document, 20.0);

    let expected_x = viewport.width - config.width;
    assert!((dims.x - expected_x).abs() < f32::EPSILON);
}

#[test]
fn minimap_segment() {
    let segment = MinimapSegment::new(5, 10, Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(segment.start, 5);
    assert_eq!(segment.end, 10);
}

#[test]
fn minimap_rect() {
    let rect = MinimapRect::new(10.0, 20.0, 100.0, 50.0);
    assert!((rect.x - 10.0).abs() < f32::EPSILON);
    assert!((rect.y - 20.0).abs() < f32::EPSILON);
    assert!((rect.width - 100.0).abs() < f32::EPSILON);
    assert!((rect.height - 50.0).abs() < f32::EPSILON);
}

use crate::theme::Color;
