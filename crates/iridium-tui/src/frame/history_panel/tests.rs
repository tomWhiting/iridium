//! Tests for the undo-tree panel.
//!
//! Everything here drives the panel the way the host does — a [`KeyEvent`] in,
//! an outcome out, a [`CellBuffer`] painted — against a real kernel whose
//! history grew through real edits and undos. The tree's own correctness is
//! the kernel's to test.

use iridium_editor::theme::Theme;
use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};

use super::{HistoryOutcome, HistoryPanel};
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

/// A `Ctrl+Alt` chord.
fn ctrl_alt(character: char) -> KeyEvent {
    KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
        is_repeat: false,
    }
}

/// Undoes one step through the kernel's own binding.
fn undo(editor: &mut Editor) {
    let _ = editor.handle_key(&ctrl('z'));
}

/// A kernel whose edits never coalesce, so each paste is one history node.
///
/// The tree groups edits landing within half a second into one undo step —
/// right for typing, wrong for a test that wants one node per action.
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

/// A kernel whose history forks at the root: root → "a" abandoned, root → "c"
/// current.
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

/// Paints the panel at 80x24 and returns every row as text.
fn painted(panel: &mut HistoryPanel, editor: &Editor) -> Vec<String> {
    let mut buffer = CellBuffer::new(80, 24);
    let styles = Palette::from_theme(&Theme::dark());
    panel.paint(&mut buffer, editor, &styles);
    (0..24).map(|row| row_text(&buffer, row)).collect()
}

#[test]
fn escape_closes_and_the_toggle_chord_closes() {
    let editor = linear_history();
    let mut panel = open_panel();
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor),
        HistoryOutcome::Closed
    );
    assert_eq!(
        panel.handle_key(&ctrl_alt('h'), &editor),
        HistoryOutcome::Closed,
        "the chord that opened the panel closes it"
    );
}

#[test]
fn the_panel_is_modal_and_swallows_what_it_does_not_bind() {
    let editor = linear_history();
    let mut panel = open_panel();
    assert_eq!(
        panel.handle_key(&ctrl('s'), &editor),
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

    // The selection starts on the current node — the newest row. One step up
    // is the state before the last edit.
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

    // Root at the top, then the abandoned "a" branch, then the current "c":
    // one step up from the current row is the parked branch.
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
fn the_panel_draws_a_rounded_box_with_the_current_row_marked() {
    let editor = linear_history();
    let mut panel = open_panel();
    let rows = painted(&mut panel, &editor);
    let top = rows[1].trim();
    assert!(
        top.starts_with('╭') && top.ends_with('╮'),
        "the top border must have rounded corners: {top:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("* edit")),
        "the current state is marked with an asterisk"
    );
    assert!(
        rows.iter().any(|row| row.contains("o start")),
        "the root row is labelled start"
    );
    assert!(
        rows.iter().any(|row| row.contains("now")),
        "fresh edits show their age"
    );
    assert!(
        !rows.iter().any(|row| row.contains(['┌', '┐', '└', '┘'])),
        "no square corner may appear anywhere"
    );
}

#[test]
fn a_fork_names_its_branch_count() {
    let editor = forked_history();
    let mut panel = open_panel();
    let rows = painted(&mut panel, &editor);
    assert!(
        rows.iter().any(|row| row.contains("(2 branches)")),
        "a fork must say it forks: {rows:?}"
    );
}

#[test]
fn a_screen_too_small_for_the_panel_still_lets_escape_work() {
    let editor = linear_history();
    let mut panel = open_panel();
    let styles = Palette::from_theme(&Theme::dark());
    let mut buffer = CellBuffer::new(10, 2);
    panel.paint(&mut buffer, &editor, &styles);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor),
        HistoryOutcome::Closed,
        "an undrawable panel must still be closable"
    );
}
