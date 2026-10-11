//! Desktop feedback reads the current kernel search outcome.
use super::*;
use iridium_editor::search::SearchOptions;

#[test]
fn automatic_refresh_failure_is_visible() {
    let (mut overlay, mut editor) = open_over("[");
    type_text(&mut overlay, &mut editor, "[");
    assert!(overlay.error().is_none());
    editor.state_mut().search.options.regex = true;
    editor.set_content("[ changed");
    let rows = rows_text(&overlay, &editor);
    assert!(rows[0].contains("Invalid regex"), "{rows:?}");
    assert!(rows[1].contains("Invalid regex:"), "{rows:?}");
}

#[test]
fn recovery_does_not_keep_showing_an_old_overlay_error() {
    let (mut overlay, mut editor) = open_over("[");
    overlay.handle_key(&alt('r'), &mut editor);
    type_text(&mut overlay, &mut editor, "[");
    assert!(overlay.error().is_some());
    assert!(editor.set_search_options(SearchOptions::default()).is_ok());
    let rows = rows_text(&overlay, &editor);
    assert!(rows[0].contains("1 of 1"), "{rows:?}");
    assert!(!rows[1].contains("Invalid regex"), "{rows:?}");
}
