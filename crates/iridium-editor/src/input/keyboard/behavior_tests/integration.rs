//! The config reaching a real [`Editor`], and `newline` flags deciding
//! which brackets expand.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

// ========== Editor-level integration (config threading) ==========

#[test]
fn editor_tab_uses_configured_width_and_spaces() {
    let mut editor = Editor::new(EditorConfig {
        tab_width: 2,
        ..EditorConfig::default()
    });
    editor.set_content("x");
    editor.set_cursor(Position::new(0, 1));

    editor.handle_key(&TAB);

    // Column 1, width 2: one space to the next tab stop.
    assert_eq!(editor.content(), "x ");
    assert_eq!(editor.cursor(), Position::new(0, 2));
}

#[test]
fn editor_enter_auto_indents_and_undo_restores() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("    foo");
    editor.set_cursor(Position::new(0, 7));

    editor.handle_key(&ENTER);
    assert_eq!(editor.content(), "    foo\n    ");
    assert_eq!(editor.cursor(), Position::new(1, 4));

    assert!(editor.undo());
    assert_eq!(editor.content(), "    foo");
    assert_eq!(editor.cursor(), Position::new(0, 7));
}

#[test]
fn editor_selection_indent_and_undo_restores_selection() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("aa\nbb");
    editor.set_selection(Position::new(0, 1), Position::new(1, 1));

    editor.handle_key(&TAB);
    assert_eq!(editor.content(), "    aa\n    bb");

    assert!(editor.undo());
    assert_eq!(editor.content(), "aa\nbb");
    let selection = editor.state().cursor.primary;
    assert_eq!(selection.anchor, Position::new(0, 1));
    assert_eq!(selection.head, Position::new(1, 1));
}

#[test]
fn editor_auto_pairs_disabled_via_config() {
    let mut editor = Editor::new(EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    });
    editor.set_content("");

    editor.handle_key(&ch('['));

    assert_eq!(editor.content(), "[");
    assert_eq!(editor.cursor(), Position::new(0, 1));
}

/// Regression test (Norn review, 2026-07-12): when one cursor's skip-over
/// (an empty edit) and another cursor's insertion land at the same position,
/// the empty edit must sort first so the skip caret does not absorb the
/// insertion's byte delta. Before the tie-break, the outcome depended on
/// which cursor was primary: with the inserting cursor primary, both carets
/// resolved to the same column and one cursor was silently merged away.
#[test]
fn skip_over_and_insertion_at_same_position_stay_distinct() {
    // Document "()", cursors between the parens (skips) and at the end
    // (inserts). Typing ')' must give "())" with distinct carets at
    // columns 2 and 3 regardless of which cursor is primary.
    for primary_first in [true, false] {
        let positions: &[(usize, usize)] = if primary_first {
            &[(0, 2), (0, 1)]
        } else {
            &[(0, 1), (0, 2)]
        };

        let mut doc = Document::new("()");
        let mut cursor = cursors_at(positions);
        let mut handler = KeyboardHandler::new();
        press(
            &mut handler,
            &ch(')'),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(doc.text(), "())", "primary_first={primary_first}");
        assert_eq!(
            cursor.cursor_count(),
            2,
            "cursors must not merge (primary_first={primary_first})"
        );
        let mut columns: Vec<usize> = cursor.all_selections().map(|s| s.head.column).collect();
        columns.sort_unstable();
        assert_eq!(columns, vec![2, 3], "primary_first={primary_first}");
    }
}

// ========== Enter expansion follows the language's `newline` flags ==========

/// `BRACKET_MASK` names positions in `PAIRS` by hand, so a reordering of that
/// array would silently start expanding quote blocks. This is the guard.
#[test]
fn the_bracket_mask_covers_exactly_the_three_brackets() {
    let masked: Vec<char> = super::behaviors::PAIRS
        .iter()
        .enumerate()
        .filter(|(index, _)| super::behaviors::BRACKET_MASK & (1_u8 << index) != 0)
        .map(|(_, &(open, _))| open)
        .collect();
    assert_eq!(masked, vec!['(', '[', '{']);
}

/// A git commit message declares brackets and sets `newline` on none of them:
/// Enter between `(` and `)` must not grow a three-line block out of a
/// parenthesis in prose.
#[test]
fn a_commit_message_does_not_expand_a_bracket_block() {
    let mut doc = doc_in(Language::GitCommit, "fix ");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    assert_eq!(doc.text(), "fix ()", "gitcommit does auto-close `(`");

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    assert_eq!(
        doc.text(),
        "fix (\n)",
        "gitcommit sets `newline` on no row: a plain line break, not a block"
    );
}

/// JSON declares `{` and `[` with `newline = true` and `(` without, so Enter
/// expands the first two and not the third.
#[test]
fn json_expands_only_the_brackets_it_marks_newline() {
    for (typed, closer, expanded) in [('{', '}', true), ('[', ']', true), ('(', ')', false)] {
        let mut doc = doc_in(Language::Json, "");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &ch(typed),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );
        press(
            &mut handler,
            &ENTER,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        let want = if expanded {
            format!("{typed}\n    \n{closer}")
        } else {
            format!("{typed}\n{closer}")
        };
        assert_eq!(doc.text(), want, "json, {typed:?}");
    }
}

/// Rust marks all three `newline = true`, so nothing about the common case
/// moved — the slice subtracts, it does not switch expansion off.
#[test]
fn rust_still_expands_every_bracket_block() {
    for (typed, closer) in [('{', '}'), ('[', ']'), ('(', ')')] {
        let mut doc = doc_in(Language::Rust, "");
        let mut cursor = cursors_at(&[(0, 0)]);
        let mut handler = KeyboardHandler::new();

        press(
            &mut handler,
            &ch(typed),
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );
        press(
            &mut handler,
            &ENTER,
            &mut doc,
            &mut cursor,
            &EditorConfig::default(),
        );

        assert_eq!(
            doc.text(),
            format!("{typed}\n    \n{closer}"),
            "rust {typed:?}"
        );
    }
}

/// A language that has not declared brackets keeps every expansion, the same
/// silence rule the closing set uses.
#[test]
fn a_language_that_declares_no_brackets_still_expands_blocks() {
    let mut doc = Document::new("");
    doc.set_language(Some("awl".to_owned()));
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('{'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );
    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "{\n    \n}");
}
