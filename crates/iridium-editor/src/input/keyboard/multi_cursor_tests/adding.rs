//! Making carets: above and below, every occurrence, and the `AltGr` chord
//! that must not be mistaken for one.
//!
//! Split out of a 1,122-line `multi_cursor_tests.rs` for #92, on the rules
//! the file already carried. The harness and the chords are in [`super`].

use super::*;

// ========== Add cursor above/below ==========

#[test]
fn add_below_is_sticky_across_a_short_line() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(0, 8)]);

    // First add clamps to the short middle line.
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(0, 8), (1, 2)]);

    // Second add: the clamped cursor's sticky column (8) lands on the long
    // third line, not the clamped column (2).
    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(0, 8), (1, 2), (2, 8)]);
}

#[test]
fn add_above_is_sticky_across_a_short_line() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new(STICKY_DOC);
    let mut cursor = cursors_at(&[(2, 8)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(heads(&cursor), vec![(2, 8), (1, 2)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    // The middle cursor's sticky 8 restores on the long first line.
    assert_eq!(heads(&cursor), vec![(2, 8), (0, 8), (1, 2)]);
}

#[test]
fn repeated_adds_extend_the_block_and_reverse_direction() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("l0\nl1\nl2\nl3\nl4");
    let mut cursor = cursors_at(&[(2, 1)]);

    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor);
    // Cloning every cursor each press grows the contiguous block upward.
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(2, 1), (0, 1), (1, 1)]);

    // Reversing direction extends the block downward by one.
    press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 4);
    assert_eq!(heads(&cursor), vec![(2, 1), (0, 1), (1, 1), (3, 1)]);
}

#[test]
fn add_below_merges_colliding_clones() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc\ndd");
    // Two adjacent cursors: cloning both down makes the upper clone land on
    // the lower cursor, which must merge (3 cursors, not 4).
    let mut cursor = cursors_at(&[(0, 1), (1, 1)]);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1), (2, 1)]);
}

#[test]
fn add_above_at_top_is_a_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(0, 0)]);

    // At the first line there is no line above and no wraparound.
    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(heads(&cursor), vec![(0, 0)]);
}

#[test]
fn add_below_at_bottom_is_a_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(2, 1)]);

    assert!(press(&mut handler, &ADD_BELOW, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn add_above_only_the_non_edge_cursors_contribute() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    // Cursor on the top line contributes nothing; the one on line 2 clones up.
    let mut cursor = cursors_at(&[(0, 1), (2, 1)]);

    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(heads(&cursor), vec![(0, 1), (1, 1), (2, 1)]);
}

// ========== Select all occurrences ==========

#[test]
fn select_all_occurrences_from_selection() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo baz foo");
    // Select the first "foo".
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    // The current occurrence stays primary; the others become secondaries.
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 8), (0, 11)), ((0, 16), (0, 19)),]
    );
}

#[test]
fn select_all_occurrences_from_word_under_caret() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo bar foo");
    // Collapsed caret inside the first "foo".
    let mut cursor = cursors_at(&[(0, 1)]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 8), (0, 11))]
    );
}

#[test]
fn select_all_occurrences_is_non_overlapping() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaa");
    // Selecting "aa" in "aaa" must match once (non-overlapping), like Ctrl+D —
    // the overlapping "aa" starting at column 1 is not a separate cursor.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn select_all_occurrences_finds_separated_matches() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa xx aa xx aa");
    // Three separated "aa" matches become three cursors.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 3);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 2)), ((0, 6), (0, 8)), ((0, 12), (0, 14))]
    );
}

#[test]
fn select_all_occurrences_adjacent_matches_stay_distinct() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aaaa");
    // "aa" in "aaaa" yields two adjacent, non-overlapping matches (0-2, 2-4).
    // Each occurrence must remain its own edit target — building the cursor
    // state directly (not via `add_cursor`, whose adjacency merge would fuse
    // them) keeps them separate.
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 2)), ((0, 2), (0, 4))]
    );
}

#[test]
fn select_all_occurrences_adjacent_matches_edit_independently() {
    // Regression for the reviewer's finding: selecting the first "foo" in
    // "foofoo" and pressing Ctrl+Shift+L must yield two occurrence cursors, so
    // typing "x" produces "xx" (two edits), not a single fused "x".
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foofoo");
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 0), Position::new(0, 3))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 0), (0, 3)), ((0, 3), (0, 6))]
    );

    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('x')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "xx");
    assert_eq!(heads(&cursor), vec![(0, 1), (0, 2)]);
}

#[test]
fn select_all_occurrences_empty_source_is_noop() {
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("   foo");

    // Collapsed caret on whitespace: no word to select, so nothing happens.
    let mut cursor = cursors_at(&[(0, 0)]);
    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_none());
    assert_eq!(cursor.cursor_count(), 1);
}

#[test]
fn select_all_occurrences_honors_explicit_whitespace_selection() {
    // Regression for the reviewer's finding: Ctrl+D accepts a non-empty
    // whitespace selection, so Ctrl+Shift+L must too — selecting the first
    // space in "a b c" selects every space, not nothing.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("a b c");
    // The single space between "a" and "b" (offset 1..2).
    let mut cursor = cursors_with(&[Selection::new(Position::new(0, 1), Position::new(0, 2))]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
    assert_eq!(
        selections(&cursor),
        vec![((0, 1), (0, 2)), ((0, 3), (0, 4))]
    );
}

#[test]
fn select_all_occurrences_unique_word_selects_the_occurrence() {
    // Reviewer finding: with a collapsed caret on a uniquely occurring word,
    // Ctrl+Shift+L must still expand the caret to select that occurrence rather
    // than short-circuiting to a no-op because there are no *other* matches.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("hello world");
    let mut cursor = cursors_at(&[(0, 1)]);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 5))]);
}

#[test]
fn select_all_occurrences_drops_unrelated_cursor_when_reference_is_unique() {
    // Reviewer finding: a unique primary occurrence plus an unrelated secondary
    // (added by a vertical/mouse path) must collapse to just the reference
    // occurrence. Previously an empty secondary short-circuited to a no-op,
    // preserving the unrelated cursor so a subsequent keystroke would edit both
    // locations.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("foo xyz");
    // Primary selects the sole "foo"; an unrelated caret sits inside "xyz".
    let mut cursor = cursors_with(&[
        Selection::new(Position::new(0, 0), Position::new(0, 3)),
        Selection::collapsed(Position::new(0, 5)),
    ]);
    assert_eq!(cursor.cursor_count(), 2);

    assert!(press(&mut handler, &SELECT_ALL_OCC, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 1);
    assert_eq!(selections(&cursor), vec![((0, 0), (0, 3))]);

    // Typing now edits only the reference occurrence.
    press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char('Z')),
        &mut doc,
        &mut cursor,
    );
    assert_eq!(doc.text(), "Z xyz");
}

// ========== AltGr (AltGraph) must not reach the add-cursor chord ==========

#[test]
fn altgraph_arrow_does_not_spawn_cursors() {
    // Reviewer finding: on non-US layouts AltGr is reported as Ctrl+Alt while
    // composing a character. An AltGraph-marked Up/Down must NOT be treated as
    // the add-cursor-above/below chord (which is Ctrl+Alt without AltGraph), or
    // it would spawn cursors and subsequent typing/deletion would apply at
    // multiple locations.
    let mut handler = KeyboardHandler::new();
    let mut doc = Document::new("aa\nbb\ncc");
    let mut cursor = cursors_at(&[(1, 1)]);

    let altgr_ctrl_alt = Modifiers {
        shift: false,
        ctrl: true,
        alt: true,
        meta: false,
        alt_graph: true,
    };
    let altgr_up = KeyEvent::new(KeyCode::Up, altgr_ctrl_alt);
    let altgr_down = KeyEvent::new(KeyCode::Down, altgr_ctrl_alt);

    // Excluded from the add-cursor dispatch, the AltGraph arrow falls through to
    // plain single-cursor vertical navigation. The essential guarantee is that
    // it never *spawns* a cursor, so a following keystroke cannot edit at
    // multiple locations.
    press(&mut handler, &altgr_up, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 1, "AltGr+Up must not add a cursor");
    assert_eq!(heads(&cursor), vec![(0, 1)]); // moved up, still one cursor

    press(&mut handler, &altgr_down, &mut doc, &mut cursor);
    assert_eq!(cursor.cursor_count(), 1, "AltGr+Down must not add a cursor");
    assert_eq!(heads(&cursor), vec![(1, 1)]);

    // The genuine Ctrl+Alt chord (AltGraph absent) still adds cursors.
    assert!(press(&mut handler, &ADD_ABOVE, &mut doc, &mut cursor).is_some());
    assert_eq!(cursor.cursor_count(), 2);
}
