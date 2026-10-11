//! Desktop search overlay behavior and rendering.
mod refresh;
use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::{SearchOutcome, SearchOverlay};
use crate::overlay::PanelFit;

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A chord under the given modifiers.
fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

/// An `Alt` chord.
fn alt(character: char) -> KeyEvent {
    chord(
        KeyCode::Char(character),
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    )
}

/// The pointer's way into a two-field panel: pressing a field is the same
/// request `Tab` makes, said by pointing instead.
#[test]
fn a_press_on_the_replace_row_puts_the_caret_in_it() {
    let (mut panel, editor) = open_over("hello world");
    let theme = Theme::dark();
    let fit = PanelFit::popover(40, 12);
    assert_eq!(
        panel
            .content(&editor, &theme, fit)
            .caret
            .map(|caret| caret.row),
        Some(0),
        "the panel opens with the caret in Find"
    );

    panel.click_row(1, 2);
    assert_eq!(
        panel
            .content(&editor, &theme, fit)
            .caret
            .map(|caret| caret.row),
        Some(1),
        "the press moved the caret into Replace"
    );

    panel.click_row(0, 2);
    assert_eq!(
        panel
            .content(&editor, &theme, fit)
            .caret
            .map(|caret| caret.row),
        Some(0),
        "and back again"
    );
}

/// ⚠️ A window with room for one row is already showing the focused field,
/// so a press on row zero must not silently mean "go to Find".
#[test]
fn a_one_row_panel_keeps_the_field_it_is_showing() {
    let (mut panel, editor) = open_over("hello world");
    let theme = Theme::dark();
    let narrow = PanelFit::popover(40, 1);
    panel.click_row(1, 2);
    panel.click_row(0, 1);
    assert_eq!(
        panel
            .content(&editor, &theme, narrow)
            .caret
            .map(|caret| caret.row),
        Some(0),
        "the one row on screen is Replace, and it still has the caret"
    );
    let roomy = PanelFit::popover(40, 12);
    assert_eq!(
        panel
            .content(&editor, &theme, roomy)
            .caret
            .map(|caret| caret.row),
        Some(1),
        "which the two-row composition confirms is still Replace"
    );
}

/// An open panel over a kernel holding `text`.
fn open_over(text: &str) -> (SearchOverlay, Editor) {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    let mut overlay = SearchOverlay::new();
    overlay.open(&mut editor);
    (overlay, editor)
}

/// Types text into the panel, one key per character.
fn type_text(overlay: &mut SearchOverlay, editor: &mut Editor, text: &str) {
    for character in text.chars() {
        assert_eq!(
            overlay.handle_key(&press(KeyCode::Char(character)), editor),
            SearchOutcome::Handled
        );
    }
}

/// The panel's rows as plain text at a generous width.
fn rows_text(overlay: &SearchOverlay, editor: &Editor) -> Vec<String> {
    let content = overlay.content(editor, &Theme::dark(), PanelFit::popover(60, 12));
    content.rows.iter().map(super::PanelRow::text).collect()
}

#[test]
fn typing_a_query_searches_the_document_as_it_goes() {
    let (mut overlay, mut editor) = open_over("alpha beta alpha");
    type_text(&mut overlay, &mut editor, "alpha");
    assert_eq!(editor.search_match_count(), 2);
}

#[test]
fn the_count_is_one_based_and_names_the_current_match() {
    let (mut overlay, mut editor) = open_over("one two one");
    type_text(&mut overlay, &mut editor, "one");
    let rows = rows_text(&overlay, &editor);
    assert!(
        rows[0].contains("1 of 2"),
        "the count segment reads honestly: {rows:?}"
    );
}

#[test]
fn enter_walks_forwards_through_the_matches() {
    let (mut overlay, mut editor) = open_over("x x x");
    type_text(&mut overlay, &mut editor, "x");
    assert_eq!(editor.current_match_index(), Some(0));
    overlay.handle_key(&press(KeyCode::Enter), &mut editor);
    assert_eq!(editor.current_match_index(), Some(1));
    overlay.handle_key(&press(KeyCode::Up), &mut editor);
    assert_eq!(editor.current_match_index(), Some(0));
}

#[test]
fn a_query_with_no_matches_says_so_rather_than_counting_to_zero() {
    let (mut overlay, mut editor) = open_over("alpha");
    type_text(&mut overlay, &mut editor, "zz");
    let rows = rows_text(&overlay, &editor);
    assert!(rows[0].contains("No matches"), "{rows:?}");
}

#[test]
fn an_empty_query_makes_no_claim_at_all() {
    let (overlay, editor) = open_over("alpha");
    let rows = rows_text(&overlay, &editor);
    assert!(!rows[0].contains("of"), "{rows:?}");
    assert!(!rows[0].contains("matches"), "{rows:?}");
}

#[test]
fn escape_closes_the_kernels_search_and_keeps_the_query() {
    let (mut overlay, mut editor) = open_over("alpha");
    type_text(&mut overlay, &mut editor, "alpha");
    assert_eq!(
        overlay.handle_key(&press(KeyCode::Escape), &mut editor),
        SearchOutcome::Closed
    );
    assert_eq!(editor.search_match_count(), 0, "the kernel's search closed");
    assert_eq!(overlay.query(), "alpha", "the field keeps its text");

    // Re-opening re-runs the kept query.
    overlay.open(&mut editor);
    assert_eq!(editor.search_match_count(), 1);
}

#[test]
fn toggles_re_run_the_search_and_read_bracketed() {
    let (mut overlay, mut editor) = open_over("Alpha alpha");
    type_text(&mut overlay, &mut editor, "alpha");
    assert_eq!(editor.search_match_count(), 2);
    assert_eq!(
        overlay.handle_key(&alt('c'), &mut editor),
        SearchOutcome::Handled
    );
    assert_eq!(editor.search_match_count(), 1, "case sensitivity narrowed");
    let rows = rows_text(&overlay, &editor);
    assert!(
        rows[0].contains("[Aa]"),
        "the active toggle brackets itself"
    );
    assert!(rows[0].contains(" \\b "), "the inactive toggle does not");
}

#[test]
fn an_unfinished_regex_is_feedback_rather_than_a_crash() {
    let (mut overlay, mut editor) = open_over("alpha");
    assert_eq!(
        overlay.handle_key(&alt('r'), &mut editor),
        SearchOutcome::Handled
    );
    type_text(&mut overlay, &mut editor, "[a-z");
    assert!(overlay.error().is_some(), "the engine's complaint is kept");
    let rows = rows_text(&overlay, &editor);
    assert!(rows[0].contains("Invalid regex"), "{rows:?}");

    // The `]` fixes it.
    type_text(&mut overlay, &mut editor, "]");
    assert!(overlay.error().is_none());
    assert_eq!(editor.search_match_count(), 5);
}

#[test]
fn tab_moves_typing_into_the_replacement_without_disturbing_the_search() {
    let (mut overlay, mut editor) = open_over("aaa");
    type_text(&mut overlay, &mut editor, "a");
    let count = editor.search_match_count();
    overlay.handle_key(&press(KeyCode::Tab), &mut editor);
    type_text(&mut overlay, &mut editor, "b");
    assert_eq!(overlay.query(), "a");
    assert_eq!(overlay.replacement(), "b");
    assert_eq!(editor.search_match_count(), count);
}

#[test]
fn replacing_the_current_match_edits_the_document_and_reports_it() {
    let (mut overlay, mut editor) = open_over("one two one");
    type_text(&mut overlay, &mut editor, "one");
    overlay.handle_key(&press(KeyCode::Tab), &mut editor);
    type_text(&mut overlay, &mut editor, "three");
    assert_eq!(
        overlay.handle_key(&chord(KeyCode::Char('r'), Modifiers::ctrl()), &mut editor),
        SearchOutcome::Replaced(1)
    );
    assert_eq!(editor.content(), "three two one");
    let rows = rows_text(&overlay, &editor);
    assert!(rows[1].contains("Replaced 1 match"), "{rows:?}");
}

#[test]
fn replacing_every_match_reports_the_count_and_keeps_searching() {
    let (mut overlay, mut editor) = open_over("x y x y x");
    type_text(&mut overlay, &mut editor, "x");
    overlay.handle_key(&press(KeyCode::Tab), &mut editor);
    type_text(&mut overlay, &mut editor, "z");
    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        overlay.handle_key(&chord(KeyCode::Char('r'), ctrl_alt), &mut editor),
        SearchOutcome::Replaced(3)
    );
    assert_eq!(editor.content(), "z y z y z");
    // The panel still shows a query, so the kernel must still be
    // searching for it — for zero matches now, honestly.
    assert_eq!(editor.search_match_count(), 0);
    let rows = rows_text(&overlay, &editor);
    assert!(rows[1].contains("Replaced 3 matches"), "{rows:?}");
}

#[test]
fn a_read_only_document_refuses_a_replacement_and_says_why() {
    let (mut overlay, mut editor) = open_over("aaa");
    editor.state_mut().read_only = true;
    type_text(&mut overlay, &mut editor, "a");
    assert_eq!(
        overlay.handle_key(&chord(KeyCode::Char('r'), Modifiers::ctrl()), &mut editor),
        SearchOutcome::Handled
    );
    assert_eq!(editor.content(), "aaa");
    let rows = rows_text(&overlay, &editor);
    assert!(rows[1].contains("read-only"), "{rows:?}");
}

#[test]
fn a_key_the_panel_does_not_bind_is_left_to_the_host() {
    let (mut overlay, mut editor) = open_over("aaa");
    assert_eq!(
        overlay.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()), &mut editor),
        SearchOutcome::Ignored,
        "a save chord must stay live while the panel is open"
    );
    let meta = Modifiers {
        meta: true,
        ..Modifiers::none()
    };
    assert_eq!(
        overlay.handle_key(&chord(KeyCode::Char('s'), meta), &mut editor),
        SearchOutcome::Ignored
    );
}

#[test]
fn a_caret_motion_in_the_query_costs_no_search_and_keeps_the_caret_visible() {
    let (mut overlay, mut editor) = open_over("ab ab");
    type_text(&mut overlay, &mut editor, "ab");
    overlay.handle_key(&press(KeyCode::Home), &mut editor);
    let content = overlay.content(&editor, &Theme::dark(), PanelFit::popover(60, 12));
    let caret = content.caret.expect("the focused field has a caret");
    assert_eq!(caret.row, 0);
    assert_eq!(
        caret.column,
        super::LABEL_WIDTH,
        "home is the field's start"
    );
}

#[test]
fn a_narrow_panel_drops_the_toggles_before_the_count() {
    let (mut overlay, mut editor) = open_over("one two one");
    type_text(&mut overlay, &mut editor, "one");
    let content = overlay.content(
        &editor,
        &Theme::dark(),
        // Label 8 + field floor 8 + gap 1 leaves 6: the count "1 of 2"
        // fits, the toggles do not.
        PanelFit::popover(23, 12),
    );
    let row = content.rows[0].text();
    assert!(row.contains("1 of 2"), "{row:?}");
    assert!(!row.contains("Aa"), "{row:?}");
}

#[test]
fn one_interior_row_shows_the_focused_field() {
    let (mut overlay, mut editor) = open_over("aaa");
    overlay.handle_key(&press(KeyCode::Tab), &mut editor);
    type_text(&mut overlay, &mut editor, "b");
    let content = overlay.content(&editor, &Theme::dark(), PanelFit::popover(40, 1));
    assert_eq!(content.rows.len(), 1);
    assert!(
        content.rows[0].text().starts_with("Replace"),
        "the field keystrokes go into must be the one shown"
    );
}
