//! Tests for the command palette panel.
//!
//! Everything here drives the panel the way the host does — a [`KeyEvent`] in,
//! an outcome out, a [`CellBuffer`] painted — against a real kernel with its
//! real registry, so what these tests prove is what a user sees. Ranking,
//! scoring and recency are the kernel's own and are tested there.

use iridium_editor::commands::palette::CommandMru;
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::{CommandPalette, PaletteOutcome};
use crate::cell::{CellBuffer, CellContent};
use crate::frame::palette::Palette;

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A bare `Ctrl` chord.
fn ctrl(character: char) -> KeyEvent {
    KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    }
}

/// An open palette over a default kernel.
fn open_palette() -> (CommandPalette, Editor, CommandMru) {
    let mut panel = CommandPalette::new();
    panel.open();
    (panel, Editor::with_defaults(), CommandMru::default())
}

/// Types text into the panel, one key per character.
fn type_text(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru, text: &str) {
    for character in text.chars() {
        assert_eq!(
            panel.handle_key(&press(KeyCode::Char(character)), editor, mru),
            PaletteOutcome::Handled
        );
    }
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

/// Paints the panel at a size and returns every row as text.
fn painted(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru) -> Vec<String> {
    let mut buffer = CellBuffer::new(80, 24);
    let styles = Palette::from_theme(&Theme::dark());
    let caret = panel.paint(&mut buffer, editor, mru, &styles);
    assert!(caret.is_some(), "an 80x24 screen fits the panel");
    (0..24).map(|row| row_text(&buffer, row)).collect()
}

#[test]
fn escape_closes_and_ctrl_k_toggles_closed() {
    let (mut panel, editor, mru) = open_palette();
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor, &mru),
        PaletteOutcome::Closed
    );
    assert_eq!(
        panel.handle_key(&ctrl('k'), &editor, &mru),
        PaletteOutcome::Closed,
        "the key that opened the palette closes it"
    );
}

#[test]
fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
    let (mut panel, editor, mru) = open_palette();
    // `Ctrl+S` is save in the host's keymap; while the palette is open it must
    // reach neither the document nor the host.
    assert_eq!(
        panel.handle_key(&ctrl('s'), &editor, &mru),
        PaletteOutcome::Handled
    );
    let alt_chord = KeyEvent {
        key: KeyCode::Char('f'),
        modifiers: Modifiers {
            shift: false,
            ctrl: false,
            alt: true,
            meta: false,
            alt_graph: false,
        },
        is_repeat: false,
    };
    assert_eq!(
        panel.handle_key(&alt_chord, &editor, &mru),
        PaletteOutcome::Handled
    );
}

#[test]
fn enter_runs_the_best_match_for_the_query() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "show all commands");
    let outcome = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
    let PaletteOutcome::Run(command) = outcome else {
        panic!("a query with matches must resolve to a command, got {outcome:?}");
    };
    assert_eq!(command.as_str(), "palette.open");
}

#[test]
fn enter_with_no_matches_stays_open_and_runs_nothing() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "qqzzxxjjqq");
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
        PaletteOutcome::Handled
    );
}

#[test]
fn the_selection_clamps_at_both_ends_rather_than_wrapping() {
    let (mut panel, editor, mru) = open_palette();
    // At the top already: `Up` must stay there, not wrap to the end.
    assert_eq!(
        panel.handle_key(&press(KeyCode::Up), &editor, &mru),
        PaletteOutcome::Handled
    );
    let top = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);

    let mut fresh = CommandPalette::new();
    fresh.open();
    assert_eq!(fresh.handle_key(&press(KeyCode::Enter), &editor, &mru), top);

    // Far past the end: the selection must sit on the last entry, and one more
    // `Down` must change nothing.
    for _ in 0..3 {
        panel.handle_key(&press(KeyCode::PageDown), &editor, &mru);
        panel.handle_key(&press(KeyCode::PageDown), &editor, &mru);
        panel.handle_key(&press(KeyCode::PageDown), &editor, &mru);
        panel.handle_key(&press(KeyCode::PageDown), &editor, &mru);
    }
    let at_end = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
        at_end,
        "`Down` at the last entry must not move"
    );
    assert_ne!(at_end, top, "the page keys must actually have moved");
}

#[test]
fn editing_the_query_resets_the_selection_to_the_top() {
    let (mut panel, editor, mru) = open_palette();
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    type_text(&mut panel, &editor, &mru, "show all commands");
    let PaletteOutcome::Run(command) = panel.handle_key(&press(KeyCode::Enter), &editor, &mru)
    else {
        panic!("the query has a match");
    };
    assert_eq!(
        command.as_str(),
        "palette.open",
        "the selection must be back on the best match, not two rows down"
    );
}

#[test]
fn a_motion_in_the_query_field_keeps_the_selection() {
    let (mut panel, editor, mru) = open_palette();
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    let selected = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
    panel.handle_key(&press(KeyCode::Home), &editor, &mru);
    panel.handle_key(&press(KeyCode::Left), &editor, &mru);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
        selected,
        "a caret motion changes no text and must not reset the selection"
    );
}

#[test]
fn the_panel_draws_a_rounded_box() {
    let (mut panel, editor, mru) = open_palette();
    let rows = painted(&mut panel, &editor, &mru);
    let top = rows[1].trim();
    assert!(
        top.starts_with('╭') && top.ends_with('╮'),
        "the top border must have rounded corners: {top:?}"
    );
    let bottom = rows
        .iter()
        .map(|row| row.trim())
        .find(|row| row.starts_with('╰'))
        .expect("a bottom border is drawn");
    assert!(
        bottom.ends_with('╯'),
        "the bottom border must have rounded corners: {bottom:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains(['┌', '┐', '└', '┘'])),
        "no square corner may appear anywhere"
    );
}

#[test]
fn the_panel_lists_commands_and_their_key_hints() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "select all");
    let rows = painted(&mut panel, &editor, &mru);
    let hit = rows
        .iter()
        .find(|row| row.contains("Select All"))
        .expect("the Select All command is listed");
    assert!(
        hit.contains("Ctrl+A"),
        "the row must carry the key that runs it: {hit:?}"
    );
}

#[test]
fn a_query_with_no_matches_says_so_in_the_panel() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "qqzzxxjjqq");
    let rows = painted(&mut panel, &editor, &mru);
    assert!(
        rows.iter().any(|row| row.contains("No matching commands")),
        "an empty result list must be stated, not blank"
    );
}

#[test]
fn the_query_is_shown_in_the_input_row_with_the_caret_after_it() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "fold");
    let mut buffer = CellBuffer::new(80, 24);
    let styles = Palette::from_theme(&Theme::dark());
    let caret = panel
        .paint(&mut buffer, &editor, &mru, &styles)
        .expect("the panel fits");
    assert_eq!(caret.row, 2, "the caret sits in the input row");
    let input = row_text(&buffer, 2);
    assert!(input.contains("> fold"), "the query is visible: {input:?}");
    // A char position, not `str::find`: the border glyph is one column but
    // three bytes, and the caret is measured in columns.
    let prompt_column = input
        .chars()
        .position(|character| character == '>')
        .expect("the prompt is drawn");
    assert_eq!(
        caret.column,
        prompt_column + 2 + "fold".len(),
        "the caret sits after the query text"
    );
}

#[test]
fn a_screen_too_small_for_an_honest_panel_gets_none() {
    let (mut panel, editor, mru) = open_palette();
    let styles = Palette::from_theme(&Theme::dark());
    for (columns, rows) in [(10, 24), (80, 3), (0, 0)] {
        let mut buffer = CellBuffer::new(columns, rows);
        assert_eq!(
            panel.paint(&mut buffer, &editor, &mru, &styles),
            None,
            "{columns}x{rows} cannot show a palette"
        );
    }
}

#[test]
fn opening_again_clears_the_query() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "fold");
    assert_eq!(panel.query(), "fold");
    panel.open();
    assert_eq!(panel.query(), "", "a palette opens aimed at nothing");
}
