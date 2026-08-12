//! Tests for the terminal file explorer panel.
//!
//! Against real directories on the real reader thread, because the panel's
//! whole job is showing what is on a disk.
//!
//! ⚠️ **What is deliberately *not* tested here.** Every key's meaning, the
//! filter, the crawl, the oil buffer, the rename planning and the refusals are
//! [`iridium_panel::explorer`]'s, and are tested there — 215 of them. Repeating
//! any of it in this file would be testing a re-export. What is this face's,
//! and so what is tested here, is: the conversion from rows to cells, the box
//! it lands in, the caret, and the one outcome this face cannot yet honour.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;

use iridium_editor::theme::Color;
use iridium_panel::{PanelBody, PanelRow, Span};

use super::{ExplorerAction, FileExplorerPanel};
use crate::cell::{CellBuffer, CellContent};
use crate::frame::palette::Palette;
use crate::frame::panel::TOP;

/// How long a listing is waited for before the test gives up.
///
/// Panics rather than carrying on: a test that went ahead with an empty tree
/// would exercise the wrong path and pass for the wrong reason.
const PATIENCE: Duration = Duration::from_secs(10);

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A directory with two files in it, so the panel has something to list.
fn project(name: &str) -> TempDir {
    let directory = TempDir::new(name);
    std::fs::write(directory.path().join("alpha.txt"), "a").expect("the fixture was written");
    std::fs::write(directory.path().join("beta.txt"), "b").expect("the fixture was written");
    directory
}

/// An open panel whose root listing has landed.
fn opened(directory: &TempDir) -> FileExplorerPanel {
    let mut panel = FileExplorerPanel::open(directory.path().to_path_buf(), false)
        .expect("the fixture directory opened");
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if panel.poll() && !panel.is_waiting() {
            return panel;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the root listing never arrived");
}

/// The text of one row, with continuation cells contributing nothing.
fn row_text(buffer: &CellBuffer, row: usize) -> String {
    let mut out = String::new();
    let Some(cells) = buffer.row(row) else {
        return out;
    };
    for cell in cells {
        match cell.content() {
            CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
            CellContent::Continuation => {},
        }
    }
    out
}

/// Paints the panel on a screen of this size and returns every row as text.
fn painted(panel: &mut FileExplorerPanel, columns: usize, rows: usize) -> Vec<String> {
    let mut buffer = CellBuffer::new(columns, rows);
    let styles = Palette::from_theme(&Theme::dark());
    panel.paint(&mut buffer, &Theme::dark(), &styles);
    (0..rows).map(|row| row_text(&buffer, row)).collect()
}

#[test]
fn the_directorys_files_reach_the_screen() {
    let directory = project("tui-explorer-lists");
    let mut panel = opened(&directory);
    let screen = painted(&mut panel, 80, 24).join("\n");
    assert!(
        screen.contains("alpha.txt"),
        "the panel drew no file it was opened on:\n{screen}"
    );
    assert!(
        screen.contains("beta.txt"),
        "only one of the two:\n{screen}"
    );
}

#[test]
fn a_run_after_a_wide_one_starts_where_that_run_actually_ended() {
    // ⭐ **The units seam's real guard, and it took two attempts to get one.**
    //
    // A row is a list of runs painted left to right, and this face has to
    // advance the column by each run's DISPLAY WIDTH — the builder counted
    // characters to lay the row out, but a terminal cell is not a character.
    // Advance by character count instead and every run after a wide one lands
    // too far left, writing over the glyph before it.
    //
    // ⚠️ The obvious test — "no row overruns the box" — **does not catch
    // this**, and that was measured rather than assumed: with the advance
    // sabotaged to `chars().count()`, all six of the other tests here still
    // passed. `paint_text` clips to the content area, so a mis-advanced run
    // damages the *content* and never the border. A test that can only pass
    // proves nothing about the thing it claims to guard.
    //
    // So this one reads the cells back. `日本` is two characters and four
    // cells; `END` must therefore begin at column four.
    let body = PanelBody {
        content_columns: 20,
        rows: vec![PanelRow::new(vec![
            Span::new("日本", Color::new(1.0, 1.0, 1.0, 1.0)),
            Span::new("END", Color::new(1.0, 1.0, 1.0, 1.0)),
        ])],
        caret: None,
    };
    let mut buffer = CellBuffer::new(40, 10);
    let styles = Palette::from_theme(&Theme::dark());
    super::paint::paint(&body, &mut buffer, &styles);

    let row = row_text(&buffer, TOP + 1);
    assert!(
        row.contains("日本END"),
        "the run after the wide one did not start where that run ended: {row:?}"
    );
}

#[test]
fn no_row_overruns_the_box_on_a_directory_of_wide_names() {
    // A weaker guard than the one above and worth keeping for what it does
    // cover: whatever the arithmetic does, the border survives it.
    let directory = TempDir::new("tui-explorer-wide");
    // Names whose characters are two cells each, so the char count and the
    // cell count differ by a factor of two on every row that shows one.
    std::fs::write(directory.path().join("日本語のファイル名.txt"), "a")
        .expect("the fixture was written");
    std::fs::write(directory.path().join("한국어파일이름.md"), "b")
        .expect("the fixture was written");
    let mut panel = opened(&directory);

    let columns = 48;
    let screen = painted(&mut panel, columns, 24);
    for (index, row) in screen.iter().enumerate() {
        // The buffer cannot hold more than `columns` cells, so the check that
        // matters is the border: every row of the box begins and ends with one
        // and a row that overflowed would have written over the right one.
        if row.trim().is_empty() {
            continue;
        }
        assert!(
            row.chars().count() <= columns,
            "row {index} is wider than the screen: {row:?}"
        );
    }

    let top = &screen[TOP];
    assert!(
        top.contains('╭') && top.contains('╮'),
        "the top border did not survive the wide names: {top:?}"
    );
    let bottom = screen
        .iter()
        .rev()
        .find(|row| row.contains('╰'))
        .expect("a bottom border was drawn");
    assert!(
        bottom.contains('╯'),
        "the bottom border lost its right corner: {bottom:?}"
    );
}

#[test]
fn the_caret_lands_on_the_query_row_inside_the_box() {
    let directory = project("tui-explorer-caret");
    let mut panel = opened(&directory);
    let mut buffer = CellBuffer::new(80, 24);
    let styles = Palette::from_theme(&Theme::dark());
    let caret = panel
        .paint(&mut buffer, &Theme::dark(), &styles)
        .expect("an 80x24 screen fits the panel and its query field");

    // The query row is the first interior row: one below the box's top border,
    // which is itself one below the screen's top edge.
    assert_eq!(caret.row, TOP + 1, "the caret left the query row");
    assert!(
        caret.column > 0 && caret.column < 80,
        "the caret landed outside the screen: {caret:?}"
    );
}

#[test]
fn a_screen_too_small_for_an_honest_panel_draws_nothing_rather_than_panicking() {
    let directory = project("tui-explorer-tiny");
    let mut panel = opened(&directory);
    let mut buffer = CellBuffer::new(8, 3);
    let styles = Palette::from_theme(&Theme::dark());
    assert_eq!(panel.paint(&mut buffer, &Theme::dark(), &styles), None);
    let screen = (0..3).map(|row| row_text(&buffer, row)).collect::<String>();
    assert!(
        screen.trim().is_empty(),
        "something was drawn on a screen that cannot hold a panel: {screen:?}"
    );
}

#[test]
fn escape_closes_the_panel() {
    let directory = project("tui-explorer-escape");
    let mut panel = opened(&directory);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape)),
        ExplorerAction::Close
    );
}

#[test]
fn the_sidebar_request_is_reported_rather_than_swallowed() {
    // ⚠️ **A channel that can only carry success is a defect.** The terminal
    // face has no sidebar placement yet, and a key that silently did nothing
    // would be indistinguishable from a key that is broken. This is the test
    // that has to be deleted — not merely edited — when step 5 lands, which is
    // the point of writing it as an equality rather than as `is_some`.
    let directory = project("tui-explorer-sidebar");
    let mut panel = opened(&directory);
    let action = panel.handle_key(&KeyEvent {
        key: KeyCode::Char('b'),
        // The shared panel binds `Ctrl+Alt+B` alongside `Cmd+B`, and only the
        // first of those is a chord a terminal can deliver. Spelled out here
        // rather than taken from a helper so the test breaks loudly if the
        // binding moves — the point of this test is that a specific key
        // reaches a specific answer.
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    });
    match action {
        ExplorerAction::Report(message) => assert!(
            message.contains("sidebar"),
            "the report said nothing about what was refused: {message:?}"
        ),
        other => panic!("the sidebar request was not reported: {other:?}"),
    }
}
