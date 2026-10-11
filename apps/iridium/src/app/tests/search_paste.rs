//! Pasting into search must never insert into the document underneath it.
use super::*;
use crate::app::modal_tests::legacy_options;

#[test]
fn field_copy_cut_and_paste_use_host_clipboard_without_editing_document() {
    let mut app = unnamed();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("cat dog cat");
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("cat".into()));
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('c'));
    assert_eq!(app.clipboard, "cat");
    app.handle_input(&press(KeyCode::Tab));
    app.handle_input(&ctrl('v'));
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .replacement(),
        ""
    );
    assert_eq!(app.clipboard, "cat");
    app.handle_input(&ctrl('v'));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .replacement(),
        "cat"
    );
    assert_eq!(app.editor().expect("editor").content(), "cat dog cat");
    app.handle_input(&press(KeyCode::Tab));
    app.handle_input(&TerminalInput::Paste("dog".into()));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        "dog"
    );
    assert_eq!(app.editor().expect("editor").search_match_count(), 1);
}

#[test]
fn pasted_find_and_replacement_text_belong_to_the_focused_fields() {
    let mut app = unnamed();
    let original = "alpha\nbeta\nalpha\nbeta";
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(original);
    app.resize(80, 24);
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("alpha\nbeta".into()));
    assert_eq!(app.editor().expect("editor").content(), original);
    assert_eq!(app.editor().expect("editor").search_match_count(), 2);
    app.handle_input(&press(KeyCode::Tab));
    app.handle_input(&TerminalInput::Paste("one\ntwo".into()));
    assert_eq!(app.editor().expect("editor").content(), original);
    let session = app.workspace.active_payload().expect("session");
    assert_eq!(session.search.query(), "alpha\nbeta");
    assert_eq!(session.search.replacement(), "one\ntwo");
}

#[test]
fn clipboard_paste_targets_search_even_when_the_document_is_read_only() {
    let mut app = App::with_config(
        Options {
            read_only: true,
            ..legacy_options()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("session");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("needle");
    app.resize(90, 12);
    app.clipboard = "needle".into();
    app.handle_input(&ctrl('f'));
    app.handle_input(&ctrl('v'));
    assert_eq!(app.editor().expect("editor").search_match_count(), 1);
    app.handle_input(&press(KeyCode::Tab));
    app.clipboard = "replacement\nline".into();
    app.handle_input(&ctrl('v'));
    let session = app.workspace.active_payload().expect("session");
    assert_eq!(session.search.query(), "needle");
    assert_eq!(session.search.replacement(), "replacement\nline");
    app.handle_input(&ctrl_alt('r'));
    assert_eq!(app.editor().expect("editor").content(), "needle");
    assert!(
        frame(&mut app, 90, 12)
            .iter()
            .any(|row| row.contains("read-only"))
    );
    assert_eq!(app.clipboard, "replacement\nline");
}

#[test]
fn custom_clipboard_binding_pastes_into_search_and_returns_to_document_on_close() {
    let mut app = with_bindings(vec![
        iridium_editor::KeyBinding::parse("f2", iridium_editor::commands::builtin::CLIPBOARD_PASTE)
            .expect("binding"),
    ]);
    app.clipboard = "cat".into();
    app.handle_input(&ctrl('f'));
    app.handle_input(&press(KeyCode::F2));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        "cat"
    );
    assert_eq!(app.editor().expect("editor").content(), "");
    app.handle_input(&press(KeyCode::Escape));
    app.handle_input(&press(KeyCode::F2));
    assert_eq!(app.editor().expect("editor").content(), "cat");
}
