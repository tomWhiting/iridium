//! `Enter`: inheriting indentation, expanding a bracket block, and
//! closing a code fence.
//!
//! Split out of a 1,690-line `behavior_tests.rs` for #92, on the rule the
//! file already carried. The harness and the shared fixtures are in
//! [`super`].

use super::*;

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
