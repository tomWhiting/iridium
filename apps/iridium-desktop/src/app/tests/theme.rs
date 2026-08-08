//! The light/dark toggle, and the surfaces it has to reach.
//!
//! A theme lives in more than one place in this face — the workspace hands it
//! to every open editor, and the compositor keeps its own copy because the
//! clear colour and the retained shaped buffers are built from it. These tests
//! are the record of "all of them, or the window lies about what theme it is
//! wearing".

use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};

#[test]
fn the_toggle_swaps_the_theme_and_swaps_it_back() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert!(
        app.workspace.theme().is_dark,
        "the session starts on the dark preset"
    );

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.workspace.theme().is_dark, "Ctrl+Alt+T reaches light");
    assert_eq!(
        app.workspace.theme().name,
        "Iridium Platinum",
        "the light side of the toggle is the ruled preset, not a generic light"
    );

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(
        app.workspace.theme().is_dark,
        "a toggle that only went one way would need a second key to come back"
    );
}

#[test]
fn the_mac_spelling_reaches_the_same_toggle() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(app.press(&meta_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.workspace.theme().is_dark, "⌘⌥T reaches light too");
}

/// The theme has to reach a tab that was not in front when it changed.
///
/// ⚠️ This is the failure a single-editor test cannot see. The kernel's
/// `Workspace::set_theme` walks every open document precisely so a background
/// tab does not come forward still wearing the old preset, and a face that
/// reached only `active_editor` would pass every other test in this file while
/// producing a window whose tabs disagree with each other.
#[test]
fn the_toggle_reaches_a_background_tab() {
    let directory = TempDir::new("desktop-theme-tabs");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    let second = fixture(&directory, "b.txt", "second");
    app.dropped(&second);
    assert_eq!(app.workspace.tab_count(), 2);

    // Toggled with the *second* tab in front, so the first is the one nothing
    // was looking at when the theme moved.
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.test_editor().state().theme.is_dark);

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.test_editor().content(), "first", "the other tab");
    assert!(
        !app.test_editor().state().theme.is_dark,
        "the background tab came forward still wearing the dark preset"
    );
}
