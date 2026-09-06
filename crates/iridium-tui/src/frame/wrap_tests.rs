//! Fixed wrapped text, caret, source styling and pointer geometry expectations.

use super::embedding_tests::{TestResult, options, row_text};
use super::{Frame, Palette};
use crate::cell::CellBuffer;
use iridium_editor::cell_layout::{Affinity, ScreenRow};
use iridium_editor::commands::CommandArgs;
use iridium_editor::{Editor, Position, Selection};

#[test]
fn cells_wrap_preserves_whitespace_unicode_tabs_and_invisible_controls() -> TestResult {
    for (text, width, expected) in [
        ("alpha beta gamma", 10, vec!["alpha beta", " gamma    "]),
        ("e\u{301}界👩‍🔬x", 3, vec!["e\u{301}界", "👩‍🔬x"]),
        ("abc\t\tX", 4, vec!["abc ", "    ", "X   "]),
        ("a\u{7}\u{1b}b", 2, vec!["ab"]),
        ("界x", 1, vec![" ", "x"]),
    ] {
        let mut editor = Editor::with_defaults();
        editor.set_content(text);
        let prepared = Frame::prepare_cells(&editor, options(width, expected.len()))?;
        let mut buffer = CellBuffer::new(width, expected.len());
        let mut frame = Frame::new();
        frame.render_cells(&prepared, &mut buffer)?;
        for (row, expected) in expected.iter().enumerate() {
            assert_eq!(row_text(&buffer, row), *expected);
        }
        let before = buffer.clone();
        frame.render_cells(&prepared, &mut buffer)?;
        assert_eq!(buffer, before);
    }
    Ok(())
}

#[test]
fn cells_caret_trailing_edges_are_bounded_and_not_false_hit_roundtrips() -> TestResult {
    for (text, width, expected_column) in [("abcd", 4, 3), ("ab界", 4, 2), ("界", 1, 0)] {
        let mut editor = Editor::with_defaults();
        editor.set_content(text);
        editor.set_cursor(Position::new(0, text.chars().count()));
        let prepared = Frame::prepare_cells(&editor, options(width, 2))?;
        assert_eq!(prepared.layout().total_rows, 1);
        let caret = prepared.layout().primary_caret.ok_or("caret missing")?;
        assert_eq!(caret.position.column, expected_column);
        assert!(caret.trailing_edge);
        assert_ne!(
            prepared
                .position_at(caret.position.column, 0)?
                .ok_or("hit missing")?
                .position,
            editor.cursor()
        );
    }
    Ok(())
}

#[test]
fn cells_secondary_affinity_scroll_and_fresh_pointer_share_rows() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("abcdefgh\nijklmnop");
    editor
        .state_mut()
        .cursor
        .secondary
        .push(Selection::collapsed(Position::new(1, 0)));
    let opts = Frame::prepare_cells(&editor, options(4, 4))?.input_options();
    editor.run_cell_command("cursor.lineEnd", CommandArgs::NONE, opts)?;
    let prepared = Frame::prepare_cells(&editor, options(4, 4))?;
    assert_eq!(prepared.carets.len(), 2);
    assert_eq!(prepared.carets[0].placement.affinity, Affinity::Upstream);
    assert_eq!(prepared.carets[1].placement.affinity, Affinity::Upstream);
    assert_eq!(prepared.carets[1].position.row, 2);
    drop(prepared);
    let scrolled = super::CellFrameOptions {
        first_row: ScreenRow(1),
        ..options(4, 2)
    };
    let prepared = Frame::prepare_cells(&editor, scrolled)?;
    let hit = prepared.position_at(2, 0)?.ok_or("scrolled hit missing")?;
    assert_eq!(hit.position, Position::new(0, 6));
    let opts = prepared.input_options();
    drop(prepared);
    editor.set_cell_pointer(
        ScreenRow(1),
        iridium_editor::cell_layout::CellColumn(2),
        false,
        opts,
    )?;
    assert_eq!(editor.cursor(), hit.position);
    Ok(())
}

#[test]
fn cells_wrapped_selections_and_search_keep_original_source_columns() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("ab界cdef");
    editor.find("de", &iridium_editor::search::SearchOptions::default())?;
    editor.set_selection(Position::new(0, 2), Position::new(0, 5));
    let prepared = Frame::prepare_cells(&editor, options(4, 3))?;
    let mut buffer = CellBuffer::new(4, 3);
    Frame::new().render_cells(&prepared, &mut buffer)?;
    assert_eq!(row_text(&buffer, 0), "ab界");
    assert_eq!(row_text(&buffer, 1), "cdef");
    let palette = Palette::from_theme(editor.get_theme());
    let base = palette.text().with_background(palette.current_line());
    assert_eq!(
        buffer.get(2, 0).ok_or("selection missing")?.style(),
        palette.selected(base)
    );
    assert_eq!(
        buffer.get(1, 1).ok_or("search missing")?.style(),
        palette.search_match(palette.selected(base), true)
    );
    Ok(())
}

#[test]
#[cfg(feature = "syntax")]
fn cells_wrapped_syntax_and_folded_placeholders_preserve_legacy_styles() -> TestResult {
    let mut editor = Editor::with_defaults();
    editor.set_content("fn outer() {\n    run();\n}\nend");
    editor.set_language(iridium_editor::Language::Rust);
    assert!(editor.fold_at(0));
    editor.state_mut().config.show_line_numbers = false;
    let mut legacy = CellBuffer::new(40, 5);
    Frame::new().render(&editor, super::Chrome::default(), &mut legacy);
    let prepared = Frame::prepare_cells(
        &editor,
        super::CellFrameOptions {
            chrome: Some(super::Chrome::default()),
            ..options(40, 5)
        },
    )?;
    let mut cells = CellBuffer::new(40, 5);
    Frame::new().render_cells(&prepared, &mut cells)?;
    assert_eq!(cells, legacy);
    assert!(row_text(&cells, 0).starts_with("fn outer() { "));
    assert!(row_text(&cells, 1).starts_with("end"));
    assert_eq!(
        prepared
            .position_at(0, 1)?
            .ok_or("folded projection missing")?
            .position,
        Position::new(3, 0)
    );
    drop(prepared);
    editor.unfold_all();
    editor.set_content("\nlet alpha = 123;");
    let prepared = Frame::prepare_cells(&editor, options(4, 8))?;
    let mut cells = CellBuffer::new(4, 8);
    Frame::new().render_cells(&prepared, &mut cells)?;
    assert_eq!(row_text(&cells, 1), "let ");
    assert_eq!(row_text(&cells, 2), "alph");
    assert_eq!(row_text(&cells, 3), "a = ");
    assert_eq!(row_text(&cells, 4), "123;");
    let mut baseline = CellBuffer::new(40, 4);
    Frame::new().render(&editor, super::Chrome::default(), &mut baseline);
    assert_eq!(
        cells.get(0, 4).ok_or("wrapped literal missing")?.style(),
        baseline.get(12, 1).ok_or("legacy literal missing")?.style()
    );
    assert_ne!(
        cells.get(0, 1).ok_or("keyword missing")?.style().foreground,
        Palette::from_theme(editor.get_theme()).text().foreground
    );
    Ok(())
}

#[cfg(feature = "syntax")]
fn parsed_editor(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_language(iridium_editor::Language::Rust);
    editor.set_cursor(Position::new(0, text.chars().count()));
    editor
}

#[cfg(feature = "syntax")]
fn parse_counters(editor: &Editor) -> (u64, u64, u64) {
    let state = editor.state();
    (
        state.document.revision(),
        state.syntax.full_parses(),
        state.syntax.incremental_parses(),
    )
}

#[cfg(feature = "syntax")]
fn rendered_bare(frame: &mut Frame, editor: &Editor) -> Result<CellBuffer, super::CellFrameError> {
    let prepared = Frame::prepare_cells(editor, options(20, 2))?;
    let mut buffer = CellBuffer::new(20, 2);
    frame.render_cells(&prepared, &mut buffer)?;
    Ok(buffer)
}

#[test]
#[cfg(feature = "syntax")]
fn cells_frame_cache_uses_actual_source_across_equal_counter_editors() -> TestResult {
    let first = parsed_editor("let x = 1;");
    let second = parsed_editor("\"a string\"");
    assert_eq!(parse_counters(&first), parse_counters(&second));
    let first_expected = rendered_bare(&mut Frame::new(), &first)?;
    let second_expected = rendered_bare(&mut Frame::new(), &second)?;
    assert_ne!(
        first_expected
            .get(0, 0)
            .ok_or("first fixture cell")?
            .style()
            .foreground,
        second_expected
            .get(0, 0)
            .ok_or("second fixture cell")?
            .style()
            .foreground
    );
    let mut reused = Frame::new();
    for (editor, expected) in [
        (&first, &first_expected),
        (&second, &second_expected),
        (&first, &first_expected),
        (&second, &second_expected),
    ] {
        let actual = rendered_bare(&mut reused, editor)?;
        assert_eq!(
            actual
                .get(0, 0)
                .ok_or("actual first cell")?
                .style()
                .foreground,
            expected
                .get(0, 0)
                .ok_or("expected first cell")?
                .style()
                .foreground
        );
        assert_eq!(&actual, expected);
    }
    Ok(())
}

#[test]
#[cfg(feature = "syntax")]
fn cells_frame_cache_refuses_divergent_clones_with_equal_ids_and_revisions() -> TestResult {
    let base = iridium_editor::Document::new("");
    let mut first_document = base.clone();
    let mut second_document = base;
    first_document.insert(Position::zero(), "let x = 1;")?;
    second_document.insert(Position::zero(), "\"a string\"")?;
    assert_eq!(first_document.id(), second_document.id());
    assert_eq!(first_document.revision(), second_document.revision());
    let mut first = parsed_editor("let x = 1;");
    let mut second = parsed_editor("\"a string\"");
    first.state_mut().document = first_document;
    second.state_mut().document = second_document;
    assert_eq!(parse_counters(&first), parse_counters(&second));
    let expected = rendered_bare(&mut Frame::new(), &second)?;
    let mut reused = Frame::new();
    rendered_bare(&mut reused, &first)?;
    // Replace the editor in the same host slot: a saved numeric address or
    // id/revision pair would not establish that the old spans describe it.
    first = second;
    let actual = rendered_bare(&mut reused, &first)?;
    assert_eq!(
        actual
            .get(0, 0)
            .ok_or("actual clone cell")?
            .style()
            .foreground,
        expected
            .get(0, 0)
            .ok_or("expected clone cell")?
            .style()
            .foreground
    );
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
#[cfg(feature = "syntax")]
fn cells_frame_cache_retains_same_source_across_windows_and_repeated_paint() -> TestResult {
    let editor = parsed_editor(&"let x = 1;\n".repeat(300));
    let first = Frame::prepare_cells(&editor, options(20, 2))?;
    let mut frame = Frame::new();
    frame.render_cells(&first, &mut CellBuffer::new(20, 2))?;
    let distant = Frame::prepare_cells(
        &editor,
        super::CellFrameOptions {
            first_row: ScreenRow(220),
            ..options(20, 2)
        },
    )?;
    let mut buffer = CellBuffer::new(20, 2);
    frame.render_cells(&distant, &mut buffer)?;
    assert_eq!(
        frame
            .highlighting
            .as_ref()
            .ok_or("missing highlight cache")?
            .rebuilds(),
        2
    );
    let before = buffer.clone();
    frame.render_cells(&distant, &mut buffer)?;
    assert_eq!(
        frame
            .highlighting
            .as_ref()
            .ok_or("missing reused cache")?
            .rebuilds(),
        2
    );
    assert_eq!(buffer, before);
    Ok(())
}
