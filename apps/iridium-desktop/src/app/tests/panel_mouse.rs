//! The mouse inside a panel: clicking a row, scrolling a list, and the
//! highlight that follows the pointer.
//!
//! Tom, 12 Aug 2026, on the shipped sidebar:
//!
//! > "I can't select anything or navigate really any of the menus — so the
//! > sidebar, or any of those things — with the mouse. Either clicking or
//! > scrolling."
//!
//! ⭐ **Every test here resolves against a stated painted frame**, exactly as
//! the shipped ladder does. The record is the app's rather than the GPU
//! painter's for this reason: what a press means is a question about the
//! frame on screen, and a question that can only be asked of a live device is
//! a question nobody asks.

use std::fmt::Write as _;

use iridium_editor::{KeyCode, Modifiers};
use iridium_file::TextFile;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, ExplorerFocus, ExplorerPlacement, Flow};
use crate::overlay::{PanelFit, PanelKind};

/// A project holding two files, so the panel lists rows the tests can aim at.
///
/// Named so they sort in a fixed order — the rows are `>` query, the root,
/// `alpha.txt`, `beta.txt` — because a test that clicked "some row" and
/// asserted "some file opened" would pass with the routing wired to the wrong
/// index.
fn project(name: &str) -> TempDir {
    let directory = TempDir::new(name);
    std::fs::write(directory.path().join("alpha.txt"), "the first file")
        .expect("the fixture file was written");
    std::fs::write(directory.path().join("beta.txt"), "the second file")
        .expect("the fixture file was written");
    directory
}

/// A session opened on `directory`, its explorer settled and placed as a
/// sidebar, with a painted frame stating where that sidebar landed.
fn sidebar_session(
    directory: &TempDir,
    rows: usize,
) -> (DesktopApp, crate::overlay::PanelGeometry) {
    let mut app = DesktopApp::new(Options {
        path: Some(directory.path().to_path_buf()),
        ..Options::default()
    })
    .expect("a session opened on the directory");
    settle(&mut app);
    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    assert_eq!(app.explorer_placement, ExplorerPlacement::Sidebar);
    let geometry = placed(0.0, 30.0, rows, 24);
    painted(&mut app, &[(PanelKind::Explorer, geometry)]);
    // ⚠️ **The frame that always precedes a click.** The shipped app composes
    // every panel once per frame, so by the time a pointer can be over one it
    // has composed at least once — and composition is where the window learns
    // which selection it is already following. A fixture that clicked before
    // any frame would exercise the first-composition path, which is a state
    // no user can be in.
    compose(&mut app, rows);
    (app, geometry)
}

/// Composes the open explorer once, exactly as a frame would.
fn compose(app: &mut DesktopApp, rows: usize) {
    let theme = app.test_editor().state().theme.clone();
    let fit = PanelFit::sidebar(24, rows);
    app.explorer
        .as_mut()
        .expect("an explorer to compose")
        .content(&theme, fit);
}

/// Polls the open explorer until its root listing has landed.
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

/// The label of the tab in front.
fn front_tab(app: &DesktopApp) -> Option<String> {
    app.test_document()
        .file
        .as_ref()
        .map(TextFile::display_name)
}

/// ⭐ **The defect, stated.** Composed row 3 of the sidebar is `beta.txt`; a
/// press on it must open `beta.txt`, which is what `Enter` on that row already
/// does. Before this landed the press was swallowed by the modal guard and
/// reached nothing at all.
#[test]
fn a_press_on_a_sidebar_row_opens_the_file_it_names() {
    let directory = project("desktop-panel-mouse-open");
    let (mut app, geometry) = sidebar_session(&directory, 4);

    point_at(&mut app, row_center(geometry, 3));
    assert_eq!(app.pointer_pressed(), Flow::Running);

    assert_eq!(
        front_tab(&app).as_deref(),
        Some("beta.txt"),
        "the row under the pointer is the file that opened"
    );
}

/// The row the pointer landed on becomes the selection, so the keyboard
/// carries on from where the mouse left off rather than from wherever it was.
#[test]
fn a_press_moves_the_selection_to_the_row_it_landed_on() {
    let directory = project("desktop-panel-mouse-select");
    let (mut app, geometry) = sidebar_session(&directory, 4);

    // Row 2 is `alpha.txt`; pressing it opens that file and leaves the
    // selection there, so `Enter` afterwards opens the same one.
    point_at(&mut app, row_center(geometry, 2));
    assert_eq!(app.pointer_pressed(), Flow::Running);
    assert_eq!(front_tab(&app).as_deref(), Some("alpha.txt"));

    let selected = app
        .explorer
        .as_ref()
        .and_then(|explorer| explorer.selected_path().map(std::path::Path::to_path_buf))
        .expect("a row is selected");
    assert_eq!(
        selected.file_name().and_then(std::ffi::OsStr::to_str),
        Some("alpha.txt"),
        "the click left the selection on the row it acted on"
    );
}

/// A press on the sidebar is also a statement about where the keys go. Without
/// this the row opens and the next arrow key still walks the document.
#[test]
fn a_press_on_the_sidebar_gives_it_the_keys() {
    let directory = project("desktop-panel-mouse-focus");
    let (mut app, geometry) = sidebar_session(&directory, 4);
    app.explorer_focus = ExplorerFocus::Document;

    point_at(&mut app, row_center(geometry, 1));
    assert_eq!(app.pointer_pressed(), Flow::Running);

    assert_eq!(
        app.explorer_focus,
        ExplorerFocus::Panel,
        "clicking a sidebar is how you start driving it"
    );
}

/// ⚠️ **The wheel's half of the defect.** `wheel` scrolled the document
/// whatever the pointer was over, so spinning the wheel on a full-height
/// sidebar scrolled the text behind it.
#[test]
fn the_wheel_over_a_panel_leaves_the_document_where_it_was() {
    let directory = TempDir::new("desktop-panel-mouse-wheel");
    let path = directory.path().join("long.txt");
    let text = (0..400).fold(String::new(), |mut text, index| {
        let _ = writeln!(text, "line {index}");
        text
    });
    std::fs::write(&path, &text).expect("the fixture file was written");

    let mut app = DesktopApp::new(Options {
        path: Some(path),
        ..Options::default()
    })
    .expect("a session opened on the file");
    assert_eq!(app.press(&meta(KeyCode::Char('b'))), Flow::Running);
    settle(&mut app);
    let geometry = placed(0.0, 30.0, 6, 24);
    painted(&mut app, &[(PanelKind::Explorer, geometry)]);
    compose(&mut app, 6);

    let before = app.test_document().scroll_y;
    point_at(&mut app, row_center(geometry, 2));
    app.wheel(&wheel_down());

    assert!(
        (app.test_document().scroll_y - before).abs() < f32::EPSILON,
        "the document must not move for a wheel spent on the panel over it"
    );
}

/// The other half: the wheel has to do something, and what it does is move the
/// panel's own window.
#[test]
fn the_wheel_over_a_panel_scrolls_that_panels_list() {
    let directory = TempDir::new("desktop-panel-mouse-wheel-list");
    for index in 0..40 {
        std::fs::write(
            directory.path().join(format!("file-{index:02}.txt")),
            "content",
        )
        .expect("the fixture file was written");
    }
    let (mut app, geometry) = sidebar_session(&directory, 6);

    point_at(&mut app, row_center(geometry, 2));
    app.wheel(&wheel_down());

    assert!(
        app.explorer
            .as_ref()
            .is_some_and(|explorer| explorer.scroll_position() > 0),
        "the wheel moved the panel's window towards the end of its list"
    );
}

/// ⚠️ **A browsed list's window must not be dragged back by the selection.**
/// The window follows the selection when the selection *moves*; a wheel moves
/// the window and nothing else, so the next composition has to leave it alone.
/// Without this the scroll is undone by the very next frame.
#[test]
fn a_wheeled_window_survives_the_next_composition() {
    let directory = TempDir::new("desktop-panel-mouse-wheel-sticks");
    for index in 0..40 {
        std::fs::write(
            directory.path().join(format!("file-{index:02}.txt")),
            "content",
        )
        .expect("the fixture file was written");
    }
    let (mut app, geometry) = sidebar_session(&directory, 6);

    point_at(&mut app, row_center(geometry, 2));
    app.wheel(&wheel_down());
    let after_wheel = app
        .explorer
        .as_ref()
        .map(crate::file_tree::FileExplorer::scroll_position)
        .expect("an explorer");
    assert!(after_wheel > 0, "the wheel moved the window");

    // Composed exactly as the frame would: composition is where the window
    // follows the selection, so this is the moment a naive `follow_selection`
    // undoes the wheel.
    compose(&mut app, 6);

    assert_eq!(
        app.explorer
            .as_ref()
            .map(crate::file_tree::FileExplorer::scroll_position),
        Some(after_wheel),
        "composing the panel again must not drag the window back to the selection"
    );
}

/// The pointer resting on a row marks it for the painter — and marks it on the
/// panel it is actually over, not on whichever panel holds that row number.
#[test]
fn the_hovered_row_is_recorded_against_the_panel_under_the_pointer() {
    let directory = project("desktop-panel-mouse-hover");
    let (mut app, geometry) = sidebar_session(&directory, 4);

    app.pointer_moved(row_center(geometry, 2).0, row_center(geometry, 2).1);
    assert_eq!(app.hover, Some((PanelKind::Explorer, 2)));

    // Off the panel entirely: nothing is hovered, so the band goes.
    app.pointer_moved(600.0, 400.0);
    assert_eq!(app.hover, None, "the highlight leaves with the pointer");
}

/// A press on a palette row runs that row's command, which is what `Enter`
/// does to the selection.
#[test]
fn a_press_on_a_palette_row_runs_that_command() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    type_into(&mut app, "hello");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    type_into(&mut app, "select all");
    // Row 0 is the query; row 1 is the first result, which is what `Enter`
    // would take.
    let geometry = placed(40.0, 60.0, 6, 40);
    painted(&mut app, &[(PanelKind::Palette, geometry)]);
    point_at(&mut app, row_center(geometry, 1));
    assert_eq!(app.pointer_pressed(), Flow::Running);

    assert!(!app.palette_open, "running a command closes the palette");
    type_into(&mut app, "z");
    assert_eq!(
        app.test_editor().content(),
        "z",
        "Select All ran, so typing replaced the document"
    );
}

/// A press on the padding below the last row is not a press on a row, and must
/// leave the panel exactly as it was rather than running the nearest thing.
#[test]
fn a_press_on_a_panels_padding_does_nothing_and_keeps_the_panel() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    // A panel geometry with room for six rows, pressed inside its bottom
    // padding: on the panel, off every row.
    let geometry = placed(40.0, 60.0, 6, 40);
    painted(&mut app, &[(PanelKind::Palette, geometry)]);
    let bottom = geometry.exterior.y + geometry.exterior.height - 1.0;
    point_at(&mut app, (geometry.content_x + 1.0, bottom));
    assert_eq!(app.pointer_pressed(), Flow::Running);

    assert!(
        app.palette_open,
        "a press on the panel's own padding leaves it up"
    );
}

/// The regression the routing must not undo: a press *outside* every panel
/// still dismisses the modal one, and still never reaches the document.
#[test]
fn a_press_outside_a_modal_panel_still_dismisses_it() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    let geometry = placed(40.0, 60.0, 6, 40);
    painted(&mut app, &[(PanelKind::Palette, geometry)]);
    point_at(&mut app, (600.0, 400.0));
    assert_eq!(app.pointer_pressed(), Flow::Running);

    assert!(!app.palette_open, "a press off the panel dismisses it");
    assert_eq!(
        app.test_editor().content(),
        "",
        "and the document never saw the press that dismissed it"
    );
}

/// A press on a menu opened *over* another panel reaches the menu. The record
/// is read topmost-first, and this is the case that proves it.
#[test]
fn a_menu_over_a_panel_takes_the_press() {
    let directory = project("desktop-panel-mouse-stacked");
    let (mut app, sidebar) = sidebar_session(&directory, 4);
    let over = placed(0.0, 30.0, 4, 24);
    painted(
        &mut app,
        &[(PanelKind::Explorer, sidebar), (PanelKind::Menu, over)],
    );

    let (x, y) = row_center(over, 1);
    assert_eq!(
        app.painted.hit(x, y).map(|hit| (hit.kind, hit.geometry)),
        Some((PanelKind::Menu, over)),
        "the panel drawn last owns the pixels the two share"
    );
}
