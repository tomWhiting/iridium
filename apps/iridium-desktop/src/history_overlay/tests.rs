//! What the panel does with a key and a click.
//!
//! Behavioural throughout: every assertion presses keys and reads what came
//! back, so the conversion of this panel's chord table into a keymap (#117)
//! could not be marked correct by a test rewritten alongside it. The bindings
//! themselves are ratcheted separately, in [`super::keymap_tests`].

use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::panel::age_text;
use super::{HistoryOutcome, HistoryPanel};
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

/// Undoes one step through the kernel's own binding.
fn undo(editor: &mut Editor) {
    let _ = editor.handle_key(&chord(KeyCode::Char('z'), Modifiers::ctrl()));
}

/// A kernel whose edits never coalesce, so each paste is one history node.
fn ungrouped_editor() -> Editor {
    let mut editor = Editor::with_defaults();
    editor.state_mut().history.set_group_timeout_ms(0);
    editor
}

/// A kernel whose history is a straight line: root → "a" → "ab".
fn linear_history() -> Editor {
    let mut editor = ungrouped_editor();
    editor.paste("a");
    editor.paste("b");
    editor
}

/// A kernel whose history forks at the root: root → "a" abandoned,
/// root → "c" current.
fn forked_history() -> Editor {
    let mut editor = ungrouped_editor();
    editor.paste("a");
    undo(&mut editor);
    editor.paste("c");
    editor
}

/// An open panel.
fn open_panel() -> HistoryPanel {
    let mut panel = HistoryPanel::new();
    panel.open();
    panel
}

/// The composed rows as plain text at a generous size.
fn rows_text(panel: &mut HistoryPanel, editor: &Editor) -> Vec<String> {
    let content = panel.content(editor, &Theme::dark(), PanelFit::popover(60, 12));
    content
        .rows
        .iter()
        .map(crate::overlay::PanelRow::text)
        .collect()
}

#[test]
fn escape_closes_and_both_toggle_spellings_close() {
    let editor = linear_history();
    let mut panel = open_panel();
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor),
        HistoryOutcome::Closed
    );
    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        panel.handle_key(&chord(KeyCode::Char('h'), ctrl_alt), &editor),
        HistoryOutcome::Closed,
        "the chord that opened the panel closes it"
    );
    let meta_alt = Modifiers {
        meta: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        panel.handle_key(&chord(KeyCode::Char('h'), meta_alt), &editor),
        HistoryOutcome::Closed,
        "the mac spelling closes it too"
    );
}

#[test]
fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
    let editor = linear_history();
    let mut panel = open_panel();
    assert_eq!(
        panel.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()), &editor),
        HistoryOutcome::Handled
    );
    assert_eq!(
        panel.handle_key(&press(KeyCode::Char('x')), &editor),
        HistoryOutcome::Handled,
        "typing must not reach the document while history is open"
    );
}

#[test]
fn moving_up_and_jumping_reaches_the_previous_state() {
    let mut editor = linear_history();
    assert_eq!(editor.content(), "ab");
    let mut panel = open_panel();

    // The selection starts on the current node — the newest row. One step
    // up is the state before the last edit.
    assert_eq!(
        panel.handle_key(&press(KeyCode::Up), &editor),
        HistoryOutcome::Handled
    );
    let HistoryOutcome::Jump(id) = panel.handle_key(&press(KeyCode::Enter), &editor) else {
        panic!("a selected row must resolve to a jump");
    };
    assert!(editor.jump_to_history_node(id), "the kernel accepts the id");
    assert_eq!(editor.content(), "a", "one row up is one edit back");
}

#[test]
fn a_parked_branch_is_reachable_by_rows_alone() {
    let mut editor = forked_history();
    assert_eq!(editor.content(), "c");
    let mut panel = open_panel();

    // Root at the top, then the abandoned "a" branch, then the current
    // "c": one step up from the current row is the parked branch.
    panel.handle_key(&press(KeyCode::Up), &editor);
    let HistoryOutcome::Jump(id) = panel.handle_key(&press(KeyCode::Enter), &editor) else {
        panic!("the parked branch resolves to a jump");
    };
    assert!(editor.jump_to_history_node(id));
    assert_eq!(
        editor.content(),
        "a",
        "the branch abandoned by undo-then-edit is reachable"
    );
}

#[test]
fn the_selection_clamps_at_both_ends() {
    let editor = linear_history();
    let mut panel = open_panel();
    // The selection starts at the newest row; `Down` must stay there.
    let at_end = panel.handle_key(&press(KeyCode::Enter), &editor);
    panel.handle_key(&press(KeyCode::Down), &editor);
    assert_eq!(panel.handle_key(&press(KeyCode::Enter), &editor), at_end);

    // `Home` is the root; `Up` from it must stay there.
    panel.handle_key(&press(KeyCode::Home), &editor);
    let at_root = panel.handle_key(&press(KeyCode::Enter), &editor);
    panel.handle_key(&press(KeyCode::Up), &editor);
    assert_eq!(panel.handle_key(&press(KeyCode::Enter), &editor), at_root);
    assert_ne!(at_root, at_end);
}

#[test]
fn the_rows_mark_the_current_state_the_root_and_the_ages() {
    let editor = linear_history();
    let mut panel = open_panel();
    let rows = rows_text(&mut panel, &editor);
    assert!(
        rows.iter().any(|row| row.contains("* edit")),
        "the current state is marked with an asterisk: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("o start")),
        "the root row is labelled start: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("now")),
        "fresh edits show their age: {rows:?}"
    );
}

#[test]
fn a_fork_names_its_branch_count() {
    let editor = forked_history();
    let mut panel = open_panel();
    let rows = rows_text(&mut panel, &editor);
    assert!(
        rows.iter().any(|row| row.contains("(2 branches)")),
        "a fork must say it forks: {rows:?}"
    );
}

#[test]
fn the_selected_row_is_flagged_for_the_painter() {
    let editor = linear_history();
    let mut panel = open_panel();
    let content = panel.content(&editor, &Theme::dark(), PanelFit::popover(60, 12));
    let selected: Vec<usize> = content
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.selected.then_some(index))
        .collect();
    assert_eq!(
        selected,
        vec![2],
        "the selection starts on the current, newest row"
    );
}

#[test]
fn ages_use_the_shortest_honest_unit() {
    assert_eq!(age_text(0), "now");
    assert_eq!(age_text(999), "now");
    assert_eq!(age_text(1_000), "1s");
    assert_eq!(age_text(59_999), "59s");
    assert_eq!(age_text(60_000), "1m");
    assert_eq!(age_text(3_599_999), "59m");
    assert_eq!(age_text(3_600_000), "1h");
    assert_eq!(age_text(86_400_000), "1d");
}
