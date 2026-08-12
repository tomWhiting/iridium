//! The bottom of the scroll, and the one property it has to have: it must not
//! depend on where the scroll currently is.
//!
//! ⚠️ **This is a feedback loop when it fails, not a wrong number.** The face
//! clamps every scroll write against
//! [`FrameCompositor::max_scroll_y`](iridium_editor::render::FrameCompositor::max_scroll_y),
//! and the limit is read *between* frames from what the last frame shaped. If
//! shaping a different window can move the limit, then a wheel tick at the
//! bottom clamps to limit A, the frame it triggers reports limit B, the next
//! tick clamps to B, and the frame that follows reports A again — the document
//! bounces between two positions for as long as the wheel is turning.

use std::fmt::Write as _;

use iridium_editor::Editor;

use crate::harness::{ActiveLanguageNoSpans, compose, compositor, gpu, target};
use crate::support::gpu::{HEIGHT, HEIGHT_F32, WIDTH};

/// Long enough to wrap several times at [`WIDTH`], so the shaped window's row
/// count is genuinely larger than its document-line count.
fn wrapping_line(index: usize) -> String {
    format!("{index}: {}", "wrap ".repeat(40))
}

/// Lines that wrap at the top, lines that do not at the bottom — so scrolling
/// from one end to the other changes how much wrapping is on screen, which is
/// the whole variable under test.
fn mixed_document(wrapping: usize, plain: usize) -> String {
    let mut text = String::new();
    for index in 0..wrapping {
        text.push_str(&wrapping_line(index));
        text.push('\n');
    }
    for index in 0..plain {
        // Ignored deliberately: writing into a `String` cannot fail, and the
        // suite's `unwrap` ban applies to tests as well as to the crate.
        let _ = writeln!(text, "short {index}");
    }
    text
}

/// ⭐ **The scroll limit is a property of the document, not of the viewport's
/// current position.**
#[test]
fn the_scroll_limit_does_not_move_when_the_window_does() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(&mixed_document(40, 40));
    let mut highlights = ActiveLanguageNoSpans;

    // Top of the document: every line on screen wraps.
    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let at_top = compositor.max_scroll_y(&editor, editor.fold_state(), HEIGHT_F32);

    // Deep enough that only the unwrapped tail is on screen.
    let deep = 60.0 * compositor.line_height();
    compose(&mut compositor, &editor, deep, &mut highlights, &gpu, &tgt);
    let at_bottom = compositor.max_scroll_y(&editor, editor.fold_state(), HEIGHT_F32);

    assert!(
        (at_top - at_bottom).abs() < 0.5,
        "the same document reported two different bottoms — {at_top} with the \
         wrapped lines on screen, {at_bottom} without them; a clamp against a \
         limit that moves with the window is what bounces the document"
    );
}

/// The same defect stated as the face experiences it: clamp, compose, and the
/// limit you clamped against must still be the limit.
#[test]
fn clamping_to_the_bottom_is_a_fixed_point() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(&mixed_document(40, 40));
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let mut scroll = compositor.max_scroll_y(&editor, editor.fold_state(), HEIGHT_F32);

    // Three rounds of "paint where the clamp said, then ask again". A face at
    // rest at the bottom does exactly this on every wheel tick that cannot
    // move it any further.
    for round in 0..3 {
        compose(
            &mut compositor,
            &editor,
            scroll,
            &mut highlights,
            &gpu,
            &tgt,
        );
        let limit = compositor.max_scroll_y(&editor, editor.fold_state(), HEIGHT_F32);
        assert!(
            (limit - scroll).abs() < 0.5,
            "round {round}: resting at {scroll} produced a frame whose limit is \
             {limit}, so the next clamp moves the document with no input"
        );
        scroll = limit;
    }
}
