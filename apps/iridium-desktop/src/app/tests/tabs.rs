//! Tabs, and the strip that shows them.
//!
//! Opening a second document must leave the first one alone — that is the
//! assertion the whole workspace conversion exists for — and the strip must
//! describe what is actually open, dirty marks included.

use iridium_editor::workspace::Node;
use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::state::Flow;
use crate::app::title::title_for;
use crate::tab_strip::TabHit;

// =========================================================================
// Tabs
// =========================================================================

/// `⌘N` makes a blank document beside what is open, and asks nothing.
///
/// Genuinely absent until 12 Aug 2026: an untitled buffer appeared only when
/// the last tab closed, so there was no way to start writing something new
/// without first finding a file to open. Additive, so there is nothing to
/// confirm — the same change of stakes that let a drop stop asking.
#[test]
fn the_new_file_chord_opens_a_blank_tab_beside_what_is_open() {
    let directory = TempDir::new("desktop-new-file");
    let (mut app, _path) = open(&directory, "a.txt", "first");
    type_into(&mut app, "x");

    assert_eq!(app.press(&meta(KeyCode::Char('n'))), Flow::Running);

    assert!(app.prompt.is_none(), "an additive verb asks nothing");
    assert_eq!(app.workspace.tab_count(), 2);
    assert_eq!(app.test_editor().content(), "");
    assert_eq!(front(&app), None, "a new file has no file yet");
    assert!(
        !app.is_dirty(),
        "an empty buffer that was never on disk is clean"
    );

    // And the edited one is still there, still edited, one chord away.
    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.test_editor().content(), "xfirst");
}

/// The blank tab a new file makes is the same one the session falls back to.
///
/// Two constructions would be two ideas of what a blank document *is* — its
/// label above all — free to disagree the moment either moved. This asserts
/// they agree by comparing the labels rather than by reading the code.
#[test]
fn a_made_blank_tab_and_a_fallback_blank_tab_are_the_same_thing() {
    let directory = TempDir::new("desktop-new-file-label");
    let (mut app, _path) = open(&directory, "a.txt", "first");

    assert_eq!(app.press(&meta(KeyCode::Char('n'))), Flow::Running);
    let made = app
        .workspace
        .active()
        .and_then(|tab| app.workspace.node(tab))
        .map(|node| node.label().to_owned());

    // Close both tabs, which drives the session onto its fallback.
    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(app.workspace.tab_count(), 1);
    let fallback = app
        .workspace
        .active()
        .and_then(|tab| app.workspace.node(tab))
        .map(|node| node.label().to_owned());

    assert_eq!(
        made, fallback,
        "the two blank documents are labelled differently"
    );
    assert!(made.is_some());
}

/// A drop opens a *second* tab and leaves the first one alone.
///
/// This is the assertion the whole workspace conversion exists for. Under
/// the single-buffer shell the dropped file overwrote what was open; the
/// old test asserted the replacement, which is the behaviour that is now
/// wrong.
#[test]
fn a_drop_adds_a_tab_rather_than_replacing_the_one_in_front() {
    let directory = TempDir::new("desktop-drop-adds");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    let second = fixture(&directory, "b.txt", "second");

    app.dropped(&second);

    assert!(app.prompt.is_none(), "an additive open asks nothing");
    assert_eq!(app.workspace.tab_count(), 2);
    assert_eq!(app.test_editor().content(), "second");
    assert_eq!(front(&app), Some("b.txt".to_owned()));

    // And the first is still there, unchanged, one chord away.
    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.test_editor().content(), "first");
    assert_eq!(front(&app), Some("a.txt".to_owned()));
}

/// A drop over unsaved work asks nothing, because it discards nothing.
///
/// The prompt that used to guard this is gone rather than changed: the
/// stake it protected against — losing the edit — cannot arise when the
/// edit stays in its own tab.
#[test]
fn a_drop_over_unsaved_changes_keeps_the_work_in_its_own_tab() {
    let directory = TempDir::new("desktop-drop-dirty");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    let second = fixture(&directory, "b.txt", "second");
    type_into(&mut app, "x");
    let unsaved = app.test_editor().content();

    app.dropped(&second);

    assert!(app.prompt.is_none(), "there is nothing left to ask about");
    assert!(
        !app.is_dirty(),
        "the tab in front is the freshly opened file"
    );

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.test_editor().content(), unsaved, "the edit survived");
    assert!(app.is_dirty(), "and is still unsaved, in its own tab");
}

/// Each tab keeps its own scroll offset across a switch.
///
/// `scroll_y` is face state, not kernel state, and it was a single field
/// on the window until this change. Two tabs sharing one would open the
/// second file already scrolled to an offset that meant something in the
/// first, and scroll the first back to the top on the way home.
#[test]
fn each_tab_keeps_its_own_scroll_offset() {
    let directory = TempDir::new("desktop-tab-scroll");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.test_document_mut().scroll_y = 40.0;

    app.dropped(&fixture(&directory, "b.txt", "second"));

    assert!(
        app.test_document().scroll_y.abs() < f32::EPSILON,
        "a new tab opens at the top"
    );

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert!(
        (app.test_document().scroll_y - 40.0).abs() < f32::EPSILON,
        "and the first tab is where it was left"
    );
}

/// Undo in one tab cannot reach into another's document.
///
/// Under the single-buffer shell this was a claim about
/// `Editor::set_content` replacing the undo tree. It is now a claim about
/// there being two trees — one per buffer — which is the stronger form of
/// the same guarantee, and the one that has to hold once tabs exist.
#[test]
fn undo_in_one_tab_cannot_restore_another_tabs_document() {
    let directory = TempDir::new("desktop-tab-undo");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    type_into(&mut app, "x");
    let other = app.test_editor().content();

    app.dropped(&fixture(&directory, "b.txt", "second"));
    assert_eq!(
        app.press(&chord(KeyCode::Char('z'), Modifiers::ctrl())),
        Flow::Running
    );

    assert_eq!(app.test_editor().content(), "second");
    assert_ne!(app.test_editor().content(), other);
    assert!(!app.is_dirty());
}

/// A drop of a path that does not exist opens an empty tab named for it —
/// the same thing the command line does, deliberately.
///
/// `TextFile::open` maps `NotFound` to a new empty file at that path
/// (`iridium-file`), which is what makes `iridium newfile.txt` work. The
/// drop path does **not** special-case it, and that is the point: two
/// different answers to "what does opening a missing file mean" is the
/// per-caller divergence this estate keeps paying for elsewhere. One
/// rule, one place.
#[test]
fn a_drop_of_a_missing_path_follows_the_shared_open_rule() {
    let directory = TempDir::new("desktop-drop-missing");
    let (mut app, _first) = open(&directory, "a.txt", "first");

    app.dropped(&directory.path().join("does-not-exist.txt"));

    assert_eq!(app.test_editor().content(), "");
    assert_eq!(
        front(&app),
        Some("does-not-exist.txt".to_owned()),
        "the buffer is named for the file it will create"
    );
}

/// The bracket chords walk the strip, and stop at its ends rather than
/// wrapping.
#[test]
fn the_bracket_chords_walk_the_strip_and_clamp_at_its_ends() {
    let directory = TempDir::new("desktop-tab-walk");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    app.dropped(&fixture(&directory, "c.txt", "third"));
    assert_eq!(app.workspace.tab_count(), 3);
    assert_eq!(front(&app), Some("c.txt".to_owned()));

    // Already at the last: next does nothing, and says nothing.
    assert_eq!(app.press(&ctrl_shift(KeyCode::Char(']'))), Flow::Running);
    assert_eq!(front(&app), Some("c.txt".to_owned()));
    assert!(
        app.message.is_none(),
        "reaching the end of the strip is an outcome, not an error"
    );

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(front(&app), Some("b.txt".to_owned()));
    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(front(&app), Some("a.txt".to_owned()));

    // And clamps at the first.
    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(front(&app), Some("a.txt".to_owned()));
    assert!(app.message.is_none());
}

/// The window title follows the tab in front.
#[test]
fn the_title_follows_the_tab_in_front() {
    let directory = TempDir::new("desktop-tab-title");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));

    assert_eq!(app.title, title_for(Some("b.txt"), false));

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.title, title_for(Some("a.txt"), false));
}

/// Closing a clean tab asks nothing and leaves the neighbour in front.
#[test]
fn closing_a_clean_tab_just_closes_it() {
    let directory = TempDir::new("desktop-close-clean");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));

    assert_eq!(app.press(&ctrl_w()), Flow::Running);

    assert!(app.prompt.is_none(), "a clean tab has nothing to ask");
    assert_eq!(app.workspace.tab_count(), 1);
    assert_eq!(front(&app), Some("a.txt".to_owned()));
    assert_eq!(app.title, title_for(Some("a.txt"), false));
}

/// Closing a tab holding unsaved work asks first, and no answer changes
/// nothing.
///
/// The kernel's `close` is about structure and knows nothing about files,
/// so left to itself it would drop the buffer — and the edit with it —
/// without a word. This is the guard, and it is the same question, on the
/// same strip, as the one that protects quitting.
#[test]
fn closing_a_tab_over_unsaved_changes_asks_first() {
    let directory = TempDir::new("desktop-close-dirty");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    type_into(&mut app, "x");
    let unsaved = app.test_editor().content();

    assert_eq!(app.press(&ctrl_w()), Flow::Running);

    assert!(app.prompt.is_some(), "a dirty tab asks before discarding");
    assert_eq!(app.workspace.tab_count(), 2, "the question closed nothing");

    assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);
    assert!(app.prompt.is_none());
    assert_eq!(app.workspace.tab_count(), 2);
    assert_eq!(
        app.test_editor().content(),
        unsaved,
        "the work is still here"
    );
}

/// Answering yes performs the close that was asked about.
#[test]
fn answering_yes_closes_the_tab_that_was_asked_about() {
    let directory = TempDir::new("desktop-close-yes");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    type_into(&mut app, "x");

    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);

    assert!(app.prompt.is_none());
    assert_eq!(app.workspace.tab_count(), 1);
    assert_eq!(app.test_editor().content(), "first");
}

/// Closing the only tab leaves a fresh untitled one, not a void.
///
/// Every accessor in this face answers `Option`, and every handler early
/// returns on `None`. A window with no tab would therefore not crash — it
/// would sit there absorbing keystrokes and painting nothing, which is a
/// worse failure than a crash because it looks like a hang.
#[test]
fn closing_the_only_tab_leaves_a_fresh_untitled_one() {
    let directory = TempDir::new("desktop-close-last");
    let (mut app, _path) = open(&directory, "a.txt", "first");
    assert_eq!(app.workspace.tab_count(), 1);

    assert_eq!(app.press(&ctrl_w()), Flow::Running);

    assert_eq!(app.workspace.tab_count(), 1, "the session still has a tab");
    assert_eq!(app.test_editor().content(), "");
    assert_eq!(front(&app), None, "and it is unnamed");
    assert_eq!(app.title, title_for(None, false));
    assert!(!app.is_dirty());
}

/// A session that has typed into the last tab is still asked before it is
/// replaced by an untitled one.
#[test]
fn closing_a_dirty_last_tab_asks_before_replacing_it() {
    let directory = TempDir::new("desktop-close-last-dirty");
    let (mut app, _path) = open(&directory, "a.txt", "first");
    type_into(&mut app, "x");

    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert!(app.prompt.is_some());
    assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);
    // The caret opens at the start of the buffer, so the typed `x` is
    // in front of the file's text, not after it.
    assert_eq!(app.test_editor().content(), "xfirst");

    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert_eq!(app.press(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(app.test_editor().content(), "");
}

// =========================================================================
// The tab strip
// =========================================================================

/// The strip shows every open tab, in the workspace's order, with exactly
/// one in front.
#[test]
fn the_strip_shows_every_tab_in_order_with_one_in_front() {
    let directory = TempDir::new("desktop-strip-order");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    app.dropped(&fixture(&directory, "c.txt", "third"));

    let content = app.tab_strip_content().expect("three tabs make a strip");
    let labels: Vec<&str> = content.tabs.iter().map(|tab| tab.label.as_str()).collect();
    assert_eq!(labels, ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(
        content.tabs.iter().filter(|tab| tab.is_active).count(),
        1,
        "exactly one tab is in front"
    );
    assert!(
        content.tabs[2].is_active,
        "the last file opened is in front"
    );
}

/// **The strip's index is the workspace's node.**
///
/// The content is built by walking `tabs()` and a press is resolved by
/// indexing `tabs()` again. Nothing forces those two walks to agree — they
/// are separate calls in separate methods — and if they ever stopped
/// agreeing, clicking a tab would bring a different file forward, which is
/// the exact failure the strip's own round-trip test cannot see because it
/// never touches a workspace.
#[test]
fn the_strips_index_names_the_node_a_press_on_it_activates() {
    let directory = TempDir::new("desktop-strip-index");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    app.dropped(&fixture(&directory, "c.txt", "third"));

    let content = app.tab_strip_content().expect("three tabs make a strip");
    for (index, item) in content.tabs.iter().enumerate() {
        let node = app
            .tab_at(Some(TabHit::Activate(index)))
            .expect("every strip entry names a node");
        assert_eq!(
            app.workspace.node(node).map(Node::label),
            Some(item.label.as_str()),
            "strip entry {index} and the node a press on it names disagree"
        );
    }
    assert_eq!(
        app.tab_at(Some(TabHit::Activate(content.tabs.len()))),
        None,
        "an index past the last tab names nothing"
    );
    assert_eq!(app.tab_at(None), None);
}

/// **The dot and the question are one test.**
///
/// The strip marks a tab dirty with [`DesktopApp::document_is_dirty`], and
/// `⌘W` decides whether to ask with [`DesktopApp::is_dirty`]. They must be
/// the same comparison asked of different tabs: a strip that marked by a
/// revision counter would show a dot on a tab that closes without a word,
/// and — worse — leave a tab undotted that stops to ask.
#[test]
fn the_strips_dirty_marker_is_the_test_that_guards_closing() {
    let directory = TempDir::new("desktop-strip-dirty");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    app.dropped(&fixture(&directory, "b.txt", "second"));
    type_into(&mut app, "x");

    let content = app.tab_strip_content().expect("two tabs make a strip");
    assert!(!content.tabs[0].is_dirty, "the untouched tab shows no dot");
    assert!(content.tabs[1].is_dirty, "the edited tab shows a dot");

    // The dotted tab is the one that stops to ask.
    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert!(app.prompt.is_some(), "the dotted tab asked");
    assert_eq!(app.press(&press(KeyCode::Char('n'))), Flow::Running);

    // The undotted one closes without a word.
    let clean = app.workspace.tabs()[0];
    assert!(app.workspace.activate(clean));
    assert_eq!(app.press(&ctrl_w()), Flow::Running);
    assert!(app.prompt.is_none(), "the undotted tab did not ask");
    assert_eq!(app.workspace.tab_count(), 1);
}

/// Undoing back to the saved bytes clears the dot, because the comparison
/// is against the file rather than against a counter that only goes up.
#[test]
fn undoing_back_to_the_saved_text_clears_the_dot() {
    let directory = TempDir::new("desktop-strip-undo");
    let (mut app, _path) = open(&directory, "a.txt", "first");
    type_into(&mut app, "x");
    assert!(
        app.tab_strip_content().expect("a strip").tabs[0].is_dirty,
        "the edited tab is dotted"
    );

    assert_eq!(
        app.press(&chord(KeyCode::Char('z'), Modifiers::ctrl())),
        Flow::Running
    );
    assert!(
        !app.tab_strip_content().expect("a strip").tabs[0].is_dirty,
        "a document undone back to what is on disk is not modified"
    );
}
