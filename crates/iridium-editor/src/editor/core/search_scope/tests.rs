use crate::{Command, CursorState, Editor, Position, Range, Selection, search::SearchOptions};

fn editor(text: &str, start: usize, end: usize) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_selection(Position::new(0, end), Position::new(0, start));
    editor.search_in_selections().expect("capture scope");
    editor
}

#[test]
fn disjoint_reversed_scopes_filter_preview_navigation_and_select_all() {
    let mut editor = editor("foo foo foo foo", 0, 3);
    editor.set_selections(CursorState::from_selections(
        Selection::new(Position::new(0, 3), Position::new(0, 0)),
        vec![Selection::new(Position::new(0, 8), Position::new(0, 11))],
    ));
    editor
        .search_in_selections()
        .expect("capture disjoint scopes");
    editor.find("foo", &SearchOptions::default()).expect("find");
    assert_eq!(editor.search_match_count(), 2);
    editor.goto_next_match();
    assert_eq!(editor.cursor(), Position::new(0, 8));
    editor.goto_next_match();
    assert_eq!(editor.cursor(), Position::new(0, 0));
    assert_eq!(editor.select_search_matches().expect("select"), 2);
    editor.paste("x");
    assert_eq!(editor.content(), "x foo x foo");
    assert!(editor.undo());
    assert_eq!(editor.content(), "foo foo foo foo");
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn scoped_regex_keeps_document_context_and_empty_endpoints() {
    let mut editor = editor("before foo after", 7, 10);
    editor
        .find(r"\bfoo\b", &SearchOptions::regex_mode())
        .expect("find");
    assert_eq!(editor.search_match_count(), 1);
    editor
        .find("^foo$", &SearchOptions::regex_mode())
        .expect("find");
    assert_eq!(
        editor.search_match_count(),
        0,
        "scope must not become the regex subject"
    );
    editor
        .find("", &SearchOptions::regex_mode())
        .expect("empty query");
    editor
        .find("(?:)", &SearchOptions::regex_mode())
        .expect("empty matches");
    assert_eq!(editor.search_match_count(), 4);
}

#[test]
fn replacements_resize_scope_and_undo_redo_restore_deleted_regions() {
    let mut editor = editor("left foo foo right foo", 5, 12);
    editor.find("foo", &SearchOptions::default()).expect("find");
    assert_eq!(editor.replace_all_matches("x"), 2);
    assert_eq!(editor.content(), "left x x right foo");
    assert_eq!(editor.search_state().scope_range_count(), Some(1));
    editor.find("x", &SearchOptions::default()).expect("find x");
    assert_eq!(editor.search_match_count(), 2);
    assert!(editor.undo());
    editor
        .find("foo", &SearchOptions::default())
        .expect("find restored");
    assert_eq!(editor.search_match_count(), 2);
    assert!(editor.redo());
    editor
        .find("x", &SearchOptions::default())
        .expect("find redo");
    assert_eq!(editor.search_match_count(), 2);
    editor.state_mut().history.finish_group();
    editor.apply_command(Command::Delete {
        range: Range::new(Position::new(0, 5), Position::new(0, 8)),
        deleted_text: "x x".to_owned(),
    });
    assert_eq!(editor.search_match_count(), 0);
    assert!(editor.undo());
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn prefix_edits_and_crlf_keep_byte_anchors_correct() {
    let mut editor = editor("foo foo", 4, 7);
    editor.find("foo", &SearchOptions::default()).expect("find");
    editor.apply_command(Command::Insert {
        position: Position::zero(),
        text: "é\r\n".to_owned(),
    });
    assert_eq!(
        editor.search_state().matches,
        vec![Range::new(Position::new(1, 4), Position::new(1, 7))]
    );
    assert!(editor.undo());
    assert_eq!(
        editor.search_state().matches,
        vec![Range::new(Position::new(0, 4), Position::new(0, 7))]
    );
}

#[test]
fn out_of_band_changes_fail_closed_and_clear_scope_is_explicit() {
    let mut editor = editor("foo foo", 0, 3);
    editor.find("foo", &SearchOptions::default()).expect("find");
    editor
        .state_mut()
        .document
        .insert(Position::zero(), "foo ")
        .expect("external edit");
    assert_eq!(editor.replace_all_matches("x"), 0);
    assert_eq!(editor.content(), "foo foo foo");
    assert!(editor.search_state().error().is_some());
    assert_eq!(editor.search_state().scope_range_count(), Some(1));
    editor.clear_search_scope();
    assert_eq!(editor.search_match_count(), 3);
    assert!(editor.search_state().error().is_none());
}

#[test]
fn failed_compound_leaves_scope_and_history_unchanged() {
    let mut editor = editor("foo foo", 4, 7);
    editor.find("foo", &SearchOptions::default()).expect("find");
    editor.apply_command(Command::Compound {
        commands: vec![
            Command::Insert {
                position: Position::zero(),
                text: "x".to_owned(),
            },
            Command::Delete {
                range: Range::new(Position::new(99, 0), Position::new(99, 1)),
                deleted_text: "?".to_owned(),
            },
        ],
    });
    assert_eq!(editor.content(), "foo foo");
    assert_eq!(
        editor.select_search_matches().expect("scope still valid"),
        1
    );
    assert_eq!(
        editor.state().cursor.primary.range(),
        Range::new(Position::new(0, 4), Position::new(0, 7))
    );
}

#[test]
fn empty_selection_cannot_start_a_scope_or_change_an_existing_scope() {
    let mut editor = editor("foo foo", 4, 7);
    editor.set_cursor(Position::zero());
    assert!(editor.search_in_selections().is_err());
    editor
        .find("foo", &SearchOptions::default())
        .expect("existing scope");
    assert_eq!(editor.search_match_count(), 1);
}

#[test]
fn jumping_between_branches_restores_their_scope_extents() {
    let mut editor = editor("foo foo", 0, 3);
    editor.find("foo", &SearchOptions::default()).expect("find");
    assert!(editor.replace_current_match("x"));
    let short = editor.state().history.current_node_id();
    assert!(editor.undo());
    assert!(editor.replace_current_match("longer"));
    let long = editor.state().history.current_node_id();
    assert!(editor.jump_to_history_node(short));
    editor
        .find("x", &SearchOptions::default())
        .expect("short branch");
    assert_eq!(editor.search_match_count(), 1);
    assert!(editor.jump_to_history_node(long));
    editor
        .find("longer", &SearchOptions::default())
        .expect("long branch");
    assert_eq!(editor.search_match_count(), 1);
    editor
        .find("foo", &SearchOptions::default())
        .expect("outside scope");
    assert_eq!(editor.search_match_count(), 0);
}

#[test]
fn scoped_key_dispatch_opens_search_and_reports_scope_in_events() {
    use crate::input::SearchAction;
    use crate::{EditorEvent, EditorKeyResult, KeyCode, KeyEvent, Modifiers};
    use std::sync::{Arc, Mutex};
    let mut editor = Editor::with_defaults();
    editor.set_content("foo foo");
    editor.set_selection(Position::zero(), Position::new(0, 3));
    let scopes = Arc::new(Mutex::new(Vec::new()));
    let events = scopes.clone();
    editor.add_listener(move |event| {
        if let EditorEvent::SearchUpdated { scope_ranges, .. } = event {
            events.lock().expect("events").push(*scope_ranges);
        }
    });
    let key = KeyEvent::new(
        KeyCode::Char('l'),
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        },
    );
    assert!(matches!(
        editor.handle_key(&key),
        EditorKeyResult::Search(SearchAction::OpenSearch)
    ));
    assert_eq!(scopes.lock().expect("events").last(), Some(&Some(1)));
    editor.clear_search_scope();
    assert_eq!(scopes.lock().expect("events").last(), Some(&None));
}

#[test]
fn sequential_host_compounds_track_new_positions_created_by_earlier_children() {
    let mut editor = editor("foo foo", 4, 7);
    editor.find("foo", &SearchOptions::default()).expect("find");
    editor.apply_command(Command::Compound {
        commands: vec![
            Command::Insert {
                position: Position::zero(),
                text: "\r".to_owned(),
            },
            Command::Insert {
                position: Position::new(1, 0),
                text: "é".to_owned(),
            },
        ],
    });
    assert_eq!(
        editor.search_state().matches,
        vec![Range::new(Position::new(1, 5), Position::new(1, 8))]
    );
    assert!(editor.undo());
    assert_eq!(
        editor.search_state().matches,
        vec![Range::new(Position::new(0, 4), Position::new(0, 7))]
    );
}

#[test]
fn quick_replacement_actions_have_separate_undo_steps() {
    let mut editor = editor("foo foo foo", 0, 7);
    editor.find("foo", &SearchOptions::default()).expect("find");
    assert!(editor.replace_current_match("x"));
    assert!(editor.replace_current_match("y"));
    assert_eq!(editor.content(), "x y foo");
    assert!(editor.undo());
    assert_eq!(editor.content(), "x foo foo");
    assert_eq!(editor.search_match_count(), 1);
    assert!(editor.undo());
    assert_eq!(editor.content(), "foo foo foo");
    assert_eq!(editor.search_match_count(), 2);
}
