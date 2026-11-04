//! Tests for the terminal file explorer panel.
//!
//! Against real directories on the real reader thread, because the panel's
//! whole job is showing what is on a disk.
//!
//! ⚠️ **What is deliberately *not* tested here.** Every key's meaning, the
//! filter, the crawl, the text editing, the rename planning and the refusals
//! are [`iridium_panel::explorer`]'s, and are tested there. Repeating any of it
//! in this file would be testing a re-export. What is this face's, and so what
//! is tested here, is: the conversion from rows to cells, the marks painted
//! over them, the box it lands in, the caret, and which of its two placements
//! it takes.

use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::test_support::TempDir;

use iridium_editor::theme::Color;
use iridium_panel::{MarkKind, PanelBody, PanelMark, PanelRow, Span};

use super::paint::Placement;
use super::{ExplorerAction, FileExplorerPanel};
use crate::cell::{CellBuffer, CellContent, Style};
use crate::frame::palette::Palette;
use crate::frame::panel::CentredBox;

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
    let mut panel = FileExplorerPanel::open(
        directory.path().to_path_buf(),
        false,
        // Nothing configured: these tests are about the panel's own keys.
        &iridium_editor::Keymap::new("user"),
        &iridium_editor::Keymap::new("face"),
    )
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

/// How many rows a band may span on a screen this tall, with no search panel.
///
/// The host's own number, from the frame, rather than `rows - 1` written here:
/// a test that computed its own would stop agreeing with the editor the first
/// time the statusline or the search panel changed height.
fn band_rows(rows: usize) -> usize {
    crate::frame::document_rows(rows, false)
}

/// Paints the panel on a screen of this size and returns every row as text.
fn painted(panel: &mut FileExplorerPanel, columns: usize, rows: usize) -> Vec<String> {
    let mut buffer = CellBuffer::new(columns, rows);
    let styles = Palette::from_theme(&Theme::dark());
    panel.paint(&mut buffer, band_rows(rows), &Theme::dark(), &styles);
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
    let centred = CentredBox::fitted(40, 10).expect("a 40x10 screen fits a panel");
    super::paint::paint(&body, Placement::Centred(centred), &mut buffer, &styles);

    let row = row_text(&buffer, centred.top + 1);
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

    let top = screen
        .iter()
        .find(|row| row.contains('╭'))
        .expect("a top border was drawn");
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
        .paint(&mut buffer, band_rows(24), &Theme::dark(), &styles)
        .expect("an 80x24 screen fits the panel and its query field");

    // The query row is the first interior row: one below the box's top border.
    let centred = CentredBox::fitted(80, band_rows(24)).expect("an 80x24 screen fits the box");
    assert_eq!(caret.row, centred.top + 1, "the caret left the query row");
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
    assert_eq!(
        panel.paint(&mut buffer, band_rows(3), &Theme::dark(), &styles),
        None
    );
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

/// The chord the shared panel binds for the sidebar.
///
/// It binds `Ctrl+Alt+B` alongside `Cmd+B`, and only the first is a chord a
/// terminal can deliver. Spelled out rather than taken from a helper, so these
/// tests break loudly if the binding moves — the point of them is that a
/// specific key reaches a specific placement.
fn sidebar_chord() -> KeyEvent {
    KeyEvent {
        key: KeyCode::Char('b'),
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    }
}

#[test]
fn the_sidebar_chord_takes_columns_from_the_document_and_gives_them_back() {
    // R4: the band takes columns *from* the document rather than floating over
    // them. What the host reads is this number, so it is what the test reads.
    let directory = project("tui-explorer-sidebar");
    let mut panel = opened(&directory);
    assert_eq!(
        panel.sidebar_columns(80, band_rows(24)),
        0,
        "a panel opens floating and costs the document nothing"
    );

    assert_eq!(panel.handle_key(&sidebar_chord()), ExplorerAction::Handled);
    let band = panel.sidebar_columns(80, band_rows(24));
    assert!(
        band > 0 && band < 40,
        "the band took none of the screen, or more than half of it: {band}"
    );

    // ⚠️ Zero is a value it writes, not a case it skips.
    assert_eq!(panel.handle_key(&sidebar_chord()), ExplorerAction::Handled);
    assert_eq!(
        panel.sidebar_columns(80, band_rows(24)),
        0,
        "the columns were not given back when the sidebar was toggled off"
    );
}

#[test]
fn a_screen_too_narrow_for_an_honest_band_keeps_floating() {
    // ⭐ The refusal has to be visible in BOTH answers or they disagree: a
    // panel that reported columns it then declined to draw in would leave the
    // document short of a band that is not there.
    let directory = project("tui-explorer-narrow");
    let mut panel = opened(&directory);
    panel.handle_key(&sidebar_chord());

    assert_eq!(
        panel.sidebar_columns(30, band_rows(24)),
        0,
        "a 30-column screen cannot hold a band and the document too"
    );

    let screen = painted(&mut panel, 30, 24).join("\n");
    assert!(
        screen.contains('╭') && screen.contains('╮'),
        "the panel drew neither a band nor a box:\n{screen}"
    );
}

#[test]
fn the_band_is_flush_at_the_left_edge_with_a_rule_down_its_right() {
    let directory = project("tui-explorer-band");
    let mut panel = opened(&directory);
    panel.handle_key(&sidebar_chord());

    let (columns, rows) = (80, 24);
    let band = panel.sidebar_columns(columns, band_rows(rows));
    let screen = painted(&mut panel, columns, rows);

    // Every row the band spans carries its rule in the same column, and the
    // statusline row is not one of them.
    for (row, text) in screen.iter().enumerate().take(rows - 1) {
        let cells: Vec<char> = text.chars().collect();
        assert_eq!(
            cells.get(band - 1),
            Some(&'│'),
            "row {row} lost the band's rule: {text:?}"
        );
    }
    let last = &screen[rows - 1];
    assert!(
        !last.starts_with('│') && last.chars().nth(band - 1) != Some('│'),
        "the band covered the statusline, which describes the document: {last:?}"
    );
}

#[test]
fn every_row_the_band_reserved_is_painted_even_past_the_last_file() {
    // ⭐ **The "zero is written, not skipped" rule, one layer down.** Two files
    // fill four or five rows of a twenty-three-row band. The document is laid
    // out to start *after* the band, so nothing else will ever paint the rest
    // of it: skip them and a full-height column reads as a short box with bare
    // screen below it.
    //
    // Sabotage check: dropping the leftover-row loop in `Placement::close`
    // fails this on the first row past the list.
    let directory = project("tui-explorer-tail");
    let mut panel = opened(&directory);
    panel.handle_key(&sidebar_chord());

    let (columns, rows) = (80, 24);
    let band = panel.sidebar_columns(columns, band_rows(rows));
    let screen = painted(&mut panel, columns, rows);

    let last_band_row = rows - 2;
    let cells: Vec<char> = screen[last_band_row].chars().collect();
    assert_eq!(
        cells.get(band - 1),
        Some(&'│'),
        "the band stopped where its list did: {:?}",
        screen[last_band_row]
    );
}

#[test]
fn a_band_stops_at_the_rows_it_was_given_and_not_at_the_screens_edge() {
    // ⚠️ **The search panel's rows are not the band's to take.** The host hands
    // in `document_rows`, which already excludes the statusline and any open
    // search panel, and the band must honour it rather than running to the
    // bottom of the buffer. Painting past it would put a file tree over a panel
    // that took its rows from the same document.
    //
    // Sabotage check: passing `buffer.height()` instead of `band_rows` into
    // `SidebarBox::fitted` fails this on row 10.
    let directory = project("tui-explorer-shortband");
    let mut panel = opened(&directory);
    panel.handle_key(&sidebar_chord());

    let (columns, rows, allowed) = (80, 24, 10);
    let band = panel.sidebar_columns(columns, allowed);
    assert!(band > 0, "a ten-row band should still fit");

    let mut buffer = CellBuffer::new(columns, rows);
    let styles = Palette::from_theme(&Theme::dark());
    panel.paint(&mut buffer, allowed, &Theme::dark(), &styles);

    let last = row_text(&buffer, allowed - 1);
    assert_eq!(
        last.chars().nth(band - 1),
        Some('│'),
        "the band did not reach the last row it was given: {last:?}"
    );
    let past = row_text(&buffer, allowed);
    assert_ne!(
        past.chars().nth(band - 1),
        Some('│'),
        "the band ran past the rows it was given: {past:?}"
    );
}

#[test]
fn the_floating_box_is_large_and_centred_on_the_rows_it_was_given() {
    // The floating placement is the front-and-centre browser: most of the
    // screen, the same margin either side, and nothing below the document's
    // rows, so the statusline stays readable under it.
    let directory = project("tui-explorer-centred");
    let mut panel = opened(&directory);
    let (columns, rows) = (120, 40);
    let screen = painted(&mut panel, columns, rows);

    let top = screen
        .iter()
        .position(|row| row.contains('╭'))
        .expect("a top border was drawn");
    let bottom = screen
        .iter()
        .position(|row| row.contains('╰'))
        .expect("a bottom border was drawn");
    let border: Vec<char> = screen[top].chars().collect();
    let left = border
        .iter()
        .position(|cell| *cell == '╭')
        .expect("a left corner");
    let right = border
        .iter()
        .position(|cell| *cell == '╮')
        .expect("a right corner");

    assert!(
        right - left + 1 >= columns * 8 / 10,
        "the box is not large: {}",
        right - left + 1
    );
    assert!(
        left.abs_diff(columns - 1 - right) <= 1,
        "the box is not centred across: {left} cells left, {} right",
        columns - 1 - right
    );
    assert!(
        bottom - top + 1 >= band_rows(rows) * 8 / 10,
        "the box is not tall"
    );
    assert!(
        top.abs_diff(band_rows(rows) - 1 - bottom) <= 1,
        "the box is not centred down the document's rows: top {top}, bottom {bottom}"
    );
    assert!(bottom < band_rows(rows), "the box ran over the statusline");
}

#[test]
fn the_floating_box_keeps_its_size_however_few_rows_it_lists() {
    // A box that grew with its content would jump as lines are typed and
    // deleted in the text being edited.
    let directory = project("tui-explorer-fixed");
    let mut panel = opened(&directory);
    let screen = painted(&mut panel, 100, 30);
    let top = screen
        .iter()
        .position(|row| row.contains('╭'))
        .expect("a top border");
    let bottom = screen
        .iter()
        .position(|row| row.contains('╰'))
        .expect("a bottom border");
    let centred = CentredBox::fitted(100, band_rows(30)).expect("the screen fits the box");
    assert_eq!(
        (top, bottom),
        (centred.top, centred.top + centred.interior_rows + 1)
    );
    for (row, text) in screen.iter().enumerate().take(bottom).skip(top + 1) {
        assert!(
            text.contains('│'),
            "interior row {row} lost its sides: {text:?}"
        );
    }
}

#[test]
fn a_mark_paints_its_background_over_the_cells_its_characters_landed_in() {
    // Marks count characters; cells are display width. `日` is one character
    // and two cells, so a selection of the character after it starts at the
    // third cell of the row, not the second.
    let white = Color::new(1.0, 1.0, 1.0, 1.0);
    let body = PanelBody {
        content_columns: 20,
        rows: vec![
            PanelRow::new(vec![Span::new("日ab", white)]).with_marks(vec![
                PanelMark {
                    start: 1,
                    end: 2,
                    kind: MarkKind::Selection,
                },
                PanelMark {
                    start: 3,
                    end: 4,
                    kind: MarkKind::Caret,
                },
            ]),
        ],
        caret: None,
    };
    let mut buffer = CellBuffer::new(40, 10);
    let styles = Palette::from_theme(&Theme::dark());
    let centred = CentredBox::fitted(40, 10).expect("a 40x10 screen fits a panel");
    super::paint::paint(&body, Placement::Centred(centred), &mut buffer, &styles);

    let row = centred.top + 1;
    let origin = centred.frame.content_origin();
    let background = |cell: usize| {
        buffer
            .get(origin + cell, row)
            .map(|cell| cell.style().background)
            .expect("the cell exists")
    };
    let selection = styles.selected(Style::DEFAULT).background;
    let caret = styles.caret(Style::DEFAULT).background;
    assert_ne!(background(0), selection, "the wide character was marked");
    assert_eq!(background(2), selection, "`a` lost its selection");
    assert_ne!(background(3), selection, "the selection ran past `a`");
    // Past the end of the text: the caret sits on the blank after `b`.
    assert_eq!(
        background(4),
        caret,
        "the caret after the text was not drawn"
    );
}

#[test]
fn tab_shows_the_listing_as_text_and_enter_makes_a_new_line() {
    // The face's half of edit mode: the panel's text reaches the cells, and a
    // key that only the kernel editor understands lands in it.
    let directory = project("tui-explorer-edit");
    let mut panel = opened(&directory);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Tab)),
        ExplorerAction::Handled
    );
    assert!(panel.buffer_editor().is_some(), "Tab did not start editing");

    let screen = painted(&mut panel, 80, 24).join("\n");
    assert!(
        screen.contains("alpha.txt"),
        "the text was not drawn:\n{screen}"
    );

    let before = panel
        .buffer_editor()
        .map(|editor| editor.state().document.line_count())
        .expect("editing");
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter)),
        ExplorerAction::Handled
    );
    let after = panel
        .buffer_editor()
        .map(|editor| editor.state().document.line_count())
        .expect("still editing");
    assert_eq!(after, before + 1, "Enter did not make a new line");
}

#[test]
fn the_editors_clipboard_and_search_requests_reach_the_host() {
    let directory = project("tui-explorer-requests");
    let mut panel = opened(&directory);
    panel.handle_key(&press(KeyCode::Tab));
    let ctrl = |character| KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    };
    assert_eq!(panel.handle_key(&ctrl('v')), ExplorerAction::Paste);
    assert_eq!(panel.handle_key(&ctrl('f')), ExplorerAction::OpenSearch);
    assert!(
        matches!(panel.handle_key(&ctrl('c')), ExplorerAction::Copy(_)),
        "a copy from the text did not reach the host"
    );
}
