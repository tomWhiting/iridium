//! Tests for the right-click context menu.
//!
//! Moved out of `mod.rs` unchanged when the module became a directory for #117
//! — `mod.rs` carries declarations, not 320 lines of assertions.

use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Keymap, Modifiers};

use super::{ContextMenu, MenuOutcome};
use crate::overlay::{PanelFit, PanelRow};
use crate::verbs::Row;

/// An empty user layer: a menu built with this answers only its own defaults.
fn no_user_keys() -> Keymap {
    Keymap::new("empty")
}

/// The fit of a comfortable window.
const FIT: PanelFit = PanelFit::popover(60, 24);

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A kernel carrying this face's commands and keymap.
fn editor() -> Editor {
    let mut editor = Editor::with_defaults();
    for meta in crate::commands::command_metas() {
        editor
            .register_command(meta)
            .expect("the kernel accepted the command");
    }
    editor
        .push_keymap(crate::commands::keymap())
        .expect("the keymap validates");
    editor
}

/// An open menu over that kernel.
fn open_menu() -> (ContextMenu, Editor) {
    let editor = editor();
    let menu = ContextMenu::open(&editor, 100.0, 200.0, &no_user_keys());
    (menu, editor)
}

/// The composed rows as plain text.
fn rows_text(menu: &ContextMenu) -> Vec<String> {
    menu.content(&Theme::dark(), FIT)
        .rows
        .iter()
        .map(PanelRow::text)
        .collect()
}

#[test]
fn every_menu_verb_is_a_command_the_registry_carries() {
    let editor = editor();
    for verb in super::menu::verbs().iter().filter_map(Row::as_verb) {
        assert!(
            editor.commands().contains(verb.id.as_str()),
            "the menu offers `{}`, which no registry carries",
            verb.id
        );
    }
}

#[test]
fn the_menu_lists_the_ruled_verb_set_in_order() {
    let (menu, _editor) = open_menu();
    let labels: Vec<String> = rows_text(&menu);
    let verbs: Vec<&str> = labels
        .iter()
        .map(|row| row.trim())
        .filter(|row| !row.is_empty())
        .map(|row| row.split("  ").next().unwrap_or(row).trim())
        .collect();
    assert_eq!(
        verbs,
        vec!["Cut", "Copy", "Paste", "Select All", "Command Palette…"],
        "D-2's buildable floor, in order"
    );
}

#[test]
fn the_labels_match_the_registry_titles_for_the_editing_verbs() {
    // The palette row is named for the panel it opens, not for the
    // command's own palette title; the four editing verbs must not drift
    // from the registry.
    let editor = editor();
    for verb in super::menu::verbs().iter().filter_map(Row::as_verb) {
        let meta = editor
            .commands()
            .get(verb.id.as_str())
            .expect("the verb is registered");
        if verb.id == iridium_editor::commands::builtin::PALETTE_OPEN {
            assert_eq!(verb.label, "Command Palette…");
        } else {
            assert_eq!(
                verb.label,
                meta.title(),
                "`{}` drifted from its registry title",
                verb.id
            );
        }
    }
}

#[test]
fn escape_closes_the_menu() {
    let (mut menu, _editor) = open_menu();
    assert_eq!(
        menu.handle_key(&press(KeyCode::Escape)),
        MenuOutcome::Closed
    );
}

#[test]
fn enter_runs_the_highlighted_verb() {
    let (mut menu, _editor) = open_menu();
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT),
        "the first row is highlighted when the menu opens"
    );
    menu = ContextMenu::open(&editor(), 0.0, 0.0, &no_user_keys());
    menu.handle_key(&press(KeyCode::Down));
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_COPY)
    );
}

#[test]
fn the_highlight_clamps_at_both_ends_rather_than_wrapping() {
    let (mut menu, _editor) = open_menu();
    // At the top already: `Up` must stay there, not wrap to the end.
    menu.handle_key(&press(KeyCode::Up));
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT)
    );

    menu = ContextMenu::open(&editor(), 0.0, 0.0, &no_user_keys());
    for _ in 0..12 {
        menu.handle_key(&press(KeyCode::Down));
    }
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::PALETTE_OPEN),
        "`Down` past the end rests on the last verb"
    );
}

#[test]
fn navigation_steps_over_the_separators() {
    let (mut menu, _editor) = open_menu();
    // Cut, Copy, Paste, ——, Select All: four `Down`s from Cut must land on
    // Select All, not on the rule above it.
    for _ in 0..3 {
        menu.handle_key(&press(KeyCode::Down));
    }
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::SELECTION_SELECT_ALL)
    );
}

#[test]
fn home_and_end_reach_the_first_and_last_verbs() {
    let (mut menu, _editor) = open_menu();
    menu.handle_key(&press(KeyCode::End));
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::PALETTE_OPEN)
    );
    menu = ContextMenu::open(&editor(), 0.0, 0.0, &no_user_keys());
    menu.handle_key(&press(KeyCode::End));
    menu.handle_key(&press(KeyCode::Home));
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_CUT)
    );
}

#[test]
fn a_read_only_document_greys_the_mutating_verbs_and_navigation_skips_them() {
    let mut editor = editor();
    editor.state_mut().read_only = true;
    let mut menu = ContextMenu::open(&editor, 10.0, 10.0, &no_user_keys());

    let content = menu.content(&Theme::dark(), FIT);
    let quiet = Theme::dark().editor.line_number;
    let cut = &content.rows[0];
    assert!(
        cut.spans.iter().all(|span| span.color == quiet),
        "a disabled row is drawn quiet: {:?}",
        cut.text()
    );

    // Cut and Paste mutate; Copy does not, so the highlight opens on Copy
    // and `Enter` runs it.
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_COPY)
    );
    menu = ContextMenu::open(&editor, 10.0, 10.0, &no_user_keys());
    menu.handle_key(&press(KeyCode::Down));
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::SELECTION_SELECT_ALL),
        "`Down` from Copy steps over the disabled Paste"
    );
}

#[test]
fn cut_and_copy_stay_enabled_with_a_collapsed_selection() {
    // D-5: the kernel's clipboard verbs act on the caret line when nothing
    // is selected, so greying them would misstate them.
    let editor = editor();
    assert!(
        editor.state().cursor.primary.is_collapsed(),
        "a fresh document has no selection"
    );
    let menu = ContextMenu::open(&editor, 10.0, 10.0, &no_user_keys());
    let content = menu.content(&Theme::dark(), FIT);
    let base = Theme::dark().editor.foreground;
    for (index, verb) in [(0_usize, "Cut"), (1, "Copy")] {
        let row = &content.rows[index];
        assert!(
            row.spans.first().is_some_and(|span| span.color == base),
            "{verb} must not be greyed: {:?}",
            row.text()
        );
    }
}

#[test]
fn the_menu_is_modal_and_swallows_what_it_does_not_bind() {
    let (mut menu, _editor) = open_menu();
    let save = KeyEvent {
        key: KeyCode::Char('s'),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    };
    assert_eq!(menu.handle_key(&save), MenuOutcome::Handled);
    assert_eq!(
        menu.handle_key(&press(KeyCode::Char('x'))),
        MenuOutcome::Handled,
        "a printable key must not reach the document"
    );
}

#[test]
fn a_row_carries_its_mac_key_hint() {
    let (menu, _editor) = open_menu();
    let rows = rows_text(&menu);
    let select_all = rows
        .iter()
        .find(|row| row.contains("Select All"))
        .expect("the menu lists Select All");
    assert!(
        select_all.contains('⌃') || select_all.contains('⌘'),
        "the hint is in mac glyphs: {select_all:?}"
    );
    assert!(
        !select_all.contains("Ctrl"),
        "the portable spelling must not appear on this face: {select_all:?}"
    );
}

#[test]
fn a_click_runs_a_row_and_a_click_on_a_separator_runs_nothing() {
    let (mut menu, _editor) = open_menu();
    assert_eq!(menu.activate(3), None, "row 3 is the separator");
    assert_eq!(
        menu.activate(1),
        Some(iridium_editor::commands::builtin::CLIPBOARD_COPY)
    );
}

#[test]
fn hovering_moves_the_highlight_only_onto_rows_that_can_run() {
    let (mut menu, _editor) = open_menu();
    assert!(menu.hover(2), "the highlight moved onto Paste");
    assert!(!menu.hover(2), "the same row does not move it again");
    assert!(!menu.hover(3), "a separator cannot be highlighted");
    assert_eq!(
        menu.handle_key(&press(KeyCode::Enter)),
        MenuOutcome::Run(iridium_editor::commands::builtin::CLIPBOARD_PASTE)
    );
}

#[test]
fn the_rows_never_exceed_the_windows_content_columns() {
    let (menu, _editor) = open_menu();
    for columns in [4_usize, 12, 20, 60] {
        let content = menu.content(&Theme::dark(), PanelFit::popover(columns, 24));
        assert!(content.content_columns <= columns);
        for row in &content.rows {
            assert!(
                row.text().chars().count() <= content.content_columns,
                "a row overran the panel: {:?}",
                row.text()
            );
        }
    }
}

#[test]
fn one_row_is_marked_selected_and_the_separators_are_marked_as_rules() {
    let (menu, _editor) = open_menu();
    let content = menu.content(&Theme::dark(), FIT);
    let selected: Vec<usize> = content
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.selected.then_some(index))
        .collect();
    assert_eq!(selected, vec![0], "the first verb is highlighted");
    let rules: Vec<usize> = content
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.separator.then_some(index))
        .collect();
    assert_eq!(rules, vec![3, 5], "the two ruled group breaks");
}
