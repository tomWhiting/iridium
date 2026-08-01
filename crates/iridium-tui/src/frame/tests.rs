//! Whole-frame tests: build an editor, render, assert on cells.

use super::*;
use crate::cell::CellContent;

/// The text of a row, with continuation cells contributing nothing.
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

#[test]
fn a_document_is_painted_line_by_line() {
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha\nbeta\ngamma");
    let mut buffer = CellBuffer::new(20, 5);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Status::default(), &mut buffer);

    assert_eq!(layout.gutter_width, 4);
    assert_eq!(&row_text(&buffer, 0)[..8], " 1  alph");
    assert!(row_text(&buffer, 1).starts_with(" 2  beta"));
    assert!(row_text(&buffer, 2).starts_with(" 3  gamma"));
}
