//! Public host replacement and cell paste stay atomic with automatic grouping enabled.

use iridium_editor::cell_layout::CellWrapParameters;
use iridium_editor::editor::{CellInputOptions, CellReplacementCursor};
use iridium_editor::{CursorState, Editor, KeyCode, KeyEvent, Position, Range, Selection};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn editor(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_undo_group_timeout_ms(u64::MAX);
    editor
}

fn type_character(editor: &mut Editor, character: char) -> TestResult {
    editor.handle_cell_key(
        &KeyEvent::simple(KeyCode::Char(character)),
        CellInputOptions {
            wrap: CellWrapParameters::new(8, 4),
            visible_rows: 3,
        },
    )?;
    Ok(())
}

fn whole(editor: &Editor) -> Result<Range, Box<dyn std::error::Error>> {
    let document = &editor.state().document;
    let end = document
        .offset_to_position(document.byte_count())
        .ok_or("fixture end must resolve")?;
    Ok(Range::new(Position::zero(), end))
}

#[test]
fn cell_transactions_complete_unicode_and_line_ending_joins_round_trip() -> TestResult {
    let cases = [
        (
            "\u{301}x",
            Position::zero(),
            Position::zero(),
            "e",
            "e\u{301}x",
            Position::new(0, 2),
        ),
        (
            "👩🔬",
            Position::new(0, 1),
            Position::new(0, 1),
            "\u{200d}",
            "👩‍🔬",
            Position::new(0, 3),
        ),
        (
            "🇦x🇧",
            Position::new(0, 1),
            Position::new(0, 2),
            "",
            "🇦🇧",
            Position::new(0, 2),
        ),
        (
            "eX",
            Position::new(0, 1),
            Position::new(0, 2),
            "\u{301}",
            "e\u{301}",
            Position::new(0, 2),
        ),
        (
            "\n",
            Position::zero(),
            Position::zero(),
            "\r",
            "\r\n",
            Position::new(1, 0),
        ),
        (
            "\r",
            Position::new(1, 0),
            Position::new(1, 0),
            "\n",
            "\r\n",
            Position::new(1, 0),
        ),
        (
            "\rX\n",
            Position::new(1, 0),
            Position::new(1, 1),
            "",
            "\r\n",
            Position::new(1, 0),
        ),
        (
            "\rX\n",
            Position::new(1, 0),
            Position::new(1, 1),
            "Z",
            "\rZ\n",
            Position::new(1, 1),
        ),
        (
            "a\r\nb\rc\n",
            Position::new(1, 0),
            Position::new(2, 1),
            "👩‍🔬\rβ\n",
            "a\r\n👩‍🔬\rβ\n\n",
            Position::new(3, 0),
        ),
    ];
    for (before, start, end, inserted, after, expected_cursor) in cases {
        let mut editor = editor(before);
        let old_cursor = editor.state().cursor.clone();
        editor.replace_cell_range(
            Range::new(start, end),
            inserted,
            CellReplacementCursor::EndOfReplacement,
        )?;
        assert_eq!(editor.content(), after, "input: {before:?}");
        assert_eq!(editor.cursor(), expected_cursor, "input: {before:?}");
        let new_cursor = editor.state().cursor.clone();
        assert!(editor.undo());
        assert_eq!(editor.content(), before, "undo input: {before:?}");
        assert_eq!(editor.state().cursor, old_cursor);
        assert!(editor.redo());
        assert_eq!(editor.content(), after, "redo input: {before:?}");
        assert_eq!(editor.state().cursor, new_cursor);
    }
    Ok(())
}

#[test]
fn cell_transactions_recall_clear_restore_and_typing_have_distinct_undo_nodes() -> TestResult {
    let mut editor = editor("");
    type_character(&mut editor, 'a')?;
    let typed = editor.current_history_node();
    type_character(&mut editor, 'b')?;
    assert_eq!(editor.current_history_node(), typed);
    let draft_cursor = editor.state().cursor.clone();
    editor.replace_cell_range(
        whole(&editor)?,
        "recalled",
        CellReplacementCursor::EndOfReplacement,
    )?;
    let recalled = editor.current_history_node();
    assert_ne!(recalled, typed);
    type_character(&mut editor, 'z')?;
    assert_ne!(editor.current_history_node(), recalled);
    assert!(editor.undo());
    assert_eq!(editor.content(), "recalled");
    assert!(editor.undo());
    assert_eq!(editor.content(), "ab");
    assert_eq!(editor.state().cursor, draft_cursor);
    assert!(editor.undo());
    assert_eq!(editor.content(), "");
    assert!(editor.redo());
    assert!(editor.redo());
    editor.replace_cell_range(
        whole(&editor)?,
        "ab",
        CellReplacementCursor::Exact(draft_cursor.clone()),
    )?;
    assert_eq!(editor.state().cursor, draft_cursor);
    editor.replace_cell_range(whole(&editor)?, "", CellReplacementCursor::EndOfReplacement)?;
    let cleared = editor.current_history_node();
    type_character(&mut editor, 'x')?;
    assert_ne!(editor.current_history_node(), cleared);
    assert!(editor.undo());
    assert_eq!(editor.content(), "");
    assert!(editor.undo());
    assert_eq!(editor.content(), "ab");
    assert_eq!(editor.state().cursor, draft_cursor);
    assert_eq!(editor.get_config().undo_group_timeout_ms, u64::MAX);
    Ok(())
}

#[test]
fn cell_transactions_exact_restore_keeps_direction_secondary_cursors_and_branches() -> TestResult {
    let mut editor = editor("abc\ndef");
    let mut draft = CursorState::new(Selection::new(Position::new(1, 3), Position::new(1, 1)));
    draft.add_cursor(Selection::new(Position::new(0, 2), Position::new(0, 0)));
    editor.state_mut().cursor = draft.clone();
    editor.replace_cell_range(
        whole(&editor)?,
        "history",
        CellReplacementCursor::EndOfReplacement,
    )?;
    let history = editor.current_history_node();
    assert!(editor.undo());
    assert_eq!(editor.state().cursor, draft);
    editor.replace_cell_range(
        whole(&editor)?,
        "ABC\nDEF",
        CellReplacementCursor::Exact(draft.clone()),
    )?;
    let alternate = editor.current_history_node();
    assert_ne!(history, alternate);
    assert!(editor.undo());
    assert_eq!(editor.content(), "abc\ndef");
    assert_eq!(editor.state().cursor, draft);
    assert_eq!(editor.history_branches().len(), 2);
    assert!(editor.redo_branch(0));
    assert_eq!(editor.content(), "history");
    assert!(editor.undo());
    assert!(editor.redo_branch(1));
    assert_eq!(editor.content(), "ABC\nDEF");
    assert_eq!(editor.state().cursor, draft);
    Ok(())
}

#[test]
fn cell_transactions_multicursor_paste_handles_crlf_join_without_relocating_insert() -> TestResult {
    let mut editor = editor("\rX\n-\rY\n");
    let mut before = CursorState::new(Selection::new(Position::new(1, 1), Position::new(1, 0)));
    before.add_cursor(Selection::new(Position::new(3, 0), Position::new(3, 1)));
    editor.state_mut().cursor = before.clone();
    editor.paste_cells("e\u{301}👩‍🔬")?;
    assert_eq!(editor.content(), "\re\u{301}👩‍🔬\n-\re\u{301}👩‍🔬\n");
    let after = editor.state().cursor.clone();
    assert_eq!(after.primary.head, Position::new(1, 5));
    assert_eq!(
        after.secondary.first().map(|selection| selection.head),
        Some(Position::new(3, 5))
    );
    assert!(editor.undo());
    assert_eq!(editor.content(), "\rX\n-\rY\n");
    assert_eq!(editor.state().cursor, before);
    assert!(editor.redo());
    assert_eq!(editor.state().cursor, after);
    assert_eq!(editor.content(), "\re\u{301}👩‍🔬\n-\re\u{301}👩‍🔬\n");
    Ok(())
}

#[test]
fn cell_transactions_paste_has_two_group_barriers_but_legacy_paste_still_groups() -> TestResult {
    let mut cell = editor("");
    type_character(&mut cell, 'a')?;
    let typed = cell.current_history_node();
    cell.paste_cells("\nβ\r\nγ")?;
    let pasted = cell.current_history_node();
    assert_ne!(typed, pasted);
    type_character(&mut cell, 'z')?;
    assert_ne!(cell.current_history_node(), pasted);
    assert!(cell.undo());
    assert_eq!(cell.content(), "a\nβ\r\nγ");
    assert!(cell.undo());
    assert_eq!(cell.content(), "a");
    assert!(cell.undo());
    assert_eq!(cell.content(), "");
    let mut legacy = editor("");
    type_character(&mut legacy, 'a')?;
    let grouped = legacy.current_history_node();
    legacy.paste("b");
    type_character(&mut legacy, 'c')?;
    assert_eq!(legacy.current_history_node(), grouped);
    assert!(legacy.undo());
    assert_eq!(legacy.content(), "");
    Ok(())
}

#[test]
fn cell_transactions_identical_text_and_cursor_preserve_typing_group() -> TestResult {
    let mut editor = editor("");
    type_character(&mut editor, 'a')?;
    let node = editor.current_history_node();
    let revision = editor.state().document.revision();
    editor.replace_cell_range(
        whole(&editor)?,
        "a",
        CellReplacementCursor::EndOfReplacement,
    )?;
    assert_eq!(editor.state().document.revision(), revision);
    assert_eq!(editor.current_history_node(), node);
    type_character(&mut editor, 'b')?;
    assert_eq!(editor.current_history_node(), node);
    assert!(editor.undo());
    assert_eq!(editor.content(), "");
    Ok(())
}

#[test]
fn cell_transactions_adjacent_paste_targets_keep_original_byte_edges() -> TestResult {
    for (before, split, end, text, after) in [
        (
            "xy",
            Position::new(0, 1),
            Position::new(0, 2),
            "\u{301}",
            "\u{301}\u{301}",
        ),
        (
            "a\rb",
            Position::new(1, 0),
            Position::new(1, 1),
            "\n",
            "\n\n",
        ),
    ] {
        let mut editor = editor(before);
        let mut original = CursorState::new(Selection::new(Position::zero(), split));
        original.add_cursor(Selection::new(split, end));
        editor.state_mut().cursor = original.clone();
        editor.paste_cells(text)?;
        assert_eq!(editor.content(), after);
        let pasted = editor.state().cursor.clone();
        assert!(editor.undo());
        assert_eq!(editor.content(), before);
        assert_eq!(editor.state().cursor, original);
        assert!(editor.redo());
        assert_eq!(editor.content(), after);
        assert_eq!(editor.state().cursor, pasted);
    }
    Ok(())
}

#[test]
fn cell_transactions_history_jump_counts_content_before_a_final_cursor_only_edge() -> TestResult {
    use iridium_editor::EditorEvent;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut editor = editor("");
    let root = editor.current_history_node();
    editor.replace_cell_range(
        whole(&editor)?,
        "needle",
        CellReplacementCursor::EndOfReplacement,
    )?;
    let content_node = editor.current_history_node();
    editor.replace_cell_range(
        Range::empty(Position::zero()),
        "",
        CellReplacementCursor::Exact(CursorState::at(Position::zero())),
    )?;
    let cursor_node = editor.current_history_node();
    assert!(editor.jump_to_history_node(root));
    assert_eq!(editor.content(), "");
    editor.update_search("needle")?;
    assert_eq!(editor.search_match_count(), 0);
    let content_events = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&content_events);
    editor.add_listener(move |event| {
        if matches!(event, EditorEvent::ContentChanged { .. }) {
            observed.fetch_add(1, Ordering::Relaxed);
        }
    });
    assert!(editor.jump_to_history_node(cursor_node));
    assert_eq!(editor.content(), "needle");
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(editor.search_match_count(), 1);
    assert_eq!(content_events.load(Ordering::Relaxed), 1);
    assert!(editor.jump_to_history_node(content_node));
    assert_eq!(editor.cursor(), Position::new(0, 6));
    assert_eq!(content_events.load(Ordering::Relaxed), 1);
    assert!(editor.jump_to_history_node(cursor_node));
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(content_events.load(Ordering::Relaxed), 1);
    assert!(editor.jump_to_history_node(root));
    assert_eq!(editor.content(), "");
    assert_eq!(editor.search_match_count(), 0);
    assert_eq!(content_events.load(Ordering::Relaxed), 2);
    Ok(())
}
