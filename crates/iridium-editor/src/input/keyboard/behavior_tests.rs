//! Document-state-level tests for the configuration-driven editing
//! behaviors: Tab/indent/outdent, auto-indent on Enter (including
//! bracket-block and code-fence expansion), and auto-closing pairs.
//!
//! Every test applies the produced command to a real document and asserts
//! both the resulting text and every cursor position.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tests::{cursors_at, cursors_with, heads};
use super::*;
use crate::editor::Editor;

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

/// Config with spaces disabled (literal tab characters).
fn tabs_config() -> EditorConfig {
    EditorConfig {
        insert_spaces: false,
        ..EditorConfig::default()
    }
}

const TAB: KeyEvent = KeyEvent::new(KeyCode::Tab, Modifiers::none());
const SHIFT_TAB: KeyEvent = KeyEvent::new(KeyCode::Tab, Modifiers::shift());
const ENTER: KeyEvent = KeyEvent::new(KeyCode::Enter, Modifiers::none());
const BACKSPACE: KeyEvent = KeyEvent::new(KeyCode::Backspace, Modifiers::none());

fn ch(c: char) -> KeyEvent {
    KeyEvent::simple(KeyCode::Char(c))
}

// ========== Tab: collapsed cursors ==========

#[test]
fn tab_pads_with_spaces_to_next_tab_stop_from_odd_column() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Column 1, width 4: pad 3 spaces to reach the tab stop at column 4.
    assert_eq!(doc.text(), "a   bc");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn tab_at_tab_stop_inserts_full_width() {
    let mut doc = Document::new("abcd");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "abcd    ");
    assert_eq!(heads(&cursor), vec![(0, 8)]);
}

#[test]
fn tab_honors_configured_width() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        tab_width: 2,
        ..EditorConfig::default()
    };

    press(&mut handler, &TAB, &mut doc, &mut cursor, &config);

    // Column 1, width 2: pad 1 space to reach the tab stop at column 2.
    assert_eq!(doc.text(), "a bc");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn tab_inserts_tab_character_when_insert_spaces_disabled() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "a\tbc");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn tab_multi_cursor_same_line_pads_each_from_its_own_column() {
    let mut doc = Document::new("abcdef");
    let mut cursor = cursors_at(&[(0, 1), (0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Cursor at column 1 pads 3, cursor at (pre-edit) column 4 pads 4; the
    // second caret lands after both insertions.
    assert_eq!(doc.text(), "a   bcd    ef");
    assert_eq!(heads(&cursor), vec![(0, 4), (0, 11)]);
}

#[test]
fn tab_multi_cursor_inserts_tab_characters() {
    let mut doc = Document::new("ab\ncd");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "a\tb\nc\td");
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);
}

// ========== Tab: selection indent ==========

#[test]
fn tab_with_selection_indents_every_touched_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(2, 1))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\n    cc");
    // The selection keeps covering the same text.
    assert_eq!(selections(&cursor), vec![((0, 5), (2, 5))]);
}

#[test]
fn tab_with_selection_indents_with_tab_characters() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(1, 2))]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &TAB, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "\taa\n\tbb");
    assert_eq!(selections(&cursor), vec![((0, 1), (1, 3))]);
}

#[test]
fn tab_selection_ending_at_column_zero_does_not_indent_that_line() {
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 0))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\ncc");
    assert_eq!(selections(&cursor), vec![((0, 4), (2, 0))]);
}

#[test]
fn tab_multi_cursor_selections_indent_all_touched_lines_once() {
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(1, 1)),
        Selection::new(Position::new(3, 0), Position::new(3, 2)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    aa\n    bb\ncc\n    dd");
    assert_eq!(
        selections(&cursor),
        vec![((0, 4), (1, 5)), ((3, 4), (3, 6))]
    );
}

#[test]
fn tab_mixed_selection_and_collapsed_cursor_indents_both_lines() {
    let mut doc = Document::new("aa\nbb");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 2)),
        Selection::collapsed(Position::new(1, 1)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Any non-collapsed selection switches Tab to line-indent mode for every
    // touched line, including lines under collapsed cursors.
    assert_eq!(doc.text(), "    aa\n    bb");
    assert_eq!(
        selections(&cursor),
        vec![((0, 4), (0, 6)), ((1, 5), (1, 5))]
    );
}

#[test]
fn undo_selection_indent_restores_text_and_selection() {
    let mut doc = Document::new("aa\nbb");
    let original = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(1, 1))]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("indent must produce a command");

    assert_eq!(doc.text(), "    aa\n    bb");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(cursor, original);
}

// ========== Shift+Tab: outdent ==========

#[test]
fn shift_tab_removes_one_level_of_spaces() {
    let mut doc = Document::new("        foo");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Only one level (4 spaces) is removed; the cursor shifts by the amount
    // actually removed.
    assert_eq!(doc.text(), "    foo");
    assert_eq!(heads(&cursor), vec![(0, 6)]);
}

#[test]
fn shift_tab_removes_leading_tab() {
    let mut doc = Document::new("\tfoo");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_removes_partial_indent_of_fewer_spaces() {
    let mut doc = Document::new("  foo");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Fewer than tab_width leading spaces: remove what is there, nothing more.
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_never_removes_non_whitespace() {
    let mut doc = Document::new("foo");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert!(cmd.is_none(), "outdent with no indentation must be a no-op");
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn shift_tab_cursor_inside_indent_floors_at_boundary() {
    let mut doc = Document::new("    foo");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The cursor sat inside the removed indentation: it floors at column 0
    // instead of underflowing.
    assert_eq!(doc.text(), "foo");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn shift_tab_selection_outdents_mixed_indentation_per_line() {
    // Line 0: full level of spaces; line 1: a tab; line 2: two spaces then a
    // tab (only the spaces before the tab are removable, up to tab_width).
    let mut doc = Document::new("    aa\n\tbb\n  \tcc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(2, 5))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "aa\nbb\n\tcc");
    // Anchor floors at the boundary of line 0; head shifts by the 2 columns
    // removed on line 2.
    assert_eq!(selections(&cursor), vec![((0, 0), (2, 3))]);
}

#[test]
fn shift_tab_multi_cursor_outdents_each_line_and_shifts_cursors() {
    let mut doc = Document::new("    aa\n\tbb");
    let mut cursor = cursors_at(&[(0, 5), (1, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "aa\nbb");
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1)]);
}

#[test]
fn undo_outdent_restores_text_and_cursors() {
    let mut doc = Document::new("    aa\n    bb");
    let original = cursors_at(&[(0, 5), (1, 5)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &SHIFT_TAB,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("outdent must produce a command");

    assert_eq!(doc.text(), "aa\nbb");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "    aa\n    bb");
    assert_eq!(cursor, original);
}

// ========== Enter: auto-indent ==========

#[test]
fn enter_inherits_leading_whitespace() {
    let mut doc = Document::new("    foo");
    let mut cursor = cursors_at(&[(0, 7)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "    foo\n    ");
    assert_eq!(heads(&cursor), vec![(1, 4)]);
}

#[test]
fn enter_inside_indentation_inherits_only_the_part_before_the_caret() {
    let mut doc = Document::new("    foo");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "  \n    foo");
    assert_eq!(heads(&cursor), vec![(1, 2)]);
}

#[test]
fn enter_with_auto_indent_disabled_inserts_bare_newline() {
    let mut doc = Document::new("    foo");
    let mut cursor = cursors_at(&[(0, 7)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_indent: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &ENTER, &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), "    foo\n");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn enter_bracket_block_expansion_puts_caret_on_middle_line() {
    let mut doc = Document::new("{}");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "{\n    \n}");
    assert_eq!(heads(&cursor), vec![(1, 4)]);
}

#[test]
fn enter_bracket_block_expansion_keeps_existing_indent() {
    let mut doc = Document::new("  fn x() {}");
    let mut cursor = cursors_at(&[(0, 10)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "  fn x() {\n      \n  }");
    assert_eq!(heads(&cursor), vec![(1, 6)]);
}

#[test]
fn enter_bracket_block_uses_tab_indent_when_configured() {
    let mut doc = Document::new("\t[]");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(&mut handler, &ENTER, &mut doc, &mut cursor, &tabs_config());

    assert_eq!(doc.text(), "\t[\n\t\t\n\t]");
    assert_eq!(heads(&cursor), vec![(1, 2)]);
}

#[test]
fn enter_after_opener_without_closer_adds_one_level() {
    let mut doc = Document::new("(a");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "(\n    a");
    assert_eq!(heads(&cursor), vec![(1, 4)]);
}

#[test]
fn enter_multi_cursor_each_cursor_inherits_its_own_indent() {
    let mut doc = Document::new("  a\n    b");
    let mut cursor = cursors_at(&[(0, 3), (1, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "  a\n  \n    b\n    ");
    assert_eq!(heads(&cursor), vec![(1, 2), (3, 4)]);
}

#[test]
fn enter_bracket_block_multi_cursor() {
    let mut doc = Document::new("{}\n{}");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "{\n    \n}\n{\n    \n}");
    assert_eq!(heads(&cursor), vec![(1, 4), (4, 4)]);
}

#[test]
fn enter_with_selection_replaces_it_and_inherits_indent() {
    let mut doc = Document::new("  abcdef");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 4), Position::new(0, 6))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "  ab\n  ef");
    assert_eq!(heads(&cursor), vec![(1, 2)]);
}

// ========== Enter: code-fence expansion ==========

#[test]
fn code_fence_expansion_inserts_closing_fence() {
    let mut doc = Document::new("```rust");
    let mut cursor = cursors_at(&[(0, 7)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "```rust\n\n```");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn code_fence_expansion_keeps_fence_indent() {
    let mut doc = Document::new("  ```");
    let mut cursor = cursors_at(&[(0, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "  ```\n  \n  ```");
    assert_eq!(heads(&cursor), vec![(1, 2)]);
}

#[test]
fn code_fence_expansion_requires_auto_indent() {
    let mut doc = Document::new("```rust");
    let mut cursor = cursors_at(&[(0, 7)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_indent: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &ENTER, &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), "```rust\n");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn text_after_fence_info_string_is_not_a_fence() {
    let mut doc = Document::new("```rust x");
    let mut cursor = cursors_at(&[(0, 9)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // Not `^\s*```\w*$`: plain auto-indent, no closing fence.
    assert_eq!(doc.text(), "```rust x\n");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn four_backticks_are_not_a_fence() {
    let mut doc = Document::new("````");
    let mut cursor = cursors_at(&[(0, 4)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "````\n");
    assert_eq!(heads(&cursor), vec![(1, 0)]);
}

#[test]
fn code_fence_expansion_multi_cursor() {
    let mut doc = Document::new("```js\n```py");
    let mut cursor = cursors_at(&[(0, 5), (1, 5)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ENTER,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "```js\n\n```\n```py\n\n```");
    assert_eq!(heads(&cursor), vec![(1, 0), (4, 0)]);
}

// ========== Auto-pairs ==========

#[test]
fn typing_opener_inserts_pair_with_caret_between() {
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "()");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn typing_closer_skips_over_existing_closer() {
    let mut doc = Document::new("()");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert!(cmd.is_some(), "skip-over must move the caret");
    assert_eq!(doc.text(), "()", "skip-over must not modify content");
    assert_eq!(heads(&cursor), vec![(0, 2)]);
}

#[test]
fn backspace_between_pair_deletes_both_halves() {
    let mut doc = Document::new("a()b");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "ab");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn backspace_between_quote_pair_deletes_both_halves() {
    let mut doc = Document::new("\"\"");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn backspace_between_non_empty_pair_deletes_single_character() {
    let mut doc = Document::new("(a)");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &BACKSPACE,
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "()");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn typing_opener_wraps_selection() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "(abc)");
    assert_eq!(selections(&cursor), vec![((0, 1), (0, 4))]);
}

#[test]
fn wrapping_backward_selection_preserves_orientation() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 3), Position::new(0, 0))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('['),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "[abc]");
    assert_eq!(selections(&cursor), vec![((0, 4), (0, 1))]);
}

#[test]
fn typing_closing_bracket_over_selection_replaces_it() {
    let mut doc = Document::new("abc");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), ")");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn quote_directly_after_word_character_stays_single() {
    let mut doc = Document::new("don");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('\''),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "don'");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn quote_after_non_word_character_pairs() {
    let mut doc = Document::new("a ");
    let mut cursor = cursors_at(&[(0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('"'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "a \"\"");
    assert_eq!(heads(&cursor), vec![(0, 3)]);
}

#[test]
fn bracket_after_word_character_still_pairs() {
    let mut doc = Document::new("foo");
    let mut cursor = cursors_at(&[(0, 3)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The word-character suppression applies to quotes only.
    assert_eq!(doc.text(), "foo()");
    assert_eq!(heads(&cursor), vec![(0, 4)]);
}

#[test]
fn auto_pairs_disabled_types_plain_characters() {
    let mut doc = Document::new("");
    let mut cursor = cursors_at(&[(0, 0)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &ch('('), &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), "(");
    assert_eq!(heads(&cursor), vec![(0, 1)]);
}

#[test]
fn auto_pairs_disabled_backspace_deletes_single_half() {
    let mut doc = Document::new("()");
    let mut cursor = cursors_at(&[(0, 1)]);
    let mut handler = KeyboardHandler::new();
    let config = EditorConfig {
        auto_pairs: false,
        ..EditorConfig::default()
    };

    press(&mut handler, &BACKSPACE, &mut doc, &mut cursor, &config);

    assert_eq!(doc.text(), ")");
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn auto_pair_multi_cursor_pairs_on_same_line() {
    let mut doc = Document::new("ab");
    let mut cursor = cursors_at(&[(0, 1), (0, 2)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    assert_eq!(doc.text(), "a()b()");
    assert_eq!(heads(&cursor), vec![(0, 2), (0, 5)]);
}

#[test]
fn auto_pair_multi_cursor_mixed_skip_and_plain_insert() {
    let mut doc = Document::new("()\nx");
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch(')'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The first cursor skips the existing closer; the second inserts one.
    assert_eq!(doc.text(), "()\nx)");
    assert_eq!(heads(&cursor), vec![(0, 2), (1, 2)]);
}

#[test]
fn auto_pair_multi_cursor_wrap_and_pair_insert() {
    let mut doc = Document::new("abc\nd");
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::collapsed(Position::new(1, 1)),
    ]);
    let mut handler = KeyboardHandler::new();

    press(
        &mut handler,
        &ch('{'),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    );

    // The selection is wrapped; the collapsed cursor gets a fresh pair.
    assert_eq!(doc.text(), "{abc}\nd{}");
    assert_eq!(
        selections(&cursor),
        vec![((0, 1), (0, 4)), ((1, 2), (1, 2))]
    );
}

#[test]
fn undo_auto_pair_insert_restores_text_and_cursors() {
    let mut doc = Document::new("ab");
    let original = cursors_at(&[(0, 1), (0, 2)]);
    let mut cursor = original.clone();
    let mut handler = KeyboardHandler::new();

    let cmd = press(
        &mut handler,
        &ch('('),
        &mut doc,
        &mut cursor,
        &EditorConfig::default(),
    )
    .expect("pair insertion must produce a command");

    cmd.inverse()
        .apply(&mut doc, &mut cursor)
        .expect("inverse must apply");
    assert_eq!(doc.text(), "ab");
    assert_eq!(cursor, original);
}

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
