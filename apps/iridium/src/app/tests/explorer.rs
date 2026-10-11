//! End-to-end explorer focus and lifecycle through terminal input and frames.
use super::*;
use std::time::{Duration, Instant};

fn settle(app: &mut App) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let screen = frame(app, 100, 30).join("\n");
        if app
            .explorer
            .as_ref()
            .is_none_or(|explorer| !explorer.is_waiting())
        {
            return screen;
        }
        assert!(
            Instant::now() < deadline,
            "explorer never settled: {screen}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn pasted_filter_and_second_match_return_after_toggle_and_successful_open() {
    let dir = TempDir::new("explorer-focus");
    let (mut app, _) = open(&dir, "start.txt", "start");
    for name in ["alpha.rs", "bravo.rs"] {
        fs::write(dir.path().join(name), name).expect("fixture");
    }
    app.handle_input(&ctrl_alt('e'));
    settle(&mut app);
    app.handle_input(&TerminalInput::Paste(".rs".into()));
    settle(&mut app);
    app.handle_input(&press(KeyCode::End));
    app.handle_input(&ctrl_alt('e'));
    assert!(!app.is_explorer_open());
    app.handle_input(&ctrl_alt('e'));
    let screen = settle(&mut app);
    assert!(screen.contains(".rs"));
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_explorer_open());
    assert_eq!(app.editor().expect("editor").content(), "bravo.rs");
    app.handle_input(&ctrl_alt('e'));
    settle(&mut app);
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_explorer_open());
    assert_eq!(
        app.workspace.document_count(),
        2,
        "reopened the existing tab"
    );
    assert_eq!(app.editor().expect("editor").content(), "bravo.rs");
}

#[test]
fn failed_file_open_keeps_the_picker_and_query_then_recovers() {
    let dir = TempDir::new("explorer-failed-open");
    let (mut app, _) = open(&dir, "start.txt", "original");
    let bad = dir.path().join("bad.txt");
    fs::write(&bad, [0xff, 0xfe, 0xff]).expect("fixture");
    app.handle_input(&ctrl_alt('e'));
    settle(&mut app);
    app.handle_input(&TerminalInput::Paste("bad.txt".into()));
    settle(&mut app);
    app.handle_input(&press(KeyCode::Enter));
    assert!(app.is_explorer_open());
    assert!(app.message().is_some());
    assert_eq!(app.workspace.document_count(), 1);
    assert_eq!(app.editor().expect("editor").content(), "original");
    assert!(settle(&mut app).contains("bad.txt"));
    let narrow = frame(&mut app, 80, 24);
    assert!(
        narrow
            .last()
            .expect("status row")
            .contains("not valid UTF-8")
    );
    fs::write(&bad, "recovered").expect("repair fixture");
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_explorer_open());
    assert_eq!(app.editor().expect("editor").content(), "recovered");
}

#[test]
fn reopening_refreshes_contents_and_keeps_sidebar_placement() {
    let dir = TempDir::new("explorer-refresh");
    let (mut app, _) = open(&dir, "old.txt", "original");
    app.handle_input(&ctrl_alt('e'));
    settle(&mut app);
    app.handle_input(&ctrl_alt('b'));
    let width = app.sidebar_columns();
    assert!(width > 0);
    app.handle_input(&ctrl_alt('e'));
    fs::remove_file(dir.path().join("old.txt")).expect("remove");
    fs::write(dir.path().join("fresh.txt"), "new").expect("new");
    app.handle_input(&ctrl_alt('e'));
    let screen = settle(&mut app);
    assert_eq!(app.sidebar_columns(), width);
    assert!(screen.contains("fresh.txt"), "{screen}");
    // The open tab still owns its document after the disk file disappeared.
    assert_eq!(app.editor().expect("editor").content(), "original");
    app.handle_input(&press(KeyCode::End));
    app.handle_input(&press(KeyCode::Enter));
    assert_eq!(app.editor().expect("editor").content(), "new");
}

#[test]
fn oil_paste_is_guarded_and_discard_returns_to_the_retained_query() {
    let dir = TempDir::new("explorer-oil-focus");
    let (mut app, _) = open(&dir, "note.txt", "original");
    app.handle_input(&ctrl_alt('e'));
    settle(&mut app);
    app.handle_input(&TerminalInput::Paste("note.txt".into()));
    settle(&mut app);
    app.handle_input(&press(KeyCode::Tab));
    app.handle_input(&TerminalInput::Paste("_renamed".into()));
    assert!(app.explorer.as_ref().expect("panel").has_unapplied_edits());
    app.handle_input(&ctrl_alt('e'));
    assert!(app.is_explorer_open());
    assert!(app.message().is_some());
    app.handle_input(&press(KeyCode::Escape));
    assert!(app.explorer.as_ref().expect("panel").has_unapplied_edits());
    app.handle_input(&press(KeyCode::Escape));
    assert!(!app.explorer.as_ref().expect("panel").has_unapplied_edits());
    app.handle_input(&ctrl_alt('e'));
    app.handle_input(&ctrl_alt('e'));
    assert!(settle(&mut app).contains("note.txt"));
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.is_explorer_open(), "reopened in browse mode");
    assert_eq!(app.editor().expect("editor").content(), "original");
    assert_eq!(
        fs::read_to_string(dir.path().join("note.txt")).expect("file"),
        "original"
    );
}

#[test]
fn asynchronous_reads_schedule_frames_and_idle_sessions_do_not() {
    let dir = TempDir::new("explorer-wakeup");
    let (mut app, _) = open(&dir, "note.txt", "original");
    assert!(app.next_frame_timeout().is_none());
    app.handle_input(&ctrl_alt('e'));
    assert_eq!(app.next_frame_timeout(), Some(Duration::from_millis(16)));
    settle(&mut app);
    assert!(app.next_frame_timeout().is_none());
    app.handle_input(&ctrl_alt('e'));
    assert!(app.next_frame_timeout().is_none());
    app.handle_input(&ctrl_alt('e'));
    assert!(app.next_frame_timeout().is_some());
    settle(&mut app);
    assert!(app.next_frame_timeout().is_none());
}
