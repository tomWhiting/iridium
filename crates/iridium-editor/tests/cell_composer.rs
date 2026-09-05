//! External-host composer integration: one live editor, reversible edits and host-owned sends.

use iridium_editor::cell_layout::{CellColumn, CellRowMap, CellWrapParameters, ScreenRow};
use iridium_editor::editor::CellInputOptions;
use iridium_editor::{
    CommandArgs, CommandCategory, CommandId, CommandMeta, Editor, EditorKeyResult, KeyBinding,
    KeyCode, KeyEvent, Keymap, Modifiers, Position,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const fn options(columns: usize) -> CellInputOptions {
    CellInputOptions {
        wrap: CellWrapParameters::new(columns, 4),
        visible_rows: 3,
    }
}

#[test]
fn public_composer_keeps_unicode_pairing_paste_resize_and_history_in_one_editor() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_undo_group_timeout_ms(0);
    editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('(')), options(8))?;
    assert_eq!(editor.content(), "()");
    assert_eq!(editor.cursor(), Position::new(0, 1));
    editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('A')), options(8))?;
    let before_paste = editor.current_history_node();
    let old_selection = editor.state().cursor.clone();
    let before_count = editor.history_snapshot().info.node_count;
    editor.paste_cells("e\u{301}👩‍🔬\nβ")?;
    assert_eq!(editor.content(), "(Ae\u{301}👩‍🔬\nβ)");
    assert_eq!(editor.cursor(), Position::new(1, 1));
    assert_eq!(editor.history_snapshot().info.node_count, before_count + 1);
    let pasted = editor.current_history_node();
    let pasted_selection = serde_json::to_string(&editor.state().cursor)?;
    for width in [8, 2, 0, 5] {
        let state = editor.state();
        let map = CellRowMap::prepare(&state.document, &state.fold_state, options(width).wrap)?;
        assert_eq!(map.parameters().columns(), width);
        assert_eq!(editor.current_history_node(), pasted);
        assert_eq!(
            serde_json::to_string(&editor.state().cursor)?,
            pasted_selection
        );
    }
    editor.set_cell_pointer(ScreenRow(0), CellColumn(0), false, options(2))?;
    assert_eq!(editor.cursor(), Position::zero());
    assert_eq!(editor.current_history_node(), pasted);
    editor.run_cell_command("history.undo", CommandArgs::NONE, options(2))?;
    assert_eq!(editor.content(), "(A)");
    assert_eq!(editor.state().cursor, old_selection);
    assert_eq!(editor.current_history_node(), before_paste);
    editor.run_cell_command("history.redo", CommandArgs::NONE, options(5))?;
    assert_eq!(
        serde_json::to_string(&editor.state().cursor)?,
        pasted_selection
    );
    editor.run_cell_command("history.undo", CommandArgs::NONE, options(5))?;
    editor.paste_cells("Z")?;
    let alternate = editor.current_history_node();
    assert_eq!(editor.content(), "(AZ)");
    assert_ne!(alternate, pasted);
    editor.run_cell_command("history.undo", CommandArgs::NONE, options(5))?;
    assert_eq!(editor.history_branches().len(), 2);
    assert!(editor.history_node(pasted).is_some());
    assert!(editor.history_node(alternate).is_some());
    assert!(editor.redo_branch(0));
    assert_eq!(editor.content(), "(Ae\u{301}👩‍🔬\nβ)");
    assert_eq!(
        serde_json::to_string(&editor.state().cursor)?,
        pasted_selection
    );
    Ok(())
}

#[test]
fn public_composer_host_selects_enter_and_alt_enter_without_paste_submitting() -> TestResult {
    const SEND: CommandId = CommandId::from_static("fixture.send");
    const SEND_ALTERNATE: CommandId = CommandId::from_static("fixture.sendAlternate");
    let mut editor = Editor::with_defaults();
    for (id, title) in [(SEND, "Send"), (SEND_ALTERNATE, "Send alternate")] {
        editor.register_command(CommandMeta::new(id, title, CommandCategory::EDITING))?;
    }
    let mut layer = Keymap::new("host submit preference");
    layer.push(KeyBinding::parse("enter", SEND)?);
    layer.push(KeyBinding::parse("alt-enter", SEND_ALTERNATE)?);
    editor.push_keymap(layer)?;
    assert_eq!(
        editor.handle_cell_key(&KeyEvent::simple(KeyCode::Enter), options(8))?,
        EditorKeyResult::HostCommand {
            command: SEND,
            args: CommandArgs::NONE
        }
    );
    assert_eq!(
        editor.handle_cell_key(
            &KeyEvent::new(
                KeyCode::Enter,
                Modifiers {
                    alt: true,
                    ..Modifiers::none()
                }
            ),
            options(8)
        )?,
        EditorKeyResult::HostCommand {
            command: SEND_ALTERNATE,
            args: CommandArgs::NONE
        }
    );
    editor.paste_cells("one\ntwo")?;
    assert_eq!(editor.content(), "one\ntwo");
    assert!(editor.pop_keymap().is_some());
    editor.handle_cell_key(&KeyEvent::simple(KeyCode::Enter), options(8))?;
    assert_eq!(editor.content(), "one\ntwo\n");
    Ok(())
}
