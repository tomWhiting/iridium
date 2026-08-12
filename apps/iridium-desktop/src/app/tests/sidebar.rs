//! The explorer's two placements, and the one thing that really separates
//! them: whether the document is reachable while the panel is on screen.
//!
//! The geometry half — that a sidebar's fit fills a band a popover's does not —
//! is proven in [`crate::overlay`], against a window these tests do not have.
//! What is proven here is the *routing*, which is where a sidebar stops being a
//! taller popover: keys, `Escape`, and the placement outliving the panel.

use std::path::Path;

use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, ExplorerFocus, ExplorerPlacement, Flow};

/// A session opened on `directory`, the way the command line would — which is
/// also what puts an explorer up without a chord.
fn opened_on(path: &Path) -> DesktopApp {
    DesktopApp::new(Options {
        path: Some(path.to_path_buf()),
        ..Options::default()
    })
    .expect("a session opened on the directory")
}

/// A directory with one file in it, so the panel has something to list.
fn project(name: &str) -> TempDir {
    let directory = TempDir::new(name);
    std::fs::write(directory.path().join("notes.md"), "a file in it")
        .expect("the fixture file was written");
    directory
}

/// ⭐ **The whole difference, in one test.** Both placements swallow keys while
/// they hold focus; only the popover is still holding them after `Escape`,
/// because only the popover is still on screen.
#[test]
fn escape_closes_a_popover_and_only_unfocuses_a_sidebar() {
    let directory = project("desktop-sidebar-escape");
    let mut app = opened_on(directory.path());
    assert_eq!(app.explorer_placement, ExplorerPlacement::Popover);

    // A popover has the keys, and `Escape` takes the panel away with them.
    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "",
        "the popover is modal, so the document never saw the key"
    );
    // Twice: the first `Escape` is spent clearing the query that keystroke
    // typed, which is the panel's own long-standing behaviour and nothing this
    // placement changes.
    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(app.explorer.is_some(), "the first escape cleared the query");
    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(app.explorer.is_none(), "a popover goes away on escape");

    // ⌘B opens one as a sidebar. It has the keys until `Escape`, and then the
    // document has them — with the panel still up.
    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
    assert!(app.explorer.is_some(), "the sidebar chord opened one");

    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "",
        "a focused sidebar is still where the keys go"
    );

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(app.explorer_has_focus(), "that escape cleared the query");
    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(
        app.explorer.is_some(),
        "escape gave the keys back, it did not take the sidebar away"
    );
    assert!(!app.explorer_has_focus());

    type_into(&mut app, "x");
    assert_eq!(
        app.test_editor().content(),
        "x",
        "the document is reachable with the sidebar still on screen"
    );
}

/// The placement is the session's, not the panel's — the panel is dropped on
/// every close, so a placement stored on it would be forgotten each time.
#[test]
fn the_placement_outlives_the_panel_that_was_in_it() {
    let directory = project("desktop-sidebar-outlives");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert!(app.explorer.is_none(), "the toggle closed the panel");
    assert_eq!(
        app.explorer_placement,
        ExplorerPlacement::Sidebar,
        "closing the panel is not a decision about where the next one goes"
    );

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert!(app.explorer.is_some(), "the toggle opened it again");
    assert!(
        app.explorer_has_focus(),
        "a panel the user just asked for takes the keys"
    );
}

/// Pressing the sidebar chord with nothing open opens one, rather than
/// answering that there is nothing to move.
#[test]
fn the_sidebar_chord_opens_a_panel_when_none_is_up() {
    let directory = project("desktop-sidebar-opens");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert!(app.explorer.is_none());

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('b'))), Flow::Running);
    assert!(
        app.explorer.is_some(),
        "the chord that asks for a sidebar produced one"
    );
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
}

/// Switching back is the same key, and it takes the modality back with it.
#[test]
fn switching_back_to_a_popover_makes_escape_close_it_again() {
    let directory = project("desktop-sidebar-back");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Popover);
    assert!(app.explorer.is_some(), "the panel did not go anywhere");

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(
        app.explorer.is_none(),
        "a popover goes away on escape again"
    );
}

/// ⚠️ An unfocused **popover** would swallow every key with nothing able to
/// reach either it or the document. The flag can be set to anything; the
/// routing does not consult it for a popover, which is what makes that state
/// unreachable rather than merely unlikely.
#[test]
fn a_popover_holds_focus_whatever_the_flag_says() {
    let directory = project("desktop-sidebar-unreachable");
    let mut app = opened_on(directory.path());

    app.explorer_focus = ExplorerFocus::Document;
    assert_eq!(app.explorer_placement, ExplorerPlacement::Popover);
    assert!(
        app.explorer_has_focus(),
        "a popover is focused by definition, not by the flag"
    );

    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "",
        "the key reached the panel rather than falling between the two"
    );
}
