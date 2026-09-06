//! Public terminal behavior with the default parser and an explicit parser-free host.

use iridium_editor::cell_layout::ScreenRow;
use iridium_editor::editor::CellReplacementCursor;
use iridium_editor::{Editor, KeyCode, KeyEvent, Language, Position, Range};
use iridium_tui::cell::{CellBuffer, CellContent};
use iridium_tui::frame::{CellFrameOptions, Chrome, Frame, Palette};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const fn options() -> CellFrameOptions<'static> {
    CellFrameOptions {
        columns: 40,
        rows: 4,
        first_row: ScreenRow(0),
        chrome: None,
    }
}

#[test]
fn selected_syntax_feature_controls_colors_without_changing_legacy_cell_geometry() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("\nlet value = 123;");
    editor.set_language(Language::Rust);
    editor.state_mut().config.highlight_current_line = false;
    let prepared = Frame::prepare_cells(
        &editor,
        CellFrameOptions {
            chrome: Some(Chrome::default()),
            ..options()
        },
    )?;
    let mut frame = Frame::new();
    let mut cells = CellBuffer::new(40, 4);
    let layout = frame.render_cells(&prepared, &mut cells)?;
    let mut legacy = CellBuffer::new(40, 4);
    frame.render(&editor, Chrome::default(), &mut legacy);
    assert_eq!(cells, legacy);
    let palette = Palette::from_theme(editor.get_theme());
    #[cfg(feature = "syntax")]
    let expected = palette.highlighted(
        iridium_editor::HighlightType::Keyword,
        palette.text().background,
    );
    #[cfg(not(feature = "syntax"))]
    let expected = palette.text();
    let keyword = cells
        .get(layout.text.origin, 1)
        .ok_or("keyword cell absent")?;
    assert_eq!(keyword.style(), expected);
    let CellContent::Grapheme(glyph) = keyword.content() else {
        return Err("keyword unexpectedly starts with a continuation".into());
    };
    let mut text = String::new();
    glyph.push_to(&mut text);
    assert_eq!(text, "l");
    assert_eq!(
        prepared
            .position_at(layout.text.origin, 1)?
            .ok_or("keyword hit absent")?
            .position,
        Position::new(1, 0)
    );
    Ok(())
}

#[test]
fn language_autopairs_unicode_paste_and_host_transactions_work_in_both_scopes() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_language(Language::Rust);
    let input = Frame::prepare_cells(&editor, options())?.input_options();
    editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('(')), input)?;
    assert_eq!(editor.content(), "()");
    assert_eq!(editor.cursor(), Position::new(0, 1));
    editor.paste_cells("e\u{301}👩‍🔬\nβ")?;
    assert_eq!(editor.content(), "(e\u{301}👩‍🔬\nβ)");
    assert!(editor.undo());
    assert_eq!(editor.content(), "()");
    assert_eq!(editor.cursor(), Position::new(0, 1));
    editor.replace_cell_range(
        Range::new(Position::zero(), Position::new(0, 2)),
        "界",
        CellReplacementCursor::EndOfReplacement,
    )?;
    assert_eq!(editor.content(), "界");
    let prepared = Frame::prepare_cells(&editor, options())?;
    let mut buffer = CellBuffer::new(40, 4);
    Frame::new().render_cells_with_primary_caret(&prepared, &mut buffer, false)?;
    assert!(
        buffer
            .get(1, 0)
            .ok_or("wide continuation absent")?
            .is_continuation()
    );
    drop(prepared);
    assert!(editor.undo());
    assert_eq!(editor.content(), "()");
    assert_eq!(editor.cursor(), Position::new(0, 1));
    assert!(editor.redo());
    assert_eq!(editor.content(), "界");
    Ok(())
}

#[test]
fn parser_free_frame_has_no_retained_cache_or_snapshot_storage() {
    assert_eq!(std::mem::size_of::<Frame>() == 0, !cfg!(feature = "syntax"));
}
