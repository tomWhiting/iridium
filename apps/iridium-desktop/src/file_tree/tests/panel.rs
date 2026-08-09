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

use super::support::{FIT, chord, lines, meta, opened, press, query_field, select_row, settle};
use crate::file_tree::compose::BROWSE_HINT;
use crate::file_tree::rows::truncate;
use crate::file_tree::{ExplorerOutcome, FileExplorer};
use crate::overlay::{PanelCaret, PanelFit};

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

    // Enter again folds it back up. This is the half that tells a toggle
    // apart from a descend, and it is Tom's ruling of 7 Aug 2026 — a folder
    // unfolds and refolds, and the panel never re-roots itself onto it.
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Enter)),
        ExplorerOutcome::Handled,
        "an open directory toggles shut rather than re-rooting the panel"
    );
    assert!(
        !lines(&mut explorer)
            .iter()
            .any(|row| row.contains("inner.txt")),
        "the children went away with the fold"
    );
    assert!(
        lines(&mut explorer).iter().any(|row| row.contains("sub")),
        "the folder itself is still on screen, still selectable — a descend \
         would have replaced the view with its contents"
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
    assert_eq!(
        query_field(&mut explorer),
        "x",
        "the character landed in the query field"
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

/// A root with one folder holding one file, for walking in and out of.
fn nested() -> TempDir {
    let directory = TempDir::new("explorer-reroot");
    let inner = directory.path().join("inner");
    std::fs::create_dir(&inner).expect("the fixture directory was made");
    std::fs::write(inner.join("leaf.rs"), "l").expect("the fixture was written");
    std::fs::write(directory.path().join("outer.rs"), "o").expect("the fixture was written");
    directory
}

#[test]
fn walking_into_a_folder_makes_it_the_root() {
    let directory = nested();
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "inner/");

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Down)),
        ExplorerOutcome::Handled
    );
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(rows[0].contains("inner"), "the new root: {rows:?}");
    assert!(rows.iter().any(|row| row.contains("leaf.rs")), "{rows:?}");
    assert!(
        rows.iter().all(|row| !row.contains("outer.rs")),
        "and nothing from above it, which is what re-rooting means: {rows:?}"
    );
}

#[test]
fn walking_in_from_a_file_uses_the_folder_holding_it() {
    // A file is a perfectly sensible thing to have selected when you press
    // "go in here", and refusing would make the key look dead half the time.
    let directory = nested();
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "outer.rs");

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Down)),
        ExplorerOutcome::Handled
    );
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("outer.rs")),
        "rooted at the folder the file was in, so the file is still there: {rows:?}"
    );
}

#[test]
fn walking_back_up_restores_the_folder_above() {
    let directory = nested();
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "inner/");
    explorer.handle_key(&meta(KeyCode::Down));
    settle(&mut explorer);

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Up)),
        ExplorerOutcome::Handled
    );
    settle(&mut explorer);

    let rows = lines(&mut explorer);
    assert!(
        rows.iter().any(|row| row.contains("outer.rs")),
        "back to the folder above, which the inner root could not see: {rows:?}"
    );
}

#[test]
fn the_control_spelling_of_the_pair_does_the_same_thing() {
    // For a keyboard with no command key. Bound together so the two cannot
    // drift into meaning different things.
    let directory = nested();
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "inner/");

    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::none()
    };
    explorer.handle_key(&chord(KeyCode::Down, ctrl));
    settle(&mut explorer);
    assert!(lines(&mut explorer)[0].contains("inner"));
}

#[test]
fn re_rooting_drops_the_query_with_the_tree_it_was_narrowing() {
    // The rows were about a different root. Keeping the text would leave a
    // query in the field describing results nobody can see.
    let directory = nested();
    let mut explorer = opened(&directory);
    select_row(&mut explorer, "inner/");
    explorer.handle_key(&meta(KeyCode::Down));
    settle(&mut explorer);

    assert_eq!(
        query_field(&mut explorer),
        "",
        "an empty query field — the row itself also carries the browse hint"
    );
}

// ------------------------------------------------------ the oil-buffer hint
//
// The panel drew a query row and nothing else, so nothing on screen ever said
// `Tab` turns the rows into an editable buffer — the feature was reachable
// only by already knowing it was there, which is no way to ship a feature.

/// The query row as plain text.
fn query_row(explorer: &mut FileExplorer, fit: PanelFit) -> String {
    explorer
        .content(&Theme::dark(), fit)
        .rows
        .first()
        .map(|row| {
            row.spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        })
        .unwrap_or_default()
}

#[test]
fn the_query_row_says_the_rows_can_be_edited() {
    let directory = TempDir::new("panel-hint");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");

    let mut explorer = opened(&directory);
    let row = query_row(&mut explorer, FIT);

    assert!(
        row.ends_with(BROWSE_HINT),
        "the hint sits on the right edge of the query row: {row:?}"
    );
    assert!(
        row.starts_with("> "),
        "and the field it shares the row with is still there: {row:?}"
    );
    assert_eq!(
        query_field(&mut explorer),
        "",
        "the hint is not text in the field"
    );
}

#[test]
fn the_hint_goes_while_a_query_is_being_typed_and_comes_back_after_it() {
    // The field owns everything right of the prompt and scrolls through it, so
    // the hint cannot stay: it would be a zone the typed text runs under. It
    // has done its job by then — whoever typed that character read it first.
    let directory = TempDir::new("panel-hint-typing");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");

    let mut explorer = opened(&directory);
    explorer.handle_key(&press(KeyCode::Char('a')));
    let typed = query_row(&mut explorer, FIT);
    assert!(
        !typed.contains(BROWSE_HINT),
        "a query in the field takes the row back: {typed:?}"
    );

    explorer.handle_key(&press(KeyCode::Backspace));
    let cleared = query_row(&mut explorer, FIT);
    assert!(
        cleared.ends_with(BROWSE_HINT),
        "and an emptied field gives it back, so the hint is not a one-shot: {cleared:?}"
    );
}

#[test]
fn a_panel_too_narrow_for_both_keeps_the_field_and_drops_the_hint() {
    // Narrow beats informative. A hint truncated to "tab to edi" reads as
    // damage, and one sitting against the caret reads as text in the field.
    let directory = TempDir::new("panel-hint-narrow");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");

    let mut explorer = opened(&directory);
    let narrow = PanelFit {
        content_columns: BROWSE_HINT.chars().count() + 2,
        max_interior_rows: 20,
    };

    let row = query_row(&mut explorer, narrow);
    assert!(
        !row.contains(BROWSE_HINT),
        "no room for the hint clear of the caret: {row:?}"
    );
    assert_eq!(
        explorer.content(&Theme::dark(), narrow).caret,
        Some(PanelCaret { row: 0, column: 2 }),
        "and the field still reports its caret, which is the half that does something"
    );
}

#[test]
fn the_hint_is_not_offered_while_the_folder_is_still_being_read() {
    // ⚠️ The honest half. `Tab` refuses until the listing lands — a buffer
    // snapshotted from half a directory would leave the rest out of the diff
    // with no way to tell which half you got — so a hint on that screen would
    // be advertising a refusal, and a key that answers "not yet" the first
    // time it is pressed is a key nobody presses twice.
    let directory = TempDir::new("panel-hint-loading");
    std::fs::write(directory.path().join("a.txt"), "a").expect("the fixture was written");

    // Deliberately not `opened`, which polls until the root has listed.
    let mut explorer =
        FileExplorer::open(directory.path().to_path_buf(), true).expect("the reader thread ran");

    let row = query_row(&mut explorer, FIT);
    assert!(
        !row.contains(BROWSE_HINT),
        "nothing to edit yet, so nothing said about editing: {row:?}"
    );
    assert!(
        matches!(
            explorer.handle_key(&press(KeyCode::Tab)),
            ExplorerOutcome::Failed(_)
        ),
        "the premise: the key this hint advertises refuses in this state"
    );

    // And once the listing lands, both change together.
    settle(&mut explorer);
    let row = query_row(&mut explorer, FIT);
    assert!(
        row.ends_with(BROWSE_HINT),
        "the rows are there and the hint says so: {row:?}"
    );
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Tab)),
        ExplorerOutcome::Handled,
        "and the key it advertises now works"
    );
}
