//! Public terminal-host composition, including input categories and one borrowed local rectangle.

use iridium_editor::cell_layout::{CellColumn, ScreenRow};
use iridium_editor::{
    CommandArgs, CommandCategory, CommandId, CommandMeta, Editor, EditorKeyResult, KeyBinding,
    KeyCode, KeyEvent, KeyPress, Keymap, Modifiers, Position,
};
use iridium_tui::cell::{CellBuffer, CellContent};
use iridium_tui::frame::{CellFrameOptions, Frame};
use iridium_tui::input::{TerminalInput, translate};
use terminput::{
    Encoding, Event, KeyCode as TerminalCode, KeyEvent as TerminalKey, KeyEventKind, KeyModifiers,
    KittyFlags,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const fn options(columns: usize, first_row: usize) -> CellFrameOptions<'static> {
    CellFrameOptions {
        columns,
        rows: 3,
        first_row: ScreenRow(first_row),
        chrome: None,
    }
}

fn from_bytes(bytes: &[u8]) -> Result<TerminalInput, Box<dyn std::error::Error>> {
    Ok(translate(
        Event::parse_from(bytes)?.ok_or("fixture terminal sequence incomplete")?,
    ))
}

fn encoded(event: &Event, encoding: Encoding) -> Result<TerminalInput, Box<dyn std::error::Error>> {
    let mut bytes = [0_u8; 256];
    let written = event.encode(&mut bytes, encoding)?;
    from_bytes(&bytes[..written])
}

fn key(event: &TerminalInput) -> Result<&KeyEvent, Box<dyn std::error::Error>> {
    match event {
        TerminalInput::Key(key) => Ok(key),
        other => Err(format!("fixture expected a key, received {other:?}").into()),
    }
}

fn row_text(buffer: &CellBuffer, row: usize) -> Result<String, Box<dyn std::error::Error>> {
    let mut text = String::new();
    for cell in buffer.row(row).ok_or("fixture row absent")? {
        if let CellContent::Grapheme(grapheme) = cell.content() {
            grapheme.push_to(&mut text);
        }
    }
    Ok(text)
}

#[test]
fn public_terminal_host_keeps_editing_rendering_pointer_and_history_coherent() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_undo_group_timeout_ms(0);
    let mut frame = Frame::new();
    let mut input = Frame::prepare_cells(&editor, options(8, 0))?.input_options();
    for bytes in [b"(".as_slice(), b"A".as_slice()] {
        editor.handle_cell_key(key(&from_bytes(bytes)?)?, input)?;
    }
    assert_eq!(editor.content(), "(A)");
    let before_paste = editor.current_history_node();
    let before_cursor = editor.state().cursor.clone();
    let pasted = from_bytes("\u{1b}[200~e\u{301}👩‍🔬\nβ\u{1b}[201~".as_bytes())?;
    match pasted {
        TerminalInput::Paste(text) => editor.paste_cells(&text)?,
        other => {
            return Err(format!("fixture expected bracketed paste, received {other:?}").into());
        },
    }
    assert_eq!(editor.content(), "(Ae\u{301}👩‍🔬\nβ)");
    let paste_node = editor.current_history_node();
    let paste_cursor = editor.state().cursor.clone();
    for (width, first, expected) in [
        (8, 0, "(Ae\u{301}👩‍🔬   "),
        (4, 1, "👩‍🔬  "),
        (8, 1, "β)      "),
    ] {
        let prepared = Frame::prepare_cells(&editor, options(width, first))?;
        let mut buffer = CellBuffer::new(width, 3);
        let layout = frame.render_cells(&prepared, &mut buffer)?;
        assert_eq!(row_text(&buffer, 0)?, expected);
        assert_eq!(layout.status_row, None);
        assert_eq!(layout.text.origin, 0);
        assert_eq!(layout.text.width, width);
        input = prepared.input_options();
        assert_eq!(editor.current_history_node(), paste_node);
        assert_eq!(editor.state().cursor, paste_cursor);
    }
    // Parent origin (20, 10) is a host concern; the local first visible row is 1.
    let parent_hit = (20_usize, 10_usize);
    let local = (parent_hit.0 - 20, parent_hit.1 - 10);
    let prepared = Frame::prepare_cells(&editor, options(8, 1))?;
    let hit = prepared
        .position_at(local.0, local.1)?
        .ok_or("visible beta hit absent")?;
    assert_eq!(hit.position, Position::new(1, 0));
    drop(prepared);
    editor.set_cell_pointer(ScreenRow(1 + local.1), CellColumn(local.0), false, input)?;
    assert_eq!(editor.cursor(), hit.position);
    assert_eq!(editor.current_history_node(), paste_node);
    editor.run_cell_command("history.undo", CommandArgs::NONE, input)?;
    assert_eq!(editor.current_history_node(), before_paste);
    assert_eq!(editor.state().cursor, before_cursor);
    editor.run_cell_command("history.redo", CommandArgs::NONE, input)?;
    assert_eq!(editor.state().cursor, paste_cursor);
    editor.run_cell_command("history.undo", CommandArgs::NONE, input)?;
    editor.paste_cells("Z")?;
    let branch_node = editor.current_history_node();
    assert_eq!(editor.content(), "(AZ)");
    assert!(editor.undo());
    assert_eq!(editor.history_branches().len(), 2);
    assert!(editor.history_node(branch_node).is_some());
    assert!(editor.redo_branch(0));
    assert_eq!(editor.current_history_node(), paste_node);
    assert_eq!(editor.state().cursor, paste_cursor);
    let prepared = Frame::prepare_cells(&editor, options(8, 0))?;
    let mut final_buffer = CellBuffer::new(8, 3);
    frame.render_cells(&prepared, &mut final_buffer)?;
    assert_eq!(row_text(&final_buffer, 0)?, "(Ae\u{301}👩‍🔬   ");
    Ok(())
}

#[test]
fn host_submit_keys_exclude_release_and_bracketed_paste() -> TestResult {
    const SEND: CommandId = CommandId::from_static("fixture.send");
    const SEND_ALT: CommandId = CommandId::from_static("fixture.sendAlternate");
    let mut editor = Editor::with_defaults();
    for (id, title) in [(SEND, "Send"), (SEND_ALT, "Send alternate")] {
        editor.register_command(CommandMeta::new(id, title, CommandCategory::EDITING))?;
    }
    let mut host = Keymap::new("host send choice");
    host.push(KeyBinding::parse("enter", SEND)?);
    host.push(KeyBinding::parse("alt-enter", SEND_ALT)?);
    editor.push_keymap(host)?;
    let input = Frame::prepare_cells(&editor, options(8, 0))?.input_options();
    for (modifiers, expected) in [(KeyModifiers::NONE, SEND), (KeyModifiers::ALT, SEND_ALT)] {
        let event = Event::Key(TerminalKey::new(TerminalCode::Enter).modifiers(modifiers));
        assert_eq!(
            editor.handle_cell_key(
                key(&encoded(&event, Encoding::Kitty(KittyFlags::all()))?)?,
                input
            )?,
            EditorKeyResult::HostCommand {
                command: expected,
                args: CommandArgs::NONE
            }
        );
    }
    let release = Event::Key(TerminalKey::new(TerminalCode::Enter).kind(KeyEventKind::Release));
    assert_eq!(
        encoded(&release, Encoding::Kitty(KittyFlags::all()))?,
        TerminalInput::Release(KeyPress::new(KeyCode::Enter, Modifiers::none()))
    );
    // The host routes event categories before keys: release has no editor call.
    let paste = from_bytes(b"\x1b[200~one\ntwo\x1b[201~")?;
    match paste {
        TerminalInput::Paste(text) => editor.paste_cells(&text)?,
        other => return Err(format!("fixture expected paste, received {other:?}").into()),
    }
    assert_eq!(editor.content(), "one\ntwo");
    assert!(editor.pop_keymap().is_some());
    editor.handle_cell_key(key(&from_bytes(b"\r")?)?, input)?;
    assert_eq!(editor.content(), "one\ntwo\n");
    Ok(())
}

#[test]
fn public_terminal_translation_keeps_text_modifiers_and_event_kinds() -> TestResult {
    for bytes in [b"Z".as_slice(), "É".as_bytes(), "界".as_bytes()] {
        let event = from_bytes(bytes)?;
        let expected = std::str::from_utf8(bytes)?
            .chars()
            .next()
            .ok_or("empty text fixture")?;
        assert_eq!(key(&event)?.key, KeyCode::Char(expected));
        assert!(!key(&event)?.modifiers.alt_graph);
    }
    for (modifiers, expected) in [
        (
            KeyModifiers::ALT,
            Modifiers {
                alt: true,
                ..Modifiers::none()
            },
        ),
        (
            KeyModifiers::SUPER,
            Modifiers {
                meta: true,
                ..Modifiers::none()
            },
        ),
    ] {
        let event = Event::Key(TerminalKey::new(TerminalCode::Char('x')).modifiers(modifiers));
        assert_eq!(
            key(&encoded(&event, Encoding::Kitty(KittyFlags::all()))?)?.modifiers,
            expected
        );
    }
    for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
        let event = Event::Key(TerminalKey::new(TerminalCode::Char('z')).kind(kind));
        assert_eq!(
            key(&encoded(&event, Encoding::Kitty(KittyFlags::all()))?)?.is_repeat,
            kind == KeyEventKind::Repeat
        );
    }
    // Legacy C0 cannot recover shifted Control-Z, and Control-I/M name Tab/Enter.
    assert_eq!(
        from_bytes(&[26])?,
        TerminalInput::Key(KeyEvent::new(KeyCode::Char('z'), Modifiers::ctrl()))
    );
    assert_eq!(
        from_bytes(&[9])?,
        TerminalInput::Key(KeyEvent::simple(KeyCode::Tab))
    );
    assert_eq!(
        from_bytes(&[13])?,
        TerminalInput::Key(KeyEvent::simple(KeyCode::Enter))
    );
    Ok(())
}
