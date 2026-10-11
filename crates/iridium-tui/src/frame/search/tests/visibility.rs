//! Search visibility comes from the same clipped, folded cells as its preview.
use super::*;
use crate::cell::Attributes;
use iridium_editor::{Selection, syntax::Language};

#[test]
fn offscreen_count_follows_scroll_resize_and_chrome_rows() {
    let mut editor = editor_with(&"foo\n".repeat(12));
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert!(
        render(&editor, &overlay, 80, 8)
            .find_row()
            .contains("7 off-screen")
    );
    assert!(
        render(&editor, &overlay, 80, 12)
            .find_row()
            .contains("3 off-screen")
    );
    editor.state_mut().viewport.first_line = 10;
    assert!(
        render(&editor, &overlay, 80, 8)
            .find_row()
            .contains("10 off-screen")
    );
    let mut buffer = CellBuffer::new(80, 8);
    let layout = Frame::new().render(
        &editor,
        Chrome {
            search: Some(&overlay),
            tab_bar: true,
            ..Chrome::default()
        },
        &mut buffer,
    );
    assert_eq!(layout.text_rows, 4);
    editor.state_mut().viewport.first_line = 0;
    let layout = Frame::new().render(
        &editor,
        Chrome {
            search: Some(&overlay),
            tab_bar: true,
            ..Chrome::default()
        },
        &mut buffer,
    );
    let screen = Screen { buffer, layout };
    assert!(screen.find_row().contains("8 off-screen"));
}

#[test]
fn horizontally_clipped_matches_and_wide_glyphs_are_counted() {
    let source = format!("猫{}猫", " ".repeat(100));
    let mut editor = editor_with(&source);
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "猫");
    assert!(
        render(&editor, &overlay, 80, 8)
            .find_row()
            .contains("1 off-screen")
    );
    editor.state_mut().viewport.scroll_offset_x = 50.0;
    assert!(
        render(&editor, &overlay, 80, 8)
            .find_row()
            .contains("1 off-screen")
    );
    editor.state_mut().viewport.scroll_offset_x = 110.0;
    assert!(
        render(&editor, &overlay, 80, 8)
            .find_row()
            .contains("2 off-screen")
    );
}

#[test]
fn hidden_fold_matches_are_not_counted_as_visible() {
    let mut editor = editor_with("fn foo() {\n    foo();\n    foo();\n}\nfoo();");
    editor.set_language(Language::Rust);
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    assert!(editor.fold_at(0));
    let folded = render(&editor, &overlay, 80, 10);
    assert!(
        folded.find_row().contains("2 off-screen"),
        "{}",
        folded.find_row()
    );
    assert!(editor.unfold_at(0));
    assert!(
        !render(&editor, &overlay, 80, 10)
            .find_row()
            .contains("off-screen")
    );
}

#[test]
fn multiline_matches_are_counted_once_and_end_boundaries_are_exclusive() {
    let mut editor = editor_with("a\nb\na\nb\na\nb");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    overlay.paste("a\\nb", &mut editor);
    let first = render(&editor, &overlay, 80, 6);
    assert!(
        first.find_row().contains("1 off-screen"),
        "{}",
        first.find_row()
    );
    editor.state_mut().viewport.first_line = 1;
    let partial = render(&editor, &overlay, 80, 6);
    assert!(partial.find_row().contains("1 off-screen"));
    // A match ending at column zero contributes no character to that line.
    overlay.handle_key(
        &KeyEvent::new(KeyCode::Char('a'), Modifiers::ctrl()),
        &mut editor,
    );
    overlay.paste("a\\n", &mut editor);
    editor.state_mut().viewport.first_line = 1;
    let boundary = render(&editor, &overlay, 80, 4);
    assert!(
        boundary.find_row().contains("3 off-screen"),
        "{}",
        boundary.find_row()
    );
}

#[test]
fn empty_regex_matches_have_position_markers_without_repainting_glyphs() {
    let mut editor = editor_with("cat\ndog\nyak");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    overlay.paste("(?m)$", &mut editor);
    let screen = render(&editor, &overlay, 80, 5);
    assert!(screen.find_row().contains("1 off-screen"));
    let origin = screen.layout.text.origin;
    let row = screen.buffer.row(1).expect("second document row");
    assert!(
        row[origin + 3]
            .style()
            .attributes
            .contains(Attributes::UNDERLINE)
    );
    assert!(
        !row[origin + 2]
            .style()
            .attributes
            .contains(Attributes::UNDERLINE)
    );
    assert_eq!(editor.content(), "cat\ndog\nyak");
    editor.state_mut().viewport.scroll_offset_x = 4.0;
    assert!(
        render(&editor, &overlay, 80, 5)
            .find_row()
            .contains("3 off-screen")
    );
}

#[test]
fn overlapping_matches_keep_current_priority_and_count_individually() {
    let mut editor = editor_with("aaa\naaa\naaa");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "aa");
    assert_eq!(editor.search_match_count(), 6);
    let screen = render(&editor, &overlay, 80, 5);
    assert!(screen.find_row().contains("2 off-screen"));
    press(&mut overlay, &mut editor, KeyCode::Down);
    assert!(
        render(&editor, &overlay, 80, 5)
            .find_row()
            .contains("2 of 6")
    );
}

#[test]
fn the_primary_caret_is_marked_among_reversed_and_forward_selections() {
    let mut editor = editor_with("alpha\nbeta");
    editor.set_selection(Position::new(0, 5), Position::new(0, 1));
    editor
        .state_mut()
        .cursor
        .add_cursor(Selection::new(Position::new(1, 0), Position::new(1, 3)));
    let mut buffer = CellBuffer::new(80, 8);
    let layout = Frame::new().render(&editor, Chrome::default(), &mut buffer);
    let primary = buffer.row(0).expect("primary line")[layout.text.origin + 1].style();
    let secondary = buffer.row(1).expect("secondary line")[layout.text.origin + 3].style();
    assert!(primary.attributes.contains(Attributes::UNDERLINE));
    assert!(!secondary.attributes.contains(Attributes::UNDERLINE));
    assert_eq!(primary.background, secondary.background);
    assert_eq!(
        layout.primary_caret,
        Some(crate::frame::CellPosition {
            column: layout.text.origin + 1,
            row: 0
        })
    );
}

#[test]
fn narrow_panels_keep_the_count_and_invalid_queries_clear_visibility() {
    let mut editor = editor_with(&"foo\n".repeat(12));
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");
    let narrow = render(&editor, &overlay, 30, 8);
    assert!(narrow.find_row().contains("1 of 12"));
    assert!(!narrow.find_row().contains("off-screen"));
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    overlay.paste("[", &mut editor);
    let invalid = render(&editor, &overlay, 80, 8);
    assert!(invalid.find_row().contains("Invalid regex"));
    assert!(!invalid.find_row().contains("off-screen"));
    assert!(invalid.message_row().contains("Invalid"));
}
