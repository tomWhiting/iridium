//! The miss side: scroll, target size, font face and font size — what the text is laid out into, and with.

use crate::harness::{
    ActiveLanguageNoSpans, WIDE_WIDTH, cold_pixels, compose, compositor, editor_over, gpu, pixels,
    target,
};
use crate::support::gpu::{FONT, HEIGHT, WIDTH};

/// A scroll that crosses a line boundary changes the viewport range: a full
/// miss (stage 2b is out of scope), never the diff path.
#[test]
fn a_line_scroll_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;
    let line_height = warm.line_height();
    // Mid-line anchored so `floor(scroll / line_height)` is robust against
    // f32 rounding on both sides of the one-line hop.
    let base = 50.25 * line_height;

    compose(&mut warm, &editor, base, &mut highlights, &gpu, &tgt);
    compose(
        &mut warm,
        &editor,
        base + line_height,
        &mut highlights,
        &gpu,
        &tgt,
    );
    assert_eq!(warm.shape_rebuilds(), 2, "a one-line scroll is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        0,
        "a viewport shift is a full rebuild, not a diff"
    );
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        base + line_height,
        &mut ActiveLanguageNoSpans,
        |_| {},
    );
    assert!(after == cold, "the scrolled frame must be byte-identical");
}

/// A surface resize changes the wrap width.
#[test]
fn a_resize_misses_and_recomposes_identically() {
    let gpu = gpu();
    let narrow = target(&gpu, WIDTH, HEIGHT);
    let wide = target(&gpu, WIDE_WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &narrow);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &wide);
    assert_eq!(warm.shape_rebuilds(), 2, "a resize is a miss");
    let after = pixels(&gpu, &wide);

    let cold = cold_pixels(
        &gpu,
        &wide,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |_| {},
    );
    assert!(after == cold, "the resized frame must be byte-identical");
}

/// Loading font data can change how `Family::Monospace` resolves.
#[test]
fn loading_a_font_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    warm.load_font(FONT.to_vec());
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a font load is a miss");
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.load_font(FONT.to_vec());
        },
    );
    assert!(after == cold, "the post-load frame must be byte-identical");
}

/// The font size is a shaping metric.
#[test]
fn a_font_size_change_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_font_size(16.0);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a font size change is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the size change must be visible");

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_font_size(16.0);
        },
    );
    assert!(
        after == cold,
        "the resized-font frame must be byte-identical"
    );
}
