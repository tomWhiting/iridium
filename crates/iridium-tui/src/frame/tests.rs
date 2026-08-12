//! Whole-frame tests: build an editor, render, assert on cells.

use super::*;
use iridium_editor::syntax::Language;

use crate::cell::{Cell, CellContent, Color};

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
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);

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
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);
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
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);
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
    frame.render(&editor, Chrome::default(), &mut buffer);
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
fn search_matches_are_marked_without_any_panel_being_open() {
    // The highlighting reads the kernel's search state, not the overlay's, so
    // a host that drives `find` from a keybinding and never opens a panel still
    // gets its matches marked. A face that keyed the marks off its own UI state
    // would show nothing here.
    let mut editor = Editor::with_defaults();
    editor.set_content("\nfoo bar foo");
    editor
        .find("foo", &iridium_editor::search::SearchOptions::default())
        .expect("a literal query cannot fail");

    let mut buffer = CellBuffer::new(40, 6);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);
    assert!(layout.search_rows.is_none(), "no panel was asked for");

    let text = layout.text.origin;
    let background = |offset: usize| {
        buffer
            .row(1)
            .and_then(|row| row.get(text + offset))
            .map(|cell| cell.style().background)
    };
    assert_ne!(background(0), background(4), "the first match is marked");
    assert_ne!(background(8), background(4), "and so is the second");
}

#[test]
fn a_search_match_keeps_the_syntax_colour_of_the_code_it_found() {
    // Only the background is a match's to change. Painting the foreground too
    // would hide the very code the search was run to read, and would do it
    // silently — the match is still marked and the count is still right.
    let mut editor = Editor::with_defaults();
    editor.set_language(Language::Rust);
    editor.set_content("\nlet alpha = 1;");

    let mut buffer = CellBuffer::new(40, 6);
    let mut frame = Frame::new();
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);
    let text = layout.text.origin;
    // Sampled one cell in: `find` parks the caret on the match's first cell,
    // and a caret paints its own colours over whatever it sits on.
    let keyword = buffer
        .row(1)
        .and_then(|row| row.get(text + 1))
        .map(|cell| cell.style().foreground);
    assert!(
        keyword.is_some_and(|colour| colour != Color::Default),
        "`let` must paint as a keyword for this test to mean anything"
    );

    editor
        .find("let", &iridium_editor::search::SearchOptions::default())
        .expect("a literal query cannot fail");
    frame.render(&editor, Chrome::default(), &mut buffer);
    let matched = buffer
        .row(1)
        .and_then(|row| row.get(text + 1))
        .map(Cell::style);

    assert_eq!(
        matched.map(|style| style.foreground),
        keyword,
        "the keyword must keep its colour under the match background"
    );
    assert!(
        matched.is_some_and(|style| style.background != Color::Default),
        "and it must still be visibly marked"
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
    let layout = frame.render(&editor, Chrome::default(), &mut buffer);
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

#[test]
fn a_left_band_moves_the_gutter_and_shrinks_the_text_area() {
    // R4: the band takes columns *from* the document. Three measures follow it
    // — the gutter's origin, the text area's origin and width — and this is
    // the test that says all three moved together rather than one of them.
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha\nbeta\ngamma");

    let bare = Frame::layout(&editor, 40, 6, Chrome::default());
    let banded = Frame::layout(
        &editor,
        40,
        6,
        Chrome {
            sidebar_columns: 12,
            ..Chrome::default()
        },
    );

    assert_eq!(banded.sidebar_columns, 12);
    assert_eq!(
        banded.gutter_width, bare.gutter_width,
        "the band must not change how many digits a line number needs"
    );
    assert_eq!(
        banded.text.origin,
        12 + bare.text.origin,
        "the gutter did not start after the band"
    );
    assert_eq!(
        banded.text.width,
        bare.text.width - 12,
        "the text area kept columns the band had taken"
    );
}

#[test]
fn a_left_band_shrinks_the_kernels_viewport_too() {
    // ⭐ The same defect the search panel has on the other axis: a viewport
    // that still claimed the band's columns would let the kernel scroll text
    // underneath the panel that took them.
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha\nbeta\ngamma");

    let bare = Frame::layout(&editor, 40, 6, Chrome::default());
    let banded = Frame::layout(
        &editor,
        40,
        6,
        Chrome {
            sidebar_columns: 12,
            ..Chrome::default()
        },
    );
    assert!(
        banded.viewport.width < bare.viewport.width,
        "the kernel's viewport still described the full width: {} vs {}",
        banded.viewport.width,
        bare.viewport.width
    );

    Frame::sync_viewport(
        &mut editor,
        40,
        6,
        Chrome {
            sidebar_columns: 12,
            ..Chrome::default()
        },
    );
    // Both sides are `cell_units` of a whole column count, so they are exactly
    // representable and an epsilon compare is a lint requirement rather than a
    // tolerance the arithmetic needs.
    assert!(
        (editor.state().viewport.width - banded.viewport.width).abs() < f32::EPSILON,
        "the kernel's viewport width was not written from the banded layout"
    );
}

#[test]
fn a_band_as_wide_as_the_screen_leaves_nothing_rather_than_panicking() {
    // Arithmetic with no guard of its own: everything downstream is a zero-width
    // area, which paints no cells and places no caret. A band WIDER than the
    // screen is clamped to it rather than wrapping a subtraction.
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha");

    for band in [40, 100] {
        let layout = Frame::layout(
            &editor,
            40,
            6,
            Chrome {
                sidebar_columns: band,
                ..Chrome::default()
            },
        );
        assert_eq!(layout.sidebar_columns, 40, "band {band} was not clamped");
        assert_eq!(layout.gutter_width, 0);
        assert_eq!(layout.text.width, 0);
        assert_eq!(layout.primary_caret, None);

        let mut buffer = CellBuffer::new(40, 6);
        let mut frame = Frame::new();
        frame.render(
            &editor,
            Chrome {
                sidebar_columns: band,
                ..Chrome::default()
            },
            &mut buffer,
        );
    }
}

#[test]
fn the_gutter_paints_after_the_band_and_not_underneath_it() {
    // ⚠️ `gutter::width` returns a width and never an origin, so until the band
    // existed the gutter wrote at absolute column zero. That is exactly the
    // assumption a band breaks, and this reads the cells back rather than the
    // geometry: sabotaging `GutterArea::origin` to zero puts " 1" under the
    // panel and fails here.
    let mut editor = Editor::with_defaults();
    editor.set_content("alpha\nbeta");

    let mut buffer = CellBuffer::new(40, 6);
    let mut frame = Frame::new();
    frame.render(
        &editor,
        Chrome {
            sidebar_columns: 12,
            ..Chrome::default()
        },
        &mut buffer,
    );

    let row = row_text(&buffer, 0);
    let cells: Vec<char> = row.chars().collect();
    assert!(
        cells[..12].iter().all(|glyph| *glyph == ' '),
        "something was painted in the band's own columns: {row:?}"
    );
    assert!(
        row.trim_start().starts_with("1  alpha"),
        "the line number did not land after the band: {row:?}"
    );
}
