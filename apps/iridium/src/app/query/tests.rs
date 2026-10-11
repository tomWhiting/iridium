//! Selection ownership and input routing around the asynchronous worker.
use super::*;
use crate::cli::Options;
use iridium_editor::{KeyCode, Modifiers, Position};

fn app() -> App {
    App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app")
}
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

#[test]
fn escape_restores_all_cursors_after_live_preview_without_changing_text() {
    let mut app = app();
    let text = r#"{"a":"same","b":"same"}"#;
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content(text);
    editor.set_cursor(Position::new(0, 2));
    let original = editor.state().cursor.clone();
    app.open_query(".b");
    let query = app.query.as_ref().expect("query");
    let ranges = iridium_query::select(text, &parse(".b", Format::Auto)).expect("query");
    preview(
        app.workspace.active_editor_mut().expect("editor"),
        query,
        ranges,
    )
    .expect("preview");
    assert_ne!(app.editor().expect("editor").state().cursor, original);
    app.answer_query(&press(KeyCode::Escape));
    assert_eq!(app.editor().expect("editor").state().cursor, original);
    assert_eq!(app.editor().expect("editor").content(), text);
    assert!(app.query.is_none());
}

#[test]
fn accepting_completed_preview_retains_selection_and_retyping_cancels_pending_accept() {
    let mut app = app();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(r#"{"a":1}"#);
    app.open_query(".a");
    app.answer_query(&press(KeyCode::Enter));
    assert!(app.query.as_ref().expect("query").accept);
    app.answer_query(&press(KeyCode::Char(' ')));
    assert!(!app.query.as_ref().expect("query").accept);
    let query = app.query.as_mut().expect("query");
    query.due = None;
    query.count = Some(1);
    app.answer_query(&press(KeyCode::Enter));
    assert!(app.query.is_none());
    assert!(app.prompt.is_none());
}

#[test]
fn stale_document_cannot_receive_results_or_restored_cursors() {
    let mut app = app();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(r#"{"a":1}"#);
    app.open_query(".a");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("changed");
    app.poll_query();
    assert!(app.query.is_none());
    assert_eq!(app.editor().expect("editor").content(), "changed");
    assert!(
        app.message()
            .expect("report")
            .text()
            .contains("document changed")
    );
}

#[test]
fn query_options_and_prefixes_preserve_filter_expression() {
    let query = parse(
        "--keys --jsonl select(.kind == \"ok\") | .name",
        Format::Auto,
    );
    assert_eq!(query.target, Target::Keys);
    assert_eq!(query.format, Format::JsonLines);
    assert_eq!(query.filter, "select(.kind == \"ok\") | .name");
    assert_eq!(command_filter("jq .a"), Some(".a"));
    assert_eq!(command_filter("jq"), Some(""));
    assert_eq!(command_filter("jqSomething"), None);
}
