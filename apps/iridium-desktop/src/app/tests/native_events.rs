use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::{open, press, type_into};
use crate::app::Flow;
use crate::events::DesktopEvent;

#[test]
fn finder_batch_opens_tabs_without_replacing_unsaved_work() {
    let directory = TempDir::new("finder-batch");
    let (mut app, original_path) = open(&directory, "original.rs", "original");
    let original = app.workspace.active().expect("original tab");
    type_into(&mut app, "dirty ");
    let second = directory.path().join("second.json");
    let third = directory.path().join("café 100%.txt");
    std::fs::write(&second, "[1,2]").expect("second fixture");
    std::fs::write(&third, "third").expect("third fixture");
    assert_eq!(
        app.native_event(DesktopEvent::OpenFiles(vec![Ok(second), Ok(third)])),
        Flow::Running
    );
    assert_eq!(app.workspace.tab_count(), 3);
    assert_eq!(app.test_editor().content(), "third");
    app.workspace.activate(original);
    assert_eq!(app.test_editor().content(), "dirty original");
    assert!(app.is_dirty());
    assert_eq!(
        std::fs::read_to_string(original_path).expect("disk"),
        "original"
    );
}

#[test]
fn valid_files_do_not_hide_earlier_open_or_url_errors() {
    let directory = TempDir::new("finder-errors");
    let (mut app, _) = open(&directory, "original.txt", "original");
    let bad = directory.path().join("binary.txt");
    let good = directory.path().join("good.txt");
    std::fs::write(&bad, [0xff, 0xff]).expect("invalid UTF-8 fixture");
    std::fs::write(&good, "good").expect("valid fixture");
    app.native_event(DesktopEvent::OpenFiles(vec![
        Ok(bad),
        Err("undecodable URL".to_owned()),
        Ok(good),
    ]));
    assert_eq!(app.workspace.tab_count(), 2);
    assert_eq!(app.test_editor().content(), "good");
    let message = app.message.as_ref().expect("batch errors retained");
    assert!(message.is_error());
    assert!(message.text().contains("binary.txt"));
    assert!(message.text().contains("undecodable URL"));
}

#[test]
fn finder_waits_for_a_close_confirmation_to_finish() {
    let directory = TempDir::new("finder-dialog");
    let (mut app, _) = open(&directory, "original.txt", "original");
    type_into(&mut app, "dirty ");
    let original = app.workspace.active();
    app.close_active_tab(false);
    assert!(app.prompt.is_some());
    let next = directory.path().join("next.txt");
    std::fs::write(&next, "next").expect("next fixture");
    app.native_event(DesktopEvent::OpenFiles(vec![Ok(next)]));
    assert_eq!(app.workspace.active(), original);
    assert_eq!(app.workspace.tab_count(), 1);
    assert_eq!(app.pending_open.len(), 1);
    app.press(&press(KeyCode::Escape));
    app.drain_native_files();
    assert!(app.pending_open.is_empty());
    assert_eq!(app.workspace.tab_count(), 2);
    assert_eq!(app.test_editor().content(), "next");
}
