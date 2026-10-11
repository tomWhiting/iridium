//! Bulk input preserves exact search values and has one search cost per paste.
use super::*;

#[test]
fn multiline_paste_searches_once_and_replacement_is_one_undo_step() {
    let original = "alpha\nbeta\nalpha\nbeta";
    let mut editor = editor_with(original);
    let mut overlay = opened(&mut editor);
    let searches = count_searches(&mut editor);
    overlay.paste("alpha\nbeta", &mut editor);
    assert_eq!(searches.load(Ordering::Relaxed), 1);
    assert_eq!(editor.search_match_count(), 2);
    assert_eq!(editor.content(), original);
    assert!(
        render(&editor, &overlay, 90, 8)
            .find_row()
            .contains("alpha␊beta")
    );
    overlay.paste("", &mut editor);
    assert_eq!(searches.load(Ordering::Relaxed), 1);
    press(&mut overlay, &mut editor, KeyCode::Tab);
    overlay.paste("one\ntwo", &mut editor);
    assert_eq!(searches.load(Ordering::Relaxed), 1);
    assert!(
        render(&editor, &overlay, 90, 8)
            .message_row()
            .contains("one␊two")
    );
    assert_eq!(
        chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt()),
        SearchOutcome::Replaced(2)
    );
    assert_eq!(editor.content(), "one\ntwo\none\ntwo");
    editor.undo();
    assert_eq!(editor.content(), original);
}

#[test]
fn pasted_regex_captures_expand_in_multiline_replacements_and_undo_exactly() {
    let original = "name=猫\nname=犬";
    let mut editor = editor_with(original);
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    overlay.paste("name=([^\\n]+)", &mut editor);
    assert_eq!(editor.search_match_count(), 2);
    press(&mut overlay, &mut editor, KeyCode::Tab);
    overlay.paste("value=$1\nend", &mut editor);
    assert_eq!(
        chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt()),
        SearchOutcome::Replaced(2)
    );
    assert_eq!(editor.content(), "value=猫\nend\nvalue=犬\nend");
    editor.undo();
    assert_eq!(editor.content(), original);
}

#[test]
fn invalid_pasted_regex_reports_error_and_can_be_repaired() {
    let mut editor = editor_with("a");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    overlay.paste("[", &mut editor);
    assert!(overlay.error().is_some());
    assert!(
        render(&editor, &overlay, 90, 8)
            .find_row()
            .contains("Invalid regex")
    );
    overlay.paste("a]", &mut editor);
    assert!(overlay.error().is_none());
    assert_eq!(editor.search_match_count(), 1);
    assert_eq!(editor.content(), "a");
}

#[test]
fn pasted_controls_are_visible_without_becoming_terminal_commands() {
    let raw = "x\r\n\t\u{1b}[31m\u{7}";
    let mut editor = editor_with(raw);
    let mut overlay = opened(&mut editor);
    overlay.paste(raw, &mut editor);
    assert_eq!(overlay.query(), raw);
    assert_eq!(editor.search_match_count(), 1);
    let row = render(&editor, &overlay, 100, 8).find_row();
    assert!(row.contains(r"x␍␊␉\u{1b}[31m\u{7}"), "{row}");
    assert!(!row.chars().any(char::is_control));
}

#[test]
fn long_query_paste_runs_one_search_and_keeps_the_caret_on_screen() {
    let raw = "abc猫\n".repeat(2_000);
    let mut editor = editor_with(&raw);
    let mut overlay = opened(&mut editor);
    let searches = count_searches(&mut editor);
    overlay.paste(&raw, &mut editor);
    assert_eq!(searches.load(Ordering::Relaxed), 1);
    assert_eq!(editor.search_match_count(), 1);
    let screen = render(&editor, &overlay, 60, 8);
    assert!(screen.find_row().contains("猫␊"));
}
