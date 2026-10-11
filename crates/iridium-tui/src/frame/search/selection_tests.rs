//! The search-to-edit transition, driven through the panel's actual keymap.

use iridium_editor::commands::builtin::SEARCH_SELECT_ALL_MATCHES;
use iridium_editor::{Editor, KeyBinding, KeyCode, KeyEvent, Keymap, Modifiers, Position};

use super::{SearchOutcome, SearchOverlay};

fn key(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

fn alt(key_code: KeyCode) -> KeyEvent {
    key(
        key_code,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    )
}

fn type_query(panel: &mut SearchOverlay, editor: &mut Editor, query: &str) {
    for c in query.chars() {
        assert_eq!(
            panel.handle_key(&key(KeyCode::Char(c), Modifiers::none()), editor),
            SearchOutcome::Handled
        );
    }
}

#[test]
fn alt_enter_hands_regex_matches_to_editing_and_keeps_query_for_reopening() {
    let mut editor = Editor::with_defaults();
    editor.set_content("task_1\ntask_22\ntask_333");
    let mut panel = SearchOverlay::new(&Keymap::new("user"));
    panel.open(&mut editor);
    panel.handle_key(&alt(KeyCode::Char('r')), &mut editor);
    type_query(&mut panel, &mut editor, "task_[0-9]+");
    assert_eq!(
        panel.handle_key(&alt(KeyCode::Enter), &mut editor),
        SearchOutcome::Selected(3)
    );
    assert!(!editor.search_state().is_active);
    assert_eq!(editor.state().cursor.cursor_count(), 3);
    editor.handle_key(&key(KeyCode::Char('X'), Modifiers::none()));
    assert_eq!(editor.content(), "X\nX\nX");
    assert!(editor.undo());
    assert_eq!(editor.state().cursor.cursor_count(), 3);
    assert_eq!(panel.query(), "task_[0-9]+");
    panel.open(&mut editor);
    assert_eq!(editor.search_match_count(), 3);
}

#[test]
fn empty_missing_and_invalid_queries_keep_the_panel_open() {
    for query in ["", "missing", "["] {
        let mut editor = Editor::with_defaults();
        editor.set_content("abc");
        let mut panel = SearchOverlay::new(&Keymap::new("user"));
        panel.open(&mut editor);
        panel.handle_key(&alt(KeyCode::Char('r')), &mut editor);
        type_query(&mut panel, &mut editor, query);
        editor.set_selection(Position::zero(), Position::new(0, 2));
        let before = editor.state().cursor.clone();
        assert_eq!(
            panel.handle_key(&alt(KeyCode::Enter), &mut editor),
            SearchOutcome::Handled
        );
        assert_eq!(editor.state().cursor, before);
        assert_eq!(editor.content(), "abc");
        if query == "[" {
            assert!(panel.error().is_some());
        }
    }
}

#[test]
fn select_matches_is_rebindable_in_the_search_panel() {
    let mut user = Keymap::new("user");
    user.push(KeyBinding::parse("f9", SEARCH_SELECT_ALL_MATCHES).expect("valid binding"));
    let mut panel = SearchOverlay::new(&user);
    let mut editor = Editor::with_defaults();
    editor.set_content("a a");
    panel.open(&mut editor);
    type_query(&mut panel, &mut editor, "a");
    assert_eq!(
        panel.handle_key(&key(KeyCode::F9, Modifiers::none()), &mut editor),
        SearchOutcome::Selected(2)
    );
}

#[test]
fn hint_tracks_rebinding_and_suppression() {
    let mut user = Keymap::new("user");
    user.push(KeyBinding::parse("f9", SEARCH_SELECT_ALL_MATCHES).expect("valid binding"));
    let mut panel = SearchOverlay::new(&user);
    assert!(
        panel
            .selection_hint
            .as_ref()
            .is_some_and(|hint| hint.contains("F9"))
    );
    let (first, rest) = KeyBinding::parse_sequence("alt+enter").expect("valid sequence");
    let mut unbound = Keymap::new("user");
    unbound.push(KeyBinding::unbound(first, &rest));
    panel.set_user_keymap(&unbound);
    assert!(panel.selection_hint.is_none());
}
