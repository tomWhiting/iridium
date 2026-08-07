//! The miss side, stage 2a: the edit path.

use iridium_editor::{Editor, KeyCode, Position};

use crate::harness::{
    ActiveLanguageNoSpans, cold_pixels, compose, compositor, document, editor_over, gpu, pixels,
    press, target,
};
use crate::support::gpu::{HEIGHT, WIDTH};
use crate::support::pixels::assert_same_frame;

/// A one-character keystroke is a miss served by per-line diffing: exactly
/// one line reshapes, and the diffed frame is byte-identical to a fresh
/// compositor composing the edited document cold (§5.4, §5.3b).
#[test]
fn a_one_character_edit_reshapes_one_line_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    assert_eq!(warm.lines_reshaped(), 0, "the cold frame is a full build");

    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "an edit is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        1,
        "a one-character edit reshapes exactly one line"
    );
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the edit must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the diffed frame must be byte-identical to a cold compose of the same state",
    );

    // A second consecutive keystroke diffs against a diffed buffer — the
    // construction must be self-consistent, not merely consistent with the
    // full build once.
    let _ = editor.handle_key(&press(KeyCode::Char('y')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        warm.lines_reshaped(),
        2,
        "the second keystroke reshapes one more"
    );
    let second = pixels(&gpu, &tgt);
    let second_cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &second,
        &second_cold,
        WIDTH,
        "diff-after-diff must stay byte-identical",
    );
}

/// The plain-text arm (syntax highlighting off) diffs through
/// `set_text_diffed`, whose construction must match `Buffer::set_text`'s.
#[test]
fn a_plain_text_edit_diffs_one_line_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    warm.set_syntax_enabled(false);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the edit misses");
    assert_eq!(warm.lines_reshaped(), 1, "and diffs exactly one line");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_syntax_enabled(false);
        },
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the plain-text diffed frame must be byte-identical",
    );
}

/// Multibyte content takes cosmic-text's full bidi analysis path when the
/// diffed setter splits lines — the construction must stay byte-identical
/// there too, not only for the ASCII fast path.
#[test]
fn a_multibyte_edit_diffs_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    let mut content = document(40);
    content.push_str("fn greet() { let s = \"héllo 🦀 ẑ\"; }\n");
    content.push_str(&document(40));
    editor.set_content(&content);
    editor.set_cursor(Position::new(40, 13));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Char('x')));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the edit misses");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the multibyte diffed frame must be byte-identical",
    );
}

/// An edit that inserts a line shifts everything below it in the buffer —
/// the diff must extend/truncate correctly and stay pixel-identical.
#[test]
fn an_edit_that_adds_a_line_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 0));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = editor.handle_key(&press(KeyCode::Enter));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the newline is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the line-inserting diff must be byte-identical",
    );
}

/// An edit that makes a line wrap changes the wrap counts and with them the
/// gutter's continuation rows — the diffed gutter must keep up.
#[test]
fn an_edit_that_changes_wrapping_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(200);
    editor.set_cursor(Position::new(5, 3));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    editor.paste("wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap wrap");
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the paste is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the wrap-changing diff must be byte-identical",
    );
}
