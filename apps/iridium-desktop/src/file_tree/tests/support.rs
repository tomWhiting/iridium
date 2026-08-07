//! Fixtures and readers shared by the explorer's tests.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;

use crate::file_tree::{ExplorerOutcome, FileExplorer};
use crate::overlay::PanelFit;

/// A project with two directories, each holding a `mod.rs` — the case a flat
/// ranked list cannot tell apart, and the reason the hierarchy is kept.
///
/// Shared by [`super::filter`] and [`super::crawl`]: the same five files are
/// what makes "the rows are drawn where they live" and "the query read a
/// folder nobody opened" two views of one fixture rather than two fixtures
/// that could drift.
pub(super) fn project() -> TempDir {
    let directory = TempDir::new("explorer-filter");
    let root = directory.path();
    for folder in ["engine", "widgets"] {
        std::fs::create_dir(root.join(folder)).expect("the fixture directory was made");
        std::fs::write(root.join(folder).join("mod.rs"), "m").expect("the fixture was written");
    }
    std::fs::write(root.join("engine/render.rs"), "r").expect("the fixture was written");
    std::fs::write(root.join("widgets/button.rs"), "b").expect("the fixture was written");
    std::fs::write(root.join("README.md"), "#").expect("the fixture was written");
    directory
}

/// A key press with no modifiers.
pub(super) fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A key press under the given modifiers.
pub(super) fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

/// The `⌘` spelling of a key — what a mac keyboard actually sends.
pub(super) fn meta(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            meta: true,
            ..Modifiers::none()
        },
    )
}

/// A window big enough for everything these tests compose.
pub(super) const FIT: PanelFit = PanelFit {
    content_columns: 40,
    max_interior_rows: 20,
};

/// How long a test waits for a directory read before calling it hung.
pub(super) const PATIENCE: Duration = Duration::from_secs(10);

/// Opens a panel on `directory`, crawling under a query, and polls until its
/// root has listed.
///
/// The reads are on a worker thread, so "the rows are there" is something
/// to wait for rather than assume — which is the whole shape this panel
/// exists to handle, and pretending otherwise in a test would hide it.
pub(super) fn opened(directory: &TempDir) -> FileExplorer {
    opened_with_crawl(directory, true)
}

/// The same, with the crawl decided by the caller.
///
/// `false` is the shape a home directory or a guessed working directory
/// opens in: the tree is browsable and the filter still narrows what has
/// been read, but a query posts no reads of its own.
pub(super) fn opened_with_crawl(directory: &TempDir, crawl: bool) -> FileExplorer {
    let mut explorer =
        FileExplorer::open(directory.path().to_path_buf(), crawl).expect("the reader thread ran");
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if explorer.poll() && !explorer.is_waiting() {
            return explorer;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the root listing never arrived");
}

/// Polls until nothing is outstanding, so a just-requested listing is on
/// screen.
pub(super) fn settle(explorer: &mut FileExplorer) {
    let deadline = Instant::now() + PATIENCE;
    while explorer.is_waiting() && Instant::now() < deadline {
        explorer.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    explorer.poll();
}

/// Moves the selection to the first list row containing `needle`.
///
/// Panics rather than returning a flag: a fixture whose row is missing is a
/// broken test, and the assertion that follows would fail somewhere less
/// informative.
pub(super) fn select_row(explorer: &mut FileExplorer, needle: &str) {
    let rows = lines(explorer);
    let index = rows
        .iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row contains {needle:?}: {rows:?}"));
    explorer.handle_key(&press(KeyCode::Home));
    for _ in 0..index {
        explorer.handle_key(&press(KeyCode::Down));
    }
}

/// Opens the directory row containing `needle` and waits for its listing.
///
/// The filter walks only what has been read, so a test about *matching* opens
/// what it means to match against — and says so, rather than relying on a
/// sweep that would quietly stop testing the thing it named.
pub(super) fn open_directory(explorer: &mut FileExplorer, needle: &str) {
    select_row(explorer, needle);
    explorer.handle_key(&press(KeyCode::Right));
    settle(explorer);
}

/// Every composed row as plain text, the query row included.
pub(super) fn all_lines(explorer: &mut FileExplorer) -> Vec<String> {
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

/// The list rows as plain text, without the query row that always precedes
/// them.
pub(super) fn lines(explorer: &mut FileExplorer) -> Vec<String> {
    let mut rows = all_lines(explorer);
    if rows.is_empty() {
        return rows;
    }
    rows.remove(0);
    rows
}

/// Types `text` into the panel one character at a time, as a user would.
pub(super) fn type_query(explorer: &mut FileExplorer, text: &str) {
    for character in text.chars() {
        assert_eq!(
            explorer.handle_key(&press(KeyCode::Char(character))),
            ExplorerOutcome::Handled,
            "a printable character goes into the query, never through the panel"
        );
    }
}

/// The index of the selected list row, or `None` when nothing is selected.
pub(super) fn selected_row(explorer: &mut FileExplorer) -> Option<usize> {
    explorer
        .content(&Theme::dark(), FIT)
        .rows
        .iter()
        .skip(1)
        .position(|row| row.selected)
}

/// The text of the selected list row.
pub(super) fn selected_text(explorer: &mut FileExplorer) -> Option<String> {
    let index = selected_row(explorer)?;
    lines(explorer).get(index).cloned()
}
