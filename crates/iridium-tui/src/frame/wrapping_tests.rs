//! Paint and hit-test share source-preserving wrapped rows.
use super::*;
use crate::cell::CellContent;
use iridium_editor::{EditorConfig, Position};
fn row(buffer: &CellBuffer, index: usize) -> String {
    let mut result = String::new();
    for cell in buffer.row(index).expect("row") {
        if let CellContent::Grapheme(g) = cell.content() {
            g.push_to(&mut result);
        }
    }
    result
}
fn editor(text: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig {
        word_wrap: true,
        show_line_numbers: false,
        ..EditorConfig::default()
    });
    editor.set_content(text);
    editor
}
#[test]
fn paints_words_with_no_source_loss_and_hits_continuations() {
    let editor = editor("hello wonderful world");
    let mut buffer = CellBuffer::new(8, 6);
    let geometry = Frame::new().render(&editor, Chrome::default(), &mut buffer);
    assert_eq!(row(&buffer, 0), "hello   ");
    assert_eq!(row(&buffer, 1), "wonderfu");
    assert_eq!(row(&buffer, 2), "l world ");
    assert_eq!(
        geometry.document_position(&editor, CellPosition { column: 3, row: 1 }),
        Some(Position::new(0, 9))
    );
    assert_eq!(
        geometry.document_position(&editor, CellPosition { column: 7, row: 0 }),
        Some(Position::new(0, 6))
    );
}
#[test]
fn gutter_is_only_numbered_once_and_never_becomes_source() {
    let mut editor = editor("abcdefghijk");
    let mut config = editor.state().config.clone();
    config.show_line_numbers = true;
    editor.set_config(config);
    let mut buffer = CellBuffer::new(10, 6);
    let geometry = Frame::new().render(&editor, Chrome::default(), &mut buffer);
    assert_eq!(row(&buffer, 0), " 1  abcdef");
    assert_eq!(row(&buffer, 1), "    ghijk ");
    assert_eq!(
        geometry.document_position(&editor, CellPosition { column: 0, row: 1 }),
        Some(Position::new(0, 6))
    );
}
#[test]
fn scrolling_continuations_moves_hits_and_caret_together() {
    let mut editor = editor("abcdefghijklmnopqrstuvwxyz");
    let mut buffer = CellBuffer::new(8, 3);
    Frame::sync_viewport(&mut editor, 8, 3, Chrome::default());
    let geometry = Frame::layout(&editor, 8, 3, Chrome::default());
    geometry.scroll_rows(&mut editor, true, 2).expect("scroll");
    let geometry = Frame::new().render(&editor, Chrome::default(), &mut buffer);
    assert_eq!(row(&buffer, 0), "qrstuvwx");
    assert_eq!(
        geometry.document_position(&editor, CellPosition { column: 1, row: 0 }),
        Some(Position::new(0, 17))
    );
    assert!(geometry.primary_caret.is_none());
    geometry.reveal_caret(&mut editor).expect("reveal");
    assert_eq!(
        Frame::layout(&editor, 8, 3, Chrome::default()).primary_caret,
        Some(CellPosition { column: 0, row: 0 })
    );
}
#[test]
fn explicit_horizontal_scroll_does_not_move_the_selection() {
    let mut editor = editor("abcdefghijklmnopqrstuvwxyz");
    let mut config = editor.state().config.clone();
    config.word_wrap = false;
    editor.set_config(config);
    Frame::sync_viewport(&mut editor, 8, 4, Chrome::default());
    let geometry = Frame::layout(&editor, 8, 4, Chrome::default());
    geometry
        .scroll_columns(&mut editor, true, 7)
        .expect("scroll");
    let mut buffer = CellBuffer::new(8, 4);
    let geometry = Frame::new().render(&editor, Chrome::default(), &mut buffer);
    assert_eq!(row(&buffer, 0), "hijklmno");
    assert_eq!(editor.cursor(), Position::new(0, 0));
    assert_eq!(
        geometry.document_position(&editor, CellPosition { column: 2, row: 0 }),
        Some(Position::new(0, 9))
    );
}
