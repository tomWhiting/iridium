use super::structural_keymap::{STRUCTURAL, STRUCTURAL_INSERT, structural_keymap};
use crate::{Editor, KeyCode, KeyEvent, Modifiers, Position};

fn editor() -> Editor {
    let mut editor = Editor::with_defaults();
    editor
        .push_keymap(structural_keymap())
        .expect("valid structural preset");
    editor
}

fn press(editor: &mut Editor, key: KeyCode) {
    editor.handle_key(&KeyEvent::simple(key));
}

fn enter(editor: &mut Editor) {
    editor.handle_key(&KeyEvent::new(KeyCode::Char(' '), Modifiers::ctrl()));
    assert_eq!(editor.mode(), Some(&STRUCTURAL));
}

#[cfg(feature = "syntax")]
fn json() -> Editor {
    let mut editor = editor();
    editor.set_content("[[11,22],33]");
    editor.set_language(crate::Language::Json);
    editor.set_cursor(Position::new(0, 2));
    editor
}

#[cfg(feature = "syntax")]
fn selected(editor: &Editor) -> Vec<String> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|selection| editor.state().document.slice(selection.range()))
        .collect()
}

#[cfg(feature = "syntax")]
#[test]
fn enter_arrows_retrace_the_selected_child_and_return_to_typing() {
    let mut editor = json();
    assert_eq!(editor.mode(), None);
    enter(&mut editor);
    assert_eq!(selected(&editor), ["11"]);
    press(&mut editor, KeyCode::Down);
    assert_eq!(selected(&editor), ["22"]);
    press(&mut editor, KeyCode::Left);
    assert_eq!(selected(&editor), ["[11,22]"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["22"]);
    press(&mut editor, KeyCode::Up);
    assert_eq!(selected(&editor), ["11"]);
    press(&mut editor, KeyCode::Char('q'));
    assert_eq!(
        editor.content(),
        "[[11,22],33]",
        "unbound letters do not type"
    );
    press(&mut editor, KeyCode::Char('i'));
    assert_eq!(editor.mode(), Some(&STRUCTURAL_INSERT));
    press(&mut editor, KeyCode::Char('9'));
    assert_eq!(editor.content(), "[[9,22],33]");
    assert!(editor.undo());
    assert_eq!(editor.content(), "[[11,22],33]");
}

#[cfg(feature = "syntax")]
#[test]
fn child_descent_does_not_restore_the_collapsed_entry_cursor() {
    let mut editor = json();
    editor.set_cursor(Position::zero());
    enter(&mut editor);
    assert_eq!(selected(&editor), ["[[11,22],33]"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["[11,22]"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["11"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["11"], "a leaf stays selected");
}

#[cfg(feature = "syntax")]
#[test]
fn child_descent_does_not_restore_a_partial_node_from_mode_entry() {
    let mut editor = json();
    editor.set_selection(Position::new(0, 2), Position::new(0, 3));
    enter(&mut editor);
    assert_eq!(selected(&editor), ["11"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["11"], "a leaf remains a whole node");
}

#[cfg(feature = "syntax")]
#[test]
fn parent_merge_and_child_retrace_restore_all_targets_for_one_edit() {
    let mut editor = json();
    enter(&mut editor);
    press(&mut editor, KeyCode::Left);
    press(&mut editor, KeyCode::Char('a'));
    assert_eq!(selected(&editor), ["11", "22"]);
    press(&mut editor, KeyCode::Left);
    assert_eq!(selected(&editor), ["[11,22]"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["11", "22"]);
    press(&mut editor, KeyCode::Escape);
    assert_eq!(editor.mode(), Some(&STRUCTURAL_INSERT));
    press(&mut editor, KeyCode::Char('9'));
    assert_eq!(editor.content(), "[[9,9],33]");
    assert!(editor.undo());
    assert_eq!(editor.content(), "[[11,22],33]");
    assert_eq!(selected(&editor), ["11", "22"]);
}

#[test]
fn a_document_without_a_parser_keeps_text_and_can_leave_the_mode() {
    let mut editor = editor();
    editor.set_content("plain text");
    editor.set_cursor(Position::zero());
    enter(&mut editor);
    for key in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char('q'),
    ] {
        press(&mut editor, key);
    }
    assert_eq!(editor.content(), "plain text");
    assert_eq!(editor.cursor(), Position::zero());
    press(&mut editor, KeyCode::Escape);
    assert_eq!(editor.mode(), Some(&STRUCTURAL_INSERT));
    press(&mut editor, KeyCode::Char('X'));
    assert_eq!(editor.content(), "Xplain text");
}

#[cfg(feature = "syntax")]
#[test]
fn counts_cancel_boundaries_and_typing_share_the_structural_key_path() {
    let mut editor = json();
    editor.set_content("[1,2,3,4]");
    editor.set_cursor(Position::new(0, 1));
    enter(&mut editor);
    press(&mut editor, KeyCode::Char('2'));
    press(&mut editor, KeyCode::Down);
    assert_eq!(selected(&editor), ["3"]);
    press(&mut editor, KeyCode::Char('2'));
    press(&mut editor, KeyCode::Escape);
    assert_eq!(
        editor.mode(),
        Some(&STRUCTURAL),
        "cancel only the pending count"
    );
    press(&mut editor, KeyCode::Up);
    assert_eq!(selected(&editor), ["2"]);
    assert!(
        editor.perform_ast_request_repeated(crate::input::AstRequest::SelectNextSibling, u32::MAX)
    );
    assert_eq!(selected(&editor), ["4"]);
    assert!(
        !editor.perform_ast_request_repeated(crate::input::AstRequest::SelectNextSibling, u32::MAX)
    );
    assert!(
        !editor.perform_ast_request_repeated(crate::input::AstRequest::SelectPreviousSibling, 0)
    );
    press(&mut editor, KeyCode::Char('i'));
    press(&mut editor, KeyCode::Char('9'));
    assert_eq!(editor.content(), "[1,2,3,9]");
    assert!(editor.undo());
    assert_eq!(editor.content(), "[1,2,3,4]");
    assert_eq!(selected(&editor), ["4"]);
}

#[cfg(feature = "syntax")]
#[test]
fn ordinary_shrink_keeps_exact_partial_selection_history() {
    let mut editor = json();
    editor.set_selection(Position::new(0, 3), Position::new(0, 2));
    enter(&mut editor);
    assert_eq!(selected(&editor), ["11"]);
    assert!(editor.perform_ast_request(crate::input::AstRequest::ShrinkSelection));
    assert_eq!(selected(&editor), ["1"]);
    assert!(editor.state().cursor.primary.is_backward());
}

#[cfg(feature = "syntax")]
#[test]
fn counted_navigation_publishes_only_the_final_selection() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut editor = json();
    editor.set_content("[1,2,3,4]");
    editor.set_cursor(Position::new(0, 1));
    enter(&mut editor);
    let count = Arc::new(AtomicUsize::new(0));
    let listener = Arc::clone(&count);
    editor.add_listener(move |event| {
        if matches!(event, crate::editor::EditorEvent::SelectionChanged { .. }) {
            listener.fetch_add(1, Ordering::Relaxed);
        }
    });
    assert!(editor.perform_ast_request_repeated(crate::input::AstRequest::SelectNextSibling, 3));
    assert_eq!(selected(&editor), ["4"]);
    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert!(!editor.perform_ast_request_repeated(crate::input::AstRequest::SelectNextSibling, 3));
    assert_eq!(count.load(Ordering::Relaxed), 1);
}

#[cfg(feature = "syntax")]
#[test]
fn descent_restores_an_extent_built_by_structural_sibling_extension() {
    let mut editor = json();
    enter(&mut editor);
    editor.handle_key(&KeyEvent::new(KeyCode::Down, Modifiers::shift()));
    assert_eq!(selected(&editor), ["11,22"]);
    press(&mut editor, KeyCode::Left);
    assert_eq!(selected(&editor), ["[11,22]"]);
    press(&mut editor, KeyCode::Right);
    assert_eq!(selected(&editor), ["11,22"]);
}
