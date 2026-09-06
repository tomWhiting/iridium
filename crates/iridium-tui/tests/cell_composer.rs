//! Public terminal-host composition, including input categories and one borrowed local rectangle.

use iridium_editor::cell_layout::{CellColumn, ScreenRow};
use iridium_editor::{
    CommandArgs, CommandCategory, CommandId, CommandMeta, CursorState, Editor, EditorKeyResult,
    KeyBinding, KeyCode, KeyEvent, KeyPress, Keymap, Modifiers, Position, Selection,
};
use iridium_tui::cell::{CellBuffer, CellContent};
use iridium_tui::frame::{CellFrameError, CellFrameOptions, Frame, Palette};
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

#[test]
fn host_primary_cursor_preserves_backward_selection_and_search_underlay() -> TestResult {
    for search in [false, true] {
        let mut editor = Editor::with_defaults();
        editor.set_content("a界bc");
        editor.state_mut().config.highlight_current_line = false;
        if search {
            editor.update_search("界")?;
            assert_eq!(editor.current_match_index(), Some(0));
        }
        editor.state_mut().cursor =
            CursorState::new(Selection::new(Position::new(0, 4), Position::new(0, 1)));
        let before_cursor = editor.state().cursor.clone();
        let before_revision = editor.state().document.revision();
        let before_history = editor.current_history_node();
        let prepared = Frame::prepare_cells(&editor, options(5, 0))?;
        let before_hit = prepared.position_at(2, 0)?;
        let mut frame = Frame::new();
        let mut legacy = CellBuffer::new(5, 3);
        let legacy_layout = frame.render_cells(&prepared, &mut legacy)?;
        let mut explicit = CellBuffer::new(5, 3);
        frame.render_cells_with_primary_caret(&prepared, &mut explicit, true)?;
        assert_eq!(explicit, legacy);
        let mut hardware = CellBuffer::new(5, 3);
        let hardware_layout =
            frame.render_cells_with_primary_caret(&prepared, &mut hardware, false)?;
        let palette = Palette::from_theme(editor.get_theme());
        let selected = palette.selected(palette.text());
        let underlay = if search {
            palette.search_match(selected, true)
        } else {
            selected
        };
        for column in [1, 2] {
            assert_eq!(
                hardware.get(column, 0).ok_or("wide cell absent")?.style(),
                underlay
            );
            assert_eq!(
                legacy.get(column, 0).ok_or("wide caret absent")?.style(),
                palette.caret(underlay)
            );
        }
        assert!(
            hardware
                .get(2, 0)
                .ok_or("continuation absent")?
                .is_continuation()
        );
        assert_eq!(
            hardware.get(3, 0).ok_or("selected b absent")?.style(),
            selected
        );
        assert_eq!(
            hardware.get(0, 0).ok_or("plain a absent")?.style(),
            palette.text()
        );
        assert_eq!(row_text(&hardware, 0)?, "a界bc");
        assert_eq!(hardware_layout.primary_caret, legacy_layout.primary_caret);
        assert_eq!(hardware_layout.caret(), legacy_layout.caret());
        assert_eq!(prepared.position_at(2, 0)?, before_hit);
        assert_eq!(editor.state().cursor, before_cursor);
        assert_eq!(editor.state().document.revision(), before_revision);
        assert_eq!(editor.current_history_node(), before_history);
    }
    Ok(())
}

#[test]
fn host_primary_cursor_keeps_trailing_wide_glyph_and_layout_hint() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("界");
    editor.state_mut().config.highlight_current_line = false;
    editor.state_mut().cursor = CursorState::at(Position::new(0, 1));
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            rows: 1,
            ..options(2, 0)
        },
    )?;
    let caret = prepared
        .layout()
        .primary_caret
        .ok_or("primary hint absent")?;
    assert!(caret.trailing_edge);
    assert_eq!((caret.position.column, caret.position.row), (0, 0));
    assert_eq!(caret.placement.column, CellColumn(2));
    let mut frame = Frame::new();
    let mut buffer = CellBuffer::new(2, 1);
    let rendered = frame.render_cells_with_primary_caret(&prepared, &mut buffer, false)?;
    assert_eq!(rendered.primary_caret, Some(caret));
    assert_eq!(rendered.caret(), Some(caret.position));
    assert_eq!(row_text(&buffer, 0)?, "界");
    assert!(
        buffer
            .get(1, 0)
            .ok_or("wide continuation absent")?
            .is_continuation()
    );
    let palette = Palette::from_theme(editor.get_theme());
    for column in [0, 1] {
        assert_eq!(
            buffer.get(column, 0).ok_or("glyph absent")?.style(),
            palette.text()
        );
    }
    Ok(())
}

#[test]
fn host_primary_cursor_keeps_first_visible_secondary_when_primary_is_offscreen() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("a\n界\nc");
    editor.state_mut().config.highlight_current_line = false;
    editor.state_mut().cursor = CursorState {
        primary: Selection::collapsed(Position::new(0, 0)),
        secondary: vec![Selection::collapsed(Position::new(1, 0))],
    };
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            rows: 1,
            ..options(2, 1)
        },
    )?;
    assert_eq!(prepared.layout().primary_caret, None);
    let mut frame = Frame::new();
    let mut buffer = CellBuffer::new(2, 1);
    let rendered = frame.render_cells_with_primary_caret(&prepared, &mut buffer, false)?;
    assert_eq!(rendered.caret(), None);
    assert_eq!(row_text(&buffer, 0)?, "界");
    let palette = Palette::from_theme(editor.get_theme());
    for column in [0, 1] {
        assert_eq!(
            buffer.get(column, 0).ok_or("secondary absent")?.style(),
            palette.caret(palette.text())
        );
    }
    Ok(())
}

#[test]
fn host_primary_cursor_keeps_coincident_secondary_carets() -> TestResult {
    // Distinct logical positions can share a physical wide-glyph caret. An
    // exact duplicate selection must also remain a painted secondary entry.
    for secondary in [Position::new(0, 0), Position::new(0, 1)] {
        let mut editor = Editor::with_defaults();
        editor.set_content("界");
        editor.state_mut().config.highlight_current_line = false;
        editor.state_mut().cursor = CursorState {
            primary: Selection::collapsed(Position::new(0, 1)),
            secondary: vec![Selection::collapsed(secondary)],
        };
        let prepared = Frame::prepare_cells(
            &editor,
            CellFrameOptions {
                rows: 1,
                ..options(2, 0)
            },
        )?;
        let mut frame = Frame::new();
        let mut buffer = CellBuffer::new(2, 1);
        frame.render_cells_with_primary_caret(&prepared, &mut buffer, false)?;
        let palette = Palette::from_theme(editor.get_theme());
        for column in [0, 1] {
            assert_eq!(
                buffer
                    .get(column, 0)
                    .ok_or("coincident caret absent")?
                    .style(),
                palette.caret(palette.text())
            );
        }
        let mut legacy = CellBuffer::new(2, 1);
        frame.render_cells(&prepared, &mut legacy)?;
        assert_eq!(buffer, legacy);
    }
    Ok(())
}

#[test]
fn host_primary_cursor_refuses_extent_mismatch_before_buffer_writes() -> TestResult {
    let editor = Editor::with_defaults();
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            rows: 1,
            ..options(2, 0)
        },
    )?;
    let mut frame = Frame::new();
    for paint_primary in [false, true] {
        let mut buffer = CellBuffer::new(3, 1);
        let palette = Palette::from_theme(editor.get_theme());
        buffer.fill(palette.selected(palette.text()));
        let before = buffer.clone();
        assert!(matches!(
            frame.render_cells_with_primary_caret(&prepared, &mut buffer, paint_primary),
            Err(CellFrameError::ExtentMismatch {
                prepared: (2, 1),
                buffer: (3, 1)
            })
        ));
        assert_eq!(buffer, before);
    }
    Ok(())
}
