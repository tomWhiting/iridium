//! Held-edge scrolling follows renderer geometry and stops when ownership ends.
use super::mouse_tests::mouse;
use super::{App, Flow};
use crate::cli::Options;
use iridium_editor::{KeyCode, KeyEvent, Position};
use iridium_tui::input::{MouseButton, MouseEventKind, TerminalInput};
use std::time::{Duration, Instant};

fn app(text: &str, width: usize, height: usize) -> App {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(text);
    app.resize(width, height);
    app
}
fn drag(app: &mut App, column: u16, row: u16) {
    mouse(app, MouseEventKind::Drag(MouseButton::Left), column, row);
}
fn tick(app: &mut App, number: u32) {
    app.poll_mouse_scroll(Instant::now() + Duration::from_secs(u64::from(number)));
}

#[test]
fn held_bottom_edge_extends_without_further_mouse_events_and_release_stops_it() {
    let text = "row text\n".repeat(70);
    let mut app = app(&text, 50, 10);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 1);
    let anchor = app.editor().expect("editor").state().cursor.primary.anchor;
    drag(&mut app, 49, 9);
    assert!(app.next_frame_timeout().is_some());
    for n in 1..=4 {
        tick(&mut app, n);
    }
    let editor = app.editor().expect("editor");
    assert_eq!(editor.state().viewport.first_line, 4);
    assert_eq!(editor.state().cursor.primary.anchor, anchor);
    assert_eq!(editor.state().cursor.primary.head.line, 11);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 49, 9);
    let selection = app.editor().expect("editor").state().cursor.clone();
    tick(&mut app, 10);
    assert_eq!(app.editor().expect("editor").state().cursor, selection);
    assert!(app.next_frame_timeout().is_none());
}

#[test]
fn returning_inside_stops_the_timer_and_top_edge_produces_a_reversed_selection() {
    let text = "line\n".repeat(40);
    let mut app = app(&text, 40, 10);
    let editor = app.workspace.active_editor_mut().expect("editor");
    let mut viewport = editor.state().viewport.clone();
    viewport.first_line = 12;
    editor.set_viewport(viewport);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 6, 3);
    drag(&mut app, 6, 0);
    tick(&mut app, 1);
    let selected = app.editor().expect("editor").state().cursor.primary;
    assert_eq!(selected.anchor.line, 15);
    assert_eq!(selected.head.line, 11);
    assert!(selected.head < selected.anchor);
    drag(&mut app, 6, 3);
    assert!(app.next_frame_timeout().is_none());
    tick(&mut app, 2);
    assert_eq!(
        app.editor().expect("editor").state().viewport.first_line,
        11
    );
}

#[test]
fn horizontal_edge_reaches_grapheme_safe_end_and_stops_at_document_boundary() {
    let text = format!("\t{}", "漢e\u{301}".repeat(20));
    let mut app = app(&text, 20, 8);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, 0);
    drag(&mut app, 19, 0);
    for n in 1..=50 {
        tick(&mut app, n);
    }
    let editor = app.editor().expect("editor");
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(0, text.chars().count())
    );
    assert!(editor.state().viewport.scroll_offset_x > 0.0);
    assert!(app.next_frame_timeout().is_none());
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(KeyCode::Char('y'))));
    assert_eq!(app.clipboard, text);
}

#[test]
fn folding_does_not_change_the_source_selected_while_scrolling() {
    let text = format!("[\n  [1,2],\n  [3,4]\n]\n{}", "tail\n".repeat(30));
    let mut app = app(&text, 40, 8);
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_language(iridium_editor::Language::Json);
    editor.fold_all();
    let column = u16::try_from(app.mouse_layout().expect("layout").text.origin).expect("column");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), column, 0);
    drag(&mut app, 39, 7);
    tick(&mut app, 1);
    let editor = app.editor().expect("editor");
    assert_eq!(editor.state().viewport.first_line, 1);
    assert_eq!(editor.state().cursor.primary.head.line, 9);
    assert_eq!(editor.state().cursor.primary.anchor, Position::new(0, 0));
}

#[test]
fn blur_keys_revision_changes_and_tiny_resizes_end_or_suspend_edge_scroll_safely() {
    for cancel in [
        TerminalInput::FocusLost,
        TerminalInput::Key(KeyEvent::simple(KeyCode::Char(':'))),
    ] {
        let mut app = app(&"line\n".repeat(40), 40, 8);
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 1);
        drag(&mut app, 39, 7);
        assert_eq!(app.handle_input(&cancel), Flow::Running);
        tick(&mut app, 1);
        assert_eq!(app.editor().expect("editor").state().viewport.first_line, 0);
        assert!(app.mouse_scroll.is_none());
    }
    let mut app = app(&"line\n".repeat(40), 40, 8);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 5, 1);
    drag(&mut app, 39, 7);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("replaced");
    tick(&mut app, 1);
    assert!(app.mouse_anchor.is_none());
    let column = u16::try_from(app.mouse_layout().expect("layout").text.origin).expect("column");
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), column, 0);
    drag(&mut app, 39, 7);
    app.resize(0, 0);
    tick(&mut app, 2);
    assert!(app.mouse_scroll.is_none());
}

#[test]
#[ignore = "manual edge-scroll timing; writes a benchmark artifact"]
fn measure_held_edge_on_a_long_unicode_line() {
    let text = "漢e\u{301}".repeat(25_000);
    let mut app = app(&text, 100, 30);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, 0);
    drag(&mut app, 99, 0);
    let mut micros = Vec::new();
    for n in 1..=35 {
        let start = Instant::now();
        tick(&mut app, n);
        if n > 4 {
            micros.push(start.elapsed().as_micros());
        }
    }
    micros.sort_unstable();
    let report = serde_json::json!({ "bytes": text.len(), "grapheme_clusters": 50_000, "samples": micros.len(), "median_microseconds": micros[micros.len()/2], "max_microseconds": micros.last(), "profile": "test opt-level=1", "tick_interval_ms": 50 });
    std::fs::write(
        "/private/tmp/iridium-drag-benchmark.json",
        serde_json::to_string_pretty(&report).expect("json"),
    )
    .expect("benchmark artifact");
}
