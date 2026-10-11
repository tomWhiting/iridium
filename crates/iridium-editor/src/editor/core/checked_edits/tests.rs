use super::*;
use crate::{EditorConfig, Position, Selection};
fn editor(text: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(text);
    editor
}
fn edit(bytes: std::ops::Range<usize>, expected: &str, replacement: &str) -> CheckedEdit {
    CheckedEdit {
        bytes,
        expected: expected.into(),
        replacement: replacement.into(),
    }
}
#[test]
fn unicode_multiline_edits_preserve_original_selections_and_one_undo_group() {
    let source = "é cat\r\n猫 cat\n";
    let mut editor = editor(source);
    editor.set_selection(Position::new(1, 2), Position::new(1, 5));
    let original = editor.state.cursor.clone();
    let revision = editor.state.document.revision();
    let edits = [edit(12..15, "cat", "犬\nnext"), edit(3..6, "cat", "dog")];
    editor
        .validate_checked_edits(revision, &edits)
        .expect("preflight");
    assert_eq!(editor.state.document.revision(), revision);
    editor.apply_checked_edits(revision, &edits).expect("apply");
    assert_eq!(editor.content(), "é dog\r\n猫 犬\nnext\n");
    assert!(editor.undo());
    assert_eq!(editor.content(), source);
    assert_eq!(editor.state.cursor, original);
    assert!(editor.redo());
    assert_eq!(editor.content(), "é dog\r\n猫 犬\nnext\n");
}
#[test]
fn invalid_later_edits_stale_revisions_and_readonly_leave_state_untouched() {
    let mut editor = editor("é cats");
    editor.set_selection(Position::new(0, 2), Position::new(0, 5));
    let revision = editor.state.document.revision();
    let cursors = editor.state.cursor.clone();
    let history = editor.state.history.current_node_id();
    for edits in [
        vec![edit(3..6, "cat", "dog"), edit(8..9, "x", "y")],
        vec![edit(0..1, "é", "e")],
        vec![edit(3..6, "wrong", "dog")],
        vec![edit(3..6, "cat", "dog"), edit(4..7, "ats", "x")],
        vec![edit(3..3, "", "x"), edit(3..3, "", "y")],
    ] {
        assert!(editor.apply_checked_edits(revision, &edits).is_err());
        assert_eq!(editor.content(), "é cats");
        assert_eq!(editor.state.document.revision(), revision);
        assert_eq!(editor.state.cursor, cursors);
        assert_eq!(editor.state.history.current_node_id(), history);
    }
    assert!(
        editor
            .apply_checked_edits(revision + 1, &[edit(3..6, "cat", "dog")])
            .is_err()
    );
    editor.state.read_only = true;
    assert!(
        editor
            .apply_checked_edits(revision, &[edit(3..6, "cat", "dog")])
            .is_err()
    );
    assert_eq!(
        editor.state.cursor.primary,
        Selection::new(Position::new(0, 2), Position::new(0, 5))
    );
}
#[test]
fn insertion_empty_noop_and_adjacent_edits_have_exact_undo() {
    let mut editor = editor("cat");
    let revision = editor.state.document.revision();
    assert_eq!(
        editor.apply_checked_edits(revision, &[]).expect("empty"),
        revision
    );
    assert_eq!(
        editor
            .apply_checked_edits(revision, &[edit(0..3, "cat", "cat")])
            .expect("noop"),
        revision
    );
    editor
        .apply_checked_edits(
            revision,
            &[
                edit(0..0, "", "["),
                edit(0..1, "c", "C"),
                edit(3..3, "", "]"),
            ],
        )
        .expect("adjacent");
    assert_eq!(editor.content(), "[Cat]");
    assert!(editor.undo());
    assert_eq!(editor.content(), "cat");
}
