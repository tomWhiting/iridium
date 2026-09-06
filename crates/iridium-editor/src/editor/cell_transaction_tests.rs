//! Atomic failure, event and history policy regressions for host cell transactions.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{CellInputError, CellReplacementCursor, Editor, EditorEvent};
use crate::document::{CursorState, Position, Range, Selection};
use crate::history::Command;
use crate::input::keyboard::cell_input::prepare_command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn editor(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_undo_group_timeout_ms(u64::MAX);
    editor
}

fn watch(editor: &mut Editor) -> Arc<AtomicUsize> {
    let sequence = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&sequence);
    editor.add_listener(move |event| {
        let digit = match event {
            EditorEvent::ContentChanged { .. } => 1,
            EditorEvent::SelectionChanged { .. } => 2,
            EditorEvent::SearchUpdated { .. } => 3,
            EditorEvent::Error { .. } => 4,
            _ => 5,
        };
        observed.store(
            observed.load(Ordering::Relaxed) * 10 + digit,
            Ordering::Relaxed,
        );
    });
    sequence
}

fn snapshot(editor: &Editor) -> Result<serde_json::Value, serde_json::Error> {
    Ok(serde_json::json!({
        "text": editor.content(),
        "revision": editor.state().document.revision(),
        "cursor": editor.state().cursor,
        "history": serde_json::to_value(editor.history_snapshot())?,
    }))
}

#[test]
fn cell_transaction_rejections_preserve_every_published_field_and_events() -> TestResult {
    let mut editor = editor("e\u{301}x\nabc");
    let events = watch(&mut editor);
    for range in [
        Range {
            start: Position::new(1, 2),
            end: Position::new(0, 0),
        },
        Range::empty(Position::new(8, 0)),
        Range::empty(Position::new(0, 99)),
        Range::empty(Position::new(0, 1)),
    ] {
        let before = snapshot(&editor)?;
        assert!(
            editor
                .replace_cell_range(range, "z", CellReplacementCursor::EndOfReplacement)
                .is_err()
        );
        assert_eq!(snapshot(&editor)?, before);
        assert_eq!(events.load(Ordering::Relaxed), 0);
    }
    let before = snapshot(&editor)?;
    assert!(
        editor
            .replace_cell_range(
                Range::empty(Position::zero()),
                "a",
                CellReplacementCursor::Exact(CursorState::at(Position::new(0, 2))),
            )
            .is_err()
    );
    assert_eq!(snapshot(&editor)?, before);
    editor.state.cursor = CursorState::at(Position::new(0, 1));
    let before = snapshot(&editor)?;
    assert!(
        editor
            .replace_cell_range(
                Range::empty(Position::zero()),
                "z",
                CellReplacementCursor::EndOfReplacement
            )
            .is_err()
    );
    assert_eq!(snapshot(&editor)?, before);
    editor.state.cursor = CursorState::at(Position::zero());
    editor.state.read_only = true;
    let before = snapshot(&editor)?;
    assert!(matches!(
        editor.replace_cell_range(
            Range::empty(Position::zero()),
            "z",
            CellReplacementCursor::EndOfReplacement
        ),
        Err(CellInputError::ReadOnly)
    ));
    assert_eq!(snapshot(&editor)?, before);
    assert_eq!(events.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn cell_transaction_failed_later_preflight_cannot_publish_prefix() -> TestResult {
    let editor = editor("ab");
    let before = snapshot(&editor)?;
    let command = Command::Compound {
        commands: vec![
            Command::Insert {
                position: Position::zero(),
                text: "e\u{301}".to_owned(),
            },
            Command::Delete {
                range: Range::new(Position::new(0, 1), Position::new(0, 2)),
                deleted_text: "b".to_owned(),
            },
        ],
    };
    // Both original coordinates resolve for the span, but the second edit now
    // ends inside the inserted grapheme bytes. The scratch prefix must stay private.
    assert!(prepare_command(command, &editor.state.document, &editor.state.cursor).is_err());
    assert_eq!(snapshot(&editor)?, before);
    Ok(())
}

#[test]
fn cell_transaction_cursor_only_replay_has_no_content_events_or_revision() -> TestResult {
    let mut editor = editor("abc");
    let events = watch(&mut editor);
    let revision = editor.state.document.revision();
    editor.replace_cell_range(
        Range::empty(Position::zero()),
        "",
        CellReplacementCursor::Exact(CursorState::new(Selection::new(
            Position::new(0, 3),
            Position::new(0, 1),
        ))),
    )?;
    assert_eq!(events.load(Ordering::Relaxed), 2);
    assert_eq!(editor.state.document.revision(), revision);
    assert!(editor.undo());
    assert_eq!(events.load(Ordering::Relaxed), 22);
    assert_eq!(editor.state.cursor, CursorState::at(Position::zero()));
    assert!(editor.redo());
    assert_eq!(events.load(Ordering::Relaxed), 222);
    assert_eq!(editor.state.document.revision(), revision);
    Ok(())
}

#[test]
fn cell_transaction_content_commit_emits_once_in_order_and_records_unchanged_cursor() -> TestResult
{
    let mut editor = editor("abc");
    let old_cursor = CursorState::new(Selection::new(Position::new(0, 3), Position::new(0, 1)));
    editor.state.cursor = old_cursor.clone();
    let events = watch(&mut editor);
    editor.replace_cell_range(
        Range::new(Position::zero(), Position::new(0, 1)),
        "z",
        CellReplacementCursor::Exact(old_cursor.clone()),
    )?;
    assert_eq!(editor.content(), "zbc");
    assert_eq!(events.load(Ordering::Relaxed), 12);
    assert!(editor.undo());
    assert_eq!(editor.content(), "abc");
    assert_eq!(editor.state.cursor, old_cursor);
    assert!(editor.redo());
    assert_eq!(editor.content(), "zbc");
    assert_eq!(editor.state.cursor, old_cursor);
    Ok(())
}

#[test]
fn cell_transaction_readonly_and_noop_preserve_pending_key_sequence() -> TestResult {
    use super::CellInputOptions;
    use crate::cell_layout::CellWrapParameters;
    use crate::{
        CommandArgs, CommandCategory, CommandId, CommandMeta, EditorKeyResult, KeyBinding, KeyCode,
        KeyEvent, Keymap,
    };
    const HOST: CommandId = CommandId::from_static("fixture.transaction");
    let mut editor = editor("");
    editor.register_command(CommandMeta::new(HOST, "Host", CommandCategory::EDITING))?;
    let mut map = Keymap::new("transaction witness");
    map.push(KeyBinding::parse("g g", HOST)?);
    editor.push_keymap(map)?;
    let options = CellInputOptions {
        wrap: CellWrapParameters::new(4, 4),
        visible_rows: 2,
    };
    editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('g')), options)?;
    editor.replace_cell_range(
        Range::empty(Position::zero()),
        "",
        CellReplacementCursor::EndOfReplacement,
    )?;
    assert!(
        editor
            .replace_cell_range(
                Range::empty(Position::new(8, 0)),
                "x",
                CellReplacementCursor::EndOfReplacement
            )
            .is_err()
    );
    editor.state.read_only = true;
    assert!(matches!(
        editor.replace_cell_range(
            Range::empty(Position::zero()),
            "x",
            CellReplacementCursor::EndOfReplacement
        ),
        Err(CellInputError::ReadOnly)
    ));
    editor.state.read_only = false;
    assert_eq!(
        editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('g')), options)?,
        EditorKeyResult::HostCommand {
            command: HOST,
            args: CommandArgs::NONE
        }
    );
    Ok(())
}

#[test]
fn cell_transaction_refreshes_search_before_publishing_content() -> TestResult {
    let mut editor = editor("needle");
    editor.update_search("needle")?;
    assert_eq!(editor.search_match_count(), 1);
    let events = watch(&mut editor);
    editor.replace_cell_range(
        Range::new(Position::zero(), Position::new(0, 6)),
        "other",
        CellReplacementCursor::EndOfReplacement,
    )?;
    assert_eq!(editor.search_match_count(), 0);
    assert_eq!(events.load(Ordering::Relaxed), 312);
    assert!(editor.undo());
    assert_eq!(editor.search_match_count(), 1);
    Ok(())
}
