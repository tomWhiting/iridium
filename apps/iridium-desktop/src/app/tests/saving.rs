//! Saving, and what a dirty buffer does to a quit.
//!
//! The chord that writes, the prompt that asks where to write, and the
//! confirmation a modified buffer puts in the way of closing — the three
//! places where a lost keystroke costs the user their work.

use iridium_editor::{Editor, KeyCode, Language, Modifiers};
use iridium_file::TextFile;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};
use crate::prompt::Prompt;

#[test]
fn the_save_chord_writes_the_file_and_cleans_the_buffer() {
    let directory = TempDir::new("desktop-save");
    let (mut app, path) = open(&directory, "a.txt", "one");
    type_into(&mut app, "x");
    assert!(app.is_dirty());

    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    assert!(!app.is_dirty(), "a saved buffer is clean");
    assert!(
        std::fs::read_to_string(&path)
            .expect("the file reads back")
            .contains('x'),
        "the edit reached the disk"
    );
    let message = app.message.as_ref().expect("the save reported itself");
    assert!(message.text().contains("wrote"), "{}", message.text());
    assert!(!message.is_error());
}

#[test]
fn the_mac_save_chord_reaches_the_same_verb() {
    let directory = TempDir::new("desktop-save-meta");
    let (mut app, path) = open(&directory, "a.txt", "one");
    type_into(&mut app, "y");

    let meta = Modifiers {
        meta: true,
        ..Modifiers::none()
    };
    assert_eq!(app.press(&chord(KeyCode::Char('s'), meta)), Flow::Running);
    assert!(!app.is_dirty());
    assert!(
        std::fs::read_to_string(&path)
            .expect("the file reads back")
            .contains('y')
    );
}

#[test]
fn a_stale_file_refuses_the_save_and_the_forced_chord_overwrites() {
    let directory = TempDir::new("desktop-stale");
    let (mut app, path) = open(&directory, "a.txt", "mine");
    type_into(&mut app, "!");

    // Something else rewrites the file underneath the session.
    std::fs::write(&path, "theirs").expect("the external write landed");

    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    let message = app.message.as_ref().expect("the refusal was reported");
    assert!(message.is_error());
    assert!(
        message.text().contains("changed on disk"),
        "{}",
        message.text()
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads back"),
        "theirs",
        "the refused save wrote anyway"
    );

    let force = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(app.press(&chord(KeyCode::Char('s'), force)), Flow::Running);
    assert!(
        std::fs::read_to_string(&path)
            .expect("the file reads back")
            .contains('!'),
        "the forced save overwrote"
    );
    assert!(!app.is_dirty());
}

#[test]
fn an_unnamed_buffer_asks_for_a_name_and_saves_under_it() {
    let directory = TempDir::new("desktop-save-as");
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    type_into(&mut app, "hi");

    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    assert!(
        matches!(app.prompt, Some(Prompt::SaveAs(_))),
        "an unnamed buffer asks for a name"
    );

    // The prompt is modal: the save chord must not save mid-question.
    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    assert!(app.prompt.is_some(), "the chord was swallowed");

    let target = directory.path().join("notes.txt");
    type_into(&mut app, &target.to_string_lossy());
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);

    assert!(app.prompt.is_none());
    assert_eq!(
        std::fs::read_to_string(&target).expect("the new file reads back"),
        "hi"
    );
    assert!(!app.is_dirty());
    assert_eq!(
        app.test_document()
            .file
            .as_ref()
            .map(TextFile::display_name),
        Some("notes.txt".to_owned())
    );
}

#[test]
fn closing_over_unsaved_changes_asks_first() {
    let directory = TempDir::new("desktop-quit");
    let (mut app, _path) = open(&directory, "a.txt", "one");
    type_into(&mut app, "x");

    assert_eq!(app.request_quit(), Flow::Running);
    assert!(app.prompt.is_some(), "a dirty session asks before quitting");

    // "No" keeps the session.
    assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);
    assert!(app.prompt.is_none());

    // Asked again, "yes" leaves.
    assert_eq!(app.request_quit(), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Exit);
}

#[test]
fn closing_a_clean_session_just_leaves() {
    let directory = TempDir::new("desktop-quit-clean");
    let (mut app, _path) = open(&directory, "a.txt", "one");
    assert_eq!(app.request_quit(), Flow::Exit);
    assert!(app.prompt.is_none());
}

#[test]
fn a_read_only_document_refuses_to_save() {
    let directory = TempDir::new("desktop-read-only");
    let (mut app, path) = open(&directory, "a.txt", "one");
    app.test_editor_mut().state_mut().read_only = true;

    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    let message = app.message.as_ref().expect("the refusal was reported");
    assert!(message.is_error());
    assert!(message.text().contains("read-only"), "{}", message.text());
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads back"),
        "one"
    );
}

/// A name that claims a different language is believed — including when it
/// claims none.
///
/// `save_as` is the one path left where a document changes what it is called
/// without changing which editor holds it. Opening a file makes a new tab with
/// a fresh editor, so the language cannot carry over there; renaming reuses
/// the editor, so it can.
#[test]
fn saving_under_a_name_no_grammar_claims_clears_the_language() {
    let directory = TempDir::new("desktop-save-as-language");
    let (mut app, _) = open(&directory, "a.rs", "fn main() {}\n");
    assert_eq!(
        app.workspace.active_editor().and_then(Editor::language),
        Some(Language::Rust),
        "the fixture must start out as Rust, or this test proves nothing"
    );

    assert_eq!(
        app.save_as(&directory.path().join("notes.log")),
        Flow::Running
    );
    assert_eq!(
        app.workspace.active_editor().and_then(Editor::language),
        None,
        "a .log is not Rust, and the old language must not outlive the name"
    );
}

/// The same path in the other direction: a rename that *does* name a language
/// adopts it.
#[test]
fn saving_under_a_name_a_grammar_claims_adopts_that_language() {
    let directory = TempDir::new("desktop-save-as-adopt");
    let (mut app, _) = open(&directory, "a.log", "fn main() {}\n");
    assert_eq!(
        app.workspace.active_editor().and_then(Editor::language),
        None,
        "the fixture must start out with no language"
    );

    assert_eq!(
        app.save_as(&directory.path().join("main.rs")),
        Flow::Running
    );
    assert_eq!(
        app.workspace.active_editor().and_then(Editor::language),
        Some(Language::Rust)
    );
}
