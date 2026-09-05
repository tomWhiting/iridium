//! Kernel-only geometry, canonical mapping and source-lifetime regression tests.

use std::error::Error;

use unicode_segmentation::UnicodeSegmentation;

use super::{
    Affinity, CellColumn, CellLayoutError, CellLine, CellRowMap, CellWrapParameters, ClusterKind,
    ScreenRow,
};
use crate::editor::FoldState;
use crate::{Document, Editor, EditorConfig, Language, Position};

type TestResult = Result<(), Box<dyn Error>>;

fn fragments(map: &CellRowMap<'_>) -> Result<Vec<String>, Box<dyn Error>> {
    map.rows()
        .iter()
        .map(|row| {
            let line = map
                .document()
                .line(row.document_line())
                .ok_or("missing line")?;
            let fragment = line.get(row.byte_range()).ok_or("invalid byte span")?;
            Ok(fragment.to_owned())
        })
        .collect()
}

#[test]
fn wraps_graphemes_without_word_backtracking_or_whitespace_loss() -> TestResult {
    let document = Document::new("alpha beta gamma");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(10, 4))?;
    assert_eq!(fragments(&map)?, ["alpha beta", " gamma"]);
    assert_eq!(map.row(ScreenRow(0))?.scalar_range(), 0..10);
    assert_eq!(map.row(ScreenRow(1))?.scalar_range(), 10..16);
    assert_eq!(map.row(ScreenRow(0))?.width(), 10);
    Ok(())
}

#[test]
fn empty_lines_line_endings_and_exact_full_rows_are_distinct() -> TestResult {
    let folds = FoldState::new();
    for (text, expected) in [
        ("", vec![""]),
        ("abcd", vec!["abcd"]),
        ("abcd\n", vec!["abcd", ""]),
        ("abcd\r\n", vec!["abcd", ""]),
        ("a\rb\r\nc\n", vec!["a", "b", "c", ""]),
        ("\n\n", vec!["", "", ""]),
    ] {
        let document = Document::new(text);
        let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(4, 4))?;
        assert_eq!(fragments(&map)?, expected, "{text:?}");
    }
    Ok(())
}

#[test]
fn tabs_remeasure_at_continuation_origins_and_oversized_atoms_progress() -> TestResult {
    let folds = FoldState::new();
    let document = Document::new("abc\t\tX");
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(4, 4))?;
    assert_eq!(fragments(&map)?, ["abc\t", "\t", "X"]);
    let second = map.row(ScreenRow(1))?;
    let tab = second.clusters().first().ok_or("missing tab")?;
    assert_eq!(tab.column(), CellColumn(0));
    assert_eq!(tab.width(), 4);
    assert_eq!(tab.kind(), ClusterKind::Tab);

    let narrow = Document::new("a\t漢z");
    let map = CellRowMap::prepare(&narrow, &folds, CellWrapParameters::new(1, 4))?;
    assert_eq!(fragments(&map)?, ["a", "\t", "漢", "z"]);
    assert_eq!(map.row(ScreenRow(1))?.width(), 4);
    assert_eq!(map.row(ScreenRow(2))?.width(), 2);
    Ok(())
}

#[test]
fn zero_width_layout_is_empty_but_still_refuses_invalid_source_positions() -> TestResult {
    let document = Document::new("e\u{301}\n");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(0, 0))?;
    assert_eq!(map.total_rows(), 0);
    assert_eq!(map.parameters().tab_width(), 1);
    assert_eq!(map.place(Position::zero(), Affinity::Downstream)?, None);
    assert_eq!(map.place(Position::new(1, 0), Affinity::Downstream)?, None);
    assert!(matches!(
        map.place(Position::new(0, 1), Affinity::Downstream),
        Err(CellLayoutError::NonGraphemeBoundary { line: 0, column: 1 })
    ));
    assert!(matches!(
        map.place(Position::new(2, 0), Affinity::Downstream),
        Err(CellLayoutError::InvalidPosition { line: 2, column: 0 })
    ));
    assert!(matches!(
        map.row(ScreenRow(0)),
        Err(CellLayoutError::InvalidRow { .. })
    ));
    Ok(())
}

#[test]
fn soft_boundary_affinity_and_trailing_space_hits_are_explicit() -> TestResult {
    let document = Document::new("ab  cd");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(4, 4))?;
    let before = map
        .place(Position::new(0, 4), Affinity::Upstream)?
        .ok_or("unplaced")?;
    let after = map
        .place(Position::new(0, 4), Affinity::Downstream)?
        .ok_or("unplaced")?;
    assert_eq!((before.row, before.column), (ScreenRow(0), CellColumn(4)));
    assert_eq!((after.row, after.column), (ScreenRow(1), CellColumn(0)));
    let hit = map.position_at(ScreenRow(0), CellColumn(90))?;
    assert_eq!(hit.position, Position::new(0, 4));
    assert_eq!(hit.affinity, Affinity::Upstream);
    assert_eq!(
        map.position_at(ScreenRow(0), CellColumn(3))?.position,
        Position::new(0, 3)
    );
    Ok(())
}

#[test]
fn wide_and_tab_continuations_hit_atom_start_not_scalar_interiors() -> TestResult {
    let document = Document::new("e\u{301}漢\t👨‍👩‍👧‍👦");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(20, 4))?;
    for (cell, scalar) in [(0, 0), (1, 2), (2, 2), (3, 3), (4, 4), (5, 4), (6, 11)] {
        assert_eq!(
            map.position_at(ScreenRow(0), CellColumn(cell))?
                .position
                .column,
            scalar
        );
    }
    for column in [1, 5, 6, 7, 8, 9, 10] {
        assert!(matches!(
            map.place(Position::new(0, column), Affinity::Downstream),
            Err(CellLayoutError::NonGraphemeBoundary { .. })
        ));
    }
    assert!(matches!(
        map.place(Position::new(0, 12), Affinity::Downstream),
        Err(CellLayoutError::InvalidPosition { .. })
    ));
    Ok(())
}

#[test]
fn invisible_runs_are_retained_and_hit_their_canonical_trailing_boundary() -> TestResult {
    let document = Document::new("\u{200b}\u{200b}a\u{1b}\u{200b}");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(1, 4))?;
    assert_eq!(map.total_rows(), 1);
    assert_eq!(map.row(ScreenRow(0))?.width(), 1);
    assert_eq!(fragments(&map)?.concat(), document.text());
    assert_eq!(
        map.position_at(ScreenRow(0), CellColumn(0))?
            .position
            .column,
        2
    );
    assert_eq!(
        map.position_at(ScreenRow(0), CellColumn(1))?
            .position
            .column,
        5
    );
    assert_eq!(
        map.place(Position::zero(), Affinity::Downstream)?
            .ok_or("unplaced")?
            .column,
        CellColumn(0)
    );
    assert_eq!(
        map.row(ScreenRow(0))?
            .clusters()
            .iter()
            .filter(|c| c.kind() == ClusterKind::Invisible)
            .count(),
        4
    );
    Ok(())
}

#[test]
fn source_spans_cover_every_grapheme_for_all_fixture_widths() -> TestResult {
    let samples = [
        "",
        "abcdef",
        "  a   b  ",
        "a\t\tb",
        "漢字",
        "e\u{301}é",
        "🇦🇺👍🏽👨‍👩‍👧‍👦",
        "\u{200b}x\u{200b}",
    ];
    let folds = FoldState::new();
    for text in samples {
        let document = Document::new(text);
        for width in [1, 2, 4, 8, 10, 80] {
            let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(width, 4))?;
            assert_eq!(fragments(&map)?.concat(), text);
            let clusters: Vec<_> = map
                .rows()
                .iter()
                .flat_map(super::CellRow::clusters)
                .collect();
            assert_eq!(clusters.len(), text.graphemes(true).count());
            let mut scalar = 0;
            let mut byte = 0;
            for cluster in clusters {
                assert_eq!(cluster.scalar_range().start, scalar);
                assert_eq!(cluster.byte_range().start, byte);
                scalar = cluster.scalar_range().end;
                byte = cluster.byte_range().end;
            }
            assert_eq!((scalar, byte), (text.chars().count(), text.len()));
            for (index, row) in map.rows().iter().enumerate() {
                for cluster in row.clusters().iter().filter(|c| c.width() > 0) {
                    let position = Position::new(0, cluster.scalar_range().start);
                    let placed = map
                        .place(position, Affinity::Downstream)?
                        .ok_or("unplaced boundary")?;
                    assert_eq!(placed.row, ScreenRow(index));
                    assert_eq!(placed.column, cluster.column());
                    let hit = map.position_at(placed.row, placed.column)?;
                    assert_eq!(hit.position, position);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn enormous_tabs_fail_unwrapped_overflow_but_wrap_without_false_overflow() -> TestResult {
    assert!(matches!(
        CellLine::measure("\t\t", usize::MAX),
        Err(CellLayoutError::CellOverflow { .. })
    ));
    let document = Document::new("\t\t");
    let folds = FoldState::new();
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(2, usize::MAX))?;
    assert_eq!(map.total_rows(), 2);
    assert_eq!(map.row(ScreenRow(1))?.width(), usize::MAX);
    assert_eq!(
        map.position_at(ScreenRow(1), CellColumn(usize::MAX - 1))?
            .position
            .column,
        1
    );
    Ok(())
}

#[test]
fn standalone_measurement_matches_existing_terminal_semantics() -> TestResult {
    let measured = CellLine::measure("a漢\te\u{301}\u{1b}", 4)?;
    assert_eq!(measured.width(), 5);
    assert_eq!(measured.scalar_count(), 6);
    assert_eq!(measured.byte_count(), "a漢\te\u{301}\u{1b}".len());
    assert_eq!(
        measured
            .clusters()
            .iter()
            .map(super::CellCluster::width)
            .collect::<Vec<_>>(),
        [1, 2, 1, 1, 0]
    );
    assert!(matches!(
        CellLine::measure("a\r\nb", 4),
        Err(CellLayoutError::LineEnding { byte: 1 })
    ));
    assert_eq!(CellLine::measure("\t", 0)?.width(), 1);
    Ok(())
}

#[test]
fn divergent_clones_with_equal_identity_and_revision_do_not_share_geometry() -> TestResult {
    let original = Document::new("x");
    let mut first = original.clone();
    let mut second = original;
    first.insert(Position::zero(), "漢")?;
    second.insert(Position::zero(), "a")?;
    assert_eq!(
        (first.id(), first.revision()),
        (second.id(), second.revision())
    );
    let folds = FoldState::new();
    let first_rows = CellRowMap::prepare(&first, &folds, CellWrapParameters::new(2, 4))?;
    let second_rows = CellRowMap::prepare(&second, &folds, CellWrapParameters::new(2, 4))?;
    assert_eq!(fragments(&first_rows)?, ["漢", "x"]);
    assert_eq!(fragments(&second_rows)?, ["ax"]);
    Ok(())
}

#[test]
fn dropping_borrows_allows_live_repreparation_after_source_and_fold_changes() -> TestResult {
    let mut document = Document::new("a");
    let mut folds = FoldState::new();
    {
        let rows = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(2, 4))?;
        assert_eq!(fragments(&rows)?, ["a"]);
    }
    document.insert(Position::zero(), "bc")?;
    folds.unfold_all();
    let rows = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(2, 4))?;
    assert_eq!(fragments(&rows)?, ["bc", "a"]);
    assert!(std::ptr::eq(rows.folds(), &raw const folds));
    Ok(())
}

#[test]
fn nested_folds_project_each_actual_visible_line_once() -> TestResult {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("fn outer() {\n    if ready {\n        run();\n    }\n}\nend");
    editor.set_language(Language::Rust);
    assert!(editor.fold_at(1));
    assert!(editor.fold_at(0));
    let state = editor.state();
    let map = CellRowMap::prepare(
        &state.document,
        &state.fold_state,
        CellWrapParameters::new(80, 4),
    )?;
    let expected: Vec<_> = (0..state.document.line_count())
        .filter(|&line| !state.fold_state.is_line_hidden(line))
        .collect();
    assert_eq!(
        map.rows()
            .iter()
            .map(super::CellRow::document_line)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(map.place(Position::new(2, 0), Affinity::Downstream)?, None);
    assert!(
        map.place(Position::new(2, 80), Affinity::Downstream)
            .is_err()
    );
    Ok(())
}

#[test]
fn zero_width_boundaries_are_measured_on_demand_once_per_borrow() -> TestResult {
    let mut document = Document::new("e\u{301}👨‍👩‍👧‍👦\nuntouched");
    let folds = FoldState::new();
    {
        let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(0, 4))?;
        assert!(
            map.unplaced_boundaries
                .iter()
                .all(|cell| cell.get().is_none())
        );
        assert!(matches!(
            map.place(Position::new(0, 1), Affinity::Downstream),
            Err(CellLayoutError::NonGraphemeBoundary { .. })
        ));
        let cell = map
            .unplaced_boundaries
            .first()
            .ok_or("missing line index")?;
        let measured = cell
            .get()
            .ok_or("demand did not initialize boundaries")?
            .as_ref()
            .map_err(Clone::clone)?;
        assert_eq!(measured, &[0, 2, 9]);
        for column in [0, 2, 9] {
            assert_eq!(
                map.place(Position::new(0, column), Affinity::Upstream)?,
                None
            );
        }
        for column in [1, 3, 4, 5, 6, 7, 8] {
            assert!(matches!(
                map.place(Position::new(0, column), Affinity::Downstream),
                Err(CellLayoutError::NonGraphemeBoundary { .. })
            ));
        }
        assert!(matches!(
            map.place(Position::new(0, 10), Affinity::Downstream),
            Err(CellLayoutError::InvalidPosition { .. })
        ));
        let repeated = cell
            .get()
            .ok_or("measured boundaries disappeared")?
            .as_ref()
            .map_err(Clone::clone)?;
        assert!(std::ptr::eq(measured, repeated));
        assert!(
            map.unplaced_boundaries
                .get(1)
                .ok_or("missing second line")?
                .get()
                .is_none()
        );
    }
    document.insert(Position::zero(), "x")?;
    let map = CellRowMap::prepare(&document, &folds, CellWrapParameters::new(0, 4))?;
    assert!(
        map.unplaced_boundaries
            .iter()
            .all(|cell| cell.get().is_none())
    );
    assert_eq!(map.place(Position::new(0, 3), Affinity::Downstream)?, None);
    assert!(matches!(
        map.place(Position::new(0, 2), Affinity::Downstream),
        Err(CellLayoutError::NonGraphemeBoundary { .. })
    ));
    Ok(())
}

#[test]
fn hidden_line_boundary_demand_preserves_errors_and_skips_other_hidden_lines() -> TestResult {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("fn outer() {\n    e\u{301};\n    untouched();\n}\nend");
    editor.set_language(Language::Rust);
    assert!(editor.fold_at(0));
    let state = editor.state();
    assert!(state.fold_state.is_line_hidden(1));
    assert!(state.fold_state.is_line_hidden(2));
    let map = CellRowMap::prepare(
        &state.document,
        &state.fold_state,
        CellWrapParameters::new(80, 4),
    )?;
    assert!(
        map.unplaced_boundaries
            .iter()
            .all(|cell| cell.get().is_none())
    );
    for column in [4, 6, 7, 4] {
        assert_eq!(
            map.place(Position::new(1, column), Affinity::Downstream)?,
            None
        );
    }
    assert!(matches!(
        map.place(Position::new(1, 5), Affinity::Downstream),
        Err(CellLayoutError::NonGraphemeBoundary { .. })
    ));
    assert!(matches!(
        map.place(Position::new(1, 8), Affinity::Downstream),
        Err(CellLayoutError::InvalidPosition { .. })
    ));
    assert!(
        map.unplaced_boundaries
            .get(1)
            .ok_or("missing demanded line")?
            .get()
            .is_some()
    );
    assert!(
        map.unplaced_boundaries
            .get(2)
            .ok_or("missing untouched line")?
            .get()
            .is_none()
    );
    Ok(())
}

#[test]
fn shared_grapheme_descriptors_preserve_fixed_control_unicode_and_tab_spans() -> TestResult {
    let measured: Vec<_> = super::clusters::measure_graphemes("e\u{301}界\t\r\n👩‍🔬").collect();
    let expected = [
        (0..2, 0..3, ClusterKind::Glyph, 1),
        (2..3, 3..6, ClusterKind::Glyph, 2),
        (3..4, 6..7, ClusterKind::Tab, 1),
        (4..6, 7..9, ClusterKind::Invisible, 0),
        (6..9, 9..20, ClusterKind::Glyph, 2),
    ];
    assert_eq!(measured.len(), expected.len());
    for (value, (scalars, bytes, kind, width)) in measured.iter().zip(expected) {
        assert_eq!(value.scalar_range(), scalars);
        assert_eq!(value.byte_range(), bytes);
        assert_eq!(value.kind(), kind);
        assert_eq!(value.width_at(CellColumn(3), 4), width);
    }
    let tab = measured.get(2).ok_or("missing tab descriptor")?;
    assert_eq!(tab.width_at(CellColumn(0), 4), 4);
    assert_eq!(tab.width_at(CellColumn(3), 0), 1);
    assert!(matches!(
        CellLine::measure("x\r\n", 4),
        Err(CellLayoutError::LineEnding { byte: 1 })
    ));
    assert!(matches!(
        CellLine::measure("\t\t", usize::MAX),
        Err(CellLayoutError::CellOverflow { line: 0 })
    ));
    Ok(())
}
