//! Selection replacement and reveal preserve state and event contracts.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{CursorState, Editor, EditorEvent, Language, Position, Selection};

#[test]
fn replacement_clamps_merges_and_retains_primary_direction() {
    let mut editor = Editor::with_defaults();
    editor.set_content("abc\ndef");
    let primary = Selection::new(Position::new(0, 2), Position::new(0, 0));
    editor.set_selections(CursorState {
        primary,
        secondary: vec![
            Selection::collapsed(Position::new(99, 99)),
            Selection::collapsed(Position::new(1, 99)),
        ],
    });
    assert_eq!(editor.state().cursor.primary, primary);
    assert_eq!(
        editor.state().cursor.secondary,
        [Selection::collapsed(Position::new(1, 3))]
    );
    assert_eq!(editor.content(), "abc\ndef");
}

#[test]
fn reveal_opens_all_required_folds_once_and_does_not_emit_selection_changes() {
    let mut editor = Editor::with_defaults();
    editor.set_language(Language::Rust);
    editor.set_content("fn a() {\n    if true {\n        call();\n    }\n}\nfn b() {\n    call();\n}\nfn c() {\n    call();\n}");
    editor.fold_all();
    editor.set_selections(CursorState::from_selections(
        Selection::new(Position::new(2, 8), Position::new(2, 12)),
        vec![Selection::collapsed(Position::new(6, 4))],
    ));
    let cursors = editor.state().cursor.clone();
    let revision = editor.state().document.revision();
    let folds = Arc::new(AtomicUsize::new(0));
    let selections = Arc::new(AtomicUsize::new(0));
    let fold_events = Arc::clone(&folds);
    let selection_events = Arc::clone(&selections);
    editor.add_listener(move |event| match event {
        EditorEvent::FoldChanged { .. } => {
            fold_events.fetch_add(1, Ordering::Relaxed);
        },
        EditorEvent::SelectionChanged { .. } => {
            selection_events.fetch_add(1, Ordering::Relaxed);
        },
        _ => {},
    });
    assert!(editor.reveal_selection_heads());
    assert!(!editor.is_line_hidden(2));
    assert!(!editor.is_line_hidden(6));
    assert!(editor.is_line_hidden(9));
    assert!(!editor.reveal_selection_heads());
    assert_eq!(editor.state().cursor, cursors);
    assert_eq!(editor.state().document.revision(), revision);
    assert_eq!(folds.load(Ordering::Relaxed), 1);
    assert_eq!(selections.load(Ordering::Relaxed), 0);
}

#[test]
#[cfg(feature = "syntax")]
fn selected_kind_requires_exact_node_boundaries_and_preserves_direction() {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22]");
    editor.set_language(Language::Json);
    editor.set_selection(Position::new(0, 3), Position::new(0, 1));
    assert_eq!(editor.selected_syntax_kind(), Some("number"));
    editor.set_selection(Position::new(0, 1), Position::new(0, 2));
    assert_eq!(editor.selected_syntax_kind(), None);
    editor.set_selection(Position::new(0, 0), Position::new(0, 7));
    assert_eq!(editor.selected_syntax_kind(), Some("array"));
    editor.set_cursor(Position::new(0, 1));
    assert_eq!(editor.selected_syntax_kind(), None);
}

#[test]
#[cfg(feature = "syntax")]
fn selected_kind_never_parses_or_uses_a_stale_tree() -> Result<(), crate::IridiumError> {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22]");
    editor.set_language(Language::Json);
    editor.set_selection(Position::new(0, 1), Position::new(0, 3));
    let parses = editor.state().syntax.full_parses();
    for _ in 0..20 {
        assert_eq!(editor.selected_syntax_kind(), Some("number"));
    }
    editor
        .state_mut()
        .document
        .insert(Position::new(0, 0), " ")?;
    assert_eq!(editor.selected_syntax_kind(), None);
    assert_eq!(editor.state().syntax.full_parses(), parses);
    Ok(())
}

#[test]
fn a_selection_without_a_parser_has_no_syntax_label() {
    let mut editor = Editor::with_defaults();
    editor.set_content("[11,22]");
    editor.set_selection(Position::new(0, 1), Position::new(0, 3));
    assert_eq!(editor.selected_syntax_kind(), None);
}
