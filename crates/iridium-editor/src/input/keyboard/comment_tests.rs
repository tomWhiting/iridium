//! Document-state-level tests for comment toggling: line-comment groups
//! (Ctrl+/), block comments (Shift+Alt+A), the language comment-syntax
//! table, and the configuration fallback token.
//!
//! Every test applies the produced command to a real document and asserts
//! both the resulting text and every cursor position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tests::{cursors_at, cursors_with, heads};
use super::*;

/// Ctrl+/ — toggle line comment.
const LINE_TOGGLE: KeyEvent = KeyEvent::new(KeyCode::Char('/'), Modifiers::ctrl());

/// Shift+Alt+A — toggle block comment.
const BLOCK_TOGGLE: KeyEvent = KeyEvent::new(
    KeyCode::Char('a'),
    Modifiers {
        shift: true,
        ctrl: false,
        alt: true,
        meta: false,
        alt_graph: false,
    },
);

/// All cursor selections in `all_selections` order, as
/// `((anchor.line, anchor.column), (head.line, head.column))` pairs.
fn selections(cursor: &CursorState) -> Vec<((usize, usize), (usize, usize))> {
    cursor
        .all_selections()
        .map(|sel| {
            (
                (sel.anchor.line, sel.anchor.column),
                (sel.head.line, sel.head.column),
            )
        })
        .collect()
}

/// Handles a key event with the given config and applies the resulting
/// command (if any), returning the command for undo assertions.
fn press(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &mut Document,
    cursor: &mut CursorState,
    config: &EditorConfig,
) -> Option<Command> {
    let result = handler.handle_key(event, document, cursor, &UndoTree::new(), config);
    match result {
        KeyResult::Command(cmd) => {
            cmd.apply(document, cursor).expect("command must apply");
            Some(cmd)
        },
        KeyResult::Handled => None,
        other => panic!("expected a command or Handled, got {other:?}"),
    }
}

/// Asserts that the key is acknowledged without producing a command.
fn press_noop(
    handler: &mut KeyboardHandler,
    event: &KeyEvent,
    document: &Document,
    cursor: &CursorState,
    config: &EditorConfig,
) {
    let result = handler.handle_key(event, document, cursor, &UndoTree::new(), config);
    assert_eq!(result, KeyResult::Handled);
}

/// Config whose only comment syntax is the fallback line token.
fn token_config(token: &str) -> EditorConfig {
    EditorConfig {
        line_comment_token: Some(token.to_string()),
        ..EditorConfig::default()
    }
}

// ========== Line toggle via the config fallback token ==========
//
// These tests exercise the group semantics without any language, so they
// hold with and without the `syntax` feature.

#[test]
fn line_toggle_comments_caret_line() {
    let mut doc = Document::new("fn main() {}");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "// fn main() {}");
    assert_eq!(heads(&cursor), vec![(0, 7)]);
}

#[test]
fn line_toggle_uncomments_commented_line() {
    let mut doc = Document::new("// foo");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn uncomment_tolerates_token_without_space() {
    let mut doc = Document::new("\t//foo");
    let mut cursor = cursors_at(&[(0, 6)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "\tfoo");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn mixed_group_comments_every_line_including_commented_ones() {
    let mut doc = Document::new("a\n// b\nc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 1))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    // VS Code semantics: a mixed group is commented, doubling the token on
    // the already-commented line.
    assert_eq!(doc.text(), "// a\n// // b\n// c");
    assert_eq!(selections(&cursor), vec![((0, 3), (2, 4))]);
}

#[test]
fn fully_commented_group_uncomments_every_line() {
    let mut doc = Document::new("// a\n// b");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 4))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "a\nb");
    assert_eq!(selections(&cursor), vec![((0, 0), (1, 1))]);
}

#[test]
fn comment_inserts_at_group_minimum_indent() {
    let mut doc = Document::new("    aa\n  bb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 4))]);
    let mut handler = KeyboardHandler::new();
    let config = token_config("//");

    press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);

    // Minimum indent of the group is 2 columns; both lines get the token
    // there, keeping the block visually aligned.
    assert_eq!(doc.text(), "  //   aa\n  // bb");

    // Toggling again restores the original text exactly.
    press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), "    aa\n  bb");
}

#[test]
fn blank_lines_are_skipped_and_do_not_block_uncomment() {
    let mut doc = Document::new("a\n\nb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 1))]);
    let mut handler = KeyboardHandler::new();
    let config = token_config("//");

    press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), "// a\n\n// b");

    // The blank line carries no token, but the group still counts as fully
    // commented: toggling again uncomments.
    press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
    assert_eq!(doc.text(), "a\n\nb");
}

#[test]
fn all_blank_group_is_a_noop() {
    let doc = Document::new("\n\n");
    let cursor = cursors_at(&[(1, 0)]);
    let mut handler = KeyboardHandler::new();

    press_noop(
        &mut handler,
        &LINE_TOGGLE,
        &doc,
        &cursor,
        &token_config("//"),
    );
    assert_eq!(doc.text(), "\n\n");
}

#[test]
fn no_language_and_no_fallback_token_is_a_noop() {
    let doc = Document::new("plain text");
    let cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press_noop(
        &mut handler,
        &LINE_TOGGLE,
        &doc,
        &cursor,
        &EditorConfig::default(),
    );
    press_noop(
        &mut handler,
        &BLOCK_TOGGLE,
        &doc,
        &cursor,
        &EditorConfig::default(),
    );
    assert_eq!(doc.text(), "plain text");
}

#[test]
fn whitespace_only_fallback_token_is_rejected() {
    let doc = Document::new("plain text");
    let cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press_noop(
        &mut handler,
        &LINE_TOGGLE,
        &doc,
        &cursor,
        &token_config("  "),
    );
    assert_eq!(doc.text(), "plain text");
}

#[test]
fn selection_ending_at_column_zero_excludes_its_last_line() {
    let mut doc = Document::new("a\nb\nc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 0))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    // The trailing caret sits at (2, 0); line 2 is not part of the block.
    assert_eq!(doc.text(), "// a\n// b\nc");
}

#[test]
fn multi_cursor_groups_toggle_independently() {
    let mut doc = Document::new("aa\n// bb");
    let mut cursor = cursors_at(&[(0, 0), (1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    // The first cursor's group is uncommented text (comment it); the second
    // cursor's group is commented (uncomment it) — in one undoable command.
    assert_eq!(doc.text(), "// aa\nbb");
    assert_eq!(heads(&cursor), vec![(0, 3), (1, 0)]);
}

#[test]
fn cursors_sharing_a_line_toggle_it_once() {
    let mut doc = Document::new("foo");
    let mut cursor = cursors_at(&[(0, 0), (0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "// foo");
    assert_eq!(heads(&cursor), vec![(0, 3), (0, 6)]);
}

// ========== Overlapping line groups (primary-order independence) ==========
//
// Reviewer scenario: a caret at (0, 0) and a selection (0, 2)..(1, 1) do not
// overlap as text ranges (so `CursorState` keeps them separate), but their
// touched-line blocks share line 0. They must merge into one component whose
// toggle decision — mixed, therefore comment everything — is the same
// regardless of which cursor is primary.

#[test]
fn overlapping_groups_merge_with_caret_primary() {
    let mut doc = Document::new("// a\nb");
    let mut cursor = cursors_with(&[
        Selection::collapsed(Position::new(0, 0)),
        Selection::new(Position::new(0, 2), Position::new(1, 1)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    // Merged component {0, 1} is mixed, so both lines are commented — line 0
    // never gets uncommented first by the caret's group.
    assert_eq!(doc.text(), "// // a\n// b");
    assert_eq!(
        selections(&cursor),
        vec![((0, 3), (0, 3)), ((0, 5), (1, 4))]
    );
}

#[test]
fn overlapping_groups_merge_with_selection_primary() {
    let mut doc = Document::new("// a\nb");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 2), Position::new(1, 1)),
        Selection::collapsed(Position::new(0, 0)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    // Same document and cursors as the caret-primary case; the text must be
    // identical — the outcome may not depend on which cursor is primary.
    assert_eq!(doc.text(), "// // a\n// b");
    assert_eq!(
        selections(&cursor),
        vec![((0, 5), (1, 4)), ((0, 3), (0, 3))]
    );
}

#[test]
fn overlapping_fully_commented_groups_uncomment_once_in_both_orders() {
    for primary_first in [true, false] {
        let caret = Selection::collapsed(Position::new(0, 0));
        let span = Selection::new(Position::new(0, 2), Position::new(1, 1));
        let order = if primary_first {
            [caret, span]
        } else {
            [span, caret]
        };

        let mut doc = Document::new("// a\n// b");
        let mut cursor = cursors_with(&order);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &token_config("//"),
        );

        // The merged component is fully commented: each line is uncommented
        // exactly once, never double-stripped or re-commented.
        assert_eq!(doc.text(), "a\nb", "primary_first = {primary_first}");
    }
}

#[test]
fn chained_overlaps_merge_into_one_component() {
    // Three cursors whose text ranges are disjoint (so `CursorState` keeps
    // all three) but whose line blocks chain pairwise: {0, 1}, {1, 2}, and
    // {2}. They form a single component; line 1 ("bb") is uncommented, so
    // the whole chain gets commented — including the already-commented ends.
    let mut doc = Document::new("// a\nbb\n// c");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 2), Position::new(1, 1)),
        Selection::new(Position::new(1, 2), Position::new(2, 1)),
        Selection::collapsed(Position::new(2, 4)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "// // a\n// bb\n// // c");
}

#[test]
fn adjacent_non_overlapping_groups_stay_independent() {
    // Blocks on consecutive lines share no line, so they must NOT merge:
    // line 0 is commented (uncomment), line 1 is not (comment) — exactly the
    // independent per-cursor behavior.
    let mut doc = Document::new("// a\nb");
    let mut cursor = cursors_at(&[(0, 0), (1, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "a\n// b");
    assert_eq!(heads(&cursor), vec![(0, 0), (1, 3)]);
}

#[test]
fn merged_component_uses_component_minimum_indent() {
    // Two overlapping blocks with different indentation: the token column is
    // the minimum over the merged component, not per original cursor.
    let mut doc = Document::new("    aa\n  bb");
    let mut cursor = cursors_with(&[
        Selection::collapsed(Position::new(0, 0)),
        Selection::new(Position::new(0, 4), Position::new(1, 4)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &LINE_TOGGLE,
        &mut doc,
        &mut cursor,
        &token_config("//"),
    );

    assert_eq!(doc.text(), "  //   aa\n  // bb");
}

#[test]
fn undo_restores_text_and_all_cursors_exactly() {
    use crate::editor::Editor;

    let mut editor = Editor::new(token_config("//"));
    editor.set_content("aa\nbb");
    editor.set_cursor(Position::new(0, 1));
    editor
        .state_mut()
        .cursor
        .add_cursor(Selection::collapsed(Position::new(1, 1)));

    editor.handle_key(&LINE_TOGGLE);
    assert_eq!(editor.content(), "// aa\n// bb");
    assert_eq!(
        heads(&editor.state().cursor),
        vec![(0, 4), (1, 4)],
        "both carets must shift with their line's token"
    );

    // A single undo must restore both the text and the exact multi-cursor
    // state.
    assert!(editor.undo());
    assert_eq!(editor.content(), "aa\nbb");
    assert_eq!(heads(&editor.state().cursor), vec![(0, 1), (1, 1)]);
}

// ========== Language-table resolution ==========
//
// Deliberately **not** gated on the `syntax` feature. Which token comments a
// language is pure data — a match on an enum both feature configurations
// have — and nothing here parses anything. Gating these was what let the
// stub build answer "this language has no comment syntax" for every language
// while a doc comment two files away asserted the path was unreachable.

mod language_table {
    use super::*;
    // The crate's own re-export, which resolves to the real enum or the stub
    // depending on the feature — so these tests name whichever one this build
    // actually has, which is the point of running them in both.
    use crate::Language;
    use crate::editor::Editor;

    /// A document tagged with a language identifier.
    fn doc_with_language(text: &str, id: &str) -> Document {
        let mut document = Document::new(text);
        document.set_language(Some(id.to_string()));
        document
    }

    #[test]
    fn every_language_maps_to_its_line_toggle_behavior() {
        // (language id, source line, line-toggled result). JSON is absent:
        // it has no comment syntax and is covered by `json_is_a_noop`.
        let cases = [
            ("rust", "let x = 1;", "// let x = 1;"),
            ("python", "x = 1", "# x = 1"),
            ("typescript", "const x = 1;", "// const x = 1;"),
            ("javascript", "const x = 1;", "// const x = 1;"),
            ("tsx", "const x = 1;", "// const x = 1;"),
            ("go", "x := 1", "// x := 1"),
            ("yaml", "key: value", "# key: value"),
            ("markdown", "hello", "<!-- hello -->"),
            ("css", "body { color: red; }", "/* body { color: red; } */"),
            ("bash", "echo hi", "# echo hi"),
            ("c", "int x = 1;", "// int x = 1;"),
            ("cpp", "int x = 1;", "// int x = 1;"),
        ];

        for (id, source, expected) in cases {
            let mut doc = doc_with_language(source, id);
            let mut cursor = cursors_at(&[(0, 0)]);
            let mut handler = KeyboardHandler::new();
            let config = EditorConfig::default();

            press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
            assert_eq!(doc.text(), expected, "commenting {id}");

            // Toggling again restores the source exactly.
            press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
            assert_eq!(doc.text(), source, "uncommenting {id}");
        }
    }

    /// JSON comments, and used not to.
    ///
    /// Before #66 this asserted the opposite: the hard-coded table gave JSON no
    /// comment syntax at all, so both toggles were no-ops. Its vendored
    /// manifest carries `line_comments = ["// "]`, and reading the manifest is
    /// what settles it — the strict specification has no comments, but nothing
    /// that edits `.json` in practice rejects them, and the answer now comes
    /// from a data file rather than a `match` arm.
    #[test]
    fn json_comments_with_the_token_its_manifest_gives_it() {
        let mut doc = doc_with_language("{\"a\": 1}", "json");
        let mut cursor = cursors_at(&[(0, 2)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );
        assert_eq!(doc.text(), "// {\"a\": 1}");

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );
        assert_eq!(doc.text(), "{\"a\": 1}");
    }

    /// JSON has a line token and no block pair, so the block toggle falls back
    /// to the line toggle rather than doing nothing.
    #[test]
    fn the_json_block_toggle_falls_back_to_its_line_token() {
        let mut doc = doc_with_language("{\"a\": 1}", "json");
        let mut cursor = cursors_at(&[(0, 2)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );
        assert_eq!(doc.text(), "// {\"a\": 1}");
    }

    /// An id no language claims falls back to the configured token.
    ///
    /// This used to be spelled with `"json"`, which was the only commentless
    /// language in the table. #66 gave JSON a token, so the fallback branch has
    /// to be reached the way it is actually reached in the wild: a document
    /// whose language nothing recognises.
    #[test]
    fn an_unrecognised_language_falls_back_to_config_token() {
        let mut doc = doc_with_language("value", "nonesuch");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &token_config("#"),
        );

        assert_eq!(doc.text(), "# value");
    }

    #[test]
    fn language_syntax_wins_over_config_token() {
        let mut doc = doc_with_language("let x = 1;", "rust");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &token_config("#"),
        );

        assert_eq!(doc.text(), "// let x = 1;");
    }

    #[test]
    fn css_line_toggle_wraps_and_unwraps_with_cursor_roundtrip() {
        let mut doc = doc_with_language("body { color: red; }", "css");
        let mut cursor = cursors_at(&[(0, 5)]);
        let mut handler = KeyboardHandler::new();
        let config = EditorConfig::default();

        press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "/* body { color: red; } */");
        assert_eq!(heads(&cursor), vec![(0, 8)]);

        press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "body { color: red; }");
        assert_eq!(heads(&cursor), vec![(0, 5)]);
    }

    /// Regression test (Norn review, 2026-07-13): endpoints at the end of a
    /// line's content must stay with that content — inside the new comment,
    /// before the inserted closing marker — not jump past `*/`.
    #[test]
    fn css_line_toggle_keeps_eol_endpoints_before_closing_marker() {
        let mut doc = doc_with_language("a", "css");
        let mut cursor = cursors_at(&[(0, 1)]);
        let mut handler = KeyboardHandler::new();
        let config = EditorConfig::default();

        press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "/* a */");
        // Caret was after "a" (column 1); it lands after "a" inside the
        // comment (column 4), not after the closing marker (column 7).
        assert_eq!(heads(&cursor), vec![(0, 4)]);

        // A selection covering "a" keeps covering exactly "a".
        let mut doc = doc_with_language("a", "css");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 1))]);
        press(&mut handler, &LINE_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "/* a */");
        assert_eq!(cursor.primary.anchor, Position::new(0, 3));
        assert_eq!(cursor.primary.head, Position::new(0, 4));
    }

    #[test]
    fn css_uncomment_tolerates_missing_padding_spaces() {
        let mut doc = doc_with_language("/*body{}*/", "css");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &LINE_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "body{}");
    }

    // ========== Block toggle ==========

    #[test]
    fn block_toggle_wraps_selection_and_roundtrips() {
        let mut doc = doc_with_language("let foo = 1;", "rust");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 4), Position::new(0, 7))]);
        let mut handler = KeyboardHandler::new();
        let config = EditorConfig::default();

        press(&mut handler, &BLOCK_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "let /* foo */ = 1;");
        // The selection covers the whole wrapped text, so the next toggle
        // sees an exactly-wrapped selection and unwraps it.
        assert_eq!(selections(&cursor), vec![((0, 4), (0, 13))]);

        press(&mut handler, &BLOCK_TOGGLE, &mut doc, &mut cursor, &config);
        assert_eq!(doc.text(), "let foo = 1;");
        assert_eq!(selections(&cursor), vec![((0, 4), (0, 7))]);
    }

    #[test]
    fn block_toggle_preserves_backward_selection_orientation() {
        let mut doc = doc_with_language("let foo = 1;", "rust");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 7), Position::new(0, 4))]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "let /* foo */ = 1;");
        assert_eq!(selections(&cursor), vec![((0, 13), (0, 4))]);
    }

    #[test]
    fn block_toggle_collapsed_cursor_inserts_empty_pair_with_caret_inside() {
        let mut doc = doc_with_language("ab", "rust");
        let mut cursor = cursors_at(&[(0, 1)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "a/*  */b");
        assert_eq!(heads(&cursor), vec![(0, 4)]);
    }

    #[test]
    fn block_toggle_unwrap_tolerates_missing_padding() {
        let mut doc = doc_with_language("/*foo*/", "rust");
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 7))]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "foo");
        assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);
    }

    #[test]
    fn block_toggle_unwrap_preserves_surrounding_whitespace() {
        let mut doc = doc_with_language("x /* foo */ y", "rust");
        // The selection includes one space on each side of the markers.
        let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 12))]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "x foo y");
        assert_eq!(selections(&cursor), vec![((0, 1), (0, 6))]);
    }

    #[test]
    fn block_toggle_multi_cursor_wraps_and_unwraps_independently() {
        let mut doc = doc_with_language("aa /* bb */\ncc", "rust");
        let mut cursor = cursors_with(&[
            Selection::new(Position::new(0, 3), Position::new(0, 11)),
            Selection::new(Position::new(1, 0), Position::new(1, 2)),
        ]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "aa bb\n/* cc */");
        assert_eq!(
            selections(&cursor),
            vec![((0, 3), (0, 5)), ((1, 0), (1, 8))]
        );
    }

    #[test]
    fn block_toggle_without_pair_falls_back_to_line_toggle() {
        let mut doc = doc_with_language("x = 1", "python");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &BLOCK_TOGGLE,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "# x = 1");
        assert_eq!(heads(&cursor), vec![(0, 2)]);
    }

    #[test]
    fn block_toggle_undo_restores_text_and_selection() {
        let mut editor = Editor::with_defaults();
        editor.set_content("let foo = 1;");
        editor.set_language(Language::Rust);
        editor.set_selection(Position::new(0, 4), Position::new(0, 7));

        editor.handle_key(&BLOCK_TOGGLE);
        assert_eq!(editor.content(), "let /* foo */ = 1;");

        assert!(editor.undo());
        assert_eq!(editor.content(), "let foo = 1;");
        assert_eq!(selections(&editor.state().cursor), vec![((0, 4), (0, 7))]);
    }

    // ========== Editor-level language wiring ==========

    #[test]
    fn editor_set_language_enables_comment_toggling() {
        let mut editor = Editor::with_defaults();
        editor.set_content("fn main() {}");
        editor.set_language(Language::Rust);
        editor.set_cursor(Position::new(0, 0));

        editor.handle_key(&LINE_TOGGLE);
        assert_eq!(editor.content(), "// fn main() {}");
    }

    /// Clearing the language has to clear it everywhere it is held.
    ///
    /// Three places hold it: the fold state, the parse tree, and an id string
    /// on the document. [`Editor::language`] reports the *first*, and comment
    /// toggling reads the *third* — so a `clear_language` that forgot the
    /// document would leave `language()` answering `None` while `Ctrl+/` went
    /// on inserting `//`. That is why this test toggles rather than only
    /// asking.
    #[test]
    fn editor_clear_language_reaches_the_id_comment_toggling_reads() {
        let mut editor = Editor::with_defaults();
        editor.set_content("fn main() {}");
        editor.set_language(Language::Rust);
        editor.clear_language();
        editor.set_cursor(Position::new(0, 0));

        assert_eq!(editor.language(), None, "the fold state kept a language");

        editor.handle_key(&LINE_TOGGLE);
        assert_eq!(
            editor.content(),
            "fn main() {}",
            "the document kept an id, so a cleared language still commented"
        );
    }

    #[test]
    fn editor_set_content_preserves_language_for_comment_toggling() {
        let mut editor = Editor::with_defaults();
        editor.set_language(Language::Python);
        editor.set_content("x = 1");
        editor.set_cursor(Position::new(0, 0));

        editor.handle_key(&LINE_TOGGLE);
        assert_eq!(editor.content(), "# x = 1");
    }
}
