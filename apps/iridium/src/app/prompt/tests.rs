use iridium_editor::Theme;
use iridium_tui::cell::CellContent;

use super::*;

/// A family emoji: seven `char`s, one cluster, two cells.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

/// A plain key press.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// Types a string into a prompt, one plain key press per character.
fn type_into(prompt: &mut Prompt, text: &str) {
    for character in text.chars() {
        assert_eq!(
            prompt.answer(&press(KeyCode::Char(character))),
            Answer::Pending
        );
    }
}

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
fn a_line_number_is_typed_and_submitted() {
    let mut prompt = Prompt::goto_line();
    type_into(&mut prompt, "412");
    assert_eq!(prompt.line(), "Go to line: 412");
    assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Goto(412));
}

#[test]
fn the_line_prompt_takes_digits_and_nothing_else() {
    let mut prompt = Prompt::goto_line();
    type_into(&mut prompt, "1a2-3 4");
    assert_eq!(prompt.line(), "Go to line: 1234");
}

#[test]
fn a_line_number_larger_than_usize_becomes_the_last_line() {
    // Not an error the user can act on: they asked for the end of the
    // file, and the caller clamps it there.
    let mut prompt = Prompt::goto_line();
    type_into(&mut prompt, "99999999999999999999999999");
    assert_eq!(
        prompt.answer(&press(KeyCode::Enter)),
        Answer::Goto(usize::MAX)
    );
}

#[test]
fn submitting_an_empty_field_cancels() {
    let mut prompt = Prompt::goto_line();
    assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Cancelled);
    let mut prompt = Prompt::save_as();
    assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Cancelled);
}

#[test]
fn escape_cancels_every_prompt() {
    for mut prompt in [
        Prompt::goto_line(),
        Prompt::save_as(),
        Prompt::confirm("Quit? (y/n)", Deed::Quit),
    ] {
        assert_eq!(prompt.answer(&press(KeyCode::Escape)), Answer::Cancelled);
    }
}

#[test]
fn a_confirmation_answers_only_yes_and_no() {
    let mut prompt = Prompt::confirm("Quit? (y/n)", Deed::Quit);
    for key in [
        KeyCode::Char('q'),
        KeyCode::Enter,
        KeyCode::Backspace,
        KeyCode::Left,
    ] {
        assert_eq!(
            prompt.answer(&press(key)),
            Answer::Pending,
            "{key:?} must be swallowed"
        );
    }
    assert_eq!(prompt.answer(&press(KeyCode::Char('n'))), Answer::Cancelled);
    assert_eq!(
        prompt.answer(&press(KeyCode::Char('Y'))),
        Answer::Do(Deed::Quit)
    );
}

#[test]
fn a_chord_is_swallowed_rather_than_reaching_the_document() {
    // The whole point of a modal prompt: a save chord typed into a
    // question about leaving must not save.
    let mut prompt = Prompt::save_as();
    let chord = KeyEvent {
        key: KeyCode::Char('s'),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    };
    assert_eq!(prompt.answer(&chord), Answer::Pending);
    assert_eq!(prompt.line(), "Save as: ");
}

#[test]
fn a_name_is_typed_and_becomes_a_path() {
    let mut prompt = Prompt::save_as();
    type_into(&mut prompt, "notes.md");
    assert_eq!(
        prompt.answer(&press(KeyCode::Enter)),
        Answer::SaveAs(PathBuf::from("notes.md"))
    );
}

#[test]
fn editing_a_name_moves_by_whole_clusters() {
    // Removing one `char` of a joined emoji would leave a dangling zero
    // width joiner in the file name.
    let mut prompt = Prompt::save_as();
    type_into(&mut prompt, FAMILY);
    assert_eq!(prompt.answer(&press(KeyCode::Backspace)), Answer::Pending);
    assert_eq!(prompt.line(), "Save as: ");
}

#[test]
fn a_paste_is_filtered_rather_than_refused() {
    let mut prompt = Prompt::save_as();
    prompt.paste("notes\n.md");
    assert_eq!(prompt.line(), "Save as: notes.md");

    let mut prompt = Prompt::goto_line();
    prompt.paste("line 42");
    assert_eq!(prompt.line(), "Go to line: 42");

    let mut prompt = Prompt::confirm("Quit? (y/n)", Deed::Quit);
    prompt.paste("yes");
    assert_eq!(prompt.line(), "Quit? (y/n)");
}

#[test]
fn the_caret_counts_cells_not_chars() {
    let mut prompt = Prompt::save_as();
    type_into(&mut prompt, "漢");
    // "Save as: " is nine cells, and one ideograph is two more.
    assert_eq!(prompt.caret_cell(), Some(11));
}

#[test]
fn a_confirmation_has_no_caret() {
    assert_eq!(
        Prompt::confirm("Quit? (y/n)", Deed::Quit).caret_cell(),
        None
    );
}

#[test]
fn the_caret_moves_and_insertion_lands_where_it_is() {
    let mut prompt = Prompt::save_as();
    type_into(&mut prompt, "ac");
    assert_eq!(prompt.answer(&press(KeyCode::Left)), Answer::Pending);
    type_into(&mut prompt, "b");
    assert_eq!(prompt.line(), "Save as: abc");

    assert_eq!(prompt.answer(&press(KeyCode::Home)), Answer::Pending);
    assert_eq!(prompt.answer(&press(KeyCode::Delete)), Answer::Pending);
    assert_eq!(prompt.line(), "Save as: bc");

    assert_eq!(prompt.answer(&press(KeyCode::End)), Answer::Pending);
    assert_eq!(prompt.caret_cell(), Some(11));
}

#[test]
fn painting_fills_the_row_and_reports_the_caret() {
    let palette = Palette::from_theme(&Theme::dark());
    let mut buffer = CellBuffer::new(20, 1);
    let mut prompt = Prompt::goto_line();
    type_into(&mut prompt, "42");

    let caret = prompt.paint(&mut buffer, 0, &palette);
    assert_eq!(row_text(&buffer, 0), "Go to line: 42      ");
    assert_eq!(caret, Some(14));
}

#[test]
fn a_line_wider_than_the_row_scrolls_to_keep_the_caret_visible() {
    let palette = Palette::from_theme(&Theme::dark());
    let mut buffer = CellBuffer::new(12, 1);
    let mut prompt = Prompt::save_as();
    type_into(&mut prompt, "abcdef");

    // "Save as: abcdef" is fifteen cells and the row holds twelve, so the
    // first four are dropped and the caret lands in the last cell.
    let caret = prompt.paint(&mut buffer, 0, &palette);
    assert_eq!(row_text(&buffer, 0), " as: abcdef ");
    assert_eq!(caret, Some(11));
}

#[test]
fn painting_a_row_that_is_not_there_is_a_no_op() {
    let palette = Palette::from_theme(&Theme::dark());
    let mut buffer = CellBuffer::new(0, 0);
    assert_eq!(Prompt::goto_line().paint(&mut buffer, 0, &palette), None);
    Message::notice("wrote it").paint(&mut buffer, 4, &palette);
    assert_eq!(buffer.width(), 0);
}

#[test]
fn a_message_paints_its_text_and_marks_a_failure() {
    let palette = Palette::from_theme(&Theme::dark());
    let mut buffer = CellBuffer::new(16, 1);

    let notice = Message::notice("wrote a.rs");
    notice.paint(&mut buffer, 0, &palette);
    assert!(!notice.is_error());
    assert_eq!(row_text(&buffer, 0), "wrote a.rs      ");

    let failure = Message::error("cannot write a.rs");
    assert!(failure.is_error());
    assert_eq!(failure.text(), "cannot write a.rs");
    assert_ne!(failure.style(&palette), notice.style(&palette));
}
