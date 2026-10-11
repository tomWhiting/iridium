use super::*;
use crate::cli::Options;
use iridium_editor::{Language, Position};
use iridium_tui::input::TerminalInput;
fn app() -> App {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content("[11,22,33]");
    editor.set_language(Language::Json);
    editor.set_selection(Position::new(0, 1), Position::new(0, 3));
    app.resize(80, 12);
    app
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::simple(code)));
}
#[test]
fn preview_can_cancel_or_accept_multicursor_change_with_exact_undo() {
    let mut app = app();
    let original = app.editor().expect("editor").state().cursor.clone();
    app.open_similar();
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        3
    );
    app.handle_input(&TerminalInput::Paste("bad".into()));
    key(&mut app, KeyCode::Char('x'));
    assert_eq!(app.editor().expect("editor").content(), "[11,22,33]");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Escape);
    assert_eq!(app.editor().expect("editor").state().cursor, original);
    app.open_similar();
    key(&mut app, KeyCode::F2);
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('c'));
    app.handle_input(&TerminalInput::Paste("9".into()));
    key(&mut app, KeyCode::Escape);
    assert_eq!(app.editor().expect("editor").content(), "[9,9,9]");
    key(&mut app, KeyCode::Char('u'));
    assert_eq!(app.editor().expect("editor").content(), "[11,22,33]");
}
#[test]
fn changed_revision_cannot_accept_old_preview() {
    let mut app = app();
    app.open_similar();
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("external");
    key(&mut app, KeyCode::Enter);
    assert!(app.similar.is_none());
    assert_eq!(app.editor().expect("editor").content(), "external");
    assert!(
        app.message
            .as_ref()
            .expect("message")
            .text()
            .contains("document changed")
    );
}

#[test]
#[ignore = "manual structural selector performance measurement"]
fn measured_five_thousand_similar_nodes() {
    use std::{io::Write as _, time::Instant};
    let mut app = app();
    let source = format!("[{}]", vec!["123"; 5_000].join(","));
    let editor = app.workspace.active_editor_mut().expect("editor");
    editor.set_content(&source);
    editor.set_selection(Position::new(0, 1), Position::new(0, 4));
    editor
        .similar_syntax_selections(SyntaxSimilarity::Kind)
        .expect("warm parser");
    let mut samples = Vec::new();
    for _ in 0..51 {
        let start = Instant::now();
        let matches = editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect("matches");
        samples.push(start.elapsed().as_micros());
        assert_eq!(matches.cursor_count(), 5_000);
    }
    samples.sort_unstable();
    writeln!(
        std::io::stdout().lock(),
        "SIMILAR nodes=5000 bytes={} median_us={} max_us={}",
        source.len(),
        samples[25],
        samples[50]
    )
    .expect("report");
}
