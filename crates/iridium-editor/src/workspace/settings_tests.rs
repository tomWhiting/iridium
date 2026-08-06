//! Tests that the window's geometry reaches *every* tab's viewport.
//!
//! Nothing paints from the kernel's viewport — every face drives its own
//! scroll offset — so a stale one is invisible right up until someone presses
//! `cursor.pageDown`, which hops by `visible_lines`. That makes this the
//! quietest of the synchronised settings: a wrong theme is seen immediately,
//! a wrong viewport is seen as "page down jumped too far, once".
//!
//! The two directions are tested separately because they fail for opposite
//! reasons. A tab open *before* the resize goes stale if the setter only
//! touches the active editor; a tab opened *after* it starts at
//! [`Viewport::default`](crate::render::Viewport) if the workspace does not
//! remember the geometry for the editors it has yet to build.

use super::{Node, Workspace};
use crate::editor::EditorConfig;
use crate::theme::Theme;

/// A window twenty rows tall: `600 / 30`.
const LINE_HEIGHT: f32 = 30.0;
const WIDTH: f32 = 1000.0;
const HEIGHT: f32 = 600.0;
const ROWS: usize = 20;

fn workspace() -> Workspace {
    Workspace::new(EditorConfig::default(), Theme::default())
}

/// The rows `cursor.pageUp` and `cursor.pageDown` would hop by on `tab`.
fn visible_lines(workspace: &Workspace, tab: super::NodeId) -> Option<usize> {
    let document = workspace.node(tab).and_then(Node::document)?;
    Some(workspace.editor(document)?.state().viewport.visible_lines)
}

#[test]
fn a_resize_reaches_a_tab_that_is_not_the_active_one() {
    // The failure this pins: sizing only `active_editor_mut` leaves every
    // background tab paging by whatever the window was when it was last in
    // front. With one tab open the two are the same editor, which is why
    // this needs two.
    let mut workspace = workspace();
    let first = workspace.open("a", "a.rs", None).expect("top level");
    let second = workspace.open("b", "b.rs", None).expect("top level");
    assert!(workspace.activate(second), "the second tab is active");

    workspace.set_viewport(LINE_HEIGHT, WIDTH, HEIGHT);

    assert_eq!(visible_lines(&workspace, second), Some(ROWS));
    assert_eq!(
        visible_lines(&workspace, first),
        Some(ROWS),
        "the background tab must page by the window it is in, not the one it was opened in"
    );
}

#[test]
fn a_tab_opened_after_a_resize_inherits_the_window() {
    // The other direction, and the argument `set_theme` already makes: a
    // workspace that applied the geometry and forgot it would give every
    // later tab the kernel's default viewport, which describes no window
    // anyone is looking at.
    let mut workspace = workspace();
    workspace.set_viewport(LINE_HEIGHT, WIDTH, HEIGHT);

    let tab = workspace.open("a", "a.rs", None).expect("top level");

    assert_eq!(visible_lines(&workspace, tab), Some(ROWS));
}

#[test]
fn the_geometry_is_readable_and_absent_until_a_face_reports_one() {
    // `None` and "800x600" are different facts: a headless session never
    // reports a window, and a default pair of dimensions would claim it did.
    let mut workspace = workspace();
    assert_eq!(workspace.viewport(), None);

    workspace.set_viewport(LINE_HEIGHT, WIDTH, HEIGHT);
    let geometry = workspace.viewport().expect("the face reported one");

    assert!((geometry.line_height - LINE_HEIGHT).abs() < f32::EPSILON);
    assert!((geometry.width - WIDTH).abs() < f32::EPSILON);
    assert!((geometry.height - HEIGHT).abs() < f32::EPSILON);
}

#[test]
fn a_resize_leaves_each_tab_scrolled_where_it_was() {
    // Only the geometry belongs to the window. Sharing a whole `Viewport`
    // would carry `first_line` across with it, and every background tab
    // would jump to wherever the active one happened to be looking.
    let mut workspace = workspace();
    let first = workspace
        .open("one\ntwo\nthree\nfour\nfive", "a.rs", None)
        .expect("top level");
    let second = workspace
        .open("one\ntwo\nthree\nfour\nfive", "b.rs", None)
        .expect("top level");

    let first_document = workspace
        .node(first)
        .and_then(Node::document)
        .expect("a tab");
    let second_document = workspace
        .node(second)
        .and_then(Node::document)
        .expect("a tab");
    workspace
        .editor_mut(first_document)
        .expect("open")
        .state_mut()
        .viewport
        .scroll_to_line(3);

    workspace.set_viewport(LINE_HEIGHT, WIDTH, HEIGHT);

    assert_eq!(
        workspace
            .editor(first_document)
            .map(|editor| editor.state().viewport.first_line),
        Some(3),
        "a resize must not move the tab that was scrolled"
    );
    assert_eq!(
        workspace
            .editor(second_document)
            .map(|editor| editor.state().viewport.first_line),
        Some(0),
        "nor drag another tab's scroll position onto it"
    );
}

#[test]
fn a_zero_height_window_leaves_a_viewport_that_pages_by_nothing() {
    // Minimised windows and a terminal whose chrome ate every row both
    // arrive here. `visible_lines` of zero is the honest answer, and the
    // kernel's `scroll_to_position` is already written against it — what
    // must not happen is a panic on the way in.
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");

    workspace.set_viewport(LINE_HEIGHT, 0.0, 0.0);

    assert_eq!(visible_lines(&workspace, tab), Some(0));
}
