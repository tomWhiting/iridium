//! Host-driven review acceptance, including failure and cancellation phases.
use super::*;
use crate::{app::App, cli::Options, file::test_support::TempDir};
use iridium_editor::Modifiers;
use iridium_tui::input::TerminalInput;
use std::{fs, path::Path, thread};
fn app(root: &Path) -> App {
    let mut app = App::with_config(
        Options {
            path: Some(root.to_owned()),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("app");
    app.close_explorer();
    app.resize(110, 24);
    app
}
fn key(app: &mut App, key: KeyCode) {
    app.handle_input(&TerminalInput::Key(KeyEvent::new(key, Modifiers::none())));
}
fn wait(app: &mut App) {
    let start = Instant::now();
    while app.project_search_waiting() {
        app.poll_project_search();
        assert!(start.elapsed() < Duration::from_secs(10), "review timeout");
        thread::sleep(Duration::from_millis(1));
    }
}
fn review(app: &App) -> &Review {
    app.project_search
        .as_ref()
        .expect("panel")
        .review
        .as_ref()
        .expect("review")
}
fn ready(app: &mut App, text: &str) {
    app.open_project_search(text);
    wait(app);
    key(app, KeyCode::F5);
    app.handle_input(&TerminalInput::Paste("dog".into()));
    wait(app);
    assert!(review(app).preview.is_some());
}
#[test]
fn review_excludes_a_file_applies_to_buffers_and_undo_restores_prior_edits() {
    let dir = TempDir::new("review-apply");
    let a = dir.path().join("a.rs");
    let b = dir.path().join("b.rs");
    fs::write(&a, "cat cat\n").expect("a");
    fs::write(&b, "cat\n").expect("b");
    let mut app = app(dir.path());
    app.add_file(&a).expect("open");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content("prior cat cat\n");
    ready(&mut app, "cat");
    let index = review(&app)
        .hits
        .iter()
        .position(|hit| hit.path.ends_with("b.rs"))
        .expect("b match");
    app.project_search
        .as_mut()
        .expect("panel")
        .review
        .as_mut()
        .expect("review")
        .selected = index;
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::F7);
    key(&mut app, KeyCode::F6);
    wait(&mut app);
    assert!(review(&app).completed);
    assert_eq!(review(&app).outcomes.len(), 1);
    assert!(review(&app).outcomes[0].contains("APPLIED, UNSAVED"));
    app.add_file(&a).expect("tab");
    assert_eq!(app.editor().expect("editor").content(), "prior dog dog\n");
    assert!(app.workspace.active_editor_mut().expect("editor").undo());
    assert_eq!(app.editor().expect("editor").content(), "prior cat cat\n");
    assert_eq!(fs::read_to_string(&a).expect("disk"), "cat cat\n");
    assert_eq!(fs::read_to_string(b).expect("disk"), "cat\n");
}
#[test]
fn same_length_external_edit_before_apply_refuses_all_documents() {
    let dir = TempDir::new("review-stale");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "cat").expect("a");
    fs::write(&b, "cat").expect("b");
    let mut app = app(dir.path());
    ready(&mut app, "cat");
    fs::write(&b, "owl").expect("external");
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    assert!(review(&app).completed);
    assert_eq!(review(&app).outcomes.len(), 2);
    assert!(
        review(&app)
            .outcomes
            .iter()
            .all(|outcome| outcome.starts_with("NOT APPLIED"))
    );
    app.add_file(&a).expect("tab");
    assert_eq!(app.editor().expect("editor").content(), "cat");
    assert_eq!(fs::read_to_string(a).expect("a"), "cat");
    assert_eq!(fs::read_to_string(b).expect("b"), "owl");
}
#[test]
fn regex_capture_preview_and_save_match_review_exactly() {
    let dir = TempDir::new("review-captures");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "猫=12\n").expect("a");
    fs::write(&b, "é=34\n").expect("b");
    let mut app = app(dir.path());
    app.open_project_search(r"(\pL+)=(\d+)");
    key(&mut app, KeyCode::F2);
    wait(&mut app);
    key(&mut app, KeyCode::F5);
    app.handle_input(&TerminalInput::Paste("$1:$2".into()));
    wait(&mut app);
    assert_eq!(review(&app).preview.as_ref().expect("preview").len(), 2);
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    assert!(
        review(&app)
            .outcomes
            .iter()
            .all(|o| o.contains("APPLIED, SAVED"))
    );
    assert_eq!(fs::read_to_string(a).expect("a"), "猫:12\n");
    assert_eq!(fs::read_to_string(b).expect("b"), "é:34\n");
}
#[test]
fn cancel_preflight_makes_no_edits_and_late_disk_change_has_explicit_outcome() {
    let dir = TempDir::new("review-cancel");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "cat").expect("a");
    fs::write(&b, "cat").expect("b");
    let mut app = app(dir.path());
    ready(&mut app, "cat");
    key(&mut app, KeyCode::F6);
    app.poll_project_search();
    key(&mut app, KeyCode::Escape);
    wait(&mut app);
    assert!(
        review(&app)
            .outcomes
            .iter()
            .all(|o| o.starts_with("NOT APPLIED"))
    );
    key(&mut app, KeyCode::Escape);
    app.poll_project_search();
    app.open_project_search("cat");
    wait(&mut app);
    key(&mut app, KeyCode::F5);
    app.handle_input(&TerminalInput::Paste("dog".into()));
    wait(&mut app);
    key(&mut app, KeyCode::F9);
    for _ in 0..20 {
        app.poll_project_search();
        if review(&app)
            .outcomes
            .iter()
            .any(|o| o.contains("APPLIED, SAVED"))
        {
            break;
        }
    }
    assert_eq!(fs::read_to_string(&a).expect("a"), "dog");
    fs::write(&b, "owl").expect("late external");
    wait(&mut app);
    assert!(
        review(&app)
            .outcomes
            .iter()
            .any(|o| o.starts_with("NOT APPLIED") && o.contains("disk changed"))
    );
    assert_eq!(fs::read_to_string(b).expect("b"), "owl");
}
#[test]
fn replacement_changes_cancel_old_preview_and_multiline_paste_is_exact() {
    let dir = TempDir::new("review-update");
    fs::write(dir.path().join("a"), "cat").expect("file");
    let mut app = app(dir.path());
    ready(&mut app, "cat");
    app.handle_input(&TerminalInput::Paste("s".into()));
    key(&mut app, KeyCode::F6);
    assert!(review(&app).operation.is_none());
    wait(&mut app);
    assert_eq!(
        review(&app).preview.as_ref().expect("preview")[0].text(),
        "dogs"
    );
    app.handle_input(&TerminalInput::Paste("\n".into()));
    wait(&mut app);
    assert_eq!(
        review(&app).preview.as_ref().expect("preview")[0].text(),
        "dogs\n"
    );
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    assert_eq!(
        fs::read_to_string(dir.path().join("a")).expect("saved"),
        "dogs\n"
    );
    for (w, h) in [(0, 0), (1, 1), (7, 2), (110, 24)] {
        app.render(&mut iridium_tui::cell::Surface::new(
            w,
            h,
            iridium_tui::cell::ColorDepth::TrueColor,
        ));
    }
}

#[cfg(unix)]
#[test]
fn failed_save_keeps_the_applied_edit_and_reports_the_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = TempDir::new("review-save-failure");
    let path = dir.path().join("file");
    fs::write(&path, "cat").expect("file");
    let mut app = app(dir.path());
    ready(&mut app, "cat");
    let original = fs::metadata(dir.path()).expect("metadata").permissions();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500))
        .expect("read-only directory");
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    fs::set_permissions(dir.path(), original).expect("restore directory permissions");
    assert_eq!(fs::read_to_string(&path).expect("disk"), "cat");
    assert_eq!(review(&app).outcomes.len(), 1);
    assert!(
        review(&app).outcomes[0].contains("APPLIED")
            && review(&app).outcomes[0].contains("cannot write"),
        "{:?}",
        review(&app).outcomes
    );
    app.add_file(&path).expect("tab");
    assert_eq!(app.editor().expect("editor").content(), "dog");
    assert!(app.is_dirty());
    assert!(app.workspace.active_editor_mut().expect("editor").undo());
    assert_eq!(app.editor().expect("editor").content(), "cat");
}
#[test]
fn readonly_source_refuses_the_whole_preflight() {
    let dir = TempDir::new("review-readonly");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "cat").expect("a");
    fs::write(&b, "cat").expect("b");
    let mut app = app(dir.path());
    app.add_file(&b).expect("open");
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .state_mut()
        .read_only = true;
    ready(&mut app, "cat");
    key(&mut app, KeyCode::F6);
    wait(&mut app);
    assert!(
        review(&app)
            .outcomes
            .iter()
            .all(|o| o.starts_with("NOT APPLIED"))
    );
    app.add_file(&a).expect("tab");
    assert_eq!(app.editor().expect("editor").content(), "cat");
}

#[test]
fn full_inspection_pages_both_values_and_cannot_apply_or_change_template() {
    let dir = TempDir::new("review-detail");
    let source = dir.path().join("source");
    let original = format!("{}TAIL", "cat".repeat(100));
    fs::write(&source, &original).expect("source");
    let mut app = app(dir.path());
    app.resize(24, 7);
    app.open_project_search(".+");
    key(&mut app, KeyCode::F2);
    wait(&mut app);
    key(&mut app, KeyCode::F5);
    let replacement = format!("{}\n猫END", "dog".repeat(100));
    app.handle_input(&TerminalInput::Paste(replacement.clone()));
    wait(&mut app);
    key(&mut app, KeyCode::F4);
    assert!(review(&app).detail.is_some());
    for _ in 0..10 {
        key(&mut app, KeyCode::PageDown);
    }
    key(&mut app, KeyCode::PageUp);
    key(&mut app, KeyCode::Tab);
    for _ in 0..10 {
        key(&mut app, KeyCode::PageDown);
    }
    let mut buffer = iridium_tui::cell::CellBuffer::new(24, 7);
    let palette = iridium_tui::frame::Palette::from_theme(&iridium_editor::Theme::default());
    review(&app).paint(&mut buffer, &palette);
    let mut rendered = String::new();
    for row in 0..7 {
        for cell in buffer.row(row).expect("row") {
            if let iridium_tui::cell::CellContent::Grapheme(g) = cell.content() {
                g.push_to(&mut rendered);
            }
        }
    }
    assert!(rendered.contains("猫END"), "{rendered}");
    app.handle_input(&TerminalInput::Paste("unexpected".into()));
    key(&mut app, KeyCode::F9);
    assert!(review(&app).operation.is_none());
    assert_eq!(review(&app).entry.text(), replacement);
    assert_eq!(fs::read_to_string(&source).expect("disk"), original);
    app.resize(12, 5);
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::PageDown);
    key(&mut app, KeyCode::Escape);
    assert!(review(&app).detail.is_none());
    assert!(review(&app).open);
    key(&mut app, KeyCode::F9);
    wait(&mut app);
    assert_eq!(fs::read_to_string(source).expect("disk"), replacement);
}
