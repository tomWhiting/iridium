//! Dot-prefixed entries: what is drawn, what is said about what is not, and
//! what the search can reach.
//!
//! Its own file rather than a corner of [`super::panel`], for the reason that
//! module gives about itself: these are all one question — **is the panel
//! honest about what it is not showing** — and a test that fails here says
//! that in one word.
//!
//! Real directories and the real reader thread, like every other suite here.
//! The interesting property is that hiding costs no read in either direction,
//! and a mocked source is exactly what would hide a re-read creeping in.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;

use super::support::{
    FIT, PATIENCE, all_lines, chord, lines, meta, opened, press, select_row, type_query,
};
use crate::file_tree::compose::{BROWSE_HINT, HIDDEN_KEY};
use crate::file_tree::{ExplorerOutcome, FileExplorer};
use crate::overlay::PanelFit;

/// A panel wide enough for the count and the tab hint together.
///
/// [`FIT`] is 40 columns and deliberately kept that way — it is the narrow
/// case, and one of the tests below is about what a panel that cannot hold
/// both decides to keep.
const WIDE: PanelFit = PanelFit {
    content_columns: 60,
    max_interior_rows: 20,
};

/// A project whose root holds two dotfiles among ordinary ones.
fn project() -> TempDir {
    let directory = TempDir::new("explorer-hidden");
    let root = directory.path();
    for name in [".env", ".gitignore", "main.rs", "README.md"] {
        std::fs::write(root.join(name), "x").expect("the fixture was written");
    }
    directory
}

/// The `Ctrl` spelling of a key — the same verb for a keyboard with no
/// command key.
fn ctrl(key: KeyCode) -> iridium_editor::KeyEvent {
    chord(
        key,
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    )
}

/// Row zero, whole — the hint is drawn in the quiet colour, so `query_field`
/// cannot see it.
fn query_row(explorer: &mut FileExplorer) -> String {
    all_lines(explorer).first().cloned().unwrap_or_default()
}

#[test]
fn dot_prefixed_rows_are_off_until_the_key_is_pressed_and_go_back() {
    let directory = project();
    let mut explorer = opened(&directory);

    let rows = lines(&mut explorer).join("\n");
    assert!(
        !rows.contains(".env") && !rows.contains(".gitignore"),
        "the panel opened showing dotfiles: {rows}"
    );
    assert!(rows.contains("main.rs"), "ordinary files are unaffected");

    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Char('.'))),
        ExplorerOutcome::Handled
    );
    let rows = lines(&mut explorer).join("\n");
    assert!(
        rows.contains(".env") && rows.contains(".gitignore"),
        "the key was pressed and the rows did not appear: {rows}"
    );

    // Both directions, in one test, because a toggle that only turns on is a
    // toggle whose second press is untested.
    assert_eq!(
        explorer.handle_key(&meta(KeyCode::Char('.'))),
        ExplorerOutcome::Handled
    );
    let rows = lines(&mut explorer).join("\n");
    assert!(
        !rows.contains(".env"),
        "the second press did not put them back: {rows}"
    );
}

#[test]
fn the_control_spelling_is_the_same_verb() {
    // The panel binds `Ctrl` beside `⌘` throughout for a keyboard without a
    // command key, and a pair where only one member is exercised is a pair
    // where the other can be quietly broken.
    let directory = project();
    let mut explorer = opened(&directory);

    assert_eq!(
        explorer.handle_key(&ctrl(KeyCode::Char('.'))),
        ExplorerOutcome::Handled
    );
    assert!(
        lines(&mut explorer).join("\n").contains(".env"),
        "Ctrl+. did not do what ⌘. does"
    );
}

#[test]
fn the_query_row_says_how_many_it_is_not_showing() {
    let directory = project();
    let mut explorer = opened(&directory);

    // ⭐ The honesty requirement, and the only reason hiding is a viewer
    // setting rather than a tree that disagrees with `ls` in silence. Two
    // dotfiles are being withheld, and the row says both the number and the
    // key that clears it.
    let row = explorer
        .content(&Theme::dark(), WIDE)
        .rows
        .first()
        .map(|row| {
            row.spans
                .iter()
                .map(|span| span.text.clone())
                .collect::<String>()
        })
        .unwrap_or_default();
    assert!(
        row.contains(&format!("2 hidden ({HIDDEN_KEY})")),
        "the query row did not say what it was withholding: {row:?}"
    );
    assert!(
        row.contains(BROWSE_HINT),
        "a panel with room for both should draw both: {row:?}"
    );

    explorer.handle_key(&meta(KeyCode::Char('.')));
    let row = explorer
        .content(&Theme::dark(), WIDE)
        .rows
        .first()
        .map(|row| {
            row.spans
                .iter()
                .map(|span| span.text.clone())
                .collect::<String>()
        })
        .unwrap_or_default();
    assert!(
        !row.contains("hidden"),
        "nothing is being withheld, so nothing may be claimed to be: {row:?}"
    );
    assert!(
        row.contains(BROWSE_HINT),
        "the tab hint is unaffected: {row:?}"
    );
}

#[test]
fn a_panel_too_narrow_for_both_keeps_the_count() {
    let directory = project();
    let mut explorer = opened(&directory);

    // [`FIT`] is 40 columns, which holds one of the two. The count is the one
    // that stays: losing the tab hint costs a feature another session of
    // being undiscovered, and losing the count makes the panel quietly show
    // less than the disk holds with nothing on screen admitting it.
    let row = query_row(&mut explorer);
    assert!(
        row.contains(&format!("2 hidden ({HIDDEN_KEY})")),
        "the narrow panel dropped the count: {row:?}"
    );
    assert!(
        !row.contains(BROWSE_HINT),
        "both were drawn into a row too narrow for them: {row:?}"
    );
}

#[test]
fn a_folder_nobody_has_opened_is_not_counted() {
    // ⚠️ The scope of the claim. A closed folder's dotfiles are in the arena
    // — the crawl reads them — and are not rows the key would produce, so
    // counting them would promise something pressing it does not deliver.
    let directory = TempDir::new("explorer-hidden-closed");
    let root = directory.path();
    std::fs::write(root.join(".env"), "x").expect("the fixture was written");
    std::fs::create_dir(root.join("src")).expect("the fixture directory was made");
    std::fs::write(root.join("src/.secret"), "x").expect("the fixture was written");
    std::fs::write(root.join("src/main.rs"), "x").expect("the fixture was written");

    let mut explorer = opened(&directory);

    // ⚠️ **`src` has to be *listed* for this test to test anything.** Opening
    // the panel reads the root and stops; a folder whose listing never landed
    // has no children to withhold, so both a correct count and a wrong one
    // would say "1" and the assertion below would hold either way. The crawl
    // is what reads a folder nobody opened, and a query is what drives it —
    // so the query goes in, the crawl runs, and the query comes back out,
    // leaving `src` in exactly the state this test is about: read, and closed.
    type_query(&mut explorer, "secret");
    let deadline = Instant::now() + PATIENCE;
    while explorer.is_waiting() && Instant::now() < deadline {
        explorer.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    explorer.handle_key(&press(KeyCode::Escape));
    let src = explorer
        .files
        .node_at(&root.join("src"))
        .expect("the crawl read `src`");
    assert!(
        explorer.files.is_listed(src),
        "the crawl never read `src`, so it has no children to withhold and this test would \
         pass against the very defect it names"
    );
    assert!(
        !explorer.tree.is_expanded(&src),
        "`src` is open, so its entries are rows the key really would add"
    );

    let row = query_row(&mut explorer);
    assert!(
        row.contains(&format!("1 hidden ({HIDDEN_KEY})")),
        "only the open root's own dotfile is a row the key would add: {row:?}"
    );

    // Open `src`, and the second one becomes a row the key would add.
    super::support::open_directory(&mut explorer, "src");
    let row = query_row(&mut explorer);
    assert!(
        row.contains(&format!("2 hidden ({HIDDEN_KEY})")),
        "opening the folder did not bring its withheld entry into the count: {row:?}"
    );
}

#[test]
fn hiding_the_selected_row_leaves_the_selection_where_the_row_was() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.handle_key(&meta(KeyCode::Char('.')));
    select_row(&mut explorer, ".env");

    let before = explorer
        .content(&Theme::dark(), FIT)
        .rows
        .iter()
        .skip(1)
        .position(|row| row.selected);
    assert!(before.is_some(), "the fixture row was not selected");

    // ⚠️ `Tree::refresh` drops a selection whose row is gone, which is right
    // — the alternative points it at whatever moved into the index — and
    // leaves the panel with nothing selected unless somebody puts it back.
    explorer.handle_key(&meta(KeyCode::Char('.')));
    let after = explorer
        .content(&Theme::dark(), FIT)
        .rows
        .iter()
        .skip(1)
        .position(|row| row.selected);
    assert!(
        after.is_some(),
        "hiding the selected row left the panel with no selection at all"
    );
    assert!(
        explorer
            .selected_path()
            .is_some_and(|path| !path.ends_with(".env")),
        "the selection stayed on a row that is no longer drawn"
    );
}

#[test]
fn a_search_cannot_offer_what_the_tree_would_not_show() {
    // ⭐ The reveal path is why this matters rather than being a nicety.
    // `Enter` on a filtered directory drops the query and asks the tree for
    // that node's index — and a node the projection never built has none, so
    // a hit the tree cannot draw is a keypress that throws the query away and
    // lands nowhere.
    let directory = project();
    let mut explorer = opened(&directory);

    type_query(&mut explorer, "env");
    let rows = lines(&mut explorer).join("\n");
    assert!(
        !rows.contains(".env"),
        "the search reached past what the tree is showing: {rows}"
    );

    explorer.handle_key(&press(KeyCode::Escape));
    explorer.handle_key(&meta(KeyCode::Char('.')));
    type_query(&mut explorer, "env");
    let rows = lines(&mut explorer).join("\n");
    assert!(
        rows.contains(".env"),
        "with hidden entries on, the search must find them: {rows}"
    );
}
