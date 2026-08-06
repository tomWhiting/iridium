//! The panel's behaviour with no query: opening, keys, scrolling.
//!
//! Real directories and the real reader thread throughout: the property that
//! matters most here — that a node is not expanded before its listing has
//! landed — is a property of the timing, and a mock would be exactly the
//! thing that hid it.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;

use super::support::{all_lines, chord, lines, opened, press};
use crate::file_tree::ExplorerOutcome;
use crate::file_tree::rows::truncate;
use crate::overlay::PanelFit;

#[test]
fn the_panel_opens_with_the_root_expanded_and_its_children_indented() {
    let directory = TempDir::new("panel-open");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");
    std::fs::create_dir(directory.path().join("sub")).expect("the fixture directory was made");

    let mut explorer = opened(&directory);
    let rows = lines(&mut explorer);

    // The root, expanded, then its children one level in — directories
    // first. A panel that opened on one collapsed row would have told
    // the user nothing, which is why `open` expands.
    assert_eq!(rows.len(), 3, "root plus two children: {rows:?}");
    assert!(rows[0].starts_with("▾ "), "the root is open: {:?}", rows[0]);
    assert!(rows[1].starts_with("  ▸ sub/"), "{:?}", rows[1]);
    assert!(rows[2].starts_with("    a.txt"), "{:?}", rows[2]);
}

#[test]
fn escape_and_the_toggle_chord_both_close_the_panel() {
    let directory = TempDir::new("panel-close");
    let mut explorer = opened(&directory);
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Escape)),
        ExplorerOutcome::Closed
    );

    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('e'), ctrl_alt)),
        ExplorerOutcome::Closed,
        "the chord that opened it puts it away"
    );

    // The mac spelling of the same chord, which is what winit reports
    // for ⌘⌥E.
    let meta_alt = Modifiers {
        meta: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('E'), meta_alt)),
        ExplorerOutcome::Closed
    );
}

#[test]
fn enter_toggles_a_directory_and_opens_a_file() {
    let directory = TempDir::new("panel-enter");
    std::fs::create_dir(directory.path().join("sub")).expect("the fixture directory was made");
    std::fs::write(directory.path().join("sub/inner.txt"), "i").expect("the fixture written");
    std::fs::write(directory.path().join("z.txt"), "z").expect("the fixture was written");

    let mut explorer = opened(&directory);

    // Row 1 is `sub/`. Enter opens it; the read then has to land.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Down)),
        ExplorerOutcome::Handled
    );
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled,
        "a directory toggles rather than opening a tab"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while explorer.is_waiting() && Instant::now() < deadline {
        explorer.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    explorer.poll();
    assert!(
        lines(&mut explorer)
            .iter()
            .any(|row| row.contains("inner.txt")),
        "the expanded directory's children are on screen"
    );

    // And on a file, Enter names the path for the host to open. The
    // panel does not touch the workspace itself — one open path, shared
    // with a drop and with Ctrl+O.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::End)),
        ExplorerOutcome::Handled
    );
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Open(directory.path().join("z.txt"))
    );
}

#[test]
fn a_key_the_panel_does_not_bind_is_swallowed_rather_than_typed() {
    let directory = TempDir::new("panel-modal");
    let mut explorer = opened(&directory);

    // Modal means modal: a chord the panel does not bind must not reach the
    // document. `Handled` is how this panel says "consumed", and there is
    // deliberately no outcome that means "pass it on".
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl())),
        ExplorerOutcome::Handled,
        "Ctrl+S while the explorer is up must not save the document behind it"
    );
    let meta = Modifiers {
        meta: true,
        ..Modifiers::none()
    };
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('a'), meta)),
        ExplorerOutcome::Handled
    );

    // A plain printable key is consumed too, and goes to the query — which
    // is the one thing in this panel that a bare character means.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('x'))),
        ExplorerOutcome::Handled
    );
    let rows = all_lines(&mut explorer);
    assert_eq!(
        rows.first().map(String::as_str),
        Some("> x"),
        "the character landed in the query row: {rows:?}"
    );
}

#[test]
fn the_window_follows_the_selection_past_the_bottom_and_back() {
    let directory = TempDir::new("panel-scroll");
    for index in 0..30 {
        std::fs::write(directory.path().join(format!("file-{index:02}.txt")), "x")
            .expect("the fixture was written");
    }

    let mut explorer = opened(&directory);
    let narrow = PanelFit {
        content_columns: 40,
        max_interior_rows: 5,
    };

    for _ in 0..20 {
        explorer.handle_key(&press(KeyCode::Down));
    }
    let rows = explorer.content(&Theme::dark(), narrow).rows;
    // The query row plus the four list rows the fit leaves after it. The
    // query row is never part of the scrolling window: a field that scrolled
    // away would leave the caret pointing at a row that is not there.
    assert_eq!(rows.len(), 5, "the window is the size the fit allows");
    assert!(
        rows.iter().skip(1).any(|row| row.selected),
        "a selection that scrolled out of the window is a panel that looks like it \
             stopped responding"
    );

    explorer.handle_key(&press(KeyCode::Home));
    let rows = explorer.content(&Theme::dark(), narrow).rows;
    assert!(
        rows.get(1).is_some_and(|row| row.selected),
        "Home brings the window back to the top with it"
    );
}

#[test]
fn a_name_too_wide_for_the_panel_is_visibly_cut() {
    // A silent cut is how someone opens the wrong file: two long names
    // sharing a prefix would render identically.
    assert_eq!(truncate("abcdef", 6), "abcdef");
    assert_eq!(truncate("abcdefg", 6), "abcde…");
    assert_eq!(truncate("abc", 0), "");
    // By characters, not bytes — a cut mid-codepoint would panic, and
    // these are filenames.
    assert_eq!(truncate("ééééé", 3), "éé…");
}
