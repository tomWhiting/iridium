//! Real Editor cell-key/command tests for Unicode, geometry and existing history.

use crate::cell_layout::{Affinity, CellColumn, CellLayoutError, CellWrapParameters, ScreenRow};
use crate::commands::{
    CommandArgs, KeyBinding, Keymap, ModifierPattern, ModifierState, StrokePattern, builtin,
};
use crate::document::{CursorState, Position, Selection};
use crate::editor::{CellInputError, CellInputOptions, Editor, EditorConfig, EditorKeyResult};
use crate::input::{KeyCode, KeyEvent, Modifiers};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn options(width: usize) -> CellInputOptions {
    CellInputOptions {
        wrap: CellWrapParameters::new(width, 4),
        visible_rows: 2,
    }
}

fn editor(text: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(text);
    editor.set_undo_group_timeout_ms(0);
    editor
}

fn key(
    editor: &mut Editor,
    code: KeyCode,
    width: usize,
) -> Result<EditorKeyResult, CellInputError> {
    editor.handle_cell_key(&KeyEvent::simple(code), options(width))
}

fn command(editor: &mut Editor, id: &str, width: usize) -> Result<EditorKeyResult, CellInputError> {
    editor.run_cell_command(id, CommandArgs::NONE, options(width))
}

#[test]
fn cell_visual_rows_preserve_cell_column_through_short_and_wide_rows() -> TestResult {
    let mut editor = editor("abcdef\nx\nab界cd");
    editor.set_cursor(Position::new(0, 3));
    for expected in [
        Position::new(0, 6),
        Position::new(1, 1),
        Position::new(2, 2),
        Position::new(2, 5),
    ] {
        key(&mut editor, KeyCode::Down, 4)?;
        assert_eq!(editor.cursor(), expected);
    }
    command(&mut editor, "cursor.lineUp", 4)?;
    assert_eq!(editor.cursor(), Position::new(2, 2));
    Ok(())
}

#[test]
fn cell_home_end_and_pointer_preserve_the_soft_break_side() -> TestResult {
    let mut editor = editor("abcdefghij");
    key(&mut editor, KeyCode::End, 4)?;
    assert_eq!(editor.cursor(), Position::new(0, 4));
    assert_eq!(editor.cell_affinity(options(4)), Affinity::Upstream);
    key(&mut editor, KeyCode::Down, 4)?;
    assert_eq!(editor.cursor(), Position::new(0, 8));
    key(&mut editor, KeyCode::Home, 4)?;
    assert_eq!(editor.cursor(), Position::new(0, 4));
    assert_eq!(editor.cell_affinity(options(4)), Affinity::Downstream);
    editor.set_cell_pointer(ScreenRow(0), CellColumn(4), false, options(4))?;
    key(&mut editor, KeyCode::Home, 4)?;
    assert_eq!(editor.cursor(), Position::zero());
    assert!(
        editor
            .set_cell_pointer(ScreenRow(0), CellColumn(0), false, options(0))
            .is_err()
    );
    Ok(())
}

#[test]
fn cell_resize_tabs_and_host_moves_discard_preferences() -> TestResult {
    let mut editor = editor("abcdef\nx\nabcdef");
    editor.set_cursor(Position::new(0, 5));
    key(&mut editor, KeyCode::Down, 10)?;
    key(&mut editor, KeyCode::Down, 4)?;
    assert_eq!(editor.cursor(), Position::new(2, 1));
    editor.set_cursor(Position::new(0, 5));
    key(&mut editor, KeyCode::Down, 10)?;
    editor.set_cursor(Position::new(1, 1));
    key(&mut editor, KeyCode::Down, 10)?;
    assert_eq!(editor.cursor(), Position::new(2, 1));
    editor.set_cursor(Position::new(0, 5));
    key(&mut editor, KeyCode::Down, 10)?;
    let changed = CellInputOptions {
        wrap: CellWrapParameters::new(10, 8),
        visible_rows: 2,
    };
    editor.run_cell_command("cursor.lineDown", CommandArgs::NONE, changed)?;
    assert_eq!(editor.cursor(), Position::new(2, 1));
    editor.set_cursor(Position::new(0, 5));
    key(&mut editor, KeyCode::Down, 10)?;
    let generation = editor.fold_state().generation();
    *editor.fold_state_mut() = crate::editor::FoldState::new();
    assert_eq!(editor.fold_state().generation(), generation);
    key(&mut editor, KeyCode::Down, 10)?;
    assert_eq!(editor.cursor(), Position::new(2, 1));

    Ok(())
}

#[test]
fn cell_page_zero_height_and_selection_direction_are_explicit() -> TestResult {
    let mut editor = editor("abcdefghijklmnop");
    editor.set_cursor(Position::new(0, 14));
    let zero = CellInputOptions {
        visible_rows: 0,
        ..options(4)
    };
    editor.run_cell_command("cursor.pageUp", CommandArgs::NONE, zero)?;
    assert_eq!(editor.cursor(), Position::new(0, 14));
    editor.handle_cell_key(
        &KeyEvent::new(KeyCode::PageUp, Modifiers::shift()),
        options(4),
    )?;
    assert_eq!(
        editor.state().cursor.primary,
        Selection::new(Position::new(0, 14), Position::new(0, 6))
    );
    command(&mut editor, "cursor.documentStart", 4)?;
    assert_eq!(editor.cursor(), Position::zero());
    command(&mut editor, "cursor.documentEnd", 4)?;
    assert_eq!(editor.cursor(), Position::new(0, 16));
    Ok(())
}

#[test]
fn cell_vertical_motion_skips_real_folded_lines() -> TestResult {
    let mut editor = editor("fn main() {\n    work();\n}\nafter");
    editor.set_language(iridium_lang::Language::Rust);
    assert!(editor.fold_at(0));
    editor.set_cursor(Position::new(0, 2));
    key(&mut editor, KeyCode::Down, 80)?;
    assert_eq!(editor.cursor(), Position::new(3, 2));
    assert!(editor.unfold_at(0));
    key(&mut editor, KeyCode::Up, 80)?;
    assert_eq!(editor.cursor(), Position::new(2, 1));
    Ok(())
}

#[test]
fn cell_horizontal_and_delete_are_extended_grapheme_atomic() -> TestResult {
    for cluster in ["e\u{301}", "👩‍👩‍👧‍👦", "🇦🇺", "👍🏽"] {
        let width = cluster.chars().count();
        let mut editor = editor(&format!("{cluster}x"));
        key(&mut editor, KeyCode::Right, 80)?;
        assert_eq!(editor.cursor(), Position::new(0, width));
        key(&mut editor, KeyCode::Left, 80)?;
        assert_eq!(editor.cursor(), Position::zero());
        key(&mut editor, KeyCode::Delete, 80)?;
        assert_eq!(editor.content(), "x");
        command(&mut editor, "history.undo", 80)?;
        assert_eq!(editor.content(), format!("{cluster}x"));
        editor.set_cursor(Position::new(0, width));
        key(&mut editor, KeyCode::Backspace, 80)?;
        assert_eq!(editor.content(), "x");
    }
    Ok(())
}

#[test]
fn cell_word_edges_and_replacement_include_whole_graphemes() -> TestResult {
    let mut editor = editor("e\u{301}");
    editor.set_cursor(Position::new(0, 2));
    command(&mut editor, "edit.deleteWordBackward", 80)?;
    assert_eq!(editor.content(), "");
    command(&mut editor, "history.undo", 80)?;
    editor.set_cursor(Position::zero());
    command(&mut editor, "edit.deleteWordForward", 80)?;
    assert_eq!(editor.content(), "");
    let mut editor = self::editor("e\u{301} e\u{301}");
    let mut cursor = CursorState::new(Selection::new(Position::zero(), Position::new(0, 2)));
    cursor.add_cursor(Selection::new(Position::new(0, 3), Position::new(0, 5)));
    editor.state_mut().cursor = cursor.clone();
    editor.paste_cells("X")?;
    assert_eq!(editor.content(), "X X");
    command(&mut editor, "history.undo", 80)?;
    assert_eq!(editor.content(), "e\u{301} e\u{301}");
    assert_eq!(editor.state().cursor, cursor);
    Ok(())
}

#[test]
fn cell_interior_host_selections_fail_before_text_or_history_changes() {
    let mut editor = editor("e\u{301}x");
    editor.set_cursor(Position::new(0, 1));
    let node = editor.state().history.current_node_id();
    assert!(matches!(
        editor.paste_cells("Y"),
        Err(CellInputError::Layout(
            CellLayoutError::NonGraphemeBoundary { line: 0, column: 1 }
        ))
    ));
    assert!(key(&mut editor, KeyCode::Backspace, 4).is_err());
    assert_eq!(editor.content(), "e\u{301}x");
    assert_eq!(editor.state().history.current_node_id(), node);
    editor.set_selection(Position::zero(), Position::new(0, 1));
    assert!(editor.paste_cells("Y").is_err());
    assert_eq!(editor.content(), "e\u{301}x");
}

#[test]
fn cell_insert_and_paste_joining_normalize_inside_the_same_undo_command() -> TestResult {
    let mut editor = editor("\u{301}x");
    key(&mut editor, KeyCode::Char('e'), 4)?;
    assert_eq!(editor.content(), "e\u{301}x");
    assert_eq!(editor.cursor(), Position::new(0, 2));
    command(&mut editor, "history.undo", 4)?;
    assert_eq!(editor.content(), "\u{301}x");
    assert_eq!(editor.cursor(), Position::zero());
    editor.paste_cells("a\r\nb\rc\n👩‍🔬")?;
    assert_eq!(editor.content(), "a\r\nb\rc\n👩‍🔬\u{301}x");
    assert_eq!(editor.cursor(), Position::new(3, 4));
    command(&mut editor, "history.undo", 4)?;
    assert_eq!(editor.content(), "\u{301}x");
    command(&mut editor, "history.redo", 4)?;
    assert_eq!(editor.cursor(), Position::new(3, 4));
    Ok(())
}

#[test]
fn cell_owned_pair_collapse_and_user_closer_rules_stay_distinct() -> TestResult {
    let mut editor = editor("r#");
    editor.set_language(iridium_lang::Language::Rust);
    editor.set_cursor(Position::new(0, 2));
    key(&mut editor, KeyCode::Char('"'), 80)?;
    assert_eq!(editor.content(), "r#\"\"#");
    key(&mut editor, KeyCode::Backspace, 80)?;
    assert_eq!(editor.content(), "r#");
    let mut supplied = self::editor("r#\"\"#");
    supplied.set_language(iridium_lang::Language::Rust);
    supplied.set_cursor(Position::new(0, 3));
    key(&mut supplied, KeyCode::Backspace, 80)?;
    assert_eq!(supplied.content(), "r##");
    let mut combining = self::editor("(\u{301})");
    combining.set_cursor(Position::new(0, 2));
    key(&mut combining, KeyCode::Backspace, 80)?;
    assert_eq!(combining.content(), ")");
    Ok(())
}

#[test]
fn cell_resize_navigation_and_history_keep_branches_and_drop_affinity() -> TestResult {
    let mut editor = editor("abcdefgh");
    key(&mut editor, KeyCode::End, 4)?;
    key(&mut editor, KeyCode::Char('X'), 4)?;
    let first = editor.state().history.current_node_id();
    key(&mut editor, KeyCode::Down, 3)?;
    command(&mut editor, "history.undo", 3)?;
    assert_eq!(editor.content(), "abcdefgh");
    assert_eq!(editor.cursor(), Position::new(0, 4));
    assert_eq!(editor.cell_affinity(options(4)), Affinity::Downstream);
    editor.paste_cells("Y")?;
    let second = editor.state().history.current_node_id();
    assert_ne!(first, second);
    command(&mut editor, "history.undo", 4)?;
    assert_eq!(editor.state().history.branch_count(), 2);
    assert!(editor.state().history.node_info(first).is_some());
    Ok(())
}

#[test]
fn cell_keymap_rebinding_chords_and_host_repeat_rules_share_one_resolver() -> TestResult {
    let mut editor = editor("abcdefghij");
    let mut layer = Keymap::new("cell test");
    let stroke = |key| StrokePattern::new(key, ModifierPattern::NONE);
    layer.push(KeyBinding::new(
        stroke(KeyCode::F2),
        &[],
        builtin::CURSOR_LINE_DOWN,
    ));
    let ctrl = |code| {
        StrokePattern::new(
            code,
            ModifierPattern::NONE.with_ctrl(ModifierState::Required),
        )
    };
    layer.push(KeyBinding::new(
        ctrl(KeyCode::Char('b')),
        &[ctrl(KeyCode::Char('d'))],
        builtin::CURSOR_LINE_UP,
    ));
    layer.push(KeyBinding::new(
        stroke(KeyCode::F5),
        &[],
        builtin::PALETTE_OPEN,
    ));
    editor.push_keymap(layer)?;
    key(&mut editor, KeyCode::F2, 4)?;
    assert_eq!(editor.cursor(), Position::new(0, 4));
    editor.handle_cell_key(
        &KeyEvent::new(KeyCode::Char('b'), Modifiers::ctrl()),
        options(4),
    )?;
    assert!(!editor.pending_key_sequence().is_empty());
    let mut repeated_leader = KeyEvent::new(KeyCode::Char('b'), Modifiers::ctrl());
    repeated_leader.is_repeat = true;
    assert_eq!(
        editor.handle_cell_key(&repeated_leader, options(4))?,
        EditorKeyResult::None
    );
    assert_eq!(editor.pending_key_sequence().len(), 1);
    assert_eq!(editor.cursor(), Position::new(0, 4));
    editor.handle_cell_key(
        &KeyEvent::new(KeyCode::Char('d'), Modifiers::ctrl()),
        options(4),
    )?;
    assert_eq!(editor.cursor(), Position::zero());
    assert!(matches!(
        key(&mut editor, KeyCode::F5, 4)?,
        EditorKeyResult::HostCommand { .. }
    ));
    let mut repeat = KeyEvent::simple(KeyCode::F5);
    repeat.is_repeat = true;
    assert!(matches!(
        editor.handle_cell_key(&repeat, options(4))?,
        EditorKeyResult::HostCommand { .. }
    ));
    let mut repeat_down = KeyEvent::simple(KeyCode::F2);
    repeat_down.is_repeat = true;
    editor.handle_cell_key(&repeat_down, options(4))?;
    assert_eq!(editor.cursor(), Position::new(0, 4));
    Ok(())
}

#[test]
fn cell_legacy_dispatch_keeps_scalar_and_document_line_semantics() -> TestResult {
    let mut legacy = editor("e\u{301}abcdefgh");
    legacy.handle_key(&KeyEvent::simple(KeyCode::Right));
    assert_eq!(legacy.cursor(), Position::new(0, 1));
    let mut cells = editor("e\u{301}abcdefgh");
    key(&mut cells, KeyCode::Right, 4)?;
    assert_eq!(cells.cursor(), Position::new(0, 2));
    legacy.run_command("cursor.lineDown", CommandArgs::NONE)?;
    assert_eq!(legacy.cursor().line, 0);
    Ok(())
}

#[test]
fn cell_typing_copy_and_host_commands_accept_zero_geometry_without_layout() -> TestResult {
    let mut editor = editor("\n\t\t\nother lines");
    key(&mut editor, KeyCode::Char('A'), 0)?;
    assert_eq!(editor.content(), "A\n\t\t\nother lines");
    assert!(matches!(
        command(&mut editor, "clipboard.copy", 0)?,
        EditorKeyResult::Clipboard(_)
    ));
    Ok(())
}

#[test]
fn cell_copy_keeps_vertical_preferences_but_horizontal_noop_discards_them() -> TestResult {
    let mut editor = editor("abcdef\nx\nabcdef");
    editor.set_cursor(Position::new(0, 5));
    key(&mut editor, KeyCode::Down, 10)?;
    command(&mut editor, "clipboard.copy", 10)?;
    key(&mut editor, KeyCode::Down, 10)?;
    assert_eq!(editor.cursor(), Position::new(2, 5));
    let mut short = self::editor("abcdef\nx");
    short.set_cursor(Position::new(0, 5));
    key(&mut short, KeyCode::Down, 10)?;
    key(&mut short, KeyCode::Right, 10)?;
    key(&mut short, KeyCode::Up, 10)?;
    assert_eq!(short.cursor(), Position::new(0, 1));
    Ok(())
}

#[test]
fn cell_counts_are_one_command_and_read_only_keeps_content() -> TestResult {
    let mut editor = editor("e\u{301}👩‍🔬🇦🇺x");
    editor.run_cell_command("cursor.charRight", CommandArgs::with_count(2), options(4))?;
    assert_eq!(editor.cursor(), Position::new(0, 5));
    editor.run_cell_command(
        "cursor.charRight",
        CommandArgs::with_count(u32::MAX),
        options(4),
    )?;
    assert_eq!(editor.cursor(), Position::new(0, 8));
    editor.state_mut().read_only = true;
    let node = editor.state().history.current_node_id();
    key(&mut editor, KeyCode::Backspace, 4)?;
    editor.paste_cells("Y")?;
    assert_eq!(editor.content(), "e\u{301}👩‍🔬🇦🇺x");
    assert_eq!(editor.state().history.current_node_id(), node);
    Ok(())
}

#[test]
fn cell_read_only_pair_attempt_never_owns_a_later_host_insertion() -> TestResult {
    let mut editor = editor("r#");
    editor.set_language(iridium_lang::Language::Rust);
    editor.set_cursor(Position::new(0, 2));
    editor.state_mut().read_only = true;
    key(&mut editor, KeyCode::Char('"'), 80)?;
    assert_eq!(editor.content(), "r#");
    editor.state_mut().read_only = false;
    editor.apply_command(crate::history::Command::Insert {
        position: Position::new(0, 2),
        text: "\"\"#".to_owned(),
    });
    editor.set_cursor(Position::new(0, 3));
    key(&mut editor, KeyCode::Backspace, 80)?;
    assert_eq!(editor.content(), "r##");
    Ok(())
}

#[test]
fn cell_read_only_history_replay_keeps_document_cursor_and_tree() -> TestResult {
    let mut editor = editor("");
    editor.paste_cells("x")?;
    let first = editor.current_history_node();
    let cursor = editor.cursor();
    editor.state_mut().read_only = true;
    editor.handle_cell_key(
        &KeyEvent::new(KeyCode::Char('z'), Modifiers::ctrl()),
        options(4),
    )?;
    assert_eq!(editor.content(), "x");
    assert_eq!(editor.cursor(), cursor);
    assert_eq!(editor.current_history_node(), first);

    editor.state_mut().read_only = false;
    command(&mut editor, "history.undo", 4)?;
    let root = editor.current_history_node();
    editor.state_mut().read_only = true;
    command(&mut editor, "history.redo", 4)?;
    assert_eq!(editor.content(), "");
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(editor.current_history_node(), root);
    assert_eq!(editor.state().history.active_branch_index(), Some(0));

    editor.state_mut().read_only = false;
    editor.paste_cells("y")?;
    command(&mut editor, "history.undo", 4)?;
    assert_eq!(editor.state().history.active_branch_index(), Some(1));
    editor.state_mut().read_only = true;
    editor.run_cell_command("history.redoBranch", CommandArgs::with_count(1), options(4))?;
    assert_eq!(editor.content(), "");
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(editor.current_history_node(), root);
    assert_eq!(editor.state().history.active_branch_index(), Some(1));

    command(&mut editor, "history.previousBranch", 4)?;
    assert_eq!(editor.state().history.active_branch_index(), Some(0));
    command(&mut editor, "history.nextBranch", 4)?;
    assert_eq!(editor.state().history.active_branch_index(), Some(1));
    assert_eq!(editor.content(), "");
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(editor.current_history_node(), root);
    editor.state_mut().read_only = false;
    editor.run_cell_command("history.redoBranch", CommandArgs::with_count(1), options(4))?;
    assert_eq!(editor.content(), "x");
    assert_eq!(editor.current_history_node(), first);
    Ok(())
}

#[test]
fn cell_each_cursor_affinity_is_validated_against_actual_state_and_options() -> TestResult {
    let mut editor = editor("abcdefgh\nijklmnop");
    editor
        .state_mut()
        .cursor
        .secondary
        .push(Selection::collapsed(Position::new(1, 0)));
    command(&mut editor, "cursor.lineEnd", 4)?;
    assert_eq!(
        editor.cell_cursor_affinity(0, options(4)),
        Some(Affinity::Upstream)
    );
    assert_eq!(
        editor.cell_cursor_affinity(1, options(4)),
        Some(Affinity::Upstream)
    );
    assert_eq!(editor.cell_cursor_affinity(2, options(4)), None);
    assert_eq!(
        editor.cell_cursor_affinity(1, options(5)),
        Some(Affinity::Downstream)
    );
    editor.set_cursor(Position::zero());
    assert_eq!(editor.cell_affinity(options(4)), Affinity::Downstream);
    assert_eq!(
        editor.cell_cursor_affinity(0, options(4)),
        Some(Affinity::Downstream)
    );
    Ok(())
}
