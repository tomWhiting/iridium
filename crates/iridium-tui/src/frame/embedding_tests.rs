//! Public borrowed-frame rectangle, freshness and legacy chrome compatibility tests.

use super::{CellFrameError, CellFrameOptions, Chrome, Frame, Status};
use crate::cell::{CellBuffer, CellContent, Style, WriteOutcome};
use iridium_editor::cell_layout::ScreenRow;
use iridium_editor::{Editor, Position};

pub(super) type TestResult = Result<(), Box<dyn std::error::Error>>;

pub(super) fn options(columns: usize, rows: usize) -> CellFrameOptions<'static> {
    CellFrameOptions {
        columns,
        rows,
        first_row: ScreenRow(0),
        chrome: None,
    }
}

pub(super) fn row_text(buffer: &CellBuffer, row: usize) -> String {
    let mut text = String::new();
    if let Some(cells) = buffer.row(row) {
        for cell in cells {
            if let CellContent::Grapheme(grapheme) = cell.content() {
                grapheme.push_to(&mut text);
            }
        }
    }
    text
}

#[test]
fn cells_bare_degenerate_extents_and_parent_origin_are_local() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("a界b\nx");
    let mut frame = Frame::new();
    for (columns, rows) in [(0, 0), (0, 3), (4, 0), (1, 1), (4, 3)] {
        let prepared = Frame::prepare_cells(&editor, options(columns, rows))?;
        let mut local = CellBuffer::new(columns, rows);
        let layout = frame.render_cells(&prepared, &mut local)?;
        assert_eq!(layout.text.width, columns);
        assert_eq!(layout.text_rows, rows);
        assert_eq!(layout.text.origin, 0);
        assert_eq!(layout.status_row, None);
        assert_eq!(layout.search_rows, None);
        assert_eq!(layout.gutter_width, 0);
        if columns == 0 || rows == 0 {
            assert_eq!(layout.caret(), None);
        }
        let mut parent = CellBuffer::new(columns + 4, rows + 4);
        for row in 0..parent.height() {
            parent.set_str(0, row, &"#".repeat(parent.width()), Style::DEFAULT);
        }
        for row in 0..rows {
            for column in 0..columns {
                let cell = local.get(column, row).ok_or("missing local cell")?;
                if let CellContent::Grapheme(grapheme) = cell.content() {
                    assert!(matches!(
                        parent.set_grapheme(column + 2, row + 1, grapheme, cell.style()),
                        WriteOutcome::Written(_)
                    ));
                }
            }
        }
        for row in 0..parent.height() {
            for column in 0..parent.width() {
                if !(1..rows + 1).contains(&row) || !(2..columns + 2).contains(&column) {
                    assert_eq!(
                        parent
                            .get(column, row)
                            .ok_or("missing parent cell")?
                            .content()
                            .clone(),
                        CellContent::Grapheme(
                            crate::cell::Grapheme::new("#").ok_or("sentinel invalid")?
                        )
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn cells_extent_mismatch_precedes_every_write_and_resize_preserves_history() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.paste_cells("abcd界\nlast")?;
    let node = editor.current_history_node();
    let cursor = editor.cursor();
    let prepared = Frame::prepare_cells(&editor, options(4, 2))?;
    let mut buffer = CellBuffer::new(5, 2);
    buffer.set_str(0, 0, "#####", Style::DEFAULT);
    let before = buffer.clone();
    assert!(matches!(
        Frame::new().render_cells(&prepared, &mut buffer),
        Err(CellFrameError::ExtentMismatch {
            prepared: (4, 2),
            buffer: (5, 2)
        })
    ));
    assert_eq!(buffer, before);
    drop(prepared);
    let prepared = Frame::prepare_cells(&editor, options(8, 4))?;
    Frame::new().render_cells(&prepared, &mut CellBuffer::new(8, 4))?;
    drop(prepared);
    assert_eq!(editor.current_history_node(), node);
    assert_eq!(editor.cursor(), cursor);
    assert!(editor.undo());
    assert_eq!(editor.content(), "");
    Ok(())
}

#[test]
fn cells_default_chrome_keeps_fixed_legacy_rows_and_allocations() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("abc\nxy");
    let chrome = Chrome {
        status: Status {
            name: Some("draft"),
            dirty: true,
        },
        sidebar_columns: 2,
        search: None,
    };
    let mut old = CellBuffer::new(20, 4);
    let legacy = Frame::new().render(&editor, chrome, &mut old);
    assert_eq!(row_text(&old, 0), "   1  abc           ");
    assert_eq!(row_text(&old, 1), "   2  xy            ");
    assert!(row_text(&old, 3).starts_with(" draft [+]"));
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            chrome: Some(chrome),
            ..options(20, 4)
        },
    )?;
    let mut cells = CellBuffer::new(20, 4);
    let layout = Frame::new().render_cells(&prepared, &mut cells)?;
    assert_eq!(cells, old);
    assert_eq!(layout.text, legacy.text);
    assert_eq!(layout.text_rows, 3);
    assert_eq!(layout.status_row, Some(3));
    assert_eq!(prepared.position_at(3, 0)?, None);
    assert_eq!(
        prepared
            .position_at(6, 0)?
            .ok_or("text hit missing")?
            .position,
        Position::zero()
    );
    drop(prepared);
    editor.state_mut().config.show_line_numbers = false;
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            chrome: Some(chrome),
            ..options(20, 4)
        },
    )?;
    assert_eq!(prepared.layout().gutter_width, 0);
    assert_eq!(prepared.layout().text.origin, 2);
    Ok(())
}

#[test]
fn cells_search_chrome_and_legacy_horizontal_clip_keep_fixed_text() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("a界b\nnext");
    editor.state_mut().config.show_line_numbers = false;
    editor.state_mut().viewport.scroll_offset_x = 2.0;
    let mut clipped = CellBuffer::new(4, 3);
    Frame::new().render(&editor, Chrome::default(), &mut clipped);
    assert_eq!(row_text(&clipped, 0), " b  ");
    editor.state_mut().viewport.scroll_offset_x = 0.0;
    let overlay = super::SearchOverlay::new(&iridium_editor::commands::Keymap::new("fixture"));
    let chrome = Chrome {
        search: Some(&overlay),
        ..Chrome::default()
    };
    let mut legacy = CellBuffer::new(40, 6);
    let old_layout = Frame::new().render(&editor, chrome, &mut legacy);
    assert_eq!(old_layout.search_rows, Some((3, 2)));
    assert!(row_text(&legacy, 3).starts_with(" Find    "));
    assert!(row_text(&legacy, 4).starts_with(" Replace "));
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            chrome: Some(chrome),
            ..options(40, 6)
        },
    )?;
    let mut cells = CellBuffer::new(40, 6);
    let layout = Frame::new().render_cells(&prepared, &mut cells)?;
    assert_eq!(cells, legacy);
    assert_eq!(layout.search_rows, old_layout.search_rows);
    assert_eq!(layout.search_caret, old_layout.search_caret);
    assert_eq!(layout.caret(), old_layout.caret());
    assert_eq!(prepared.position_at(10, 3)?, None);
    Ok(())
}

#[test]
fn cells_legacy_line_measurement_keeps_control_clusters_and_scalar_spans() {
    let line = super::LineLayout::new("a\r\n\t界e\u{301}", 4);
    assert_eq!(line.width(), 7);
    assert_eq!(line.char_count(), 7);
    assert_eq!(line.byte_count(), 10);
    let runs: Vec<_> = line
        .clusters()
        .iter()
        .map(|cluster| {
            (
                cluster.column(),
                cluster.width(),
                cluster.char_index(),
                cluster.char_count(),
                cluster.byte_index(),
                cluster.byte_count(),
            )
        })
        .collect();
    assert_eq!(
        runs,
        [
            (0, 1, 0, 1, 0, 1),
            (1, 0, 1, 2, 1, 2),
            (1, 3, 3, 1, 3, 1),
            (4, 2, 4, 1, 4, 3),
            (6, 1, 5, 2, 7, 3)
        ]
    );
}
