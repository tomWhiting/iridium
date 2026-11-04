//! The explorer's listing edited as text, in this face: the keys that reach
//! it and the search panel aimed at it. The marks drawn over it are
//! [`crate::overlay`]'s, and are tested there.

use std::path::Path;

use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow, SearchTarget};

/// A session opened on a directory holding `alpha.txt` and `beta.txt`, with
/// the root listing landed.
fn project(name: &str) -> (TempDir, DesktopApp) {
    let directory = TempDir::new(name);
    std::fs::write(directory.path().join("alpha.txt"), "a").expect("the fixture was written");
    std::fs::write(directory.path().join("beta.txt"), "b").expect("the fixture was written");
    let app = opened_on(directory.path());
    (directory, settled(app))
}

/// A session opened the way the command line opens one.
fn opened_on(path: &Path) -> DesktopApp {
    DesktopApp::new(Options {
        path: Some(path.to_path_buf()),
        ..Options::default()
    })
    .expect("a session opened on the directory")
}

/// Polls the open explorer until its root listing has landed.
fn settled(mut app: DesktopApp) -> DesktopApp {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let explorer = app.explorer.as_mut().expect("a panel to settle");
    while std::time::Instant::now() < deadline {
        if explorer.poll() && !explorer.is_waiting() {
            return app;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("the root listing never arrived");
}

/// The listing's lines while it is being edited.
fn listing(app: &DesktopApp) -> Vec<String> {
    let editor = app
        .explorer
        .as_ref()
        .and_then(|explorer| explorer.buffer_editor())
        .expect("the listing is being edited");
    let document = &editor.state().document;
    (0..document.line_count())
        .map(|line| document.line(line).unwrap_or_default())
        .collect()
}

#[test]
fn command_s_in_the_listing_asks_and_y_renames() {
    let (directory, mut app) = project("desktop-explorer-text-apply");
    assert_eq!(app.press(&press(KeyCode::Tab)), Flow::Running);
    type_into(&mut app, "2");
    assert_eq!(listing(&app)[0], "alpha.txt2");

    assert_eq!(app.press(&meta(KeyCode::Char('s'))), Flow::Running);
    assert!(
        directory.path().join("alpha.txt").exists(),
        "⌘S applied without asking"
    );
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert!(
        directory.path().join("alpha.txt2").exists(),
        "y did not rename"
    );
}

#[test]
fn enter_in_the_listing_makes_a_new_line_and_keeps_the_document_untouched() {
    let (_directory, mut app) = project("desktop-explorer-text-enter");
    app.press(&press(KeyCode::Tab));
    app.press(&press(KeyCode::Enter));
    type_into(&mut app, "gamma.txt");
    assert_eq!(listing(&app), ["alpha.txt", "gamma.txt", "beta.txt"]);
    assert_eq!(
        app.test_editor().content(),
        "",
        "the keys reached the document"
    );
}

#[test]
fn command_f_in_the_listing_searches_the_listing_not_the_document() {
    let (_directory, mut app) = project("desktop-explorer-text-search");
    app.press(&press(KeyCode::Tab));
    assert_eq!(app.press(&meta(KeyCode::Char('f'))), Flow::Running);
    assert!(
        app.search_open && app.search_target == SearchTarget::Explorer,
        "⌘F did not search the listing"
    );

    type_into(&mut app, "beta");
    assert_eq!(
        listing(&app),
        ["alpha.txt", "beta.txt"],
        "the query edited the listing"
    );
    let matches = app
        .explorer
        .as_ref()
        .and_then(|explorer| explorer.buffer_editor())
        .map(|editor| editor.search_state().all_matches().len());
    assert_eq!(matches, Some(1), "the listing was not searched");

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.search_open, "escape left the search open");
    assert!(
        app.explorer
            .as_ref()
            .is_some_and(|explorer| explorer.buffer_editor().is_some()),
        "closing the search left the listing too"
    );
}

#[test]
fn a_search_aimed_at_the_listing_closes_when_the_listing_does() {
    let (_directory, mut app) = project("desktop-explorer-text-orphan");
    app.press(&press(KeyCode::Tab));
    app.press(&meta(KeyCode::Char('f')));
    assert!(app.search_open && app.search_target == SearchTarget::Explorer);

    // However the panel went, the next key finds no search aimed at it.
    app.explorer = None;
    app.press(&press(KeyCode::Char('z')));
    assert!(
        !app.search_open,
        "a search outlived the listing it searched"
    );
    assert_eq!(app.search_target, SearchTarget::Document);
    assert_eq!(
        app.test_editor().content(),
        "z",
        "the document did not get its keys back"
    );
}
