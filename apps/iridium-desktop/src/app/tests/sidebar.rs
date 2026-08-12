//! The explorer's two placements, and the one thing that really separates
//! them: whether the document is reachable while the panel is on screen.
//!
//! The geometry half — that a sidebar's fit fills a band a popover's does not —
//! is proven in [`crate::overlay`], against a window these tests do not have.
//! What is proven here is the *routing*, which is where a sidebar stops being a
//! taller popover: keys, `Escape`, and the placement outliving the panel.

use std::path::Path;

use iridium_editor::KeyCode;
use iridium_editor::commands::builtin::EXPLORER_TOGGLE_SIDEBAR;
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

/// Polls the open explorer until its root listing has landed.
///
/// Panics on the deadline rather than carrying on: a test that went ahead with
/// an empty tree would exercise the wrong path and pass for the wrong reason.
fn settle(app: &mut DesktopApp) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let explorer = app.explorer.as_mut().expect("a panel to settle");
    while std::time::Instant::now() < deadline {
        if explorer.poll() && !explorer.is_waiting() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("the root listing never arrived");
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

/// ⭐ **The bug Tom found: `⌘B` twice must put the explorer away.**
///
/// It used to name a *transition* — "move to the other placement" — so the
/// second press dropped the panel back into the middle of the window rather
/// than dismissing it. Tom, 12 Aug 2026: *"when you command-B a second time, it
/// just alternates between that and the central version … that's an issue"*.
/// The key now names a **state**, which is the only kind of toggle a hand can
/// count on without counting.
#[test]
fn the_sidebar_chord_twice_puts_the_explorer_away() {
    let directory = project("desktop-sidebar-twice");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
    assert!(app.explorer.is_some(), "the first press put a sidebar up");

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert!(
        app.explorer.is_none(),
        "the second press must put it away, not move it to the middle"
    );

    // And the floating panel is still reachable, which is what keeps both
    // placements on two keys instead of three.
    assert_eq!(
        app.explorer_placement,
        ExplorerPlacement::Popover,
        "putting the sidebar away gives the explorer back to ⌘⌥E"
    );
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('e'))), Flow::Running);
    assert!(app.explorer.is_some());
    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(
        app.explorer.is_none(),
        "and it is a popover again, so escape closes it"
    );
}

/// A floating panel is *moved* rather than closed, because the key names the
/// sidebar and there is no sidebar yet to put away.
#[test]
fn the_sidebar_chord_moves_a_floating_panel_rather_than_closing_it() {
    let directory = project("desktop-sidebar-moves");
    let mut app = opened_on(directory.path());
    assert_eq!(app.explorer_placement, ExplorerPlacement::Popover);
    assert!(app.explorer.is_some(), "opening on a directory puts one up");

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
    assert!(
        app.explorer.is_some(),
        "the panel itself did not go anywhere"
    );
}

/// ⚠️ **The placement may not move ahead of a close that was refused.**
///
/// Closing is refused while the rows are being edited as text and the edits
/// have not been applied. A placement given back at that moment would leave a
/// sidebar drawn on screen while the reserved band, the hit test and the key
/// routing had all been told it was a popover.
#[test]
fn a_refused_close_leaves_the_sidebar_a_sidebar() {
    let directory = project("desktop-sidebar-refused");
    let mut app = opened_on(directory.path());

    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);

    // The rows arrive on a reader thread, and `Tab` needs one to edit. The
    // paint path polls once a frame; there are no frames here, so this is that
    // poll — same deadline-and-panic shape the file-tree suite uses.
    settle(&mut app);

    // Into the oil buffer, then one keystroke into a filename.
    assert_eq!(app.press(&press(KeyCode::Tab)), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('z'))), Flow::Running);
    // ⭐ The precondition, asserted rather than assumed: a fixture that never
    // reached the state under test would pass this whole test for the wrong
    // reason.
    assert!(
        app.explorer
            .as_ref()
            .is_some_and(crate::file_tree::FileExplorer::has_unapplied_edits),
        "the fixture must actually be holding unapplied edits"
    );

    // ⚠️ Run rather than pressed, and the difference is the point. Edit mode
    // has its own key table ending in a catch-all, so `⌘B` is swallowed there
    // and never reaches this command — a keypress here would assert nothing
    // about the refusal, because no close would have been attempted. The
    // palette and an unfocused sidebar both reach it this way.
    assert_eq!(
        app.run_host_command(&EXPLORER_TOGGLE_SIDEBAR),
        Flow::Running
    );
    assert!(app.explorer.is_some(), "the close was refused");
    assert_eq!(
        app.explorer_placement,
        ExplorerPlacement::Sidebar,
        "so the panel is still a sidebar, and everything downstream must agree"
    );
    assert!(app.message.is_some(), "and the refusal was said out loud");
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
