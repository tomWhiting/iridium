//! Whole-frame tests: build an editor, render, assert on cells.

use super::*;
use iridium_editor::syntax::Language;

use crate::cell::{CellContent, Color};

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

/// The background of one cell, or `None` when the row is short.
fn background_at(buffer: &CellBuffer, row: usize, column: usize) -> Option<Color> {
    buffer
        .row(row)?
        .get(column)
        .map(|cell| cell.style().background)
}

#[test]
fn the_caret_line_takes_the_current_line_background() {
    // The active-line background is the only thing distinguishing the line the
    // caret is on. Painting every line the same is a silent regression: the
    // frame still renders, the text is still right, and the editor simply
    // stops showing where you are.
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha\nbeta\ngamma");
    assert!(
        editor.state().config.highlight_current_line,
        "the default config must highlight the caret line for this to mean anything"
    );

    let mut buffer = CellBuffer::new(20, 5);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Status::default(), &mut buffer);
    let column = layout.text.origin;

    // Sampled five cells in, not at the origin: the caret sits at the origin
    // and paints its own background there, so the origin differs between the
    // two rows whatever the current-line setting does.
    let caret_row = background_at(&buffer, 0, column + 5);
    let other_row = background_at(&buffer, 1, column + 5);
    assert!(
        caret_row.is_some() && other_row.is_some(),
        "both rows painted"
    );
    assert_ne!(
        caret_row, other_row,
        "the caret line must be distinguishable from the line below it"
    );
}

#[test]
fn highlighting_repaints_when_the_document_changes() {
    // The cache is keyed on parse count *and* document revision. Dropping the
    // revision half leaves last generation's colours on screen after an edit.
    //
    // The sampled cell is on the second line deliberately: the caret starts at
    // the origin and paints its own style over that cell, so line 0 reports the
    // caret's colours rather than the text's.
    let mut editor = Editor::with_defaults();
    editor.set_language(Language::Rust);
    editor.set_content("\nlet alpha = 1;");

    let mut buffer = CellBuffer::new(40, 4);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Status::default(), &mut buffer);
    let column = layout.text.origin;
    let keyword = buffer
        .row(1)
        .and_then(|row| row.get(column))
        .map(|cell| cell.style().foreground);
    assert!(
        keyword.is_some_and(|fg| fg != Color::Default),
        "`let` must paint as a keyword for this test to mean anything, got {keyword:?}"
    );

    // Replacing the keyword with an identifier must change that cell: `let` is
    // a keyword and `xyz` is not.
    editor.set_content("\nxyz alpha = 1;");
    frame.render(&editor, Status::default(), &mut buffer);
    let identifier = buffer
        .row(1)
        .and_then(|row| row.get(column))
        .map(|cell| cell.style().foreground);

    assert_ne!(
        keyword, identifier,
        "a keyword replaced by an identifier must repaint in a different colour"
    );
}

#[test]
fn a_nested_span_wins_over_the_span_containing_it() {
    // Spans are painted outermost first so the innermost wins. Sorting by end
    // ascending instead paints the inner span first and lets the outer one
    // overwrite it, which shows up as a construct losing its interior colours.
    let mut editor = Editor::with_defaults();
    editor.set_language(Language::Rust);
    editor.set_content("\nfn main() { let x = 1; }");

    let mut buffer = CellBuffer::new(60, 4);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Status::default(), &mut buffer);
    let Some(row) = buffer.row(1) else {
        panic!("the second row must be painted");
    };
    let text = layout.text.origin;
    let colours: Vec<_> = row[text..text + 24]
        .iter()
        .map(|cell| cell.style().foreground)
        .collect();

    // `fn` and the `let` nested inside the body must both carry a keyword
    // colour, and it must differ from the colour of the identifier beside it.
    let outer_keyword = colours[0];
    let inner_keyword = colours[12];
    let identifier = colours[3];
    assert_ne!(
        outer_keyword,
        Color::Default,
        "`fn` must be highlighted, got {colours:?}"
    );
    assert_eq!(
        outer_keyword, inner_keyword,
        "a keyword nested in a block keeps its own colour, got {colours:?}"
    );
    assert_ne!(
        inner_keyword, identifier,
        "the nested keyword must not take the identifier colour, got {colours:?}"
    );
}
