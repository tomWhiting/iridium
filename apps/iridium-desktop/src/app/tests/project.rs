//! Opening a directory rather than a file.
//!
//! A session opened on a directory has no file to show, and showing nothing is
//! not the answer: what was asked for is the *project*, so the explorer is what
//! comes up, rooted where the command line pointed.
//!
//! # The failure these tests were written against
//!
//! Before them, `iridium-desktop .` refused to start. The path went straight to
//! [`iridium_file::TextFile::open`], which read a directory as text and failed
//! with the operating system's `Is a directory`. There was no way to open a
//! project at all — the explorer could only be reached by opening a file first
//! and letting [`crate::project::explorer_root`] guess upwards from it.

use std::path::Path;

use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};

/// A session opened on `directory`, the way the command line would.
fn opened_on(path: &Path) -> DesktopApp {
    DesktopApp::new(Options {
        path: Some(path.to_path_buf()),
        ..Options::default()
    })
    .expect("a session opened on the directory")
}

#[test]
fn a_directory_opens_the_explorer_rooted_there() {
    let directory = TempDir::new("desktop-project-open");
    std::fs::write(directory.path().join("notes.md"), "a file in it")
        .expect("the fixture file was written");

    let app = opened_on(directory.path());
    let explorer = app
        .explorer
        .as_ref()
        .expect("a session opened on a directory shows it");
    assert_eq!(
        explorer.root_path(),
        directory.path(),
        "the panel is rooted where the command line pointed"
    );
}

/// A directory is not a file, so the session has the same untitled buffer a
/// session with no argument at all has — and no tab claiming to be a folder.
#[test]
fn a_directory_leaves_the_session_on_an_untitled_buffer() {
    let directory = TempDir::new("desktop-project-untitled");
    let app = opened_on(directory.path());
    assert_eq!(app.workspace.tab_count(), 1);
    assert_eq!(
        app.test_editor().content(),
        "",
        "nothing was read as text out of a directory"
    );
}

/// ⭐ The root is the session's, not the panel's.
///
/// Closing the explorer drops it outright — the panel owns a reader thread and
/// an arena, and a hidden one would keep both. So re-opening builds a new one,
/// and without the session remembering what was asked for it would fall back to
/// [`crate::project::explorer_root`]'s guess from the active file. That file is
/// the untitled buffer, so the guess would land on the working directory: the
/// panel would come back somewhere else entirely.
#[test]
fn the_project_root_survives_closing_and_reopening_the_panel() {
    let directory = TempDir::new("desktop-project-reopen");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert!(app.explorer.is_none(), "the toggle closed the panel");

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert_eq!(
        app.explorer
            .as_ref()
            .expect("the toggle opened it again")
            .root_path(),
        directory.path(),
        "the panel came back where the session was opened, not where a guess put it"
    );
}

/// A file still opens a file, and still opens no panel: the explorer is a thing
/// you ask for with `Ctrl+Alt+E`, and a session that put it up uninvited would
/// be answering a question nobody asked.
#[test]
fn a_file_still_opens_a_file_and_no_panel() {
    let directory = TempDir::new("desktop-project-file");
    let (app, _path) = open(&directory, "notes.md", "still a file");
    assert!(app.explorer.is_none(), "a file asked for no panel");
    assert_eq!(app.test_editor().content(), "still a file");
}

/// ⚠️ A path that does not exist is a *file* — the one nobody has written yet.
///
/// Worth a test of its own because the directory branch is `Path::is_dir`, and
/// that answers `false` for a path that is not there. So `iridium-desktop
/// notes-i-have-not-written.md` keeps the behaviour it has always had: an empty
/// buffer that knows its own name and writes there when saved, with no panel.
/// A directory branch that read "not a file" rather than "is a directory" would
/// have turned that into a project rooted at a folder that does not exist.
#[test]
fn a_path_that_does_not_exist_yet_is_a_file_to_be_written() {
    let directory = TempDir::new("desktop-project-absent");
    let missing = directory.path().join("nowhere.md");
    let app = DesktopApp::new(Options {
        path: Some(missing),
        ..Options::default()
    })
    .expect("a file that is not written yet still opens");
    assert!(app.explorer.is_none(), "a file asked for no panel");
    assert_eq!(app.test_editor().content(), "");
}
