//! View behavior in the terminal host.
use super::*;

#[test]
fn the_statusline_names_the_file_and_marks_unsaved_changes() {
    let directory = TempDir::new("app-statusline");
    let (mut app, _) = open(&directory, "notes.md", "hello\n");

    let clean = frame(&mut app, 40, 6);
    let status = clean.last().expect("there is a statusline").clone();
    assert!(status.contains("notes.md"), "{status:?}");
    assert!(!status.contains("[+]"), "{status:?}");

    type_text(&mut app, "x");
    let dirty = frame(&mut app, 40, 6);
    let status = dirty.last().expect("there is a statusline").clone();
    assert!(status.contains("[+]"), "{status:?}");
}

#[test]
fn an_open_prompt_takes_the_statusline_row() {
    let directory = TempDir::new("app-prompt-row");
    let (mut app, _) = open(&directory, "notes.md", "hello\n");
    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "12");

    let rows = frame(&mut app, 40, 6);
    let status = rows.last().expect("there is a statusline");
    assert!(status.starts_with("Go to line: 12"), "{status:?}");
    assert!(!status.contains("notes.md"), "{status:?}");
}

#[test]
fn a_message_is_shown_until_the_next_key() {
    let directory = TempDir::new("app-message");
    let (mut app, _) = open(&directory, "a.txt", "hello");
    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);

    let rows = frame(&mut app, 40, 6);
    let status = rows.last().expect("there is a statusline");
    assert!(status.starts_with("wrote a.txt"), "{status:?}");

    type_text(&mut app, "x");
    assert!(app.message().is_none(), "the next key clears it");
}

#[test]
fn the_document_text_reaches_the_screen() {
    let directory = TempDir::new("app-paint");
    let (mut app, _) = open(&directory, "a.txt", "alpha\nbeta\n");
    let rows = frame(&mut app, 40, 6);
    assert!(rows[0].contains("alpha"), "{:?}", rows[0]);
    assert!(rows[1].contains("beta"), "{:?}", rows[1]);
}

#[test]
fn a_search_panel_opens_and_closes_without_disturbing_the_document() {
    let directory = TempDir::new("app-search");
    let (mut app, _) = open(&directory, "a.txt", "alpha\nbeta\nalpha\n");

    assert_eq!(app.handle_input(&ctrl('f')), Flow::Running);
    assert!(app.is_searching());

    type_text(&mut app, "beta");
    assert_eq!(
        app.editor().expect("active editor").content(),
        "alpha\nbeta\nalpha\n",
        "typing into the panel is not typing into the document"
    );
    assert_eq!(app.editor().expect("active editor").search_match_count(), 1);

    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.is_searching());
}

#[test]
fn a_save_chord_still_works_while_the_search_panel_is_open() {
    // The panel reports what it does not bind, so the host's verbs stay live.
    let directory = TempDir::new("app-search-save");
    let (mut app, path) = open(&directory, "a.txt", "alpha\n");
    type_text(&mut app, "x");

    assert_eq!(app.handle_input(&ctrl('f')), Flow::Running);
    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "xalpha\n"
    );
}

#[test]
fn a_resize_is_written_into_the_kernels_viewport() {
    let directory = TempDir::new("app-resize");
    let (mut app, _) = open(&directory, "a.txt", "one\ntwo\nthree\n");
    app.resize(80, 25);

    let viewport = &app.editor().expect("active editor").state().viewport;
    assert!(
        (viewport.line_height - 1.0).abs() < f32::EPSILON,
        "one cell is one unit"
    );
    // The tab strip and statusline each reserve a row.
    assert_eq!(viewport.visible_lines, 23);
}

#[test]
fn focus_events_change_nothing_in_the_document() {
    let mut app = unnamed();
    type_text(&mut app, "abc");
    assert_eq!(app.handle_input(&TerminalInput::FocusLost), Flow::Running);
    assert_eq!(app.handle_input(&TerminalInput::FocusGained), Flow::Running);
    assert_eq!(app.editor().expect("active editor").content(), "abc");
}
