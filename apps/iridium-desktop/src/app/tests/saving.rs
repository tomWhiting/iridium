//! Saving, and what a dirty buffer does to a quit.
//!
//! The chord that writes, the prompt that asks where to write, and the
//! confirmation a modified buffer puts in the way of closing — the three
//! places where a lost keystroke costs the user their work.

use std::path::Path;

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

/// The gap Tom found on 12 Aug 2026: a **named** document had nowhere to go.
///
/// `save_as` was written, tested and complete; `⌘S` reaches it only for an
/// unnamed buffer, because for a named one it must write rather than ask. So
/// the whole verb was reachable from exactly one state and invisible from every
/// other. This is the door, pressed the way a person presses it.
#[test]
fn a_named_document_can_be_written_somewhere_else_from_the_keyboard() {
    let directory = TempDir::new("desktop-save-as-named");
    let (mut app, original) = open(&directory, "a.txt", "one");
    type_into(&mut app, "x");

    assert_eq!(app.press(&meta_shift(KeyCode::Char('s'))), Flow::Running);
    let Some(Prompt::SaveAs(_)) = &app.prompt else {
        panic!("⌘⇧S did not ask where to write");
    };
    assert_eq!(
        app.prompt.as_ref().map(Prompt::line),
        Some(format!("Save as: {}", original.display())),
        "the field starts from the name the document already has"
    );

    // The caret is at the end, so one keystroke changes the name — which is
    // the whole reason the field is prefilled rather than empty.
    type_into(&mut app, "2");
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert!(app.prompt.is_none());

    let renamed = directory.path().join("a.txt2");
    assert_eq!(
        std::fs::read_to_string(&renamed).expect("the new file reads back"),
        "xone"
    );
    assert_eq!(front(&app).as_deref(), Some("a.txt2"));
    assert!(!app.is_dirty());
    assert_eq!(
        std::fs::read_to_string(&original).expect("the original reads back"),
        "one",
        "a save-as must not touch the file it moved away from"
    );
}

/// A file already at the target is **asked about**, not refused and not
/// silently replaced.
///
/// Refusing outright would make "put this over that" unsayable, which is half
/// of what the verb is for; overwriting silently would destroy a file the user
/// never named in this session. So both answers exist, and "no" has to be
/// completely inert — that is the half of this test that matters.
#[test]
fn a_save_as_over_an_existing_file_asks_first_and_no_leaves_it_alone() {
    let directory = TempDir::new("desktop-save-as-exists");
    let (mut app, original) = open(&directory, "a.txt", "mine");
    let occupied = fixture(&directory, "b.txt", "theirs");

    assert_eq!(app.save_as(&occupied), Flow::Running);
    assert!(
        matches!(app.prompt, Some(Prompt::Confirm { .. })),
        "an occupied path asked nothing"
    );

    assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);
    assert_eq!(
        std::fs::read_to_string(&occupied).expect("the file reads back"),
        "theirs",
        "answering no wrote anyway"
    );
    assert_eq!(
        front(&app).as_deref(),
        Some("a.txt"),
        "answering no moved the document anyway"
    );

    // Asked again, "yes" replaces it — and the document moves with it.
    assert_eq!(app.save_as(&occupied), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(
        std::fs::read_to_string(&occupied).expect("the file reads back"),
        "mine"
    );
    assert_eq!(front(&app).as_deref(), Some("b.txt"));
    assert_eq!(
        std::fs::read_to_string(&original).expect("the original reads back"),
        "mine",
        "the original was written by the earlier save and must be untouched now"
    );
}

/// ⭐ A save-as that **failed** must leave the document attached to the file it
/// had.
///
/// This is why the write goes through a clone. Renaming the document's own
/// `TextFile` first and saving second would, on any failure, leave the buffer
/// believing it lives at a path nothing was ever written to — and the next `⌘S`
/// would then create that file without a word, which is the quiet version of
/// losing an afternoon's work.
#[test]
fn a_failed_save_as_leaves_the_document_attached_to_the_file_it_had() {
    let directory = TempDir::new("desktop-save-as-failed");
    let (mut app, original) = open(&directory, "a.txt", "one");
    type_into(&mut app, "x");

    // A directory that does not exist, so the atomic write has nowhere to put
    // its temporary file. Not `Appeared`, so no question is asked.
    let nowhere = directory.path().join("no-such-folder").join("b.txt");
    assert_eq!(app.save_as(&nowhere), Flow::Running);
    assert!(app.prompt.is_none(), "a failure asked a question");
    let message = app.message.as_ref().expect("the failure was reported");
    assert!(message.is_error(), "{}", message.text());

    assert_eq!(
        front(&app).as_deref(),
        Some("a.txt"),
        "the document followed a write that never happened"
    );
    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    assert_eq!(
        std::fs::read_to_string(&original).expect("the original reads back"),
        "xone",
        "the next save went somewhere other than the file the document has"
    );
    assert!(!nowhere.exists());
}

/// Writing to the path the document already has is a **save**, and gets the
/// save's guard rather than the save-as question.
///
/// The clone a save-as makes has no baseline, so left to itself it would report
/// a file that has sat untouched all session as one that "appeared" and ask
/// permission to overwrite it — a nuisance in place of the real protection.
/// Delegating gets the byte-comparison guard back, and this is the test that
/// the delegation happens.
#[test]
fn saving_as_the_path_it_already_has_gets_the_staleness_guard() {
    let directory = TempDir::new("desktop-save-as-same");
    let (mut app, path) = open(&directory, "a.txt", "mine");
    type_into(&mut app, "!");
    std::fs::write(&path, "theirs").expect("the external write landed");

    assert_eq!(app.save_as(&path), Flow::Running);
    assert!(
        app.prompt.is_none(),
        "it asked about overwriting its own file"
    );
    let message = app.message.as_ref().expect("the refusal was reported");
    assert!(
        message.text().contains("changed on disk"),
        "{}",
        message.text()
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads back"),
        "theirs"
    );
}

/// ⭐ A relative name lands under the **session's** root, not the process's
/// working directory.
///
/// A bundle launched from Finder inherits `/` as its working directory — the
/// case `crate::project` exists because of, and one no test run from a terminal
/// could reproduce by accident. `notes.md` typed into a new file would resolve
/// to `/notes.md` and fail on a permission denial nobody could explain.
#[test]
fn a_relative_name_is_written_under_the_sessions_root() {
    let directory = TempDir::new("desktop-save-as-relative");
    let (mut app, _) = open(&directory, "a.txt", "one");

    assert_eq!(app.save_as(Path::new("notes.md")), Flow::Running);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("notes.md"))
            .expect("the file landed beside the open one"),
        "one"
    );
    assert_eq!(front(&app).as_deref(), Some("notes.md"));
}

/// Read-only protects the file the buffer came from, not the buffer's text.
///
/// Writing back to that file is refused; writing the text somewhere else is
/// not, because a buffer that could never be got out of the editor at all would
/// be a dead end rather than a protection.
#[test]
fn a_read_only_document_can_be_written_elsewhere_but_not_over_its_own_file() {
    let directory = TempDir::new("desktop-save-as-read-only");
    let (mut app, path) = open(&directory, "a.txt", "one");
    app.test_editor_mut().state_mut().read_only = true;

    assert_eq!(app.save_as(&path), Flow::Running);
    let message = app.message.as_ref().expect("the refusal was reported");
    assert!(message.text().contains("read-only"), "{}", message.text());

    let elsewhere = directory.path().join("copy.txt");
    assert_eq!(app.save_as(&elsewhere), Flow::Running);
    assert_eq!(
        std::fs::read_to_string(&elsewhere).expect("the copy reads back"),
        "one"
    );
}

/// ⭐ A byte-order mark survives being saved under a new name.
///
/// This is what makes the clone the right construction rather than a stylistic
/// choice: `TextFile::new` would build a file that has never seen a mark and
/// drop it, silently changing a file some tools require it in. `rename` keeps
/// the mark and drops only the baseline, which is the part that genuinely is
/// unknown about a new path.
#[test]
fn a_byte_order_mark_survives_a_save_under_a_new_name() {
    let directory = TempDir::new("desktop-save-as-bom");
    let path = directory.path().join("a.txt");
    std::fs::write(&path, b"\xEF\xBB\xBFone").expect("the fixture was written");
    let mut app = DesktopApp::new(Options {
        path: Some(path),
        ..Options::default()
    })
    .expect("the session opened");

    let target = directory.path().join("b.txt");
    assert_eq!(app.save_as(&target), Flow::Running);
    let bytes = std::fs::read(&target).expect("the new file reads back");
    assert_eq!(
        bytes, b"\xEF\xBB\xBFone",
        "the mark was dropped by the save-as"
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
