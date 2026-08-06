//! The explorer panel's tests.
//!
//! Real directories and the real reader thread throughout: the property that
//! matters most here — that a node is not expanded before its listing has
//! landed — is a property of the timing, and a mock would be exactly the
//! thing that hid it.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;

use super::panel::truncate;
use super::{ExplorerOutcome, FileExplorer};
use crate::overlay::PanelFit;

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A key press under the given modifiers.
fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

/// A window big enough for everything these tests compose.
const FIT: PanelFit = PanelFit {
    content_columns: 40,
    max_interior_rows: 20,
};

/// Opens a panel on `directory` and polls until its root has listed.
///
/// The reads are on a worker thread, so "the rows are there" is something
/// to wait for rather than assume — which is the whole shape this panel
/// exists to handle, and pretending otherwise in a test would hide it.
fn opened(directory: &TempDir) -> FileExplorer {
    let mut explorer =
        FileExplorer::open(directory.path().to_path_buf()).expect("the reader thread ran");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if explorer.poll() && !explorer.is_waiting() {
            return explorer;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the root listing never arrived");
}

/// The panel's rows as plain text, in order.
fn lines(explorer: &mut FileExplorer) -> Vec<String> {
    explorer
        .content(&Theme::dark(), FIT)
        .rows
        .iter()
        .map(|row| {
            row.spans
                .iter()
                .map(|span| span.text.clone())
                .collect::<String>()
        })
        .collect()
}

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

    // Modal means modal: a character key while the panel is up must not
    // reach the document. `Handled` is how this panel says "consumed",
    // and there is deliberately no outcome that means "pass it on".
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('x'))),
        ExplorerOutcome::Handled
    );
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl())),
        ExplorerOutcome::Handled
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
    assert_eq!(rows.len(), 5, "the window is the size the fit allows");
    assert!(
        rows.iter().any(|row| row.selected),
        "a selection that scrolled out of the window is a panel that looks like it \
             stopped responding"
    );

    explorer.handle_key(&press(KeyCode::Home));
    let rows = explorer.content(&Theme::dark(), narrow).rows;
    assert!(
        rows.first().is_some_and(|row| row.selected),
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
