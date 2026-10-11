//! Real host input, background results, dirty buffers and stale-result refusal.
use super::*;
use crate::cli::Options;
use crate::file::test_support::TempDir;
use iridium_editor::{Modifiers, Position};
use iridium_tui::{
    cell::{ColorDepth, Surface},
    input::TerminalInput,
};
use std::{fs, thread};
fn press(key: KeyCode) -> TerminalInput {
    TerminalInput::Key(KeyEvent::new(key, Modifiers::none()))
}
fn app(dir: &TempDir) -> App {
    let mut app = App::with_config(
        Options {
            path: Some(dir.path().to_owned()),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.close_explorer();
    app.resize(100, 24);
    app
}
fn settled(app: &mut App) {
    let start = Instant::now();
    while app.project_search_waiting() {
        app.poll_project_search();
        assert!(start.elapsed() < Duration::from_secs(5), "search timeout");
        thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn palette_live_query_opens_unicode_match_in_a_tab_and_preserves_unsaved_buffer() {
    let dir = TempDir::new("project-search");
    let a = dir.path().join("a.rs");
    let b = dir.path().join("b.rs");
    fs::write(&a, "disk").expect("a");
    fs::write(&b, "é猫 needle\n").expect("b");
    let mut app = app(&dir);
    app.add_file(&a).expect("open");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("unsaved");
    app.handle_input(&press(KeyCode::Char(':')));
    app.handle_input(&TerminalInput::Paste("grep needle".into()));
    settled(&mut app);
    assert!(app.project_search_open());
    assert_eq!(app.project_search.as_ref().expect("panel").matches.len(), 1);
    app.handle_input(&press(KeyCode::Enter));
    assert!(!app.project_search_open());
    assert!(app.file().expect("file").path().ends_with("b.rs"));
    assert_eq!(
        app.editor().expect("editor").state().cursor.primary,
        iridium_editor::Selection::new(Position::new(0, 3), Position::new(0, 9))
    );
    app.add_file(&a).expect("tab");
    assert_eq!(app.editor().expect("editor").content(), "unsaved");
    assert_eq!(fs::read_to_string(a).expect("disk"), "disk");
}
#[test]
fn newer_queries_discard_old_results_and_escape_never_edits_document() {
    let dir = TempDir::new("project-search");
    fs::write(dir.path().join("many"), "old\n".repeat(50_000)).expect("file");
    fs::write(dir.path().join("new"), "fresh").expect("file");
    let mut app = app(&dir);
    app.open_project_search("old");
    app.project_search.as_mut().expect("panel").due = Some(Instant::now());
    app.poll_project_search();
    app.open_project_search("fresh");
    settled(&mut app);
    let hits = &app.project_search.as_ref().expect("panel").matches;
    assert_eq!(hits.len(), 1);
    assert_eq!(&*hits[0].line_text, "fresh");
    app.handle_input(&press(KeyCode::Escape));
    assert!(!app.project_search_open());
    assert_eq!(app.editor().expect("editor").content(), "");
}
#[test]
fn stale_open_buffer_and_changed_disk_line_are_refused() {
    let dir = TempDir::new("project-search");
    let path = dir.path().join("a");
    fs::write(&path, "needle").expect("file");
    let mut app = app(&dir);
    app.open_project_search("needle");
    settled(&mut app);
    fs::write(&path, "changed").expect("external");
    app.handle_input(&press(KeyCode::Enter));
    assert!(app.project_search_open());
    assert!(
        app.project_search
            .as_ref()
            .expect("panel")
            .feedback
            .contains("changed")
    );
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("needle");
    app.open_project_search("needle");
    settled(&mut app);
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("elsewhere");
    app.handle_input(&press(KeyCode::Enter));
    assert!(
        app.project_search
            .as_ref()
            .expect("panel")
            .feedback
            .contains("buffer changed")
    );
}
#[test]
fn toggles_globs_invalid_regex_and_tiny_surfaces_stay_in_the_panel() {
    let dir = TempDir::new("project-search");
    fs::write(dir.path().join("a.rs"), "NEEDLE").expect("file");
    fs::write(dir.path().join("b.txt"), "needle").expect("file");
    let mut app = app(&dir);
    app.open_project_search("needle");
    app.handle_input(&press(KeyCode::F3));
    app.handle_input(&press(KeyCode::Tab));
    app.handle_input(&TerminalInput::Paste("*.rs;!b.txt".into()));
    settled(&mut app);
    assert_eq!(app.project_search.as_ref().expect("panel").matches.len(), 1);
    for (w, h) in [(0, 0), (1, 1), (8, 2), (100, 24)] {
        app.render(&mut Surface::new(w, h, ColorDepth::TrueColor));
    }
    app.handle_input(&press(KeyCode::Tab));
    app.open_project_search("[");
    app.handle_input(&press(KeyCode::F2));
    settled(&mut app);
    let panel = app.project_search.as_ref().expect("panel");
    assert!(panel.matches.is_empty());
    assert!(panel.feedback.contains("regex"));
}

#[test]
fn all_file_matches_become_editable_selections_with_one_undo() {
    let dir = TempDir::new("project-multi");
    let path = dir.path().join("a.rs");
    fs::write(&path, "é cat\n猫 cat\n").expect("file");
    let mut app = app(&dir);
    app.open_project_search("cat");
    settled(&mut app);
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Enter,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    )));
    assert!(!app.project_search_open());
    assert_eq!(
        app.editor().expect("editor").state().cursor.cursor_count(),
        2
    );
    app.handle_input(&press(KeyCode::Char('i')));
    app.handle_input(&TerminalInput::Paste("dog".into()));
    assert_eq!(app.editor().expect("editor").content(), "é dog\n猫 dog\n");
    assert!(app.workspace.active_editor_mut().expect("editor").undo());
    assert_eq!(app.editor().expect("editor").content(), "é cat\n猫 cat\n");
    assert_eq!(fs::read_to_string(path).expect("disk"), "é cat\n猫 cat\n");
}

#[test]
fn project_query_select_all_replacement_and_word_motion_stay_in_field() {
    let dir = TempDir::new("project-field");
    fs::write(dir.path().join("a"), "needle").expect("file");
    let mut app = app(&dir);
    app.open_project_search("wrong");
    app.handle_input(&TerminalInput::Key(KeyEvent::new(
        KeyCode::Char('a'),
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    )));
    app.handle_input(&TerminalInput::Paste("needle".into()));
    settled(&mut app);
    assert_eq!(app.project_search.as_ref().expect("panel").matches.len(), 1);
    assert_eq!(app.editor().expect("editor").content(), "");
}

#[test]
#[ignore = "manual result-row rendering measurement"]
fn measured_long_line_project_results_rendering() {
    use std::io::Write as _;
    let dir = TempDir::new("project-render-benchmark");
    fs::write(
        dir.path().join("long"),
        format!("{} needle", "猫".repeat(300_000)),
    )
    .expect("file");
    let mut app = app(&dir);
    app.open_project_search("needle");
    settled(&mut app);
    let panel = app.project_search.as_ref().expect("panel");
    assert_eq!(panel.matches.len(), 1);
    let palette =
        iridium_tui::frame::Palette::from_theme(app.editor().expect("editor").get_theme());
    let mut buffer = iridium_tui::cell::CellBuffer::new(100, 24);
    let mut timings = Vec::new();
    for _ in 0..101 {
        let start = Instant::now();
        panel.paint(&mut buffer, &palette);
        timings.push(start.elapsed().as_micros());
    }
    timings.sort_unstable();
    writeln!(
        std::io::stderr().lock(),
        "PROJECT_RESULT_PAINT source_bytes=900007 median_us={} max_us={}",
        timings[50],
        timings[100]
    )
    .expect("metrics");
}
