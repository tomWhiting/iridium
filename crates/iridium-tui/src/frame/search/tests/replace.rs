//! Replacing a match, and moving between the find and replace fields.
//!
//! Split out of a 1,136-line `tests.rs` for #92, on the rules the file
//! already carried. The fixtures and the [`Screen`](super::Screen) reader are
//! in [`super`].

use super::*;

// ==================== Replace ====================

#[test]
fn replacing_the_current_match_edits_the_document_and_reports_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    let outcome = chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('r'),
        Modifiers::ctrl(),
    );
    assert_eq!(outcome, SearchOutcome::Replaced(1));
    assert_eq!(editor.state().document.text(), "baz bar foo");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("Replaced 1 match"), "{row}");
}

#[test]
fn replacing_every_match_is_one_step_and_reports_the_count() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    let outcome = chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());
    assert_eq!(outcome, SearchOutcome::Replaced(2));
    assert_eq!(editor.state().document.text(), "baz bar baz");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("Replaced 2 matches"), "{row}");

    editor.undo();
    assert_eq!(
        editor.state().document.text(),
        "foo bar foo",
        "replace-all must be a single undoable step"
    );
}

#[test]
fn replacing_everything_leaves_the_panel_searching_for_what_it_shows() {
    // `replace_all_matches` closes the kernel's search. A panel still showing a
    // query while nothing is searching for it is a lie, and the highlighting
    // would vanish without a word.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());

    assert!(
        editor.is_searching(),
        "the query is still on screen, so it must still be running"
    );
    assert_eq!(editor.search_state().query, "foo");
    assert_eq!(editor.search_match_count(), 0, "they are all gone now");
}

#[test]
fn replacing_a_match_with_nothing_deletes_it() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo ");
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), ctrl_alt());
    assert_eq!(editor.state().document.text(), "bar foo");
}

#[test]
fn a_replacement_with_nothing_to_replace_says_so() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "zzz");

    let outcome = chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('r'),
        Modifiers::ctrl(),
    );
    assert_eq!(outcome, SearchOutcome::Handled);
    assert_eq!(editor.state().document.text(), "foo bar foo");

    let row = render(&editor, &overlay, 60, 8).message_row();
    assert!(row.contains("No match to replace"), "{row}");
}

#[test]
fn a_read_only_document_refuses_a_replacement_and_says_why() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    editor.state_mut().read_only = true;

    for modifiers in [Modifiers::ctrl(), ctrl_alt()] {
        let outcome = chord(&mut overlay, &mut editor, KeyCode::Char('r'), modifiers);
        assert_eq!(outcome, SearchOutcome::Handled);
        assert_eq!(editor.state().document.text(), "foo bar foo");
        let row = render(&editor, &overlay, 60, 8).message_row();
        assert!(row.contains("Document is read-only"), "{row}");
    }
}

// ==================== The two fields ====================

#[test]
fn tab_moves_between_the_query_and_the_replacement() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "baz");

    assert_eq!(overlay.query(), "foo");
    assert_eq!(overlay.replacement(), "baz");

    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "t");
    assert_eq!(overlay.query(), "foot", "tab must come back to the query");
}

#[test]
fn typing_a_replacement_does_not_disturb_the_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let before = editor.current_match_index();
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "zzzz");

    assert_eq!(editor.search_match_count(), 2);
    assert_eq!(editor.current_match_index(), before);
    assert_eq!(editor.search_state().query, "foo");
}

#[test]
fn backspacing_the_query_re_runs_the_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo b");
    assert_eq!(editor.search_match_count(), 1);

    press(&mut overlay, &mut editor, KeyCode::Backspace);
    press(&mut overlay, &mut editor, KeyCode::Backspace);
    assert_eq!(overlay.query(), "foo");
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn a_caret_motion_in_the_query_costs_no_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Enter);
    assert_eq!(editor.current_match_index(), Some(1));

    let searches = count_searches(&mut editor);
    for code in [KeyCode::Left, KeyCode::Right, KeyCode::Home, KeyCode::End] {
        press(&mut overlay, &mut editor, code);
        assert_eq!(
            searches.load(Ordering::Relaxed),
            0,
            "{code:?} must not restart the search"
        );
        assert_eq!(editor.current_match_index(), Some(1));
    }
}

#[test]
fn typing_a_replacement_costs_no_search() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    press(&mut overlay, &mut editor, KeyCode::Tab);

    let searches = count_searches(&mut editor);
    type_text(&mut overlay, &mut editor, "zzzz");
    assert_eq!(
        searches.load(Ordering::Relaxed),
        0,
        "the replacement text is not a query and must cost no pass over the document"
    );
}

#[test]
fn typing_a_query_costs_exactly_one_search_per_keystroke() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    let searches = count_searches(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(searches.load(Ordering::Relaxed), 3);
}

#[test]
fn editing_inside_the_query_re_runs_it_from_the_middle() {
    let mut editor = editor_with("fXoo food");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert_eq!(editor.search_match_count(), 1);

    press(&mut overlay, &mut editor, KeyCode::Home);
    press(&mut overlay, &mut editor, KeyCode::Right);
    type_text(&mut overlay, &mut editor, "X");
    assert_eq!(overlay.query(), "fXoo");
    assert_eq!(editor.search_match_count(), 1);
    assert_eq!(editor.cursor(), Position::new(0, 0));
}
