//! What the palette does with a key, a click and a paste.
//!
//! Behavioural throughout: every assertion presses keys and reads what came
//! back, so the conversion of this panel's chord table into a keymap (#117)
//! could not be marked correct by a test that had been rewritten alongside it.
//! The bindings themselves are ratcheted separately, in [`super::keymap_tests`].

use iridium_editor::commands::palette::CommandMru;
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::{CommandPalette, PaletteOutcome};
use crate::overlay::PanelFit;

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

/// A bare ⌘ chord.
fn meta(character: char) -> KeyEvent {
    KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers {
            meta: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    }
}

/// An open palette over a kernel carrying this face's commands and keys.
fn open_palette() -> (CommandPalette, Editor, CommandMru) {
    let mut panel = CommandPalette::new();
    panel.open();
    let mut editor = Editor::with_defaults();
    for meta in crate::commands::command_metas() {
        editor
            .register_command(meta)
            .expect("the kernel accepted the command");
    }
    editor
        .push_keymap(crate::commands::keymap())
        .expect("the keymap validates");
    (panel, editor, CommandMru::default())
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

/// The composed rows as plain text at a generous size.
fn rows_text(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru) -> Vec<String> {
    let content = panel.content(editor, mru, &Theme::dark(), PanelFit::popover(60, 13));
    content
        .rows
        .iter()
        .map(crate::overlay::PanelRow::text)
        .collect()
}

#[test]
fn escape_closes_and_both_toggle_spellings_close() {
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
    assert_eq!(
        panel.handle_key(&meta('k'), &editor, &mru),
        PaletteOutcome::Closed,
        "the mac spelling closes it too"
    );
}

#[test]
fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
    let (mut panel, editor, mru) = open_palette();
    // `Ctrl+S` is save in the host's keymap; while the palette is open it
    // must reach neither the document nor the host.
    assert_eq!(
        panel.handle_key(&ctrl('s'), &editor, &mru),
        PaletteOutcome::Handled
    );
    assert_eq!(
        panel.handle_key(&meta('s'), &editor, &mru),
        PaletteOutcome::Handled,
        "the ⌘ save spelling is swallowed the same way"
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

    // Far past the end: the selection must sit on the last entry, and one
    // more `Down` must change nothing.
    for _ in 0..12 {
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
fn the_panel_lists_commands_and_their_mac_key_hints() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "select all");
    let rows = rows_text(&mut panel, &editor, &mru);
    let hit = rows
        .iter()
        .find(|row| row.contains("Select All"))
        .expect("the Select All command is listed");
    // The kernel decides which binding is primary; what this face decides
    // is the spelling — mac glyphs, so `Ctrl+A` reads `⌃A` and the ⌘
    // layer's rows read `⌘…`, never the portable `Ctrl+` text.
    assert!(
        hit.contains("⌃A") || hit.contains("⌘A"),
        "the row must carry the key in mac glyphs: {hit:?}"
    );
    assert!(
        !hit.contains("Ctrl"),
        "the portable spelling must not appear on this face: {hit:?}"
    );
}

#[test]
fn a_query_with_no_matches_says_so_in_the_panel() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "qqzzxxjjqq");
    let rows = rows_text(&mut panel, &editor, &mru);
    assert!(
        rows.iter().any(|row| row.contains("No matching commands")),
        "an empty result list must be stated, not blank"
    );
}

#[test]
fn the_query_is_shown_in_the_input_row_with_the_caret_after_it() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "fold");
    let content = panel.content(&editor, &mru, &Theme::dark(), PanelFit::popover(60, 13));
    assert!(
        content.rows[0].text().starts_with("> fold"),
        "the query is visible: {:?}",
        content.rows[0].text()
    );
    let caret = content.caret.expect("the query field has a caret");
    assert_eq!(caret.row, 0, "the caret sits in the input row");
    assert_eq!(
        caret.column,
        2 + "fold".len(),
        "the caret sits after the query text"
    );
}

#[test]
fn one_result_row_is_marked_selected() {
    let (mut panel, editor, mru) = open_palette();
    let content = panel.content(&editor, &mru, &Theme::dark(), PanelFit::popover(60, 13));
    let selected: Vec<usize> = content
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.selected.then_some(index))
        .collect();
    assert_eq!(selected, vec![1], "the first result is the selection");
}

#[test]
fn a_match_outside_the_title_is_annotated_beside_it() {
    let (mut panel, editor, mru) = open_palette();
    // "w!" is an alias of this face's Save Anyway, not part of its title.
    type_text(&mut panel, &editor, &mru, "w!");
    let rows = rows_text(&mut panel, &editor, &mru);
    let hit = rows
        .iter()
        .find(|row| row.contains("Save Anyway"))
        .expect("the alias match lists the command");
    assert!(
        hit.contains("w!"),
        "the matched alias must be shown so the highlight sits on truth: {hit:?}"
    );
}

#[test]
fn opening_again_clears_the_query() {
    let (mut panel, editor, mru) = open_palette();
    type_text(&mut panel, &editor, &mru, "fold");
    assert_eq!(panel.query(), "fold");
    panel.open();
    assert_eq!(panel.query(), "", "a palette opens aimed at nothing");
}

#[test]
fn a_paste_lands_in_the_query_with_control_characters_dropped() {
    let (mut panel, editor, mru) = open_palette();
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    panel.paste("se\nlect");
    assert_eq!(panel.query(), "select");
    // The paste changed the text, so the selection is back at the top.
    let outcome = panel.handle_key(&press(KeyCode::Enter), &editor, &mru);
    let mut fresh = CommandPalette::new();
    fresh.open();
    fresh.paste("select");
    assert_eq!(
        fresh.handle_key(&press(KeyCode::Enter), &editor, &mru),
        outcome
    );
}

#[test]
fn matched_characters_are_recoloured_on_the_clusters_they_name() {
    let spans = crate::line::highlighted_spans(
        "Save Anyway",
        &[0, 5],
        iridium_editor::theme::Color::new(1.0, 1.0, 1.0, 1.0),
        iridium_editor::theme::Color::new(1.0, 0.0, 0.0, 1.0),
    );
    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(text, "Save Anyway", "recolouring loses no text");
    assert_eq!(spans[0].text, "S", "the first matched character");
    assert_eq!(spans[1].text, "ave ");
    assert_eq!(spans[2].text, "A", "the second matched character");
}
