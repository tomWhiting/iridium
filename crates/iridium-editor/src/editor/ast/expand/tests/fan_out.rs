//! A mixed structural fan-out must retain targets that cannot split further.
use super::*;
use crate::{CursorState, Position, Selection};

#[test]
fn spreading_children_preserves_a_primary_leaf_and_its_direction() {
    let source = "[1,[2,3]]";
    let mut editor = editor(Language::Json, source);
    let primary = Selection::new(Position::new(0, 2), Position::new(0, 1));
    editor.set_selections(CursorState::from_selections(
        primary,
        vec![Selection::new(Position::new(0, 3), Position::new(0, 8))],
    ));
    ast(&mut editor, &AST_CURSOR_ON_EVERY_CHILD);
    assert_eq!(editor.state().cursor.primary, primary);
    assert_eq!(selected(&editor, source), ["1", "2", "3"]);
    let selections = editor.state().cursor.clone();
    editor.paste("9");
    assert_eq!(editor.content(), "[9,[9,9]]");
    assert!(editor.undo());
    assert_eq!(editor.content(), source);
    assert_eq!(editor.state().cursor, selections);
}

#[test]
fn spreading_children_preserves_a_secondary_leaf() {
    let source = "[[1,2],3]";
    let mut editor = editor(Language::Json, source);
    editor.set_selections(CursorState::from_selections(
        Selection::new(Position::new(0, 1), Position::new(0, 6)),
        vec![Selection::new(Position::new(0, 7), Position::new(0, 8))],
    ));
    ast(&mut editor, &AST_CURSOR_ON_EVERY_CHILD);
    assert_eq!(selected(&editor, source), ["1", "2", "3"]);
}
