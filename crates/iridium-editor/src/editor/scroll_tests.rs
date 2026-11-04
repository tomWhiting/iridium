//! The viewport surface: scrolling without touching keyboard state, the margin,
//! following the caret, placing it, paging with it, and the content event.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use super::{Editor, EditorConfig, EditorEvent};
use crate::commands::{
    CommandArgs, KeyBinding, Keymap, ModifierPattern, ModifierState, StrokePattern, builtin,
};
use crate::document::Position;
use crate::input::{CaretAlignment, KeyCode, KeyEvent, Modifiers, ViewRequest};

/// `count` numbered lines.
fn lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An editor over `count` lines with a viewport of `rows` rows, in cell units.
fn editor(count: usize, rows: u16) -> Editor {
    editor_with(count, rows, EditorConfig::default())
}

fn editor_with(count: usize, rows: u16, config: EditorConfig) -> Editor {
    let mut editor = Editor::new(config);
    editor.set_content(&lines(count));
    editor.set_viewport_geometry(80.0, f32::from(rows), 1.0);
    editor
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::simple(code)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, Modifiers::ctrl())
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(
        code,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    )
}

fn ctrl_alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(
        code,
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
    )
}

// ===== Scrolling leaves the keyboard handler alone =====

#[test]
fn a_scroll_between_two_vertical_moves_keeps_the_sticky_column() {
    let mut editor = Editor::with_defaults();
    editor.set_content("long line here\nab\nanother long line\n");
    editor.set_viewport_geometry(80.0, 2.0, 1.0);
    editor.set_cursor(Position::new(0, 10));

    editor.handle_key(&key(KeyCode::Down));
    assert_eq!(editor.cursor(), Position::new(1, 2));
    assert!(editor.scroll_view(1));
    editor.handle_key(&key(KeyCode::Down));
    assert_eq!(
        editor.cursor(),
        Position::new(2, 10),
        "the sticky column must survive a scroll"
    );
}

#[test]
fn a_scroll_does_not_abort_a_half_typed_chord() {
    let mut editor = editor(100, 10);
    let mut layer = Keymap::new("chord");
    let ctrl_b = StrokePattern::new(
        KeyCode::Char('b'),
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    );
    let ctrl_e = StrokePattern::new(
        KeyCode::Char('e'),
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    );
    layer.push(KeyBinding::new(
        ctrl_b,
        &[ctrl_e],
        builtin::CURSOR_DOCUMENT_END,
    ));
    editor.push_keymap(layer).expect("Ctrl+B is free");

    editor.handle_key(&ctrl(KeyCode::Char('b')));
    assert!(!editor.pending_key_sequence().is_empty());
    editor.scroll_view(5);
    editor.set_viewport_geometry(80.0, 12.0, 1.0);
    assert!(
        !editor.pending_key_sequence().is_empty(),
        "neither a scroll nor a resize may abandon the chord"
    );
    editor.handle_key(&ctrl(KeyCode::Char('e')));
    assert_eq!(editor.cursor().line, 99);
}

#[test]
fn a_scroll_does_not_forget_which_cursor_was_added_last() {
    let mut editor = editor(100, 10);
    editor.set_cursor(Position::new(2, 0));
    editor.handle_key(&ctrl_alt(KeyCode::Down));
    editor.handle_key(&ctrl_alt(KeyCode::Down));
    assert_eq!(editor.state().cursor.cursor_count(), 3);

    editor.scroll_view(4);
    editor.handle_key(&ctrl(KeyCode::Char('u')));
    assert_eq!(
        editor.state().cursor.cursor_count(),
        2,
        "remove-last-cursor must still know the order after a scroll"
    );
}

// ===== Scrolling itself =====

#[test]
fn scrolling_moves_the_view_and_no_caret() {
    let mut editor = editor(100, 10);
    editor.set_cursor(Position::new(3, 2));
    assert!(editor.scroll_view(7));
    assert_eq!(editor.view_top(), 7);
    assert_eq!(editor.cursor(), Position::new(3, 2));
    assert!(editor.scroll_view(-3));
    assert_eq!(editor.view_top(), 4);
    assert!(editor.scroll_view(-100));
    assert_eq!(editor.view_top(), 0);
    assert!(!editor.scroll_view(-1), "the top is a wall");
}

#[test]
fn the_view_stops_with_the_last_line_at_the_bottom_unless_told_otherwise() {
    let mut editor = editor(100, 10);
    editor.scroll_view(1_000);
    assert_eq!(editor.view_top(), 90);

    let mut past = editor_with(
        100,
        10,
        EditorConfig {
            scroll_past_end: true,
            ..EditorConfig::default()
        },
    );
    past.scroll_view(1_000);
    assert_eq!(past.view_top(), 99, "the last line may reach the top");
}

#[test]
fn a_wheel_notch_is_the_configured_number_of_lines() {
    let mut editor = editor(100, 10);
    editor.scroll_wheel(2);
    assert_eq!(editor.view_top(), 6);
    editor.scroll_wheel(-1);
    assert_eq!(editor.view_top(), 3);

    let mut one = editor_with(
        100,
        10,
        EditorConfig {
            scroll_wheel_lines: 1,
            ..EditorConfig::default()
        },
    );
    one.scroll_wheel(2);
    assert_eq!(one.view_top(), 2);
}

#[test]
fn the_scroll_line_keys_scroll_and_leave_the_caret() {
    let mut editor = editor(100, 10);
    editor.set_cursor(Position::new(4, 1));
    editor.handle_key(&ctrl(KeyCode::Down));
    editor.handle_key(&ctrl(KeyCode::Down));
    assert_eq!(editor.view_top(), 2);
    assert_eq!(editor.cursor(), Position::new(4, 1));
    editor.handle_key(&ctrl(KeyCode::Up));
    assert_eq!(editor.view_top(), 1);
    assert_eq!(editor.cursor(), Position::new(4, 1));
}

#[test]
fn a_scroll_command_honours_its_count() {
    let mut editor = editor(100, 10);
    editor
        .run_command(
            builtin::VIEW_SCROLL_LINE_DOWN.as_str(),
            CommandArgs::with_count(4),
        )
        .expect("a kernel command");
    assert_eq!(editor.view_top(), 4);
}

// ===== The margin =====

#[test]
fn moving_down_keeps_the_margin_below_the_caret() {
    let mut editor = editor(100, 20);
    for _ in 0..40 {
        editor.handle_key(&key(KeyCode::Down));
        editor.follow_caret();
        let row = editor.caret_row().expect("the caret is followed");
        assert!(row <= 20 - 1 - 5, "row {row} is inside the bottom margin");
    }
    assert_eq!(editor.caret_row(), Some(14));
}

#[test]
fn the_margin_gives_way_at_the_ends_of_the_document() {
    let mut editor = editor(100, 20);
    editor.set_cursor(Position::new(99, 0));
    editor.follow_caret();
    assert_eq!(
        editor.caret_row(),
        Some(19),
        "the last line can reach the bottom"
    );
    editor.set_cursor(Position::new(0, 0));
    editor.follow_caret();
    assert_eq!(editor.caret_row(), Some(0));
}

#[test]
fn a_small_viewport_caps_the_margin() {
    let mut editor = editor(100, 4);
    for _ in 0..10 {
        editor.handle_key(&key(KeyCode::Down));
        editor.follow_caret();
    }
    // Four rows allow a margin of one: the caret rides row two.
    assert_eq!(editor.caret_row(), Some(2));
}

// ===== Following the caret =====

#[test]
fn following_does_not_undo_a_scroll_until_the_caret_moves() {
    let mut editor = editor(100, 10);
    editor.follow_caret();
    editor.scroll_view(50);
    assert!(!editor.follow_caret(), "nothing moved, so nothing follows");
    assert_eq!(editor.view_top(), 50);
    assert_eq!(editor.caret_row(), None);

    editor.handle_key(&key(KeyCode::Right));
    assert!(editor.follow_caret());
    assert!(editor.caret_row().is_some());
}

#[test]
fn typing_brings_the_view_back_to_the_caret() {
    let mut editor = editor(100, 10);
    editor.follow_caret();
    editor.scroll_view(50);
    editor.handle_key(&key(KeyCode::Char('x')));
    assert!(editor.follow_caret());
    assert_eq!(editor.caret_row(), Some(0));
}

#[test]
fn a_resize_keeps_a_visible_caret_visible_and_leaves_a_scrolled_view_alone() {
    let mut editor = editor(100, 20);
    editor.set_cursor(Position::new(18, 0));
    editor.follow_caret();
    editor.set_viewport_geometry(80.0, 10.0, 1.0);
    assert!(
        editor.caret_row().is_some(),
        "a shrink must not hide the caret"
    );

    editor.scroll_view(60);
    let top = editor.view_top();
    editor.set_viewport_geometry(80.0, 12.0, 1.0);
    assert_eq!(editor.view_top(), top, "a caret scrolled away stays away");
}

#[test]
fn a_line_height_that_is_not_positive_is_ignored() {
    let mut editor = editor(100, 10);
    editor.set_viewport_geometry(80.0, 10.0, 0.0);
    assert_eq!(editor.state().viewport.visible_lines, 10);
    editor.set_viewport_geometry(80.0, 10.0, f32::NAN);
    assert_eq!(editor.state().viewport.visible_lines, 10);
}

// ===== Placing the caret's line =====

#[test]
fn the_caret_line_can_be_centred_raised_and_lowered() {
    let mut editor = editor(100, 11);
    editor.set_cursor(Position::new(50, 0));

    editor.align_caret(CaretAlignment::Center);
    assert_eq!(editor.caret_row(), Some(5));
    editor.align_caret(CaretAlignment::Top);
    assert_eq!(
        editor.caret_row(),
        Some(5),
        "eleven rows cap the margin at five"
    );

    let mut wide = editor_with(
        100,
        11,
        EditorConfig {
            scroll_margin: 2,
            ..EditorConfig::default()
        },
    );
    wide.set_cursor(Position::new(50, 0));
    wide.align_caret(CaretAlignment::Top);
    assert_eq!(wide.caret_row(), Some(2));
    wide.align_caret(CaretAlignment::Bottom);
    assert_eq!(wide.caret_row(), Some(8));
    assert_eq!(wide.cursor(), Position::new(50, 0), "no caret moves");
}

#[test]
fn recentre_cycles_centre_top_bottom_and_starts_over_after_a_move() {
    let mut editor = editor_with(
        100,
        11,
        EditorConfig {
            scroll_margin: 1,
            ..EditorConfig::default()
        },
    );
    editor.set_cursor(Position::new(50, 0));
    editor.handle_key(&ctrl(KeyCode::Char('l')));
    assert_eq!(editor.caret_row(), Some(5));
    editor.handle_key(&ctrl(KeyCode::Char('l')));
    assert_eq!(editor.caret_row(), Some(1));
    editor.handle_key(&ctrl(KeyCode::Char('l')));
    assert_eq!(editor.caret_row(), Some(9));
    editor.handle_key(&ctrl(KeyCode::Char('l')));
    assert_eq!(editor.caret_row(), Some(5), "and round again");

    editor.handle_key(&ctrl(KeyCode::Char('l')));
    editor.handle_key(&key(KeyCode::Down));
    editor.handle_key(&ctrl(KeyCode::Char('l')));
    assert_eq!(editor.caret_row(), Some(5), "a move starts the cycle over");
}

#[test]
fn every_view_request_is_reachable_by_id() {
    for (id, request) in [
        (
            builtin::VIEW_CENTER_CARET,
            ViewRequest::Align(CaretAlignment::Center),
        ),
        (
            builtin::VIEW_CARET_TO_TOP,
            ViewRequest::Align(CaretAlignment::Top),
        ),
        (
            builtin::VIEW_CARET_TO_BOTTOM,
            ViewRequest::Align(CaretAlignment::Bottom),
        ),
    ] {
        let mut by_id = editor(100, 11);
        by_id.set_cursor(Position::new(50, 0));
        by_id
            .run_command(id.as_str(), CommandArgs::NONE)
            .expect("a kernel command");
        let mut direct = editor(100, 11);
        direct.set_cursor(Position::new(50, 0));
        direct.perform_view_request(request);
        assert_eq!(by_id.view_top(), direct.view_top(), "{id}");
    }
}

// ===== Paging moves the view with the caret =====

#[test]
fn page_down_keeps_the_caret_on_its_row() {
    let mut editor = editor(200, 20);
    editor.set_cursor(Position::new(8, 0));
    editor.follow_caret();
    let row = editor.caret_row();
    assert_eq!(row, Some(8));

    editor.handle_key(&key(KeyCode::PageDown));
    assert_eq!(editor.cursor().line, 28);
    assert_eq!(editor.caret_row(), row, "the page moved under the caret");
    editor.handle_key(&key(KeyCode::PageUp));
    assert_eq!(editor.cursor().line, 8);
    assert_eq!(editor.caret_row(), row);
}

#[test]
fn half_a_page_moves_the_caret_and_the_view_together() {
    let mut editor = editor(200, 20);
    editor.set_cursor(Position::new(10, 0));
    editor.follow_caret();
    let top = editor.view_top();

    editor.handle_key(&alt(KeyCode::PageDown));
    assert_eq!(editor.cursor().line, 20);
    assert_eq!(editor.view_top(), top + 10);
    editor
        .run_command(
            builtin::VIEW_SCROLL_HALF_PAGE_UP.as_str(),
            CommandArgs::NONE,
        )
        .expect("a kernel command");
    assert_eq!(editor.cursor().line, 10);
    assert_eq!(editor.view_top(), top);
}

#[test]
fn half_a_page_keeps_the_sticky_column() {
    let mut text = String::new();
    for index in 0..40 {
        text.push_str(if index % 2 == 0 {
            "a long line of text\n"
        } else {
            "x\n"
        });
    }
    let mut editor = Editor::with_defaults();
    editor.set_content(&text);
    editor.set_viewport_geometry(80.0, 10.0, 1.0);
    editor.set_cursor(Position::new(0, 12));
    // Half of ten rows is five lines: line 5 is short, line 10 long again.
    editor.handle_key(&alt(KeyCode::PageDown));
    assert_eq!(editor.cursor(), Position::new(5, 1));
    editor.handle_key(&alt(KeyCode::PageDown));
    assert_eq!(editor.cursor(), Position::new(10, 12));
}

// ===== Events =====

#[test]
fn nothing_is_built_for_an_editor_nobody_listens_to() {
    // The behavioural half: with no listener the edit simply happens. The cost
    // half is measured by the terminal face's `scroll_bench`.
    let mut editor = editor(10, 5);
    editor.handle_key(&key(KeyCode::Char('x')));
    assert!(editor.content().starts_with("xline 0"));
}

#[test]
fn a_content_listener_reads_the_text_it_was_sent() {
    // No grouping, so the undo below takes back one keystroke.
    let mut editor = editor_with(
        3,
        5,
        EditorConfig {
            undo_group_timeout_ms: 0,
            ..EditorConfig::default()
        },
    );
    let seen: Arc<Mutex<Vec<(String, u64)>>> = Arc::default();
    let sink = Arc::clone(&seen);
    editor.add_listener(move |event| {
        if let EditorEvent::ContentChanged { content } = event {
            sink.lock()
                .expect("not poisoned")
                .push((content.text(), content.revision()));
        }
    });

    editor.handle_key(&key(KeyCode::Char('x')));
    editor.handle_key(&key(KeyCode::Char('y')));
    assert!(editor.undo());

    let seen = std::mem::take(&mut *seen.lock().expect("not poisoned"));
    let texts: Vec<&str> = seen.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "xline 0\nline 1\nline 2",
            "xyline 0\nline 1\nline 2",
            "xline 0\nline 1\nline 2",
        ]
    );
    assert!(
        seen.windows(2).all(|pair| pair[0].1 < pair[1].1),
        "every event carries a newer revision"
    );
}

#[test]
fn a_snapshot_kept_by_a_listener_is_not_changed_by_later_edits() {
    let mut editor = editor(1, 5);
    let kept: Arc<Mutex<Vec<crate::TextSnapshot>>> = Arc::default();
    let sink = Arc::clone(&kept);
    editor.add_listener(move |event| {
        if let EditorEvent::ContentChanged { content } = event {
            sink.lock().expect("not poisoned").push(content.clone());
        }
    });
    editor.handle_key(&key(KeyCode::Char('a')));
    editor.handle_key(&key(KeyCode::Char('b')));
    let kept = std::mem::take(&mut *kept.lock().expect("not poisoned"));
    assert_eq!(kept[0], "aline 0");
    assert_eq!(kept[1], "abline 0");
}

#[test]
fn a_scroll_is_announced_once_and_a_blocked_one_not_at_all() {
    let mut editor = editor(100, 10);
    let firsts: Arc<Mutex<Vec<usize>>> = Arc::default();
    let sink = Arc::clone(&firsts);
    editor.add_listener(move |event| {
        if let EditorEvent::ScrollChanged { first_line } = event {
            sink.lock().expect("not poisoned").push(*first_line);
        }
    });
    editor.scroll_view(3);
    editor.scroll_view(-10);
    editor.scroll_view(-1);
    assert_eq!(*firsts.lock().expect("not poisoned"), vec![3, 0]);
}

// ===== Folds count once =====

#[cfg(feature = "syntax")]
#[test]
fn a_folded_region_scrolls_as_one_line() {
    use std::fmt::Write as _;

    let mut editor = Editor::with_defaults();
    let mut text = String::from("fn folded() {\n");
    for index in 0..30 {
        writeln!(text, "    let x{index} = {index};").expect("a String accepts writes");
    }
    text.push_str("}\n");
    text.push_str(&lines(40));
    editor.set_content(&text);
    editor.set_language(iridium_lang::Language::Rust);
    editor.set_viewport_geometry(80.0, 10.0, 1.0);
    assert!(editor.fold_at(0));

    // Visual line one is the first line after the fold: document line 32.
    editor.scroll_view(1);
    assert_eq!(
        editor
            .state()
            .fold_state
            .visual_to_document_line(editor.view_top()),
        32
    );

    // A caret inside the fold is drawn on its header and revealed there.
    editor.set_cursor(Position::new(10, 0));
    editor.follow_caret();
    assert_eq!(editor.view_top(), 0);
    assert_eq!(editor.caret_row(), Some(0));
}
