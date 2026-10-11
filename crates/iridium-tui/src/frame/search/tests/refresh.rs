//! Feedback follows the kernel even when another command triggers a refresh.
use super::*;
use iridium_editor::search::SearchOptions;

#[test]
fn automatic_refresh_diagnostic_is_visible_without_an_overlay_query_event() {
    let mut editor = editor_with("[");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "[");
    assert!(overlay.error().is_none());
    editor.state_mut().search.options.regex = true;
    editor.set_content("[ changed");
    let screen = render(&editor, &overlay, 100, 8);
    assert!(
        screen.find_row().contains("Invalid regex"),
        "{}",
        screen.find_row()
    );
    assert!(
        screen.message_row().contains("Invalid regex:"),
        "{}",
        screen.message_row()
    );
}

#[test]
fn recovery_clears_stale_overlay_diagnostic_in_rendered_feedback() {
    let mut editor = editor_with("[");
    let mut overlay = opened(&mut editor);
    overlay.handle_key(&KeyEvent::new(KeyCode::Char('r'), alt()), &mut editor);
    type_text(&mut overlay, &mut editor, "[");
    assert!(overlay.error().is_some());
    assert!(editor.set_search_options(SearchOptions::default()).is_ok());
    let screen = render(&editor, &overlay, 100, 8);
    assert!(
        screen.find_row().contains("1 of 1"),
        "{}",
        screen.find_row()
    );
    assert!(
        !screen.message_row().contains("Invalid regex"),
        "{}",
        screen.message_row()
    );
}
