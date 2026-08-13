//! Where the panel lands on the frame, and the keys it deliberately leaves
//! to the document.
//!
//! Split out of a 1,136-line `tests.rs` for #92, on the rules the file
//! already carried. The fixtures and the [`Screen`](super::Screen) reader are
//! in [`super`].

use super::*;

// ==================== Geometry ====================

#[test]
fn the_panel_takes_its_rows_from_the_document() {
    let editor = editor_with("one\ntwo\nthree");
    let overlay = SearchOverlay::new(&iridium_editor::Keymap::new("empty"));

    let without = Frame::layout(&editor, 60, 8, Chrome::default());
    assert_eq!(without.text_rows, 7);
    assert_eq!(without.search_rows, None);

    let with = Frame::layout(
        &editor,
        60,
        8,
        Chrome {
            status: crate::frame::Status::default(),
            search: Some(&overlay),
            sidebar_columns: 0,
        },
    );
    assert_eq!(with.text_rows, 5, "two rows go to the panel");
    assert_eq!(with.search_rows, Some((5, 2)));
    assert_eq!(with.status_row, Some(7));
}

#[test]
fn the_kernels_viewport_shrinks_with_the_panel() {
    // A viewport that still claimed the panel's rows would let the kernel
    // scroll a match underneath the panel that found it.
    let mut editor = editor_with("one\ntwo\nthree");
    let overlay = SearchOverlay::new(&iridium_editor::Keymap::new("empty"));
    let chrome = Chrome {
        status: crate::frame::Status::default(),
        search: Some(&overlay),
        sidebar_columns: 0,
    };

    Frame::sync_viewport(&mut editor, 60, 8, Chrome::default());
    assert_eq!(editor.state().viewport.visible_lines, 7);

    Frame::sync_viewport(&mut editor, 60, 8, chrome);
    assert_eq!(editor.state().viewport.visible_lines, 5);
}

#[test]
fn the_caret_belongs_to_the_focused_field_while_the_panel_is_open() {
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 8);
    let (first, _) = screen.layout.search_rows.expect("panel");
    let caret = screen.layout.caret().expect("caret");
    assert_eq!(caret.row, first, "not the document's caret");
    assert_eq!(screen.layout.search_caret, Some(caret));

    press(&mut overlay, &mut editor, KeyCode::Tab);
    let switched = render(&editor, &overlay, 60, 8);
    assert_eq!(
        switched.layout.caret().map(|position| position.row),
        Some(first + 1),
        "the caret must follow the focus to the replacement field"
    );
}

#[test]
fn a_screen_with_room_for_one_panel_row_shows_the_focused_field() {
    // Keystrokes must never go into a field the user cannot see.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 2);
    assert_eq!(screen.layout.search_rows, Some((0, 1)));
    assert!(
        screen.find_row().starts_with(" Find"),
        "{}",
        screen.find_row()
    );

    press(&mut overlay, &mut editor, KeyCode::Tab);
    let switched = render(&editor, &overlay, 60, 2);
    assert!(
        switched.find_row().starts_with(" Replace"),
        "{}",
        switched.find_row()
    );
}

#[test]
fn a_screen_with_no_room_at_all_still_renders() {
    let mut editor = editor_with("foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let screen = render(&editor, &overlay, 60, 1);
    assert_eq!(screen.layout.search_rows, None);
    assert_eq!(screen.layout.text_rows, 0);
    assert_eq!(screen.layout.caret(), None);
}

#[test]
fn a_narrow_panel_drops_the_toggles_before_the_count() {
    // The count answers the question being asked; the toggles only say how it
    // was asked. Losing the query text to either of them would be worse still.
    let mut editor = editor_with("foo bar foo");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foo");

    let wide = render(&editor, &overlay, 60, 8).find_row();
    assert!(wide.contains("Aa") && wide.contains("1 of 2"), "{wide}");

    // Both widths leave room for the count and not for the toggles beside it,
    // and 36 is the width where the toggles would fit *instead* of the count if
    // the priority between them were the other way round.
    for width in [30, 36] {
        let narrow = render(&editor, &overlay, width, 8).find_row();
        assert!(narrow.contains("1 of 2"), "at {width}: {narrow}");
        assert!(!narrow.contains("Aa"), "at {width}: {narrow}");
        assert!(
            narrow.contains("foo"),
            "the query itself must survive at {width}: {narrow}"
        );
    }
}

#[test]
fn a_panel_too_narrow_for_the_count_keeps_the_query_whole() {
    // The field's floor is what stops the chrome eating the query. Without it
    // the count is drawn and the query scrolls away behind it — the panel then
    // answers a question whose text the user can no longer read.
    let mut editor = editor_with("foobarbaz");
    let mut overlay = opened(&mut editor);
    type_text(&mut overlay, &mut editor, "foobarbaz");

    let row = render(&editor, &overlay, 20, 8).find_row();
    assert!(row.contains("foobarbaz"), "{row}");
    assert!(!row.contains("1 of 1"), "{row}");
}

#[test]
fn a_long_engine_message_is_clipped_rather_than_dropped() {
    let mut editor = editor_with("abc");
    let mut overlay = opened(&mut editor);
    chord(&mut overlay, &mut editor, KeyCode::Char('r'), alt());
    type_text(&mut overlay, &mut editor, "[a-z");

    let row = render(&editor, &overlay, 40, 8).message_row();
    assert!(
        row.contains("Invalid"),
        "the beginning of the complaint is worth more than nothing: {row}"
    );
}

// ==================== What the panel does not bind ====================

#[test]
fn a_key_the_panel_does_not_bind_is_left_to_the_host() {
    let mut editor = editor_with("foo");
    let mut overlay = opened(&mut editor);

    let unbound = [
        (KeyCode::Char('s'), Modifiers::ctrl()),
        (KeyCode::Char('q'), alt()),
        (KeyCode::F1, Modifiers::none()),
        (KeyCode::PageDown, Modifiers::none()),
        (
            KeyCode::Char('a'),
            Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: true,
                alt_graph: false,
            },
        ),
    ];
    for (code, modifiers) in unbound {
        assert_eq!(
            chord(&mut overlay, &mut editor, code, modifiers),
            SearchOutcome::Ignored,
            "{code:?} with {modifiers:?} must stay the host's"
        );
    }
    assert!(
        overlay.query().is_empty(),
        "and none of them typed anything"
    );
}

#[test]
fn a_shifted_letter_types_the_letter_it_is() {
    let mut editor = editor_with("Foo");
    let mut overlay = opened(&mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('F'),
        Modifiers::shift(),
    );
    assert_eq!(overlay.query(), "F");
}
