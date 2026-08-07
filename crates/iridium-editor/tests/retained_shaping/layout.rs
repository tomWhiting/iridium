//! The miss side: scroll, target size, font face and font size — what the text is laid out into, and with.

use crate::harness::{
    ActiveLanguageNoSpans, WIDE_WIDTH, cold_pixels, compose, compositor, editor_over, gpu, pixels,
    target,
};
use crate::support::gpu::{FONT, FONT_SIZE, HEIGHT, WIDTH};
use crate::support::pixels::assert_same_frame;

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
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the scrolled frame must be byte-identical",
    );
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
    assert_same_frame(
        &after,
        &cold,
        WIDE_WIDTH,
        "the resized frame must be byte-identical",
    );
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
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the post-load frame must be byte-identical",
    );
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
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the resized-font frame must be byte-identical",
    );
}

/// A font size change must move the *measured character width* with it.
///
/// ⚠️ **The test above cannot catch this, and the reason is worth stating.**
/// It compares a warm compositor against a cold one, and both reach the new
/// size the same way — load the font at 14, then set 16. So both carry the
/// same stale character width, the frames agree, and a pixel-identity oracle
/// is blind to a defect that is identical on both comparands.
///
/// This one uses an oracle that cannot share the staleness: a compositor whose
/// font was loaded *at* the new size, which is correct by construction because
/// `load_font` is a remeasuring path.
///
/// What rides on the number: `char_width` is what turns a column into an x
/// coordinate and an x coordinate back into a column, so a stale one misplaces
/// the caret by an error that grows with the column — and sizes the gutter
/// wrongly by the same factor. It is not a cosmetic quantity.
#[test]
fn a_font_size_change_remeasures_the_character_width() {
    let gpu = gpu();
    let doubled = FONT_SIZE * 2.0;

    // The oracle: size set first, font loaded second, so the measurement is
    // taken at the size under test. This is the order `compositor` itself
    // uses, and the order both faces use at startup.
    let mut measured_at_the_new_size = compositor(&gpu);
    measured_at_the_new_size.set_font_size(doubled);
    measured_at_the_new_size.load_font(FONT.to_vec());
    let honest = measured_at_the_new_size.char_width();

    // The path a display change takes: a font is already loaded, and only the
    // size changes. No face can reach it today only because no face can
    // re-apply the scale at all — which is the whole of #35.
    let mut resized_after_loading = compositor(&gpu);
    assert!(
        resized_after_loading.set_font_size(doubled),
        "the doubled size must be inside the renderer's bounds, or this test \
         proves nothing"
    );

    assert!(
        (resized_after_loading.char_width() - honest).abs() < f32::EPSILON,
        "a compositor resized after loading its font reports {} where the same \
         font measured at the same size is {honest} — every column-to-x answer \
         is out by that ratio",
        resized_after_loading.char_width()
    );
}
