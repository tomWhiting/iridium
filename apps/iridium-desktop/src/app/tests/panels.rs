//! The modal surfaces: palette, history, search, the context menu — and the
//! title that follows the document underneath them.
//!
//! Each one holds key focus while it is up, and each one gives it back on
//! `Escape`. These tests are the record of that: a key spent on a panel is a
//! key the document never sees.

use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};
use crate::app::title::title_for;

#[test]
fn the_palette_is_modal_and_escape_gives_the_document_back() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    assert!(app.palette_open, "Ctrl+K opens the palette");

    // Modal: typing goes to the query, not the document.
    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "",
        "the keystroke fed the query"
    );
    assert_eq!(app.palette.query(), "x");

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.palette_open);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(app.test_editor().content(), "y", "the document is back");
}

#[test]
fn the_mac_spellings_reach_the_same_overlays() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    assert!(app.palette_open, "⌘K opens the palette");
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    assert!(!app.palette_open, "⌘K closes it again");

    let meta_alt = Modifiers {
        meta: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        app.press(&chord(KeyCode::Char('h'), meta_alt)),
        Flow::Running
    );
    assert!(app.history_open, "⌘⌥H toggles the undo tree");

    let mut second = DesktopApp::new(Options { path: None }).expect("a session opened");
    assert_eq!(second.press(&meta(KeyCode::Char('f'))), Flow::Running);
    assert!(second.search_open, "⌘F opens the search panel");
}

#[test]
fn a_kernel_command_runs_from_the_palette_and_is_remembered() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "hello");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    type_into(&mut app, "select all");
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert!(!app.palette_open, "running a command closes the palette");

    // The whole document is selected, so typing replaces it.
    type_into(&mut app, "z");
    assert_eq!(app.test_editor().content(), "z");
    assert!(
        !app.mru.is_empty(),
        "the dispatched command was recorded for recency"
    );
}

#[test]
fn a_face_command_runs_from_the_palette() {
    let directory = TempDir::new("desktop-palette-save");
    let (mut app, path) = open(&directory, "a.txt", "one");
    type_into(&mut app, "x");
    assert_eq!(
        app.press(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        Flow::Running
    );
    type_into(&mut app, "save");
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert!(
        std::fs::read_to_string(&path)
            .expect("the file reads back")
            .contains('x'),
        "the palette's Save reached the same save path as the chord"
    );
    assert!(!app.is_dirty());
}

#[test]
fn reopening_the_palette_clears_the_query() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    type_into(&mut app, "fold");
    assert_eq!(app.palette.query(), "fold");
    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    assert_eq!(app.palette.query(), "", "a palette opens aimed at nothing");
}

#[test]
fn a_right_press_opens_the_context_menu_at_the_pointer() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "hello");
    app.pointer.set_position(120.0, 240.0);
    app.secondary_pressed();

    let menu = app.menu.as_ref().expect("the right button opens the menu");
    assert_eq!(
        menu.anchor(),
        (120.0, 240.0),
        "the menu hangs from the click"
    );
    assert_eq!(
        app.test_editor().content(),
        "hello",
        "opening the menu edits nothing"
    );
}

#[test]
fn a_right_press_while_a_modal_panel_is_open_opens_no_menu() {
    // Modal means modal: the press is spent on the panel that is up.
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    app.pointer.set_position(120.0, 240.0);
    app.secondary_pressed();
    assert!(app.menu.is_none(), "no menu opens over the palette");
    assert!(app.palette_open, "and the palette stays up");
}

#[test]
fn the_context_menu_is_modal_and_escape_gives_the_document_back() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    app.pointer.set_position(10.0, 10.0);
    app.secondary_pressed();
    assert!(app.menu.is_some());

    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "",
        "the keystroke was swallowed by the menu"
    );

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(app.menu.is_none(), "Escape closes the menu");
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(app.test_editor().content(), "y", "the document is back");
}

#[test]
fn enter_runs_the_highlighted_menu_verb_through_the_kernel() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "hello");
    app.pointer.set_position(10.0, 10.0);
    app.secondary_pressed();

    // Down to Select All: Cut, Copy, Paste, ——, Select All.
    for _ in 0..3 {
        assert_eq!(app.press(&press(KeyCode::Down)), Flow::Running);
    }
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert!(app.menu.is_none(), "running a verb closes the menu");
    assert!(
        !app.mru.is_empty(),
        "a menu run trains the same recency list a palette run does"
    );

    // The whole document is selected, so typing replaces it.
    type_into(&mut app, "z");
    assert_eq!(app.test_editor().content(), "z");
}

#[test]
fn the_menus_palette_row_opens_the_palette() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    app.pointer.set_position(10.0, 10.0);
    app.secondary_pressed();
    assert_eq!(app.press(&press(KeyCode::End)), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert!(app.menu.is_none());
    assert!(
        app.palette_open,
        "the host command behind the row reached `dispatch_host_command`"
    );
}

#[test]
fn losing_focus_closes_the_menu() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    app.pointer.set_position(10.0, 10.0);
    app.secondary_pressed();
    assert!(app.menu.is_some());
    app.blurred();
    assert!(
        app.menu.is_none(),
        "a menu does not outlive the window's focus"
    );
}

#[test]
fn a_click_outside_the_open_menu_dismisses_it_and_never_reaches_the_document() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "hello");
    app.pointer.set_position(10.0, 10.0);
    app.secondary_pressed();
    assert!(app.menu.is_some());

    app.pointer.set_position(900.0, 900.0);
    assert_eq!(app.pointer_pressed(), Flow::Running);
    assert!(app.menu.is_none(), "the click outside dismissed the menu");
    assert_eq!(
        app.test_editor().content(),
        "hello",
        "the dismissing click never reached the document"
    );
}

#[test]
fn a_click_outside_the_palette_dismisses_it_and_never_reaches_the_document() {
    // D-4: a modal panel swallows the click that dismisses it. Before the
    // context-menu slice the press fell straight through to the document
    // while the palette stayed on screen.
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "hello");
    assert_eq!(app.press(&meta(KeyCode::Char('k'))), Flow::Running);
    assert!(app.palette_open, "⌘K opens the palette");

    app.pointer.set_position(12.0, 34.0);
    app.pointer_pressed();

    assert!(
        !app.palette_open,
        "a click outside the palette dismisses it"
    );
    assert_eq!(
        app.test_editor().content(),
        "hello",
        "the dismissing click never reached the document"
    );
}

#[test]
fn a_click_outside_the_undo_tree_dismisses_it_too() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
    assert!(app.history_open, "Ctrl+Alt+H opens the undo tree");

    app.pointer.set_position(12.0, 34.0);
    app.pointer_pressed();

    assert!(!app.history_open, "a click outside the panel dismisses it");
}

#[test]
fn a_click_while_the_search_panel_is_open_still_reaches_the_document() {
    // The search panel is deliberately *not* modal — keys it does not bind
    // stay the host's — so D-4 does not touch it: a click while it is open
    // belongs to the document, and the panel stays up.
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    assert_eq!(
        app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
        Flow::Running
    );
    assert!(app.search_open);

    app.pointer.set_position(12.0, 34.0);
    app.pointer_pressed();

    assert!(app.search_open, "a click does not dismiss the search panel");
}

#[test]
fn the_history_panel_toggles_and_is_modal() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "a");
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
    assert!(app.history_open, "Ctrl+Alt+H opens the undo tree");

    // Modal: typing must not reach the document while history is open.
    assert_eq!(app.press(&press(KeyCode::Char('x'))), Flow::Running);
    assert_eq!(app.test_editor().content(), "a");

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
    assert!(!app.history_open, "the toggle chord closes it");
}

#[test]
fn jumping_from_the_history_panel_walks_states_and_keeps_the_panel_open() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    // One node per edit, so the rows are predictable.
    app.test_editor_mut()
        .state_mut()
        .history
        .set_group_timeout_ms(0);
    type_into(&mut app, "a");
    type_into(&mut app, "b");
    assert_eq!(app.test_editor().content(), "ab");

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('h'))), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Up)), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Enter)), Flow::Running);
    assert_eq!(
        app.test_editor().content(),
        "a",
        "one row up is one edit back"
    );
    assert!(
        app.history_open,
        "the panel stays open so the * can be watched moving"
    );
    assert!(app.message.is_none(), "a good jump reports nothing");
}

#[test]
fn the_search_panel_opens_finds_and_closes_without_disturbing_the_document() {
    let mut app = DesktopApp::new(Options { path: None }).expect("an empty session opened");
    type_into(&mut app, "alpha beta alpha");
    assert_eq!(
        app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
        Flow::Running
    );
    assert!(app.search_open, "Ctrl+F opens the search panel");

    type_into(&mut app, "alpha");
    assert_eq!(
        app.test_editor().search_match_count(),
        2,
        "typing searches live"
    );
    assert_eq!(
        app.test_editor().content(),
        "alpha beta alpha",
        "the query went to the field, not the document"
    );

    assert_eq!(app.press(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.search_open);
    assert_eq!(
        app.test_editor().search_match_count(),
        0,
        "closing the panel closes the kernel's search"
    );
    assert_eq!(app.test_editor().content(), "alpha beta alpha");
}

#[test]
fn a_save_chord_still_works_while_the_search_panel_is_open() {
    let directory = TempDir::new("desktop-search-save");
    let (mut app, path) = open(&directory, "a.txt", "one");
    type_into(&mut app, "!");
    assert_eq!(
        app.press(&chord(KeyCode::Char('f'), Modifiers::ctrl())),
        Flow::Running
    );
    assert_eq!(app.press(&ctrl_s()), Flow::Running);
    assert!(
        std::fs::read_to_string(&path)
            .expect("the file reads back")
            .contains('!'),
        "the panel left the save chord to the host"
    );
    assert!(app.search_open, "saving did not close the search");
}

#[test]
fn the_language_is_read_from_the_extension_as_every_face_reads_it() {
    use std::path::Path;

    use iridium_editor::Language;

    use crate::app::files::language_of;

    assert_eq!(language_of(Path::new("main.rs")), Some(Language::Rust));
    assert_eq!(language_of(Path::new("a/b/mod.RS")), Some(Language::Rust));
    assert_eq!(
        language_of(Path::new("index.ts")),
        Some(Language::TypeScript)
    );
    assert_eq!(language_of(Path::new("notes.md")), Some(Language::Markdown));
    assert_eq!(
        language_of(Path::new("unclaimed.xyz")),
        None,
        "an extension no grammar claims is plain text"
    );
    assert_eq!(
        language_of(Path::new("Makefile")),
        None,
        "a name without an extension is plain text"
    );
}

#[test]
fn opening_a_rust_file_primes_the_highlight_cache() {
    let directory = TempDir::new("desktop-syntax");
    let (mut app, _path) = open(&directory, "a.rs", "fn main() {}\n");
    assert_eq!(
        app.test_editor().language(),
        Some(iridium_editor::Language::Rust),
        "the session set the language from the name"
    );
    app.test_refresh_syntax();
    assert_eq!(
        app.test_document().syntax.rebuilds(),
        1,
        "the parsed document yields spans on the first refresh"
    );
    app.test_refresh_syntax();
    assert_eq!(
        app.test_document().syntax.rebuilds(),
        1,
        "an idle frame rebuilds nothing"
    );

    type_into(&mut app, "x");
    app.test_refresh_syntax();
    assert_eq!(
        app.test_document().syntax.rebuilds(),
        2,
        "a keystroke's reparse rebuilds once"
    );
}

#[test]
fn the_title_names_the_document_and_its_dirty_state() {
    assert_eq!(title_for(None, false), "[No Name] — iridium");
    assert_eq!(title_for(None, true), "[No Name] [+] — iridium");
    assert_eq!(title_for(Some("a.rs"), false), "a.rs — iridium");
    assert_eq!(title_for(Some("a.rs"), true), "a.rs [+] — iridium");
}

#[test]
fn a_dirty_buffer_becomes_clean_again_when_undone_to_its_saved_state() {
    // Content-based dirtiness, not a revision counter: undoing the only
    // edit makes the document what the disk holds, so it is clean.
    let directory = TempDir::new("desktop-dirty-undo");
    let (mut app, _path) = open(&directory, "a.txt", "one");
    assert!(!app.is_dirty());
    type_into(&mut app, "x");
    assert!(app.is_dirty());
    assert_eq!(
        app.press(&chord(KeyCode::Char('z'), Modifiers::ctrl())),
        Flow::Running
    );
    assert!(!app.is_dirty(), "an undone edit leaves a clean buffer");
}
